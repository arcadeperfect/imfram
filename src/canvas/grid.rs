//! The background grid, Blender's 2D-editor way: decades of lines, the
//! finer decade fading in as you zoom, so there are always lines to read
//! distance by and never a wall of them.

use egui::{Color32, Painter, Rect, Stroke};

use super::camera::Camera;

/// The finest lines are at least this far apart on screen.
const MIN_PX: f32 = 10.0;
/// ... and are fully in by this spacing.
const FULL_PX: f32 = 60.0;

/// The world spacing of the finest visible decade, and how far it has
/// faded in (0..1). The next decade up is `step * 10`, always fully in.
pub fn step(zoom: f32) -> (f32, f32) {
    let step = 10f32.powf((MIN_PX / zoom).log10().ceil());
    let px = step * zoom;
    (step, ((px - MIN_PX) / (FULL_PX - MIN_PX)).clamp(0.0, 1.0))
}

pub fn paint(p: &Painter, view: Rect, cam: &Camera, line: Color32, axis: Color32) {
    let (fine, fade) = step(cam.zoom);
    let world = cam.visible(view);
    let lines = |step: f32, color: Color32| {
        if color.a() == 0 {
            return;
        }
        let stroke = Stroke::new(1.0, color);
        let mut x = (world.min.x / step).floor() * step;
        while x <= world.max.x {
            let sx = cam.to_screen(view, egui::pos2(x, 0.0)).x;
            p.vline(sx.round() + 0.5, view.y_range(), stroke);
            x += step;
        }
        let mut y = (world.min.y / step).floor() * step;
        while y <= world.max.y {
            let sy = cam.to_screen(view, egui::pos2(0.0, y)).y;
            p.hline(view.x_range(), sy.round() + 0.5, stroke);
            y += step;
        }
    };
    lines(fine, line.gamma_multiply(fade * 0.5));
    lines(fine * 10.0, line);
    let o = cam.to_screen(view, egui::Pos2::ZERO);
    if view.x_range().contains(o.x) {
        p.vline(o.x.round() + 0.5, view.y_range(), Stroke::new(1.0, axis));
    }
    if view.y_range().contains(o.y) {
        p.hline(view.x_range(), o.y.round() + 0.5, Stroke::new(1.0, axis));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_finest_decade_is_never_too_dense() {
        for z in [0.003, 0.05, 0.9, 1.0, 1.1, 7.0, 200.0] {
            let (s, f) = step(z);
            assert!(s * z >= MIN_PX - 1e-3, "zoom {z}: lines {} px apart", s * z);
            assert!(s * z < MIN_PX * 10.0 + 1e-3, "zoom {z}: a finer decade would fit");
            assert!((0.0..=1.0).contains(&f));
        }
        assert_eq!(step(1.0).0, 10.0);
    }
}
