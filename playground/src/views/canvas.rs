//! Playground tab for `imfram::canvas`: a board with every kind of content
//! on it — vector shapes (concave, curved, an open stroke), a generated
//! picture, a generated clip playing on the board's clock, and notes. Drop
//! a PNG on it and it lands where you let go.

use std::sync::Arc;

use egui::{pos2, vec2, Color32, Pos2, Stroke, TextureHandle, TextureId, TextureOptions, Ui};
use imfram::canvas::{Board, Canvas, Clip, FrameSource, ItemId, Note, Path, Picture, Shape, Xform};
use imfram::widgets::keystrip::{self, Cap, Reveal};

#[derive(Default)]
pub struct State {
    board: Board,
    canvas: Canvas<ItemId>,
    /// Kept alive: the board holds only ids.
    textures: Vec<TextureHandle>,
    ready: bool,
    status: String,
    edits: usize,
}

/// A clip whose frames are textures already uploaded.
struct Frames {
    ids: Vec<TextureId>,
    size: [usize; 2],
    fps: f64,
}

impl FrameSource for Frames {
    fn size(&self) -> [usize; 2] {
        self.size
    }
    fn duration(&self) -> f64 {
        self.ids.len() as f64 / self.fps
    }
    fn frame(&self, t: f64) -> Option<TextureId> {
        self.ids.get(((t * self.fps) as usize).min(self.ids.len().saturating_sub(1))).copied()
    }
}

fn upload(ui: &Ui, name: &str, size: [usize; 2], rgba: &[u8]) -> TextureHandle {
    ui.ctx().load_texture(name, egui::ColorImage::from_rgba_unmultiplied(size, rgba), TextureOptions::LINEAR)
}

/// A sunset: a sky gradient, a sun, a dark sea with a glint.
fn picture(w: usize, h: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            let (u, v) = (x as f32 / w as f32, y as f32 / h as f32);
            let sea = v > 0.62;
            let sun = ((u - 0.62).powi(2) + ((v - 0.55) * 1.4).powi(2)).sqrt();
            let mut c = if sea {
                let glint = (1.0 - ((u - 0.62).abs() * 9.0)).max(0.0) * (((y as f32 * 0.9).sin() * 0.5 + 0.5) * 0.6);
                [0.05 + glint, 0.08 + glint * 0.8, 0.18 + glint * 0.4]
            } else {
                [0.95 - v * 0.6, 0.45 - v * 0.3, 0.35 + v * 0.4]
            };
            if !sea && sun < 0.12 {
                c = [1.0, 0.85, 0.55];
            }
            out.extend(c.map(|f| (f.clamp(0.0, 1.0) * 255.0) as u8));
            out.push(255);
        }
    }
    out
}

/// One frame of the clip: a ball bouncing across bars.
fn clip_frame(w: usize, h: usize, i: usize, n: usize) -> Vec<u8> {
    let t = i as f32 / n as f32;
    let bx = t * w as f32;
    let by = h as f32 * (0.25 + 0.6 * (t * std::f32::consts::TAU * 2.0).sin().abs());
    let mut out = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            let bar = ((x as f32 / w as f32 * 7.0 + t * 7.0) as usize) % 7;
            let bars = [[200, 200, 200], [200, 200, 30], [30, 200, 200], [30, 200, 30], [200, 30, 200], [200, 30, 30], [30, 30, 200]];
            let mut c = bars[bar].map(|v: u8| v / 2);
            let d = ((x as f32 - bx).powi(2) + (y as f32 - by).powi(2)).sqrt();
            if d < h as f32 * 0.12 {
                c = [250, 250, 250];
            }
            out.extend(c);
            out.push(255);
        }
    }
    out
}

fn star(points: usize, outer: f32, inner: f32) -> Path {
    let pts: Vec<Pos2> = (0..points * 2)
        .map(|i| {
            let a = i as f32 / (points * 2) as f32 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
            let r = if i % 2 == 0 { outer } else { inner };
            pos2(a.cos() * r, a.sin() * r)
        })
        .collect();
    Path::polygon(&pts)
}

fn heart() -> Path {
    Path::new(pos2(0.0, 60.0))
        .cubic_to(pos2(-90.0, 0.0), pos2(-60.0, -70.0), pos2(0.0, -30.0))
        .cubic_to(pos2(60.0, -70.0), pos2(90.0, 0.0), pos2(0.0, 60.0))
        .close()
}

fn squiggle() -> Path {
    Path::new(pos2(-150.0, 0.0))
        .cubic_to(pos2(-100.0, -80.0), pos2(-50.0, 80.0), pos2(0.0, 0.0))
        .cubic_to(pos2(50.0, -80.0), pos2(100.0, 80.0), pos2(150.0, 0.0))
}

