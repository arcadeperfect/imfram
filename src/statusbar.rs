//! The status bar: one thin strip along a window's bottom. Keyed segments
//! on the left (what is selected, what mode) and on the right (frame,
//! zoom, memory: monospace so they do not jitter), a flash message that
//! takes the left for a few seconds, and an optional progress strip along
//! the top edge. A segment marked stale draws in the weak text colour
//! until it is set again. The app owns a [`StatusBar`], sets segments from
//! anywhere, and calls [`StatusBar::show`] once a frame after its dock.
//! Colours come from the context's `Visuals`.

use egui::{Color32, CornerRadius, Id, Pos2, Rect, RichText, Ui};
use web_time::Instant;

/// The strip's height, margins included.
pub const HEIGHT: f32 = 22.0;
/// The progress strip along the top edge.
const PROGRESS_H: f32 = 2.0;
/// Horizontal padding at each end and between segments.
const PAD: f32 = 8.0;
/// Seconds over which a flash fades at its end.
const FLASH_FADE: f32 = 0.5;

#[derive(Clone, Debug)]
struct Seg {
    key: String,
    text: String,
    stale: bool,
}

/// The app's status bar.
#[derive(Default)]
pub struct StatusBar {
    left: Vec<Seg>,
    right: Vec<Seg>,
    /// A transient message and the seconds it has left.
    flash: Option<(String, f32)>,
    /// A fraction and what it is of, drawn as a thin bar.
    pub progress: Option<(f32, String)>,
    last: Option<Instant>,
}

fn set_in(v: &mut Vec<Seg>, key: &str, text: String) {
    match v.iter_mut().find(|s| s.key == key) {
        Some(s) => {
            s.text = text;
            s.stale = false;
        }
        None => v.push(Seg { key: key.to_owned(), text, stale: false }),
    }
}

impl StatusBar {
    pub fn new() -> Self {
        Self::default()
    }

    /// A keyed segment on the left: made on the first call, replaced in
    /// place after. Keys keep the order they were first set in.
    pub fn set(&mut self, key: &str, text: impl Into<String>) {
        set_in(&mut self.left, key, text.into());
    }

    /// A keyed segment on the right, in monospace.
    pub fn set_right(&mut self, key: &str, text: impl Into<String>) {
        set_in(&mut self.right, key, text.into());
    }

    /// Drop a segment from either side.
    pub fn remove(&mut self, key: &str) {
        self.left.retain(|s| s.key != key);
        self.right.retain(|s| s.key != key);
    }

    /// Mark a segment out of date: it draws weak until set again.
    pub fn stale(&mut self, key: &str) {
        for s in self.left.iter_mut().chain(self.right.iter_mut()).filter(|s| s.key == key) {
            s.stale = true;
        }
    }

