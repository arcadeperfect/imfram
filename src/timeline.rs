//! A timeline strip: a frame ruler with the playhead, key marks and an
//! in/out range, under a transport row. Owns time navigation (zoom and pan
//! along time through `nav`), scrubbing, key dragging and playback stepping.
//! The keys stay the caller's: a drag reports `KeyMoved` and the caller
//! moves its data, so a drag of shift-all is a batch of those. Colours come
//! from `Visuals`.
//!
//! Hands: LMB on the strip scrubs (a key under the press drags that key,
//! shift drags all); MMB / alt+LMB pan time; ctrl+MMB and the wheel zoom
//! about the pointer; Home fits the range; Space plays; arrows step;
//! I / O set the in / out point at the playhead.

use crate::nav::{self, DragNav};
use egui::{pos2, vec2, Align2, FontId, Key, PointerButton, Rect, Sense, Shape, Stroke, Ui};

const RULER_H: f32 = 20.0;
const TRACK_H: f32 = 16.0;
/// Half the width of a key diamond, and how far a press may miss one.
const KEY_R: f32 = 5.0;
const KEY_HIT: f32 = 7.0;
/// Fewest pixels between two ruler labels.
const LABEL_PX: f32 = 64.0;
const MIN_FPP: f64 = 0.005;
const MAX_FPP: f64 = 400.0;
/// Blank time either side of a fitted range, as a fraction of its length.
const FIT_PAD: f64 = 0.02;
const FIELD_W: f32 = 56.0;
const FIELD_H: f32 = 18.0;
const TAG_PAD: f32 = 3.0;
const LABEL_FONT: f32 = 10.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Event {
    /// The playhead moved: a scrub, a step, a typed frame or playback.
    Scrub(i64),
    /// Move the key at `from` to `to` (the caller owns the keys).
    KeyMoved { from: i64, to: i64 },
    Play,
    Pause,
    /// `range` or `in_out` changed.
    RangeChanged,
}

pub struct Timeline {
    /// `first..=last` frame.
    pub range: (i64, i64),
    pub playhead: i64,
    pub in_out: Option<(i64, i64)>,
    /// `(frame at the strip's left edge, frames per pixel)`; a non-positive
    /// scale means "fit on the next show".
    pub view: (f64, f64),
    pub playing: bool,
    pub fps: f32,
    /// Fractional frames owed by playback.
    acc: f32,
    grab: Option<Grab>,
    strip: Rect,
}

/// A key being dragged: where it is now (the caller has moved it there).
struct Grab {
    at: i64,
    all: bool,
    /// Frames between the press and the key, so it does not jump.
    offset: f64,
}

impl Timeline {
    pub fn new(first: i64, last: i64) -> Self {
        Self {
            range: (first, last),
            playhead: first,
            in_out: None,
            view: (first as f64, 0.0),
            playing: false,
            fps: 24.0,
            acc: 0.0,
            grab: None,
            strip: Rect::NOTHING,
        }
    }

    /// The ruler + key strip drawn last show (tests, overlays).
    pub fn strip_rect(&self) -> Rect {
        self.strip
    }

    pub fn show(&mut self, ui: &mut Ui, keys: &[i64]) -> Vec<Event> {
        let mut ev = Vec::new();
        let bar = self.transport(ui, &mut ev);
        let (rect, r) = ui.allocate_exact_size(vec2(ui.available_width(), RULER_H + TRACK_H), Sense::click_and_drag());
        self.strip = rect;
        if self.view.1 <= 0.0 {
            self.view = fit_view(self.range, rect.width());
        }
        self.navigate(ui, &r, rect);
        self.gestures(ui, &r, rect, keys, &mut ev);
        if ui.rect_contains_pointer(rect.union(bar)) && ui.memory(|m| m.focused().is_none()) {
            self.hotkeys(ui, rect, &mut ev);
        }
        if self.playing {
            let dt = ui.input(|i| i.stable_dt);
            if let Some(f) = self.tick(dt) {
                ev.push(Event::Scrub(f));
            }
            ui.ctx().request_repaint();
        }
        self.paint(ui, rect, keys);
        ev
    }