impl State {
    fn build(&mut self, ui: &Ui) {
        let b = &mut self.board;
        b.add(Xform::at(pos2(-420.0, -230.0)), Note::new(
            "Hands\n\nMMB / alt+LMB pan, wheel zoom, ctrl+MMB drag-zoom.\nHome: everything. F: the selection.",
            vec2(260.0, 150.0), Color32::from_rgb(255, 236, 140)));
        b.add(Xform { pos: pos2(-140.0, -250.0), rot: 0.05, ..Default::default() }, Note::new(
            "G / R / S\n\nGrab, rotate, scale. X / Y lock an axis, ctrl snaps, shift is fine. Click keeps, Esc undoes.",
            vec2(250.0, 160.0), Color32::from_rgb(180, 230, 255)));
        b.add(Xform { pos: pos2(130.0, -240.0), rot: -0.04, ..Default::default() }, Note::new(
            "Double-click a note to write in it.\nshift+D duplicates, X deletes, ] and [ restack.\nDrop a PNG anywhere.",
            vec2(250.0, 150.0), Color32::from_rgb(200, 255, 190)));

        let (pw, ph) = (480, 300);
        let pic = upload(ui, "canvas-picture", [pw, ph], &picture(pw, ph));
        b.add(Xform::at(pos2(-260.0, 60.0)), Picture::new(pic.id(), [pw, ph]));
        self.textures.push(pic);

        let (cw, ch, n) = (320, 180, 48);
        let frames: Vec<TextureHandle> = (0..n).map(|i| upload(ui, &format!("canvas-clip-{i}"), [cw, ch], &clip_frame(cw, ch, i, n))).collect();
        let source = Frames { ids: frames.iter().map(|t| t.id()).collect(), size: [cw, ch], fps: 24.0 };
        self.textures.extend(frames);
        b.add(Xform { pos: pos2(250.0, 40.0), rot: 0.06, ..Default::default() }, Clip::new(Arc::new(source)));
        b.playing = true;

        b.add(Xform::at(pos2(-380.0, 330.0)), Shape { path: star(5, 80.0, 34.0), fill: Color32::from_rgb(250, 200, 60), stroke: Stroke::new(3.0, Color32::from_rgb(120, 70, 10)) });
        b.add(Xform::at(pos2(-150.0, 330.0)), Shape { path: heart(), fill: Color32::from_rgb(230, 60, 90), stroke: Stroke::NONE });
        b.add(Xform::at(pos2(160.0, 330.0)), Shape { path: squiggle(), fill: Color32::TRANSPARENT, stroke: Stroke::new(6.0, Color32::from_rgb(90, 200, 230)) });
        self.ready = true;
    }

    /// A dropped PNG, decoded and placed. Anything else is reported.
    fn drop_file(&mut self, ui: &Ui, file: &egui::DroppedFileHandle, at: Pos2) {
        let name = file.path().file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        match file.bytes().and_then(|b| decode_png(&b)) {
            Ok((size, rgba)) => {
                let tex = upload(ui, &name, size, &rgba);
                let id = self.board.add(Xform::at(at), Picture::new(tex.id(), size));
                self.textures.push(tex);
                self.canvas.selection = vec![id];
                self.status = format!("dropped {name} ({}x{})", size[0], size[1]);
            }
            Err(e) => self.status = format!("{name}: {e}"),
        }
    }
}

fn decode_png(bytes: &[u8]) -> Result<([usize; 2], Vec<u8>), String> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = dec.read_info().map_err(|e| format!("not a PNG ({e})"))?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or("too big")?];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let px = (info.width * info.height) as usize;
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf[..px * 4].to_vec(),
        png::ColorType::Rgb => buf[..px * 3].chunks(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf[..px * 2].chunks(2).flat_map(|c| [c[0], c[0], c[0], c[1]]).collect(),
        png::ColorType::Grayscale => buf[..px].iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return Err("indexed PNG not expanded".into()),
    };
    Ok(([info.width as usize, info.height as usize], rgba))
}

pub fn ui(ui: &mut Ui, s: &mut State, strip: bool, reveal: &mut Reveal) {
    if !s.ready {
        s.build(ui);
    }
    s.board.tick(ui.input(|i| i.stable_dt) as f64);
    ui.horizontal(|ui| {
        if ui.button(if s.board.playing { "⏸ pause" } else { "⏵ play" }).clicked() {
            s.board.playing = !s.board.playing;
        }
        if ui.button("+ note").clicked() {
            let id = s.board.add(Xform::at(s.canvas.cam.centre), Note::new("new note", vec2(200.0, 120.0), Color32::from_rgb(255, 210, 230)));
            s.canvas.selection = vec![id];
        }
        ui.checkbox(&mut s.canvas.show_grid, "grid");
        ui.weak(format!("{} items · {:.0}% · {} edits", s.board.items.len(), s.canvas.cam.zoom * 100.0, s.edits));
        if let Some(&id) = s.canvas.selection.last() {
            if let Some(item) = s.board.get(id) {
                ui.weak(format!("· {} at {:.0}, {:.0}", item.content.kind(), item.xf.pos.x, item.xf.pos.y));
            }
        }
        if !s.status.is_empty() {
            ui.weak(format!("· {}", s.status));
        }
    });
    let out = s.canvas.show(ui, &mut s.board);
    if out.committed {
        s.edits += 1;
    }
    for (file, at) in &out.drops {
        s.drop_file(ui, file, *at);
    }
    if strip && !s.canvas.busy() {
        let mods = ui.input(|i| i.modifiers);
        let rect = out.response.rect;
        let caps = [
            Cap { cap: "G R S", does: "grab rotate scale", held: false },
            Cap { cap: "MMB", does: "pan", held: false },
            Cap { cap: "ctrl MMB", does: "zoom", held: mods.ctrl || mods.command },
            Cap { cap: "Home", does: "frame all", held: false },
            Cap { cap: "F", does: "frame selected", held: false },
            Cap { cap: "shift D", does: "duplicate", held: mods.shift },
        ];
        keystrip::show(ui, rect, rect, &caps, reveal, false);
    }
}
