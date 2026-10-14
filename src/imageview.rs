//! An image viewer: a texture at a zoom and pan over a checkerboard, with
//! Blender hands (`nav`), F to fit, 1 for 1:1, a pixel readout under the
//! pointer and a strip with the zoom and exposure / gamma sliders.
//!
//! Filtering is the texture's, not the draw's: egui samples with the
//! `TextureOptions` the texture was loaded with, so a caller who wants crisp
//! texels past 1:1 loads with `TextureOptions::NEAREST`; this view snaps the
//! image origin to whole physical pixels at zoom >= 1 so texels sit on the
//! pixel grid. There is no shader hook, so exposure is approximated by
//! tinting the quad with 2^exposure clamped to 1 (it darkens; it cannot
//! brighten past the texture), and gamma is only stored for the caller's own
//! display path. Colours come from `Visuals`.

use crate::nav::{self, DragNav};
use egui::{pos2, vec2, Align, Color32, FontId, Key, Layout, PointerButton, Rect, Response, Sense, TextureId, Ui, UiBuilder, Vec2};

const CHECKER_CELL: f32 = 8.0;
const MIN_ZOOM: f32 = 1.0 / 64.0;
const MAX_ZOOM: f32 = 256.0;
/// Image edge to view edge when fitted, as a fraction of the view.
const FIT_MARGIN: f32 = 0.96;
const PAD: f32 = 6.0;
const STRIP_H: f32 = 22.0;
const SLIDER_W: f32 = 80.0;
const READOUT_FONT: f32 = 11.0;
const EXPOSURE_RANGE: std::ops::RangeInclusive<f32> = -6.0..=6.0;
const GAMMA_RANGE: std::ops::RangeInclusive<f32> = 0.2..=4.0;

pub struct ImageView {
    pub zoom: f32,
    /// Image centre from the view centre, in points.
    pub pan: Vec2,
    pub fit_pending: bool,
    /// Stops; applied as a tint (see the module doc).
    pub exposure: f32,
    /// Stored only; the caller's display path applies it.
    pub gamma: f32,
}

impl Default for ImageView {
    fn default() -> Self {
        Self { zoom: 1.0, pan: Vec2::ZERO, fit_pending: true, exposure: 0.0, gamma: 1.0 }
    }
}

