//! Tilt menus: the menubar dropdown, swung.
//!
//! A bar chip ("prefs") rests in the menubar like any button. Hover it and it
//! SWINGS about its left edge down to [`TILT`], typing itself out to its full
//! name ("preferences") on the way, and its rows unfold below it at the same
//! tilt — a sheared slab whose left edge stays plumb under the chip, so a
//! menu hangs straight down however far it tilts. A row that holds more (a
//! submenu, a choice) appends its own panel to its LEFT when that fits whole
//! on screen and clear of every open panel — its chip carrying the row's own
//! diagonal on up-left — and otherwise to its RIGHT, the chip starting where
//! the row ends. Either way it hangs plumb ([`SUB_STACK_DEG`]), its edge on
//! the parent's. On the right it steps right (down, at the screen's edge) until it
//! clears every panel already open, so nothing ever sits over the menu you
//! came from.
//!
//! Leaves are live where they sit: a tick box toggles on the press, a slider
//! drags along its own tilted track (absolute — the value is where the
//! pointer is, never a sum of deltas), a choice's options are radio rows.
//! The icon on the far end of an open chip pops the menu out as an ordinary
//! floating window drawn from the SAME items, straight, for when it should
//! stay.
//!
//! Depends only on egui (manifesto rule 4, the `rgbslider` precedent): the
//! caller builds a [`Menu`] per frame from its own state and applies the
//! [`Event`]s that come back. Colours come from the context's `Visuals`,
//! which `theme::sync` already fills from the theme slots.

use egui::{pos2, vec2, Align2, Color32, Id, Pos2, Rect, Shape, Stroke, Vec2};
use std::collections::BTreeSet;

pub type ItemId = &'static str;

/// How far a top chip swings, clockwise, in degrees. Every panel's text
/// reads at this tilt.
pub const TILT_DEG: f32 = 24.0;
/// The top panel's rows stack straight down (screen degrees, y down).
pub const TOP_STACK_DEG: f32 = 90.0;
/// A submenu's rows stack straight down too, so its left and right edges
/// run plumb beside its parent's.
pub const SUB_STACK_DEG: f32 = 90.0;

const ROW_H: f32 = 22.0;
const CHIP_H: f32 = 24.0;
const PAD: f32 = 8.0;
const GUTTER: f32 = 18.0;
const TRACK_W: f32 = 104.0;
const VALUE_W: f32 = 48.0;
const POP_W: f32 = 22.0;
/// How far panels may touch before they count as overlapping: a child's
/// left edge sits ON its parent's right edge.
const GAP: f32 = 0.5;
/// Hovering another branch at a level that already has one open waits this
/// long, so a pointer cutting across rows does not snap the open one shut.
const DWELL: f64 = 0.22;
/// The pointer may leave the menu this long before it folds (the slide from
/// a row to its child crosses open ground).
const LINGER: f64 = 0.6;

/// One row of a menu.
pub enum Item {
    Check { id: ItemId, label: String, on: bool },
    Slider { id: ItemId, label: String, value: f32, min: f32, max: f32, step: f32, unit: &'static str },
    /// A dropdown: its options slide out as radio rows.
    Choice { id: ItemId, label: String, options: Vec<String>, selected: Option<usize> },
    Sub { label: String, items: Vec<Item> },
    /// A button. Fires and folds the menu.
    Action { id: ItemId, label: String },
}

impl Item {
    pub fn check(id: ItemId, label: impl Into<String>, on: bool) -> Self {
        Item::Check { id, label: label.into(), on }
    }
    pub fn slider(id: ItemId, label: impl Into<String>, value: f32, min: f32, max: f32) -> Self {
        Item::Slider { id, label: label.into(), value, min, max, step: 0.0, unit: "" }
    }
    /// Snap to multiples of `step` (1 = integers).
    pub fn step(mut self, s: f32) -> Self {
        if let Item::Slider { step, .. } = &mut self {
            *step = s;
        }
        self
    }
    pub fn unit(mut self, u: &'static str) -> Self {
        if let Item::Slider { unit, .. } = &mut self {
            *unit = u;
        }
        self
    }
    pub fn choice(
        id: ItemId,
        label: impl Into<String>,
        options: Vec<String>,
        selected: Option<usize>,
    ) -> Self {
        Item::Choice { id, label: label.into(), options, selected }
    }
    pub fn sub(label: impl Into<String>, items: Vec<Item>) -> Self {
        Item::Sub { label: label.into(), items }
    }
    pub fn action(id: ItemId, label: impl Into<String>) -> Self {
        Item::Action { id, label: label.into() }
    }
    fn label(&self) -> &str {
        match self {
            Item::Check { label, .. }
            | Item::Slider { label, .. }
            | Item::Choice { label, .. }
            | Item::Sub { label, .. }
            | Item::Action { label, .. } => label,
        }
    }
}

/// One bar chip and what unfolds under it.
pub struct Menu {
    pub id: &'static str,
    /// What the bar shows at rest.
    pub short: String,
    /// What the chip types itself out to as it swings.
    pub full: String,
    pub items: Vec<Item>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Toggle(ItemId, bool),
    Slide(ItemId, f32),
    Pick(ItemId, usize),
    Action(ItemId),
}

/// A sheared coordinate frame: `x` along the text (at `angle`), `y` down
/// the rows along the stack direction — scaled so a unit of `y` is a pixel
/// measured square to the text, which keeps a row `ROW_H` thick whatever
/// the shear.
#[derive(Clone, Copy, Debug)]
struct Frame {
    o: Pos2,
    angle: f32,
    u: Vec2,
    w: Vec2,
}

impl Frame {
    fn new(o: Pos2, angle: f32, stack: f32) -> Self {
        let u = vec2(angle.cos(), angle.sin());
        let s = (stack - angle).sin().max(0.2);
        Self { o, angle, u, w: vec2(stack.cos(), stack.sin()) / s }
    }
    fn at(&self, x: f32, y: f32) -> Pos2 {
        self.o + self.u * x + self.w * y
    }
    fn local(&self, p: Pos2) -> Vec2 {
        let d = p - self.o;
        let det = self.u.x * self.w.y - self.u.y * self.w.x;
        vec2((d.x * self.w.y - d.y * self.w.x) / det, (self.u.x * d.y - self.u.y * d.x) / det)
    }
    /// Square to the text, pointing down the rows.
    #[cfg(test)]
    fn normal(&self) -> Vec2 {
        vec2(-self.u.y, self.u.x)
    }
    fn shifted(&self, dx: f32) -> Self {
        Self { o: self.at(dx, 0.0), ..*self }
    }
    fn quad(&self, r: Rect) -> Vec<Pos2> {
        vec![
            self.at(r.min.x, r.min.y),
            self.at(r.max.x, r.min.y),
            self.at(r.max.x, r.max.y),
            self.at(r.min.x, r.max.y),
        ]
    }
}

/// What a panel's row is: an item, or one option of a choice.
#[derive(Clone, Copy)]
enum Row<'a> {
    Item(&'a Item),
    Opt { id: ItemId, idx: usize, label: &'a str, on: bool },
}

impl Row<'_> {
    fn label(&self) -> &str {
        match self {
            Row::Item(it) => it.label(),
            Row::Opt { label, .. } => label,
        }
    }
    fn branches(&self) -> bool {
        matches!(self, Row::Item(Item::Sub { .. } | Item::Choice { .. }))
    }
}

struct Panel<'a> {
    frame: Frame,
    title: String,
    title_w: f32,
    chip_h: f32,
    width: f32,
    label_w: f32,
    rows: Vec<Row<'a>>,
    /// 0..1 per row: how far it has unfolded.
    reveal: Vec<f32>,
    /// Draws the pop-out icon on its chip (the top panel).
    pop: bool,
    /// A child that opened on its row's LEFT (the row's chevron says so).
    left: bool,
}