    /// Advance playback by `dt` seconds; the new frame if it moved. Loops
    /// over the in/out range, else the whole range.
    pub fn tick(&mut self, dt: f32) -> Option<i64> {
        if !self.playing {
            return None;
        }
        self.acc += dt * self.fps;
        let n = self.acc.floor();
        if n < 1.0 {
            return None;
        }
        self.acc -= n;
        let (a, b) = self.loop_range();
        let len = (b - a + 1).max(1);
        let mut f = self.playhead + n as i64;
        if f > b || f < a {
            f = a + (f - a).rem_euclid(len);
        }
        (f != self.playhead).then(|| {
            self.playhead = f;
            f
        })
    }

    fn loop_range(&self) -> (i64, i64) {
        let (a, b) = self.in_out.unwrap_or(self.range);
        (a.min(b), a.max(b))
    }

    fn scrub_to(&mut self, f: i64, ev: &mut Vec<Event>) {
        let f = f.clamp(self.range.0.min(self.range.1), self.range.1.max(self.range.0));
        if f != self.playhead {
            self.playhead = f;
            ev.push(Event::Scrub(f));
        }
    }

    fn set_playing(&mut self, on: bool, ev: &mut Vec<Event>) {
        if on != self.playing {
            self.playing = on;
            self.acc = 0.0;
            ev.push(if on { Event::Play } else { Event::Pause });
        }
    }

    fn transport(&mut self, ui: &mut Ui, ev: &mut Vec<Event>) -> Rect {
        ui.horizontal(|ui| {
            let (a, b) = self.range;
            if ui.small_button("⏮").clicked() {
                self.scrub_to(a, ev);
            }
            if ui.small_button("⏴").clicked() {
                self.scrub_to(self.playhead - 1, ev);
            }
            if ui.small_button(if self.playing { "⏸" } else { "⏵" }).clicked() {
                self.set_playing(!self.playing, ev);
            }
            if ui.small_button("⏵").clicked() {
                self.scrub_to(self.playhead + 1, ev);
            }
            if ui.small_button("⏭").clicked() {
                self.scrub_to(b, ev);
            }
            let mut f = self.playhead;
            if crate::numfield::fixed(ui, FIELD_W, FIELD_H, egui::DragValue::new(&mut f).speed(0.5)).changed() {
                self.scrub_to(f, ev);
            }
            ui.weak("range");
            let (mut a, mut b) = self.range;
            let ra = crate::numfield::fixed(ui, FIELD_W, FIELD_H, egui::DragValue::new(&mut a).speed(0.5));
            let rb = crate::numfield::fixed(ui, FIELD_W, FIELD_H, egui::DragValue::new(&mut b).speed(0.5));
            if ra.changed() || rb.changed() {
                self.range = (a.min(b), b.max(a));
                ev.push(Event::RangeChanged);
            }
        })
        .response
        .rect
    }

    fn navigate(&mut self, ui: &Ui, r: &egui::Response, rect: Rect) {
        let mods = ui.input(|i| i.modifiers);
        let button = [PointerButton::Middle, PointerButton::Primary, PointerButton::Secondary]
            .into_iter()
            .find(|b| r.dragged_by(*b));
        if let Some(b) = button {
            match nav::drag_nav(b, mods) {
                Some(DragNav::Pan) => self.view.0 -= r.drag_delta().x as f64 * self.view.1,
                Some(DragNav::Zoom) => {
                    let x = ui.input(|i| i.pointer.press_origin()).map_or(rect.center().x, |p| p.x);
                    self.view = zoom_view(self.view, x - rect.min.x, nav::drag_zoom(r.drag_delta()));
                }
                None => {}
            }
        }
        if r.hovered() {
            let w = nav::wheel_nav(ui);
            if w.zoom != 1.0 {
                let x = r.hover_pos().map_or(rect.center().x, |p| p.x);
                self.view = zoom_view(self.view, x - rect.min.x, w.zoom);
            }
            self.view.0 -= w.pan.x as f64 * self.view.1;
        }
    }