impl ImageView {
    /// `size` is the texture's pixel size; `sample` reads a pixel for the
    /// readout. Takes all the space it is given.
    pub fn show(&mut self, ui: &mut Ui, tex: TextureId, size: [usize; 2], sample: Option<&dyn Fn([usize; 2]) -> [f32; 4]>) -> Response {
        let (rect, r) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let img = vec2(size[0] as f32, size[1] as f32);
        if self.fit_pending {
            self.zoom = fit_zoom(rect.size(), img);
            self.pan = Vec2::ZERO;
            self.fit_pending = false;
        }
        self.navigate(ui, &r, rect);
        if r.hovered() && ui.memory(|m| m.focused().is_none()) {
            if ui.input(|i| i.key_pressed(Key::F)) {
                self.fit_pending = true;
            }
            if ui.input(|i| i.key_pressed(Key::Num1)) {
                (self.pan, self.zoom) = zoom_about(self.pan, self.zoom, Vec2::ZERO, 1.0 / self.zoom);
            }
        }
        let img_rect = image_rect(rect, img, self.zoom, self.pan, ui.pixels_per_point());
        let p = ui.painter_at(rect);
        crate::colorpicker::paint_checker(&p, rect, CHECKER_CELL);
        p.image(tex, img_rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), exposure_tint(self.exposure));
        if let Some(pos) = r.hover_pos().filter(|p| img_rect.contains(*p)) {
            let px = pixel_at(img_rect, self.zoom, pos, size);
            let mut text = format!("{:>5} {:>5}", px[0], px[1]);
            if let Some(s) = sample {
                let c = s(px);
                text += &format!("   {:.4} {:.4} {:.4} {:.4}", c[0], c[1], c[2], c[3]);
            }
            let vis = ui.visuals();
            let galley = p.layout_no_wrap(text, FontId::monospace(READOUT_FONT), vis.text_color());
            let bg = Rect::from_min_size(rect.left_bottom() - vec2(-PAD, PAD + galley.size().y + PAD), galley.size() + vec2(PAD * 2.0, PAD));
            p.rect_filled(bg, 3.0, vis.panel_fill.gamma_multiply(0.85));
            p.galley(bg.min + vec2(PAD, PAD * 0.5), galley, vis.text_color());
        }
        self.overlay(ui, rect);
        r
    }

    fn navigate(&mut self, ui: &Ui, r: &Response, rect: Rect) {
        let mods = ui.input(|i| i.modifiers);
        let button = [PointerButton::Middle, PointerButton::Primary, PointerButton::Secondary]
            .into_iter()
            .find(|b| r.dragged_by(*b));
        if let Some(b) = button {
            match nav::drag_nav(b, mods) {
                Some(DragNav::Pan) => self.pan += r.drag_delta(),
                Some(DragNav::Zoom) => {
                    let at = ui.input(|i| i.pointer.press_origin()).unwrap_or(rect.center());
                    (self.pan, self.zoom) = zoom_about(self.pan, self.zoom, at - rect.center(), nav::drag_zoom(r.drag_delta()));
                }
                None => {}
            }
        }
        if r.hovered() {
            let w = nav::wheel_nav(ui);
            self.pan += w.pan;
            if w.zoom != 1.0 {
                let at = r.hover_pos().unwrap_or(rect.center());
                (self.pan, self.zoom) = zoom_about(self.pan, self.zoom, at - rect.center(), w.zoom);
            }
        }
    }

    /// Zoom %, exposure and gamma in a strip at the top left.
    fn overlay(&mut self, ui: &mut Ui, rect: Rect) {
        let bar = Rect::from_min_size(rect.min + vec2(PAD, PAD), vec2((SLIDER_W + 64.0) * 2.0 + 60.0, STRIP_H));
        let fill = ui.visuals().panel_fill.gamma_multiply(0.85);
        let mut child = ui.new_child(UiBuilder::new().max_rect(bar).layout(Layout::left_to_right(Align::Center)));
        child.painter().rect_filled(bar, 3.0, fill);
        child.spacing_mut().slider_width = SLIDER_W;
        child.add_space(PAD);
        child.monospace(format!("{:>5.0}%", self.zoom * 100.0));
        child.add(egui::Slider::new(&mut self.exposure, EXPOSURE_RANGE).text("exp").fixed_decimals(2));
        child.add(egui::Slider::new(&mut self.gamma, GAMMA_RANGE).text("γ").fixed_decimals(2));
    }
}

/// The zoom that fits an `img`-sized picture into `view`, with a margin.
pub fn fit_zoom(view: Vec2, img: Vec2) -> f32 {
    let z = (view.x / img.x.max(1.0)).min(view.y / img.y.max(1.0)) * FIT_MARGIN;
    z.clamp(MIN_ZOOM, MAX_ZOOM)
}

