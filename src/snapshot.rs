//! Headless panel snapshots: run a piece of UI through egui with no window
//! and rasterize what it asked to draw.
//!
//! A properties panel is a picture, and "it drew some shapes" is not the same
//! claim as "it drew the right picture". This renders egui's own tessellated
//! output — the exact vertices the GPU would receive — so a panel can be
//! LOOKED at in CI, in a terminal, or by whoever is reading the diff. It has
//! already caught selection ellipses leaking across a panel and a colour disc
//! that was a wall of clipped primaries.
//!
//! Text is skipped: glyphs live in a font atlas this has no access to, and
//! painting their quads solid would be worse than leaving them out.
//!
//! Test-only, and deliberately dependency-free beyond `egui` + `png`.

use egui::{epaint, Pos2, Rect, Vec2};

pub struct Shot {
    pub buf: Vec<u8>,
    pub w: usize,
    pub h: usize,
    /// Supersample factor: image pixels per egui point.
    pub ss: f32,
}

impl Shot {
    /// RGB at an image pixel, for assertions about what was actually drawn.
    pub fn px(&self, x: usize, y: usize) -> [u8; 3] {
        let o = (y.min(self.h - 1) * self.w + x.min(self.w - 1)) * 3;
        [self.buf[o], self.buf[o + 1], self.buf[o + 2]]
    }

    /// RGB at a position in egui POINTS, which is what the UI code thinks in.
    pub fn at(&self, x: f32, y: f32) -> [u8; 3] {
        self.px((x * self.ss) as usize, (y * self.ss) as usize)
    }

    pub fn save(&self, name: &str) -> std::path::PathBuf {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("target/{name}.png"));
        let file = std::fs::File::create(&path).unwrap();
        let mut enc =
            png::Encoder::new(std::io::BufWriter::new(file), self.w as u32, self.h as u32);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&self.buf).unwrap();
        eprintln!("wrote {}", path.display());
        path
    }
}

/// Run `build` in a windowless egui pass sized `w_pt` x `h_pt` and rasterize.
pub fn shoot(w_pt: f32, h_pt: f32, build: impl FnOnce(&mut egui::Ui)) -> Shot {
    let ss = 2.0f32;
    let ctx = egui::Context::default();
    let mut input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(w_pt, h_pt))),
        ..Default::default()
    };
    input.viewports.values_mut().for_each(|v| v.native_pixels_per_point = Some(1.0));
    let mut once = Some(build);
    let mut out = ctx.run_ui(input, |ui| {
        if let Some(f) = once.take() {
            f(ui);
        }
    });
    out.textures_delta.clear();
    let prims = ctx.tessellate(std::mem::take(&mut out.shapes), 1.0);
    let (w, h) = ((w_pt * ss) as usize, (h_pt * ss) as usize);
    Shot { buf: rasterize(&prims, w, h, ss), w, h, ss }
}