    /// Plain LMB: a key under the press drags it, anything else scrubs.
    fn gestures(&mut self, ui: &Ui, r: &egui::Response, rect: Rect, keys: &[i64], ev: &mut Vec<Event>) {
        let mods = ui.input(|i| i.modifiers);
        if nav::drag_nav(PointerButton::Primary, mods).is_some() {
            return;
        }
        let view = self.view;
        let frame_at = move |x: f32| px_to_frame(view, rect.min.x, x);
        if r.drag_started_by(PointerButton::Primary) {
            if let Some(p) = ui.input(|i| i.pointer.press_origin()) {
                let in_track = p.y >= rect.min.y + RULER_H;
                let hit = keys
                    .iter()
                    .copied()
                    .map(|k| (k, (frame_to_px(view, rect.min.x, k as f64) - p.x).abs()))
                    .filter(|(_, d)| in_track && *d <= KEY_HIT)
                    .min_by(|a, b| a.1.total_cmp(&b.1));
                self.grab = hit.map(|(k, _)| Grab { at: k, all: mods.shift, offset: frame_at(p.x) - k as f64 });
            }
        }
        if r.dragged_by(PointerButton::Primary) {
            if let Some(p) = r.interact_pointer_pos() {
                if let Some(g) = &mut self.grab {
                    let to = (frame_at(p.x) - g.offset).round() as i64;
                    if to != g.at {
                        let d = to - g.at;
                        if g.all {
                            ev.extend(keys.iter().map(|&k| Event::KeyMoved { from: k, to: k + d }));
                        } else {
                            ev.push(Event::KeyMoved { from: g.at, to });
                        }
                        g.at = to;
                    }
                } else {
                    self.scrub_to(frame_at(p.x).round() as i64, ev);
                }
            }
        }
        if r.drag_stopped() {
            self.grab = None;
        }
        if r.clicked_by(PointerButton::Primary) {
            if let Some(p) = r.interact_pointer_pos() {
                self.scrub_to(frame_at(p.x).round() as i64, ev);
            }
        }
    }

    fn hotkeys(&mut self, ui: &Ui, rect: Rect, ev: &mut Vec<Event>) {
        let pressed = |k: Key| ui.input(|i| i.key_pressed(k));
        if pressed(Key::Home) {
            self.view = fit_view(self.range, rect.width());
        }
        if pressed(Key::Space) {
            self.set_playing(!self.playing, ev);
        }
        if pressed(Key::ArrowLeft) {
            self.scrub_to(self.playhead - 1, ev);
        }
        if pressed(Key::ArrowRight) {
            self.scrub_to(self.playhead + 1, ev);
        }
        if pressed(Key::I) || pressed(Key::O) {
            let (a, b) = self.in_out.unwrap_or(self.range);
            self.in_out = Some(if pressed(Key::I) { (self.playhead, b.max(self.playhead)) } else { (a.min(self.playhead), self.playhead) });
            ev.push(Event::RangeChanged);
        }
    }