impl Panel<'_> {
    fn row_rect(&self, i: usize) -> Rect {
        let y = self.chip_h + 2.0 + i as f32 * (ROW_H + 1.0);
        Rect::from_min_size(pos2(0.0, y), vec2(self.width, ROW_H))
    }
    fn chip_rect(&self) -> Rect {
        Rect::from_min_size(Pos2::ZERO, vec2(self.title_w, self.chip_h))
    }
    fn bottom(&self) -> f32 {
        self.rows.len().checked_sub(1).map_or(self.chip_h, |n| self.row_rect(n).max.y)
    }
    fn track(&self) -> (f32, f32) {
        let x0 = PAD + GUTTER + self.label_w + PAD;
        (x0, x0 + TRACK_W)
    }
    fn pop_rect(&self) -> Rect {
        Rect::from_min_size(pos2(self.title_w - POP_W, 0.0), vec2(POP_W, self.chip_h))
    }
    fn hit(&self, p: Pos2) -> Option<Hit> {
        let l = self.frame.local(p).to_pos2();
        if self.chip_rect().contains(l) {
            return Some(if self.pop && self.pop_rect().contains(l) { Hit::Pop } else { Hit::Chip });
        }
        (0..self.rows.len())
            .find(|&i| self.reveal[i] > 0.5 && self.row_rect(i).contains(l))
            .map(Hit::Row)
    }
    /// Everything it covers: chip and rows as one convex parallelogram.
    fn outline(&self) -> Vec<Pos2> {
        let w = self.width.max(self.title_w);
        self.frame.quad(Rect::from_x_y_ranges(0.0..=w, 0.0..=self.bottom()))
    }
    fn bounds(&self) -> Rect {
        let mut pts = self.frame.quad(self.chip_rect());
        if !self.rows.is_empty() {
            pts.extend(self.frame.quad(Rect::from_x_y_ranges(0.0..=self.width, 0.0..=self.bottom())));
        }
        Rect::from_points(&pts)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Hit {
    Chip,
    Pop,
    Row(usize),
}

#[derive(Default)]
pub struct TiltBar {
    open: Option<&'static str>,
    opened_at: f64,
    /// The open branch at each level: (row index in that level's panel,
    /// when it opened).
    path: Vec<(usize, f64)>,
    /// A change waiting out [`DWELL`]: (level, branch to open or None to
    /// fold that level, since).
    dwell: Option<(usize, Option<usize>, f64)>,
    left_at: Option<f64>,
    /// A slider being dragged: (level, row).
    drag: Option<(usize, usize)>,
    /// Menus popped out into floating windows.
    pub floating: BTreeSet<&'static str>,
    /// Headless renders park the pointer here (events pushed mid-pass never
    /// reach `hover_pos`).
    #[cfg(test)]
    test_ptr: Option<Pos2>,
    /// Row centres per level, last pass, for a headless render to aim at.
    #[cfg(test)]
    test_rows: Vec<Vec<Pos2>>,
}

fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

fn font() -> egui::FontId {
    egui::FontId::proportional(13.0)
}

fn text_w(ctx: &egui::Context, s: &str) -> f32 {
    ctx.fonts_mut(|fo| fo.layout_no_wrap(s.to_owned(), font(), Color32::WHITE).size().x)
}

/// Text whose LEFT-CENTRE sits on `at`, reading along `angle`.
fn text(ctx: &egui::Context, painter: &egui::Painter, at: Pos2, angle: f32, s: &str, c: Color32) {
    let g = ctx.fonts_mut(|fo| fo.layout_no_wrap(s.to_owned(), font(), c));
    let pos = at - vec2(0.0, g.size().y * 0.5);
    painter.add(egui::epaint::TextShape::new(pos, g, c).with_angle_and_anchor(angle, Align2::LEFT_CENTER));
}

fn fmt_value(v: f32, step: f32, unit: &str) -> String {
    if step >= 1.0 {
        format!("{v:.0}{unit}")
    } else if v.abs() >= 100.0 {
        format!("{v:.1}{unit}")
    } else {
        format!("{v:.2}{unit}")
    }
}

impl TiltBar {
    /// Draw the bar chips into `ui` (a horizontal menubar) and the unfolded
    /// menu over everything. Returns what the user changed this frame.
    pub fn bar(&mut self, ui: &mut egui::Ui, menus: &[Menu]) -> Vec<Event> {
        let ctx = ui.ctx().clone();
        let now = ctx.input(|i| i.time);
        let (ptr, pressed, down) = ctx
            .input(|i| (i.pointer.hover_pos(), i.pointer.primary_pressed(), i.pointer.primary_down()));
        #[cfg(test)]
        let ptr = self.test_ptr.or(ptr);
        let vis = ui.visuals().clone();
        let screen = ctx.content_rect();
        let mut events = Vec::new();

        // The resting chips. Their rects are where a swing pivots.
        let mut rest: Vec<Rect> = Vec::with_capacity(menus.len());
        let mut bar_hover = None;
        let home = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, Id::new("tilt-home")));
        for (mi, m) in menus.iter().enumerate() {
            let w = text_w(&ctx, &m.short) + 2.0 * PAD;
            let (rect, resp) =
                ui.allocate_exact_size(vec2(w, ui.spacing().interact_size.y), egui::Sense::click());
            // egui's hover is not enough: an open menu's area can sit over
            // a neighbouring chip and swallow it, so switching menus waited
            // until the pointer left that area. Our own layers never block.
            let ours = |p: Pos2| {
                ctx.layer_id_at(p).is_none_or(|l| {
                    l.order == egui::Order::Foreground
                        && menus.iter().any(|m| l.id == Id::new(("tilt-area", m.id)))
                })
            };
            let over = resp.hovered()
                || ptr.is_some_and(|p| rect.contains(p) && ours(p))
                || self.test_over(rect);
            if over {
                bar_hover = Some(mi);
            }
            let t = ctx.animate_value_with_time(
                Id::new(("tilt-swing", m.id)),
                if self.open == Some(m.id) { 1.0 } else { 0.0 },
                0.08,
            );
            if t < 0.02 {
                let wv = if over { &vis.widgets.hovered } else { &vis.widgets.inactive };
                ui.painter().rect_filled(rect, 0.0, wv.weak_bg_fill);
                ui.painter().text(rect.center(), Align2::CENTER_CENTER, &m.short, font(), wv.fg_stroke.color);
            } else if self.open != Some(m.id) {
                // Swinging home: the chip alone, its rows already gone.
                let p = top_panel(&ctx, m, rect, t);
                paint_chip(&ctx, &home, &vis, &p, t, false);
            }
            rest.push(rect);
        }

        if let Some(mi) = bar_hover {
            if self.open != Some(menus[mi].id) && self.drag.is_none() {
                self.close();
                self.open = Some(menus[mi].id);
                self.opened_at = now;
            }
        }
        let Some(mi) = self.open.and_then(|id| menus.iter().position(|m| m.id == id)) else {
            self.close();
            self.floating_retain(menus);
            return events;
        };
        let menu = &menus[mi];
        let t = ctx.animate_value_with_time(Id::new(("tilt-swing", menu.id)), 1.0, 0.08);

        let panels = self.layout(&ctx, menu, rest[mi], t, now, screen);
        #[cfg(test)]
        {
            self.test_rows = panels
                .iter()
                .map(|p| {
                    (0..p.rows.len()).map(|i| p.frame.at(60.0, p.row_rect(i).center().y)).collect()
                })
                .collect();
        }

        // Deepest panel first: the newest is the one under the hand.
        let hit = ptr.and_then(|p| {
            panels.iter().enumerate().rev().find_map(|(lv, pn)| pn.hit(p).map(|h| (lv, h)))
        });

        // Swallow the pointer while it is ON the menu, so nothing under it
        // takes the click; off it, everything below works as ever.
        let bounds = panels.iter().map(|p| p.bounds()).fold(Rect::NOTHING, |a, b| a.union(b));
        let painter = egui::Area::new(Id::new(("tilt-area", menu.id)))
            .order(egui::Order::Foreground)
            .fixed_pos(bounds.min)
            .interactable(hit.is_some() || self.drag.is_some())
            .show(&ctx, |ui| {
                ui.allocate_rect(bounds, egui::Sense::click_and_drag());
                ui.painter().clone().with_clip_rect(screen)
            })
            .inner;

        // A slider drag: absolute, from the pointer's place on the track.
        if let Some((lv, ri)) = self.drag {
            if !down {
                self.drag = None;
            } else if let (Some(p), Some(pn)) = (ptr, panels.get(lv)) {
                if let Some(Row::Item(Item::Slider { id, value, min, max, step, .. })) = pn.rows.get(ri) {
                    let v = slider_value(pn, p, *min, *max, *step);
                    if v != *value {
                        events.push(Event::Slide(id, v));
                    }
                }
            }
        }

        let inside = hit.is_some() || bar_hover.is_some() || self.drag.is_some();
        if inside {
            self.left_at = None;
        } else {
            let since = *self.left_at.get_or_insert(now);
            if now - since > LINGER || pressed {
                self.close();
                return events;
            }
        }

        // Hover opens branches and folds stale ones.
        if let (Some((lv, Hit::Row(ri))), None) = (hit, self.drag) {
            let want = panels[lv].rows[ri].branches().then_some(ri);
            let open_here = self.path.get(lv).map(|b| b.0);
            if open_here == want {
                self.dwell = None;
            } else if open_here.is_none() {
                // Nothing open at this level: no reason to wait.
                self.set_branch(lv, want, now);
            } else {
                match self.dwell {
                    Some((dl, dr, since)) if dl == lv && dr == want => {
                        if now - since > DWELL {
                            self.set_branch(lv, want, now);
                        }
                    }
                    _ => self.dwell = Some((lv, want, now)),
                }
            }
        } else if matches!(hit, Some((_, Hit::Chip | Hit::Pop))) {
            self.dwell = None;
        }

        // Presses act at once: a menu is for quick hands.
        if pressed {
            match hit {
                Some((_, Hit::Pop)) => {
                    if !self.floating.remove(menu.id) {
                        self.floating.insert(menu.id);
                    }
                    self.close();
                    return events;
                }
                Some((lv, Hit::Row(ri))) => match panels[lv].rows[ri] {
                    Row::Item(Item::Check { id, on, .. }) => events.push(Event::Toggle(id, !on)),
                    Row::Item(Item::Slider { id, value, min, max, step, .. }) => {
                        if let Some(p) = ptr {
                            let v = slider_value(&panels[lv], p, *min, *max, *step);
                            if v != *value {
                                events.push(Event::Slide(id, v));
                            }
                        }
                        self.drag = Some((lv, ri));
                    }
                    Row::Item(Item::Action { id, .. }) => {
                        events.push(Event::Action(id));
                        self.close();
                        return events;
                    }
                    Row::Item(Item::Sub { .. } | Item::Choice { .. }) => {
                        self.set_branch(lv, Some(ri), now)
                    }
                    Row::Opt { id, idx, .. } => events.push(Event::Pick(id, idx)),
                },
                _ => {}
            }
        }

        let hover_row = match hit {
            Some((lv, Hit::Row(ri))) => Some((lv, ri)),
            _ => None,
        };
        let floating = self.floating.contains(menu.id);
        for (lv, pn) in panels.iter().enumerate() {
            let dim = if lv + 1 < panels.len() { 0.6 } else { 1.0 };
            let active = self.path.get(lv).map(|b| b.0);
            let opens_left = panels.get(lv + 1).is_some_and(|c| c.left);
            self.paint_rows(&ctx, &painter, &vis, pn, lv, dim, hover_row, active, opens_left);
            paint_chip(&ctx, &painter, &vis, pn, if lv == 0 { t } else { 1.0 }, floating);
        }

        let settling = now - self.opened_at < 1.0
            || self.path.last().is_some_and(|b| now - b.1 < 1.0)
            || self.dwell.is_some()
            || self.left_at.is_some();
        if settling || t < 1.0 {
            ctx.request_repaint();
        }
        self.floating_retain(menus);
        events
    }

    /// The popped-out menus, as ordinary windows over the same items.
    pub fn floating_windows(&mut self, ctx: &egui::Context, menus: &[Menu]) -> Vec<Event> {
        let mut events = Vec::new();
        let ids: Vec<_> = self.floating.iter().copied().collect();
        for id in ids {
            let Some(m) = menus.iter().find(|m| m.id == id) else { continue };
            let mut open = true;
            egui::Window::new(&m.full)
                .id(Id::new(("tilt-float", id)))
                .open(&mut open)
                .resizable(false)
                .show(ctx, |ui| flat(ui, &m.items, &mut events));
            if !open {
                self.floating.remove(id);
            }
        }
        events
    }

    #[cfg(test)]
    fn test_over(&self, r: Rect) -> bool {
        self.test_ptr.is_some_and(|p| r.contains(p))
    }
    #[cfg(not(test))]
    fn test_over(&self, _: Rect) -> bool {
        false
    }

    fn floating_retain(&mut self, menus: &[Menu]) {
        self.floating.retain(|id| menus.iter().any(|m| m.id == *id));
    }

    fn close(&mut self) {
        self.open = None;
        self.path.clear();
        self.dwell = None;
        self.left_at = None;
        self.drag = None;
    }

    fn set_branch(&mut self, level: usize, row: Option<usize>, now: f64) {
        self.path.truncate(level);
        if let Some(r) = row {
            self.path.push((r, now));
        }
        self.dwell = None;
    }

    fn layout<'a>(
        &mut self,
        ctx: &egui::Context,
        m: &'a Menu,
        rest: Rect,
        t: f32,
        now: f64,
        screen: Rect,
    ) -> Vec<Panel<'a>> {
        let mut top = top_panel(ctx, m, rest, t);
        unfold(&mut top.reveal, (now - self.opened_at) as f32 - 0.06);
        let mut panels = vec![top];
        let mut keep = 0;
        let tilt = TILT_DEG.to_radians();
        for (lv, &(ri, at)) in self.path.iter().enumerate() {
            let parent = &panels[lv];
            let Some(row) = parent.rows.get(ri) else { break };
            let (title, rows): (&str, Vec<Row>) = match row {
                Row::Item(Item::Sub { label, items }) => (label, items.iter().map(Row::Item).collect()),
                Row::Item(Item::Choice { id, label, options, selected }) => (
                    label,
                    options
                        .iter()
                        .enumerate()
                        .map(|(idx, o)| Row::Opt { id, idx, label: o, on: *selected == Some(idx) })
                        .collect(),
                ),
                _ => break,
            };
            keep = lv + 1;

            let age = (now - at) as f32;
            let fr = Frame::new(Pos2::ZERO, tilt, SUB_STACK_DEG.to_radians());
            let label_w = rows.iter().map(|r| text_w(ctx, r.label())).fold(0.0, f32::max);
            let title_w = text_w(ctx, title) + 2.0 * PAD;
            let width = panel_width(label_w, &rows, ctx).max(title_w);
            let bottom = CHIP_H + 2.0 + rows.len() as f32 * (ROW_H + 1.0);
            let outline = |o: Pos2| Frame { o, ..fr }.quad(Rect::from_x_y_ranges(0.0..=width, 0.0..=bottom));
            let r = parent.row_rect(ri);
            let taken: Vec<Vec<Pos2>> = panels.iter().map(|p| p.outline()).collect();
            let clear = |o: Pos2| !taken.iter().any(|t| convex_overlap(&outline(o), t, -GAP));
            let on_screen = |o: Pos2| {
                outline(o).iter().all(|p| {
                    p.x >= screen.min.x + 4.0 && p.x <= screen.max.x - 4.0 && p.y >= screen.min.y + 4.0
                })
            };
            // LEFT first, if and only if it fits whole and clear: the panel's
            // right edge on the row's left end, its chip on the row's own
            // diagonal carried on up-left, so the rows read as one slant.
            let meet_l = parent.frame.at(0.0, r.center().y);
            let left = meet_l - fr.u * width - fr.w * (CHIP_H * 0.5);
            let on_left = on_screen(left) && clear(left);
            let mut origin = if on_left {
                left
            } else {
                // Else RIGHT: the chip starts where the row ends, then the
                // panel steps right until it clears every panel already
                // open. Only a screen edge in the way sends it down instead.
                let meet = parent.frame.at(parent.width, r.center().y);
                let mut origin = meet - fr.w * (CHIP_H * 0.5);
                let max_x = |o: Pos2| outline(o).iter().map(|p| p.x).fold(f32::MIN, f32::max);
                origin.x -= (max_x(origin) - (screen.max.x - 4.0)).max(0.0);
                for _ in 0..600 {
                    if clear(origin) {
                        break;
                    }
                    if max_x(origin + vec2(4.0, 0.0)) <= screen.max.x - 4.0 {
                        origin.x += 4.0;
                    } else {
                        origin.y += 4.0;
                    }
                }
                origin
            };
            // It eases out of the row, away from it.
            let k = (1.0 - ease(age / 0.07)) * 14.0;
            origin += fr.u * if on_left { k } else { -k };
            let mut reveal = vec![0.0; rows.len()];
            unfold(&mut reveal, age - 0.08);
            panels.push(Panel {
                frame: Frame { o: origin, ..fr },
                title: title.to_owned(),
                title_w,
                chip_h: CHIP_H,
                width,
                label_w,
                rows,
                reveal,
                pop: false,
                left: on_left,
            });
        }
        self.path.truncate(keep);
        panels
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_rows(
        &self,
        ctx: &egui::Context,
        painter: &egui::Painter,
        vis: &egui::Visuals,
        p: &Panel,
        lv: usize,
        dim: f32,
        hover: Option<(usize, usize)>,
        active: Option<usize>,
        opens_left: bool,
    ) {
        let accent = vis.selection.bg_fill;
        for (i, row) in p.rows.iter().enumerate() {
            let a = p.reveal[i];
            if a <= 0.0 {
                continue;
            }
            // Each row slides in from the left as it unfolds.
            let f = p.frame.shifted(-(1.0 - a) * 18.0);
            let r = p.row_rect(i);
            let lit = active == Some(i);
            let hot = hover == Some((lv, i)) || lit;
            let alpha = a * if lit { 1.0 } else { dim };
            let bg = if hot { vis.widgets.hovered.weak_bg_fill } else { vis.extreme_bg_color };
            painter.add(Shape::convex_polygon(f.quad(r), bg.gamma_multiply(a), Stroke::NONE));
            if lit {
                // The row a child slid out of keeps an accent edge.
                painter.line_segment(
                    [f.at(0.0, r.min.y), f.at(0.0, r.max.y)],
                    Stroke::new(3.0, accent),
                );
            }
            let ink = vis.text_color().gamma_multiply(alpha);
            let weak = vis.weak_text_color().gamma_multiply(alpha);
            let mid = r.center().y;
            let gx = PAD + GUTTER * 0.5 - 2.0;

            // The gutter says what the row does.
            match row {
                Row::Item(Item::Check { on, .. }) => {
                    let q = f.quad(Rect::from_center_size(pos2(gx, mid), vec2(11.0, 11.0)));
                    if *on {
                        painter.add(Shape::convex_polygon(q, accent.gamma_multiply(alpha), Stroke::NONE));
                        let tick = vec![f.at(gx - 3.5, mid), f.at(gx - 1.0, mid + 3.0), f.at(gx + 4.0, mid - 3.0)];
                        let c = vis.selection.stroke.color.gamma_multiply(alpha);
                        painter.add(Shape::line(tick, Stroke::new(1.6, c)));
                    } else {
                        painter.add(Shape::closed_line(q, Stroke::new(1.2, weak)));
                    }
                }
                Row::Opt { on, .. } => {
                    painter.circle_stroke(f.at(gx, mid), 4.5, Stroke::new(1.2, weak));
                    if *on {
                        painter.circle_filled(f.at(gx, mid), 2.8, accent.gamma_multiply(alpha));
                    }
                }
                Row::Item(Item::Sub { .. } | Item::Choice { .. }) => {
                    // A chevron pointing the side the child opened on.
                    let dir = if lit && opens_left { -f.u } else { f.u };
                    let n = vec2(-dir.y, dir.x);
                    let c = f.at(gx, mid);
                    let col = if lit { accent } else { weak };
                    painter.add(Shape::line(
                        vec![c - dir * 2.0 + n * 3.5, c + dir * 2.0, c - dir * 2.0 - n * 3.5],
                        Stroke::new(1.5, col),
                    ));
                }
                Row::Item(Item::Action { .. }) => {
                    painter.circle_filled(f.at(gx, mid), 1.8, weak);
                }
                Row::Item(Item::Slider { .. }) => {}
            }

            text(ctx, painter, f.at(PAD + GUTTER, mid), f.angle, row.label(), ink);

            match row {
                Row::Item(Item::Slider { value, min, max, step, unit, .. }) => {
                    let (x0, x1) = p.track();
                    let k = ((value - min) / (max - min)).clamp(0.0, 1.0);
                    let xk = x0 + (x1 - x0) * k;
                    let rail = vis.widgets.inactive.bg_fill.gamma_multiply(alpha);
                    painter.line_segment([f.at(x0, mid), f.at(x1, mid)], Stroke::new(3.0, rail));
                    painter.line_segment([f.at(x0, mid), f.at(xk, mid)], Stroke::new(3.0, accent.gamma_multiply(alpha)));
                    let big = self.drag == Some((lv, i)) || hover == Some((lv, i));
                    let knob = vis.strong_text_color().gamma_multiply(alpha);
                    painter.circle_filled(f.at(xk, mid), if big { 5.5 } else { 4.5 }, knob);
                    text(ctx, painter, f.at(x1 + PAD, mid), f.angle, &fmt_value(*value, *step, unit), ink);
                }
                Row::Item(Item::Choice { options, selected, .. }) => {
                    let s = selected.and_then(|i| options.get(i)).map_or("—", String::as_str);
                    text(ctx, painter, f.at(PAD + GUTTER + p.label_w + PAD, mid), f.angle, s, weak);
                }
                _ => {}
            }
        }
    }
}