/// Zoom by `factor` keeping the image point `at` (from the view centre)
/// under the pointer. Returns the new `(pan, zoom)`.
pub fn zoom_about(pan: Vec2, zoom: f32, at: Vec2, factor: f32) -> (Vec2, f32) {
    let new = (zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
    let f = new / zoom;
    (at + (pan - at) * f, new)
}

/// Where the image lands in `view`; at 1:1 and up the origin snaps to a
/// physical pixel so texels line up with the screen.
pub fn image_rect(view: Rect, img: Vec2, zoom: f32, pan: Vec2, ppp: f32) -> Rect {
    let mut min = view.center() + pan - img * zoom * 0.5;
    if zoom >= 1.0 {
        min = (min * ppp).round() / ppp;
    }
    Rect::from_min_size(min, img * zoom)
}

/// The texel under `pos`, clamped into the image.
pub fn pixel_at(img_rect: Rect, zoom: f32, pos: egui::Pos2, size: [usize; 2]) -> [usize; 2] {
    let p = (pos - img_rect.min) / zoom;
    [(p.x.floor().max(0.0) as usize).min(size[0].saturating_sub(1)), (p.y.floor().max(0.0) as usize).min(size[1].saturating_sub(1))]
}

/// 2^stops as a quad tint, clamped at white: a stand-in for exposure.
pub fn exposure_tint(stops: f32) -> Color32 {
    let v = (stops.exp2().clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgb(v, v, v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_puts_the_long_side_just_inside_the_view() {
        let z = fit_zoom(vec2(800.0, 400.0), vec2(256.0, 256.0));
        assert!((z - 400.0 / 256.0 * FIT_MARGIN).abs() < 1e-5);
        assert!(fit_zoom(vec2(10.0, 10.0), vec2(4096.0, 4096.0)) >= MIN_ZOOM);
    }

    #[test]
    fn zooming_about_a_point_keeps_that_image_point_under_it() {
        let (pan, zoom) = (vec2(30.0, -10.0), 2.0);
        let at = vec2(100.0, 50.0);
        // The image point under `at`, in texels from the image centre.
        let texel = (at - pan) / zoom;
        let (pan2, zoom2) = zoom_about(pan, zoom, at, 1.5);
        assert!((zoom2 - 3.0).abs() < 1e-6);
        let now = pan2 + texel * zoom2;
        assert!((now - at).length() < 1e-3, "point drifted to {now:?}");
        assert_eq!(zoom_about(pan, zoom, at, 1e9).1, MAX_ZOOM, "zoom is clamped");
    }

    #[test]
    fn the_readout_names_the_texel_under_the_pointer() {
        let view = Rect::from_min_size(egui::Pos2::ZERO, vec2(400.0, 300.0));
        let r = image_rect(view, vec2(16.0, 16.0), 4.0, Vec2::ZERO, 1.0);
        assert_eq!(r.size(), vec2(64.0, 64.0));
        assert_eq!(pixel_at(r, 4.0, r.min + vec2(9.0, 21.0), [16, 16]), [2, 5]);
        assert_eq!(pixel_at(r, 4.0, r.max + vec2(50.0, 50.0), [16, 16]), [15, 15], "clamped into the image");
        let snapped = image_rect(view, vec2(16.0, 16.0), 1.0, vec2(0.3, 0.3), 2.0);
        assert_eq!(snapped.min * 2.0, (snapped.min * 2.0).round(), "snapped to physical pixels");
    }

    #[test]
    fn exposure_tints_down_and_clamps_at_white() {
        assert_eq!(exposure_tint(0.0), Color32::WHITE);
        assert_eq!(exposure_tint(3.0), Color32::WHITE);
        assert_eq!(exposure_tint(-1.0), Color32::from_gray(128));
    }

    fn mesh_bounds(shapes: &[egui::epaint::ClippedShape], tex: TextureId) -> Option<Rect> {
        fn walk(s: &egui::Shape, tex: TextureId, out: &mut Option<Rect>) {
            match s {
                egui::Shape::Mesh(m) if m.texture_id == tex => *out = Some(m.calc_bounds()),
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, tex, out)),
                _ => {}
            }
        }
        let mut out = None;
        shapes.iter().for_each(|c| walk(&c.shape, tex, &mut out));
        out
    }

    #[test]
    fn the_image_quad_is_the_texture_times_the_zoom() {
        let ctx = egui::Context::default();
        let tex = TextureId::User(7);
        let mut v = ImageView { zoom: 2.0, fit_pending: false, ..Default::default() };
        let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(400.0, 300.0))), ..Default::default() };
        let mut out = ctx.run_ui(input, |ui| {
            v.show(ui, tex, [32, 16], None);
        });
        let b = mesh_bounds(&out.shapes, tex).expect("the image quad was drawn");
        assert_eq!(b.size(), vec2(64.0, 32.0));
        out.textures_delta.clear();
    }
}