    fn paint(&self, ui: &Ui, rect: Rect, keys: &[i64]) {
        let vis = ui.visuals();
        let p = ui.painter_at(rect);
        let fx = |f: f64| frame_to_px(self.view, rect.min.x, f);
        let ruler = Rect::from_min_max(rect.min, pos2(rect.max.x, rect.min.y + RULER_H));
        p.rect_filled(rect, 0.0, vis.extreme_bg_color);
        p.rect_filled(ruler, 0.0, vis.widgets.noninteractive.bg_fill);
        // The in/out range (else the whole range) lit over both lanes.
        let (a, b) = self.loop_range();
        let lit = Rect::from_x_y_ranges(fx(a as f64 - 0.5)..=fx(b as f64 + 0.5), rect.y_range());
        p.rect_filled(lit.intersect(rect), 0.0, vis.selection.bg_fill.linear_multiply(0.3));
        // Ticks: a label every `step`, a minor tick between.
        let step = ruler_step(self.view.1);
        let minor = if step >= 5 { step / 5 } else { 1 };
        let tick = vis.widgets.noninteractive.fg_stroke.color;
        let first = (px_to_frame(self.view, rect.min.x, rect.min.x) / minor as f64).floor() as i64 * minor;
        let last = px_to_frame(self.view, rect.min.x, rect.max.x).ceil() as i64;
        let mut f = first;
        while f <= last {
            let x = fx(f as f64);
            let major = f.rem_euclid(step) == 0;
            let h = if major { RULER_H * 0.5 } else { RULER_H * 0.25 };
            p.line_segment([pos2(x, ruler.max.y - h), pos2(x, ruler.max.y)], Stroke::new(1.0, tick));
            if major {
                p.text(pos2(x + 3.0, ruler.min.y + 1.0), Align2::LEFT_TOP, f, FontId::proportional(LABEL_FONT), vis.weak_text_color());
            }
            f += minor;
        }
        let cy = ruler.max.y + TRACK_H * 0.5;
        for &k in keys {
            let x = fx(k as f64);
            let held = self.grab.as_ref().is_some_and(|g| g.at == k);
            let fill = if held { vis.widgets.hovered.fg_stroke.color } else { vis.widgets.inactive.fg_stroke.color };
            let pts = vec![pos2(x, cy - KEY_R), pos2(x + KEY_R, cy), pos2(x, cy + KEY_R), pos2(x - KEY_R, cy)];
            p.add(Shape::convex_polygon(pts, fill, Stroke::new(1.0, vis.extreme_bg_color)));
        }
        let x = fx(self.playhead as f64);
        p.line_segment([pos2(x, rect.min.y), pos2(x, rect.max.y)], Stroke::new(1.5, vis.selection.stroke.color));
        let galley = p.layout_no_wrap(self.playhead.to_string(), FontId::proportional(LABEL_FONT), vis.selection.stroke.color);
        let tag = Rect::from_min_size(pos2(x + 2.0, ruler.min.y + 1.0), galley.size() + vec2(TAG_PAD * 2.0, TAG_PAD));
        p.rect_filled(tag, 2.0, vis.selection.bg_fill);
        p.galley(tag.min + vec2(TAG_PAD, TAG_PAD * 0.5), galley, vis.selection.stroke.color);
    }
}

/// Where `frame` lands, with the strip's left edge at `x0`.
pub fn frame_to_px(view: (f64, f64), x0: f32, frame: f64) -> f32 {
    x0 + ((frame - view.0) / view.1) as f32
}

pub fn px_to_frame(view: (f64, f64), x0: f32, x: f32) -> f64 {
    view.0 + (x - x0) as f64 * view.1
}

/// The view that shows all of `range` in `width` pixels, a little slack
/// either side.
pub fn fit_view(range: (i64, i64), width: f32) -> (f64, f64) {
    let n = (range.1 - range.0).abs() as f64 + 1.0;
    let pad = n * FIT_PAD;
    let fpp = ((n + 2.0 * pad) / width.max(1.0) as f64).clamp(MIN_FPP, MAX_FPP);
    (range.0.min(range.1) as f64 - 0.5 - pad, fpp)
}

/// Zoom by `factor` (> 1 in) keeping the frame `x` pixels from the left
/// edge where it is.
pub fn zoom_view(view: (f64, f64), x: f32, factor: f32) -> (f64, f64) {
    let anchor = view.0 + x as f64 * view.1;
    let fpp = (view.1 / factor.max(1e-6) as f64).clamp(MIN_FPP, MAX_FPP);
    (anchor - x as f64 * fpp, fpp)
}