/// The top panel, swung `t` of the way (0 = resting in the bar).
fn top_panel<'a>(ctx: &egui::Context, m: &'a Menu, rest: Rect, t: f32) -> Panel<'a> {
    let rows: Vec<Row> = m.items.iter().map(Row::Item).collect();
    // Type the full name out as it swings, from the short name's length.
    let (ns, nf) = (m.short.chars().count(), m.full.chars().count());
    let n = (ns as f32 + (nf as f32 - ns as f32).max(0.0) * t).round() as usize;
    let title: String = m.full.chars().take(n.max(1)).collect();
    let short_w = text_w(ctx, &m.short) + 2.0 * PAD;
    let full_w = text_w(ctx, &m.full) + 2.0 * PAD + POP_W;
    let title_w = short_w + (full_w - short_w) * t;
    let label_w = rows.iter().map(|r| text_w(ctx, r.label())).fold(0.0, f32::max);
    let width = panel_width(label_w, &rows, ctx).max(full_w);
    let chip_h = rest.height();
    Panel {
        // Pivot on the chip's left edge, halfway down.
        frame: Frame {
            o: rest.left_top(),
            ..Frame::new(Pos2::ZERO, TILT_DEG.to_radians() * t, TOP_STACK_DEG.to_radians())
        },
        title,
        title_w,
        chip_h,
        width,
        label_w,
        reveal: vec![0.0; rows.len()],
        rows,
        pop: true,
        left: false,
    }
}