fn rasterize(prims: &[egui::ClippedPrimitive], w: usize, h: usize, ss: f32) -> Vec<u8> {
    let mut buf = vec![18u8; w * h * 3];
    for p in prims {
        let epaint::Primitive::Mesh(m) = &p.primitive else { continue };
        // egui carries the clip as metadata on the primitive; a rasterizer
        // that ignores it shows shapes the app never draws.
        let clip = [
            (p.clip_rect.min.x * ss).max(0.0) as usize,
            (p.clip_rect.min.y * ss).max(0.0) as usize,
            (p.clip_rect.max.x * ss).min(w as f32) as usize,
            (p.clip_rect.max.y * ss).min(h as f32) as usize,
        ];
        for tri in m.indices.chunks_exact(3) {
            let v: Vec<&epaint::Vertex> =
                tri.iter().map(|i| &m.vertices[*i as usize]).collect();
            // Skip glyphs. Testing UVs against `WHITE_UV` was wrong: not
            // every solid shape writes that exact constant, and filled
            // circles did not — so points, pucks and dots were silently
            // missing from every snapshot while lines and rects appeared.
            //
            // What actually distinguishes a glyph is that it SAMPLES: its
            // three vertices carry different UVs. A solid fill shares one.
            let uv = v[0].uv;
            if v.iter().any(|q| (q.uv.x - uv.x).abs() > 1e-6 || (q.uv.y - uv.y).abs() > 1e-6)
            {
                continue;
            }
            let pts: Vec<[f32; 2]> = v.iter().map(|q| [q.pos.x * ss, q.pos.y * ss]).collect();
            let x0 = (pts.iter().map(|q| q[0]).fold(f32::MAX, f32::min).floor().max(0.0)
                as usize)
                .max(clip[0]);
            let x1 = (pts.iter().map(|q| q[0]).fold(f32::MIN, f32::max).ceil().min(w as f32)
                as usize)
                .min(clip[2]);
            let y0 = (pts.iter().map(|q| q[1]).fold(f32::MAX, f32::min).floor().max(0.0)
                as usize)
                .max(clip[1]);
            let y1 = (pts.iter().map(|q| q[1]).fold(f32::MIN, f32::max).ceil().min(h as f32)
                as usize)
                .min(clip[3]);
            let d = (pts[1][0] - pts[0][0]) * (pts[2][1] - pts[0][1])
                - (pts[2][0] - pts[0][0]) * (pts[1][1] - pts[0][1]);
            if d.abs() < 1e-9 {
                continue;
            }
            for y in y0..y1 {
                for x in x0..x1 {
                    let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                    let b1 = ((px - pts[0][0]) * (pts[2][1] - pts[0][1])
                        - (pts[2][0] - pts[0][0]) * (py - pts[0][1]))
                        / d;
                    let b2 = ((pts[1][0] - pts[0][0]) * (py - pts[0][1])
                        - (px - pts[0][0]) * (pts[1][1] - pts[0][1]))
                        / d;
                    let b0 = 1.0 - b1 - b2;
                    if b0 < 0.0 || b1 < 0.0 || b2 < 0.0 {
                        continue;
                    }
                    let ws = [b0, b1, b2];
                    let mut c = [0.0f32; 4];
                    for (k, q) in v.iter().enumerate() {
                        let a = q.color.to_array();
                        for j in 0..4 {
                            c[j] += a[j] as f32 * ws[k];
                        }
                    }
                    // egui colours are premultiplied.
                    let a = c[3] / 255.0;
                    let o = (y * w + x) * 3;
                    for j in 0..3 {
                        buf[o + j] =
                            (c[j] + buf[o + j] as f32 * (1.0 - a)).clamp(0.0, 255.0) as u8;
                    }
                }
            }
        }
    }
    buf
}

#[cfg(test)]
mod tests {
    /// The rasterizer has to handle every shape the UI actually draws.
    /// Filled circles were silently missing for a while, which quietly
    /// removed every control point and puck from every snapshot.
    #[test]
    fn solid_shapes_all_rasterize() {
        let shot = super::shoot(120.0, 40.0, |ui| {
            let (rect, _) =
                ui.allocate_exact_size(egui::vec2(110.0, 30.0), egui::Sense::hover());
            let p = ui.painter();
            p.rect_filled(
                egui::Rect::from_min_size(rect.min, egui::vec2(20.0, 20.0)),
                0.0,
                egui::Color32::RED,
            );
            p.circle_filled(rect.min + egui::vec2(45.0, 10.0), 9.0, egui::Color32::GREEN);
            p.line_segment(
                [rect.min + egui::vec2(70.0, 2.0), rect.min + egui::vec2(70.0, 18.0)],
                egui::Stroke::new(3.0, egui::Color32::BLUE),
            );
        });
        shot.save("snapshot-shapes");
        let rect = shot.at(5.0, 5.0);
        let circ = shot.at(45.0, 10.0);
        let line = shot.at(70.0, 10.0);
        assert!(rect[0] > 100, "rect missing: {rect:?}");
        assert!(circ[1] > 100, "FILLED CIRCLE MISSING: {circ:?}");
        assert!(line[2] > 100, "line missing: {line:?}");
    }
}

/// Drop a frame's output a test does not need. egui insists every texture
/// delta is applied; this says the test knows better.
pub fn discard(mut out: egui::FullOutput) {
    out.textures_delta.clear();
}