/// Frames between ruler labels: the first of 1, 2, 5 × 10ⁿ that keeps
/// them `LABEL_PX` apart.
pub fn ruler_step(frames_per_px: f64) -> i64 {
    let need = frames_per_px * LABEL_PX as f64;
    let mut mag = 1i64;
    loop {
        for m in [1, 2, 5] {
            let step = m * mag;
            if step as f64 >= need {
                return step;
            }
        }
        mag = mag.saturating_mul(10);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_and_pixels_round_trip() {
        let view = (10.0, 0.25);
        let x = frame_to_px(view, 100.0, 30.0);
        assert_eq!(x, 180.0);
        assert_eq!(px_to_frame(view, 100.0, x), 30.0);
    }

    #[test]
    fn the_ruler_step_is_a_round_number_that_keeps_labels_apart() {
        for fpp in [0.01, 0.1, 0.37, 1.0, 2.5, 40.0] {
            let s = ruler_step(fpp);
            assert!(s as f64 / fpp >= LABEL_PX as f64, "fpp {fpp}: step {s} too dense");
            let lead = s.to_string().trim_end_matches('0').to_owned();
            assert!(["1", "2", "5"].contains(&lead.as_str()), "fpp {fpp}: step {s} is not round");
        }
        assert_eq!(ruler_step(0.01), 1);
        assert_eq!(ruler_step(1.0), 100);
    }

    #[test]
    fn fit_shows_the_whole_range_and_zoom_keeps_the_anchor_still() {
        let v = fit_view((1, 240), 600.0);
        assert!(px_to_frame(v, 0.0, 0.0) < 1.0 && px_to_frame(v, 0.0, 600.0) > 240.0);
        let z = zoom_view(v, 150.0, 2.0);
        assert!((px_to_frame(z, 0.0, 150.0) - px_to_frame(v, 0.0, 150.0)).abs() < 1e-9);
        assert!((z.1 - v.1 / 2.0).abs() < 1e-12);
    }

    #[test]
    fn playback_advances_by_fps_and_loops_the_in_out_range() {
        let mut t = Timeline::new(1, 240);
        t.fps = 24.0;
        t.in_out = Some((10, 12));
        t.playhead = 10;
        assert_eq!(t.tick(0.5), None, "paused: no motion");
        t.playing = true;
        assert_eq!(t.tick(1.0 / 48.0), None, "half a frame owed, none due");
        assert_eq!(t.tick(1.0 / 48.0), Some(11));
        assert_eq!(t.tick(2.0 / 24.0), Some(10), "12 -> wraps to 10");
    }

    fn run(ctx: &egui::Context, t: &mut Timeline, keys: &[i64], events: Vec<egui::Event>) -> Vec<Event> {
        let mut out = Vec::new();
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(600.0, 80.0))),
            events,
            ..Default::default()
        };
        crate::snapshot::discard(ctx.run_ui(input, |ui| out = t.show(ui, keys)));
        out
    }

    fn press(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Default::default() }
    }

    #[test]
    fn a_left_drag_on_the_ruler_scrubs_and_on_a_key_moves_it() {
        let ctx = egui::Context::default();
        let mut t = Timeline::new(1, 240);
        let keys = [50i64, 100];
        run(&ctx, &mut t, &keys, vec![]);
        let (s, view) = (t.strip_rect(), t.view);
        let at = |f: i64, lane: f32| pos2(frame_to_px(view, s.min.x, f as f64), s.min.y + lane);
        let ruler = at(120, RULER_H * 0.5);
        run(&ctx, &mut t, &keys, vec![egui::Event::PointerMoved(ruler), press(ruler, true)]);
        let moved = ruler + vec2(40.0, 0.0);
        let ev = run(&ctx, &mut t, &keys, vec![egui::Event::PointerMoved(moved)]);
        let want = px_to_frame(t.view, s.min.x, moved.x).round() as i64;
        assert_eq!(ev, vec![Event::Scrub(want)]);
        run(&ctx, &mut t, &keys, vec![press(moved, false)]);
        let key = at(100, RULER_H + TRACK_H * 0.5);
        run(&ctx, &mut t, &keys, vec![egui::Event::PointerMoved(key), press(key, true)]);
        let ev = run(&ctx, &mut t, &keys, vec![egui::Event::PointerMoved(key + vec2(40.0, 0.0))]);
        assert_eq!(ev.len(), 1, "{ev:?}");
        assert!(matches!(ev[0], Event::KeyMoved { from: 100, to } if to > 100), "{ev:?}");
    }

    #[test]
    fn the_playhead_line_and_range_shade_are_drawn() {
        let mut t = Timeline::new(1, 240);
        t.playhead = 120;
        t.in_out = Some((60, 180));
        let shot = crate::snapshot::shoot(600.0, 80.0, |ui| {
            t.show(ui, &[30, 90]);
        });
        let s = t.strip_rect();
        let x = frame_to_px(t.view, s.min.x, 120.0);
        let line = shot.at(x, s.max.y - TRACK_H * 0.5);
        let dark = shot.at(frame_to_px(t.view, s.min.x, 20.0), s.max.y - 2.0);
        let lit = shot.at(frame_to_px(t.view, s.min.x, 150.0), s.max.y - 2.0);
        assert_ne!(line, dark, "playhead line missing at x={x}");
        assert_ne!(lit, dark, "in/out range not shaded");
    }
}