fn paint_chip(ctx: &egui::Context, painter: &egui::Painter, vis: &egui::Visuals, p: &Panel, t: f32, floating: bool) {
    let fill = lerp_color(vis.widgets.inactive.weak_bg_fill, vis.selection.bg_fill, t);
    let r = p.chip_rect();
    painter.add(Shape::convex_polygon(p.frame.quad(r), fill, Stroke::NONE));
    let ink = lerp_color(vis.widgets.inactive.fg_stroke.color, vis.selection.stroke.color, t);
    text(ctx, painter, p.frame.at(PAD, r.center().y), p.frame.angle, &p.title, ink);
    if p.pop && t > 0.6 {
        // Pop-out: a box with an arrow leaving its corner.
        let a = ((t - 0.6) / 0.4).clamp(0.0, 1.0);
        let c = p.pop_rect().center() - vec2(2.0, 0.0);
        let s = 4.0;
        let col = if floating { vis.strong_text_color() } else { ink.gamma_multiply(0.8) };
        let st = Stroke::new(1.3, col.gamma_multiply(a));
        let f = &p.frame;
        let bx = vec![
            f.at(c.x - s, c.y - s + 2.0),
            f.at(c.x + s - 2.0, c.y - s + 2.0),
            f.at(c.x + s - 2.0, c.y + s),
            f.at(c.x - s, c.y + s),
        ];
        painter.add(Shape::closed_line(bx, st));
        let tip = f.at(c.x + s + 1.0, c.y - s - 1.0);
        painter.line_segment([f.at(c.x, c.y), tip], st);
        painter.line_segment([tip, f.at(c.x + 1.0, c.y - s - 1.0)], st);
        painter.line_segment([tip, f.at(c.x + s + 1.0, c.y - 1.0)], st);
    }
}

