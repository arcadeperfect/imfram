//! Toasts: short notices stacked in the bottom-right of the window, each
//! sliding in, holding a few seconds, and fading out. Four kinds, told
//! apart by a coloured bar on the left; a sticky toast stays until
//! clicked; a progress toast updates in place and times out once it
//! completes. Hovering one pauses its clock. The app owns a [`Toasts`],
//! pushes into it from anywhere, and calls [`Toasts::show`] once a frame;
//! [`Toasts::drain_log`] hands past messages to a log panel. Colours come
//! from the context's `Visuals`; time from `web_time` so the web build
//! keeps up.

use egui::{Align2, Color32, Context, CornerRadius, Id, Order, Pos2, Rect, Sense, Ui};
use web_time::Instant;

/// A toast's width in points.
const WIDTH: f32 = 320.0;
/// A toast's height before it has been laid out once.
const MIN_H: f32 = 40.0;
/// The coloured bar on the left.
const BAR: f32 = 4.0;
/// The progress strip along the bottom.
const PROGRESS_H: f32 = 3.0;
/// From the content rect's corner, and between toasts.
const MARGIN: f32 = 12.0;
const GAP: f32 = 6.0;
/// Inner padding.
const PAD: i8 = 8;
/// How far a toast slides in from, in points.
const SLIDE: f32 = 40.0;
/// Seconds for the slide / fade.
const ANIM_SECS: f32 = 0.18;
/// Seconds a toast holds, by kind.
const HOLD_INFO: f32 = 4.0;
const HOLD_WARN: f32 = 6.0;
const HOLD_ERROR: f32 = 8.0;
/// Past messages kept when nobody drains them.
const LOG_CAP: usize = 200;
/// Toasts drawn at once; older ones wait their turn.
const MAX_SHOWN: usize = 6;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Info,
    Success,
    Warn,
    Error,
}

impl Kind {
    fn hold(self) -> f32 {
        match self {
            Kind::Info | Kind::Success => HOLD_INFO,
            Kind::Warn => HOLD_WARN,
            Kind::Error => HOLD_ERROR,
        }
    }

    /// The bar's colour, from the theme: info and success are the accent
    /// and link colours, warn and error the theme's own.
    pub fn color(self, v: &egui::Visuals) -> Color32 {
        match self {
            Kind::Info => v.hyperlink_color,
            Kind::Success => v.selection.bg_fill.gamma_multiply(1.4).to_opaque(),
            Kind::Warn => v.warn_fg_color,
            Kind::Error => v.error_fg_color,
        }
    }
}

/// One notice. Build with [`Toast::new`] or [`Toast::sticky`], hand to
/// [`Toasts::add`]; [`Toasts::push`] does both for the plain case.
#[derive(Clone, Debug)]
pub struct Toast {
    pub kind: Kind,
    pub text: String,
    /// Stays until clicked.
    pub sticky: bool,
    /// A progress toast's fraction; `>= 1.0` lets it time out.
    pub progress: Option<f32>,
    /// A progress toast's key, so updates find it.
    key: Option<Id>,
    /// Seconds left on the clock.
    remaining: f32,
    /// 0 (hidden) .. 1 (fully in); eased toward `alive`.
    anim: f32,
    alive: bool,
    /// The pointer was over it last frame: the clock pauses.
    hovered: bool,
    /// Height last laid out at.
    h: f32,
    serial: u64,
}

impl Toast {
    pub fn new(kind: Kind, text: impl Into<String>) -> Self {
        let (sticky, progress, key, remaining) = (false, None, None, kind.hold());
        Self { kind, text: text.into(), sticky, progress, key, remaining, anim: 0.0, alive: true, hovered: false, h: MIN_H, serial: 0 }
    }

    /// A toast that waits to be clicked away.
    pub fn sticky(kind: Kind, text: impl Into<String>) -> Self {
        Self { sticky: true, ..Self::new(kind, text) }
    }

    /// Whether it is on its way out.
    pub fn dismissed(&self) -> bool {
        !self.alive
    }

    fn timed(&self) -> bool {
        !self.sticky && self.progress.is_none_or(|f| f >= 1.0)
    }
}

/// The app's toasts: push from anywhere, show once a frame.
#[derive(Default)]
pub struct Toasts {
    items: Vec<Toast>,
    log: Vec<(Kind, String)>,
    last: Option<Instant>,
    next: u64,
}

impl Toasts {
    pub fn new() -> Self {
        Self::default()
    }

    /// A plain timed toast.
    pub fn push(&mut self, kind: Kind, text: impl Into<String>) {
        self.add(Toast::new(kind, text));
    }

    pub fn add(&mut self, mut toast: Toast) {
        self.next += 1;
        toast.serial = self.next;
        self.remember(toast.kind, &toast.text);
        self.items.push(toast);
    }

