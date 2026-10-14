//! The camera: which world point sits at the view's centre, and how many
//! screen points one world unit spans. Centre-anchored, so resizing the
//! panel keeps what you were looking at in the middle. Pure.

use egui::{vec2, Pos2, Rect, Vec2};

#[derive(Clone, Copy, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
pub struct Camera {
    /// The world point at the view's centre.
    pub centre: Pos2,
    /// Screen points per world unit.
    pub zoom: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self { centre: Pos2::ZERO, zoom: 1.0 }
    }
}

/// Zoom limits: past 256x a world unit is a quarter of a 1080p screen;
/// under 1/256 a 100k-unit board is a postage stamp.
pub const ZOOM_MIN: f32 = 1.0 / 256.0;
pub const ZOOM_MAX: f32 = 256.0;

impl Camera {
    pub fn to_screen(&self, view: Rect, world: Pos2) -> Pos2 {
        view.center() + (world - self.centre) * self.zoom
    }

    pub fn to_world(&self, view: Rect, screen: Pos2) -> Pos2 {
        self.centre + (screen - view.center()) / self.zoom
    }

    /// The world rect the view shows.
    pub fn visible(&self, view: Rect) -> Rect {
        Rect::from_center_size(self.centre, view.size() / self.zoom)
    }

    /// Move the picture by a screen-space delta (the canvas follows the hand).
    pub fn pan(&mut self, screen_delta: Vec2) {
        self.centre -= screen_delta / self.zoom;
    }

    /// Zoom by `factor`, keeping the world point under `screen` where it is.
    pub fn zoom_about(&mut self, view: Rect, screen: Pos2, factor: f32) {
        let anchor = self.to_world(view, screen);
        self.zoom = (self.zoom * factor).clamp(ZOOM_MIN, ZOOM_MAX);
        self.centre = anchor - (screen - view.center()) / self.zoom;
    }

    /// The camera that frames `bounds` in `view`, with `margin` screen
    /// points round it, zoomed in no further than `max_zoom`.
    pub fn framing(view: Rect, bounds: Rect, margin: f32, max_zoom: f32) -> Self {
        let room = (view.size() - vec2(margin, margin) * 2.0).max(vec2(1.0, 1.0));
        let size = bounds.size().max(vec2(1e-3, 1e-3));
        let zoom = (room.x / size.x).min(room.y / size.y).clamp(ZOOM_MIN, max_zoom.min(ZOOM_MAX));
        Self { centre: bounds.center(), zoom }
    }

    /// Part way from `self` to `to`: centre linearly, zoom in log space (so
    /// a big zoom change glides at an even pace instead of lurching).
    pub fn lerp(&self, to: &Camera, t: f32) -> Camera {
        let z = (self.zoom.ln() + (to.zoom.ln() - self.zoom.ln()) * t).exp();
        // Keep the centre's path in screen terms: interpolating it linearly
        // in world units drifts sideways when the zoom changes a lot, so
        // weight by the zoom we are at.
        let w = if (to.zoom - self.zoom).abs() < 1e-6 { t } else { (z - self.zoom) / (to.zoom - self.zoom) };
        Camera { centre: self.centre + (to.centre - self.centre) * w.clamp(0.0, 1.0), zoom: z }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;

    fn view() -> Rect {
        Rect::from_min_size(pos2(100.0, 50.0), vec2(800.0, 600.0))
    }

    #[test]
    fn screen_and_world_round_trip() {
        let c = Camera { centre: pos2(30.0, -20.0), zoom: 2.5 };
        let p = pos2(-7.0, 42.0);
        assert!((c.to_world(view(), c.to_screen(view(), p)) - p).length() < 1e-4);
        assert_eq!(c.to_screen(view(), c.centre), view().center());
    }

    #[test]
    fn zooming_keeps_the_point_under_the_pointer() {
        let mut c = Camera { centre: pos2(5.0, 5.0), zoom: 1.0 };
        let at = pos2(700.0, 120.0);
        let under = c.to_world(view(), at);
        c.zoom_about(view(), at, 3.7);
        assert!((c.to_world(view(), at) - under).length() < 1e-3);
        c.zoom_about(view(), at, 1e12);
        assert_eq!(c.zoom, ZOOM_MAX);
    }

    #[test]
    fn panning_follows_the_hand() {
        let mut c = Camera { centre: Pos2::ZERO, zoom: 4.0 };
        let p = pos2(10.0, 10.0);
        let before = c.to_screen(view(), p);
        c.pan(vec2(40.0, -8.0));
        assert!((c.to_screen(view(), p) - (before + vec2(40.0, -8.0))).length() < 1e-4);
    }

    #[test]
    fn framing_fits_the_bounds_inside_the_margin() {
        let b = Rect::from_min_max(pos2(-100.0, 0.0), pos2(300.0, 100.0));
        let c = Camera::framing(view(), b, 20.0, 100.0);
        let shown = Rect::from_min_max(c.to_screen(view(), b.min), c.to_screen(view(), b.max));
        assert!(view().shrink(19.9).contains_rect(shown));
        assert!((shown.width() - 760.0).abs() < 0.01, "the long side fills the room");
        assert_eq!(Camera::framing(view(), Rect::from_min_size(Pos2::ZERO, vec2(1.0, 1.0)), 0.0, 2.0).zoom, 2.0);
    }

    #[test]
    fn a_glide_ends_where_it_was_going() {
        let a = Camera { centre: Pos2::ZERO, zoom: 0.01 };
        let b = Camera { centre: pos2(1000.0, 0.0), zoom: 10.0 };
        let end = a.lerp(&b, 1.0);
        assert!((end.centre - b.centre).length() < 1e-3 && (end.zoom - b.zoom).abs() < 1e-4);
        let mid = a.lerp(&b, 0.5);
        assert!((mid.zoom - (0.01f32 * 10.0).sqrt()).abs() < 1e-4, "zoom glides in log space");
    }
}