/// Do two convex polygons come within `gap` of each other? Separating
/// axes over both polygons' edge normals.
fn convex_overlap(a: &[Pos2], b: &[Pos2], gap: f32) -> bool {
    for poly in [a, b] {
        for i in 0..poly.len() {
            let e = poly[(i + 1) % poly.len()] - poly[i];
            let n = vec2(-e.y, e.x).normalized();
            let span = |q: &[Pos2]| {
                q.iter().map(|p| p.to_vec2().dot(n)).fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)))
            };
            let (a0, a1) = span(a);
            let (b0, b1) = span(b);
            if a1 + gap <= b0 || b1 + gap <= a0 {
                return false;
            }
        }
    }
    true
}

/// Rows unfold one after another, `age` seconds into the panel's life.
fn unfold(reveal: &mut [f32], age: f32) {
    for (i, r) in reveal.iter_mut().enumerate() {
        *r = ease((age - i as f32 * 0.0125) / 0.06);
    }
}

fn panel_width(label_w: f32, rows: &[Row], ctx: &egui::Context) -> f32 {
    let right = rows
        .iter()
        .map(|r| match r {
            Row::Item(Item::Slider { .. }) => TRACK_W + PAD + VALUE_W,
            Row::Item(Item::Choice { options, .. }) => {
                options.iter().map(|o| text_w(ctx, o)).fold(0.0, f32::max)
            }
            _ => 0.0,
        })
        .fold(0.0, f32::max);
    PAD + GUTTER + label_w + if right > 0.0 { PAD + right } else { 0.0 } + PAD
}