    /// A toast keyed by `id` that updates in place: the first call makes
    /// it, later ones move its bar and text. At `fraction >= 1.0` it
    /// becomes a success toast on the clock.
    pub fn progress(&mut self, id: impl std::hash::Hash + std::fmt::Debug, fraction: f32, text: impl Into<String>) {
        let key = Id::new(id);
        let fraction = fraction.clamp(0.0, 1.0);
        let text = text.into();
        if let Some(t) = self.items.iter_mut().find(|t| t.key == Some(key) && t.alive) {
            let was_done = t.progress.is_some_and(|f| f >= 1.0);
            t.text = text;
            t.progress = Some(fraction);
            if fraction >= 1.0 && !was_done {
                t.kind = Kind::Success;
                t.remaining = Kind::Success.hold();
                let (kind, text) = (t.kind, t.text.clone());
                self.remember(kind, &text);
            }
            return;
        }
        let mut t = Toast::new(if fraction >= 1.0 { Kind::Success } else { Kind::Info }, text);
        t.progress = Some(fraction);
        t.key = Some(key);
        self.add(t);
    }

    /// Start every toast on its way out.
    pub fn dismiss_all(&mut self) {
        self.items.iter_mut().for_each(|t| t.alive = false);
    }

    /// Toasts still on screen (including those fading out).
    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Everything pushed since the last drain, oldest first, for a log.
    pub fn drain_log(&mut self) -> Vec<(Kind, String)> {
        std::mem::take(&mut self.log)
    }

    fn remember(&mut self, kind: Kind, text: &str) {
        if self.log.len() >= LOG_CAP {
            self.log.remove(0);
        }
        self.log.push((kind, text.to_owned()));
    }

    /// Advance clocks and animations by `dt` seconds. `show` does this
    /// from the wall clock; an app driving its own time may call it.
    pub fn tick(&mut self, dt: f32) {
        for t in &mut self.items {
            // The fade starts the tick after the clock runs out, so a
            // toast is never born and gone in one long frame.
            let target = if t.alive { 1.0 } else { 0.0 };
            let step = dt / ANIM_SECS;
            t.anim = if t.anim < target { (t.anim + step).min(target) } else { (t.anim - step).max(target) };
            if t.alive && t.timed() && !t.hovered {
                t.remaining -= dt;
                if t.remaining <= 0.0 {
                    t.alive = false;
                }
            }
        }
        self.items.retain(|t| t.alive || t.anim > 0.0);
    }

    /// Anything still moving (a slide or a fade in flight).
    fn animating(&self) -> bool {
        self.items.iter().any(|t| t.anim != if t.alive { 1.0 } else { 0.0 })
    }