    /// The text of a segment, if set.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.left.iter().chain(self.right.iter()).find(|s| s.key == key).map(|s| s.text.as_str())
    }

    /// A message that takes the left side for `secs`, then fades.
    pub fn flash(&mut self, text: impl Into<String>, secs: f32) {
        self.flash = Some((text.into(), secs.max(0.0)));
    }

    /// The flash still showing, if any.
    pub fn flashing(&self) -> Option<&str> {
        self.flash.as_ref().map(|(s, _)| s.as_str())
    }

    /// Advance the flash clock by `dt` seconds. `show` does this from the
    /// wall clock; an app driving its own time may call it.
    pub fn tick(&mut self, dt: f32) {
        if let Some((_, left)) = &mut self.flash {
            *left -= dt;
            if *left <= 0.0 {
                self.flash = None;
            }
        }
    }

    /// The strip, as a bottom panel of `ui`.
    pub fn show(&mut self, ui: &mut Ui) {
        let now = Instant::now();
        let dt = self.last.map_or(0.0, |l| (now - l).as_secs_f32().min(0.1));
        self.last = Some(now);
        self.tick(dt);
        if self.flash.is_some() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
        } else {
            self.last = None;
        }

        let vis = ui.visuals().clone();
        let frame = egui::Frame::NONE
            .fill(vis.panel_fill)
            .inner_margin(egui::Margin { left: PAD as i8, right: PAD as i8, top: 0, bottom: 0 });
        egui::Panel::bottom(Id::new("imfram-statusbar")).exact_size(HEIGHT).frame(frame).show(ui, |ui| {
            let rect = ui.max_rect();
            let outer = Rect::from_min_max(Pos2::new(rect.min.x - PAD, rect.min.y), Pos2::new(rect.max.x + PAD, rect.max.y));
            let p = ui.painter();
            p.hline(outer.x_range(), outer.min.y, vis.widgets.noninteractive.bg_stroke);
            if let Some((f, _)) = &self.progress {
                let track = Rect::from_min_size(outer.min, egui::vec2(outer.width(), PROGRESS_H));
                p.rect_filled(track, CornerRadius::ZERO, vis.faint_bg_color);
                let fill = Rect::from_min_size(outer.min, egui::vec2(outer.width() * f.clamp(0.0, 1.0), PROGRESS_H));
                p.rect_filled(fill, CornerRadius::ZERO, vis.selection.bg_fill);
            }
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = PAD;
                self.left_side(ui, &vis);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = PAD;
                    for s in &self.right {
                        let c = if s.stale { vis.weak_text_color() } else { vis.text_color() };
                        ui.label(RichText::new(&s.text).monospace().color(c));
                    }
                });
            });
        });
    }

    fn left_side(&self, ui: &mut Ui, vis: &egui::Visuals) {
        if let Some((text, left)) = &self.flash {
            let a = (left / FLASH_FADE).clamp(0.0, 1.0);
            let c = vis.strong_text_color().gamma_multiply(a);
            ui.label(RichText::new(text).color(c));
            return;
        }
        if let Some((f, what)) = &self.progress {
            let c = vis.weak_text_color();
            ui.label(RichText::new(format!("{what} {:.0}%", f.clamp(0.0, 1.0) * 100.0)).color(c));
        }
        for s in &self.left {
            let c: Color32 = if s.stale { vis.weak_text_color() } else { vis.text_color() };
            ui.label(RichText::new(&s.text).color(c));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_replaces_a_keyed_segment_in_place_and_keeps_the_order() {
        let mut b = StatusBar::new();
        b.set("mode", "edit");
        b.set("sel", "3 nodes");
        b.set("mode", "paint");
        assert_eq!(b.left.len(), 2);
        assert_eq!(b.left[0].text, "paint");
        assert_eq!(b.get("sel"), Some("3 nodes"));
        b.set_right("frame", "1001");
        assert_eq!(b.get("frame"), Some("1001"));
        b.remove("sel");
        assert_eq!(b.get("sel"), None);
    }

    #[test]
    fn a_flash_runs_its_seconds_then_goes() {
        let mut b = StatusBar::new();
        b.flash("saved", 2.0);
        b.tick(1.5);
        assert_eq!(b.flashing(), Some("saved"));
        b.tick(0.6);
        assert_eq!(b.flashing(), None);
    }

    #[test]
    fn stale_marks_a_segment_weak_until_it_is_set_again() {
        let mut b = StatusBar::new();
        b.set_right("mem", "1.2 GB");
        b.stale("mem");
        assert!(b.right[0].stale);
        b.set_right("mem", "1.3 GB");
        assert!(!b.right[0].stale);
    }

    #[test]
    fn the_strip_sits_at_the_bottom_with_a_progress_bar_across_its_top() {
        let (w, h) = (400.0, 200.0);
        let mut b = StatusBar::new();
        b.set("mode", "edit");
        b.set_right("frame", "1001");
        b.progress = Some((0.5, "baking".to_string()));
        let shot = crate::snapshot::shoot(w, h, |ui| b.show(ui));
        let bg = shot.at(w / 2.0, h / 2.0);
        let top = h - HEIGHT;
        let strip = shot.at(w / 2.0, h - HEIGHT / 2.0);
        assert_ne!(strip, bg, "the panel is filled: {strip:?}");
        assert_eq!(shot.at(w / 2.0, top - 4.0), bg, "and nothing above it");
        let done = shot.at(w * 0.25, top + PROGRESS_H / 2.0);
        let todo = shot.at(w * 0.75, top + PROGRESS_H / 2.0);
        assert_ne!(done, todo, "the bar stops halfway: {done:?} vs {todo:?}");
        let accent = egui::Visuals::default().selection.bg_fill;
        assert!(done[2] > done[0], "it is the accent colour {accent:?}: {done:?}");
    }
}