fn slider_value(p: &Panel, ptr: Pos2, min: f32, max: f32, step: f32) -> f32 {
    let (x0, x1) = p.track();
    let k = ((p.frame.local(ptr).x - x0) / (x1 - x0)).clamp(0.0, 1.0);
    let v = min + (max - min) * k;
    if step > 0.0 {
        ((v / step).round() * step).clamp(min, max)
    } else {
        v
    }
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_premultiplied(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()), l(a.a(), b.a()))
}

/// The same items, straight, for a floating window.
fn flat(ui: &mut egui::Ui, items: &[Item], ev: &mut Vec<Event>) {
    for it in items {
        match it {
            Item::Check { id, label, on } => {
                let mut v = *on;
                if ui.checkbox(&mut v, label).changed() {
                    ev.push(Event::Toggle(id, v));
                }
            }
            Item::Slider { id, label, value, min, max, step, unit } => {
                let mut v = *value;
                let mut s = egui::Slider::new(&mut v, *min..=*max).text(label).suffix(*unit);
                if *step > 0.0 {
                    s = s.step_by(*step as f64);
                }
                if ui.add(s).changed() {
                    ev.push(Event::Slide(id, v));
                }
            }
            Item::Choice { id, label, options, selected } => {
                let shown = selected.and_then(|i| options.get(i)).map_or("—", String::as_str);
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt(("tilt-flat", *id))
                        .selected_text(shown)
                        .show_ui(ui, |ui| {
                            for (i, o) in options.iter().enumerate() {
                                if ui.add(egui::Button::selectable(*selected == Some(i), o)).clicked() {
                                    ev.push(Event::Pick(id, i));
                                }
                            }
                        });
                    ui.label(label.as_str());
                });
            }
            Item::Sub { label, items } => {
                ui.collapsing(label.as_str(), |ui| flat(ui, items, ev));
            }
            Item::Action { id, label } => {
                if ui.button(label.as_str()).clicked() {
                    ev.push(Event::Action(id));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sheared_frame_round_trips() {
        for stack in [TOP_STACK_DEG, SUB_STACK_DEG] {
            let f = Frame::new(pos2(40.0, 10.0), TILT_DEG.to_radians(), stack.to_radians());
            let l = f.local(f.at(30.0, 12.0));
            assert!((l.x - 30.0).abs() < 1e-3 && (l.y - 12.0).abs() < 1e-3, "{l:?}");
            // A unit of y is a pixel square to the text.
            let step = f.at(0.0, 10.0) - f.at(0.0, 0.0);
            assert!((step.dot(f.normal()) - 10.0).abs() < 1e-3);
        }
    }

    /// Every panel hangs plumb: its left and right edges straight down, so a
    /// child's edges line up with its parent's.
    #[test]
    fn panels_hang_and_lean() {
        let t = TILT_DEG.to_radians();
        let top = Frame::new(Pos2::ZERO, t, TOP_STACK_DEG.to_radians());
        let d0 = top.at(0.0, 100.0) - top.at(0.0, 0.0);
        assert!(d0.x.abs() < 1e-3 && d0.y > 0.0, "{d0:?}");
        let child = Frame::new(Pos2::ZERO, t, SUB_STACK_DEG.to_radians());
        let d1 = child.at(0.0, 100.0) - child.at(0.0, 0.0);
        assert!(d1.x.abs() < 1e-3 && d1.y > 0.0, "{d1:?}");
    }

    fn panel(items: &[Item]) -> Panel<'_> {
        Panel {
            frame: Frame::new(pos2(100.0, 20.0), TILT_DEG.to_radians(), TOP_STACK_DEG.to_radians()),
            title: "x".into(),
            title_w: 60.0,
            chip_h: CHIP_H,
            width: 300.0,
            label_w: 50.0,
            reveal: vec![1.0; items.len()],
            rows: items.iter().map(Row::Item).collect(),
                pop: true,
            left: false,
        }
    }

    /// A slider reads the pointer's place on its TILTED track, absolutely:
    /// the same point gives the same value whatever came before.
    #[test]
    fn a_slider_reads_the_tilted_track() {
        let items = [Item::slider("s", "gain", 0.0, 0.0, 10.0)];
        let p = panel(&items);
        let (x0, x1) = p.track();
        let mid = p.row_rect(0).center().y;
        let at = |k: f32| p.frame.at(x0 + (x1 - x0) * k, mid);
        assert!((slider_value(&p, at(0.5), 0.0, 10.0, 0.0) - 5.0).abs() < 1e-3);
        assert_eq!(slider_value(&p, at(0.26), 0.0, 10.0, 1.0), 3.0);
        assert_eq!(slider_value(&p, at(2.0), 0.0, 10.0, 0.0), 10.0);
        // And the row is hit where it is drawn, not where it would be flat.
        assert_eq!(p.hit(at(0.5)), Some(Hit::Row(0)));
        let flat_mid = pos2(100.0 + (x0 + x1) * 0.5, 20.0 + mid);
        assert_eq!(p.hit(flat_mid), None);
        // The chip's far end is the pop-out.
        let pr = p.pop_rect().center();
        assert_eq!(p.hit(p.frame.at(pr.x, pr.y)), Some(Hit::Pop));
    }

    #[test]
    fn overlap_is_separating_axes() {
        let sq = |x: f32, y: f32| vec![pos2(x, y), pos2(x + 10.0, y), pos2(x + 10.0, y + 10.0), pos2(x, y + 10.0)];
        assert!(convex_overlap(&sq(0.0, 0.0), &sq(5.0, 5.0), 0.0));
        assert!(!convex_overlap(&sq(0.0, 0.0), &sq(20.0, 0.0), 6.0));
        assert!(convex_overlap(&sq(0.0, 0.0), &sq(14.0, 0.0), 6.0), "inside the gap");
    }

    fn demo() -> Vec<Menu> {
        vec![
            Menu {
                id: "prefs",
                short: "prefs".into(),
                full: "preferences".into(),
                items: vec![
                    Item::check("links", "value links over the graph", true),
                    Item::check("rgb", "collapse rgb triples", false),
                    Item::slider("magnet", "magnet reach", 60.0, 0.0, 200.0).step(1.0).unit("u"),
                    Item::sub(
                        "display",
                        vec![
                            Item::choice(
                                "view",
                                "transform",
                                vec!["ACES 1.3 · sRGB".into(), "AgX · sRGB".into(), "Standard · sRGB".into()],
                                Some(1),
                            ),
                            Item::slider("exp", "exposure", 0.5, -6.0, 6.0).unit(" st"),
                            Item::slider("gamma", "gamma", 1.0, 0.2, 4.0),
                        ],
                    ),
                    Item::action("keys", "keys…"),
                    Item::action("colours", "colours…"),
                ],
            },
            Menu { id: "globals", short: "globals".into(), full: "global settings".into(), items: vec![] },
            Menu { id: "colour", short: "colour".into(), full: "colour management".into(), items: vec![] },
        ]
    }

    /// Look at it: `cargo test -p app tiltmenu_render -- --ignored`, then
    /// `target/tiltmenu.png` — prefs swung open, display slid out, and its
    /// transform choice slid out of that.
    #[test]
    #[ignore]
    fn tiltmenu_render() {
        let menus = demo();
        let mut bar = TiltBar::default();
        crate::uishot::render("tiltmenu", 1200.0, 560.0, 2.0, 170, |ui, frame| {
            let at0 = egui::Panel::top("bar")
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.strong("untitled");
                        for b in ["open", "save", "save as"] {
                            let _ = ui.button(b);
                        }
                        // Room on the left, so a child can open there.
                        ui.add_space(460.0);
                        let before = ui.cursor().min;
                        bar.bar(ui, &menus);
                        before
                    })
                    .inner
                })
                .inner
                + vec2(20.0, 10.0);
            // Hover the chip, then walk to "display", then to its
            // "transform" choice, the way a hand would.
            let row = |lv: usize, i: usize| bar.test_rows.get(lv).and_then(|r| r.get(i)).copied();
            let pos = match frame {
                0..40 => Some(at0),
                40..100 => row(0, 3),
                _ => row(1, 0),
            };
            if pos.is_some() {
                bar.test_ptr = pos;
            }
        });
    }
}