    /// Draw the stack in the bottom-right of the content rect.
    pub fn show(&mut self, ctx: &Context) {
        let now = Instant::now();
        let dt = self.last.map_or(0.0, |l| (now - l).as_secs_f32().min(0.1));
        self.last = Some(now);
        self.tick(dt);
        if self.items.is_empty() {
            self.last = None;
            return;
        }

        let content = ctx.content_rect();
        let right = content.max.x - MARGIN;
        let mut bottom = content.max.y - MARGIN;
        let vis = ctx.global_style().visuals.clone();
        for t in self.items.iter_mut().rev().take(MAX_SHOWN) {
            let ease = 1.0 - (1.0 - t.anim).powi(2);
            let slide = (1.0 - ease) * SLIDE;
            let top = bottom - t.h * ease.max(0.01);
            let pos = Pos2::new(right - WIDTH + slide, top);
            let id = Id::new("imfram-toast").with(t.serial);
            let r = egui::Area::new(id)
                .order(Order::Tooltip)
                .fixed_pos(pos)
                .movable(false)
                .constrain(false)
                .show(ctx, |ui| {
                    ui.set_opacity(ease);
                    draw(ui, t, &vis)
                })
                .inner;
            t.h = r.rect.height();
            t.hovered = r.hovered();
            if r.clicked() {
                t.alive = false;
            }
            bottom -= (t.h + GAP) * ease;
        }

        if self.animating() {
            ctx.request_repaint();
        } else if self.items.iter().any(|t| t.alive && t.timed() && !t.hovered) {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}

/// One toast's body: a framed row with the kind's bar on the left and a
/// progress strip along the bottom. Returns the frame's response.
fn draw(ui: &mut Ui, t: &Toast, vis: &egui::Visuals) -> egui::Response {
    let color = t.kind.color(vis);
    let frame = egui::Frame::new()
        .fill(vis.window_fill())
        .stroke(vis.window_stroke())
        .corner_radius(CornerRadius::same(3))
        .inner_margin(egui::Margin { left: PAD + BAR as i8, right: PAD, top: PAD, bottom: PAD });
    let out = frame.show(ui, |ui| {
        ui.set_width(WIDTH - BAR - 2.0 * PAD as f32);
        ui.label(egui::RichText::new(&t.text).color(vis.text_color()));
        if let Some(f) = t.progress {
            if f < 1.0 {
                ui.label(egui::RichText::new(format!("{:.0}%", f * 100.0)).weak().small());
            }
        }
    });
    let rect = out.response.rect;
    let p = ui.painter();
    let bar = Rect::from_min_max(rect.min, Pos2::new(rect.min.x + BAR, rect.max.y));
    p.rect_filled(bar, CornerRadius { nw: 3, sw: 3, ne: 0, se: 0 }, color);
    if let Some(f) = t.progress {
        let track = Rect::from_min_max(Pos2::new(rect.min.x + BAR, rect.max.y - PROGRESS_H), rect.max);
        p.rect_filled(track, CornerRadius::ZERO, vis.faint_bg_color);
        let fill = Rect::from_min_size(track.min, egui::vec2(track.width() * f, PROGRESS_H));
        p.rect_filled(fill, CornerRadius::ZERO, color);
    }
    if t.sticky {
        p.text(
            Pos2::new(rect.max.x - PAD as f32, rect.min.y + PAD as f32),
            Align2::RIGHT_TOP,
            "×",
            egui::FontId::proportional(12.0),
            vis.weak_text_color(),
        );
    }
    out.response.interact(Sense::click())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settle(t: &mut Toasts) {
        t.tick(ANIM_SECS);
    }

    #[test]
    fn a_pushed_toast_holds_its_time_then_fades_away() {
        let mut t = Toasts::new();
        t.push(Kind::Info, "hello");
        settle(&mut t);
        assert_eq!(t.items[0].anim, 1.0);
        t.tick(HOLD_INFO - 0.5);
        assert!(t.items[0].alive);
        t.tick(1.0);
        assert!(t.items[0].dismissed(), "the clock ran out");
        assert_eq!(t.len(), 1, "still fading");
        settle(&mut t);
        assert!(t.is_empty());
    }

    #[test]
    fn hovering_pauses_the_clock_and_a_sticky_toast_has_none() {
        let mut t = Toasts::new();
        t.push(Kind::Warn, "careful");
        t.add(Toast::sticky(Kind::Error, "stay"));
        t.items[0].hovered = true;
        t.tick(HOLD_WARN * 3.0);
        assert!(t.items[0].alive, "hovered: the clock did not run");
        assert!(t.items[1].alive, "sticky: no clock at all");
        t.items[0].hovered = false;
        t.tick(HOLD_WARN + 0.1);
        assert!(!t.items[0].alive);
        assert!(t.items[1].alive);
        t.dismiss_all();
        settle(&mut t);
        assert!(t.is_empty());
    }

    #[test]
    fn progress_updates_one_toast_in_place_and_completes_at_one() {
        let mut t = Toasts::new();
        t.progress("job", 0.2, "loading 20%");
        t.progress("job", 0.6, "loading 60%");
        assert_eq!(t.len(), 1);
        assert_eq!(t.items[0].text, "loading 60%");
        assert_eq!(t.items[0].kind, Kind::Info);
        t.tick(HOLD_INFO * 2.0);
        assert!(t.items[0].alive, "a live progress toast does not time out");
        t.progress("job", 1.0, "loaded");
        assert_eq!(t.items[0].kind, Kind::Success);
        t.tick(HOLD_INFO + 0.1);
        assert!(t.items[0].dismissed());
    }

    #[test]
    fn the_log_keeps_every_message_once_and_drains() {
        let mut t = Toasts::new();
        t.push(Kind::Info, "one");
        t.progress("j", 0.5, "half");
        t.progress("j", 1.0, "done");
        let log = t.drain_log();
        let texts: Vec<&str> = log.iter().map(|(_, s)| s.as_str()).collect();
        assert_eq!(texts, ["one", "half", "done"]);
        assert_eq!(log[2].0, Kind::Success);
        assert!(t.drain_log().is_empty());
    }

    #[test]
    fn a_toast_draws_in_the_bottom_right_with_its_kind_bar() {
        let (w, h) = (480.0, 320.0);
        let mut t = Toasts::new();
        t.push(Kind::Error, "boom");
        settle(&mut t);
        let shot = crate::snapshot::shoot(w, h, |ui| {
            // A new Area's first frame is egui's invisible sizing pass; the
            // second call in the same frame is the one that paints.
            ui.ctx().options_mut(|o| o.warn_on_id_clash = false);
            t.show(ui.ctx());
            t.show(ui.ctx());
        });
        let bar_x = w - MARGIN - WIDTH + BAR / 2.0;
        let y = h - MARGIN - MIN_H / 2.0;
        let bg = shot.at(w / 2.0, h / 2.0);
        let bar = shot.at(bar_x, y);
        assert_ne!(bar, bg, "the error bar is painted: {bar:?}");
        assert!(bar[0] > bar[1] + 40 && bar[0] > bar[2] + 40, "and it is the error red: {bar:?}");
        assert_eq!(shot.at(MARGIN, MARGIN), bg, "the top-left is empty");
        let body = shot.at(w - MARGIN - WIDTH / 2.0, y);
        assert_ne!(body, bg, "the body is framed");
    }
}
