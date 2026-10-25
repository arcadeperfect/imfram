//! One tool window's frame: a bar along its top (the app's buttons, and
//! "panels": every tab, open or closed, and where), then its tree of
//! panels, drawn at rects that ANIMATE: while a tab is dragged, every
//! leaf glides to where it would sit if the tab were let go here, so the
//! layout you'd get is the one you see before you commit to it. Move on
//! and they glide back.
//!
//! Dragging a tab: press its button and move. Within the window the
//! leaf under the pointer takes it (its middle: as a tab; a side: a new
//! leaf split off that side; the window's edge: split off the whole
//! window). Past the window's edge the drag is the dock's (`Docks::track`):
//! another window takes it the same way, from its own frame, with the
//! pointer handed in; the desktop makes it a window of its own.

use std::collections::HashMap;

use egui::{pos2, vec2, Align2, Color32, FontId, Id, Pos2, Rect, Sense, Stroke, Ui};

use super::tree::{Dir, Drop, LeafId, Side, Tree, Zone};
use super::{Ask, Tab};
use crate::shell::UiApp;

/// What the app draws in a window: each tab, and anything round the dock.
pub trait Panels<T: Tab> {
    /// Before egui sees the frame's input: claim or rewrite events (a
    /// canvas' chords). `ctx` is the window's; `tabs` are its tabs.
    fn raw_input(&mut self, _ctx: &egui::Context, _raw: &mut egui::RawInput, _tabs: &[T]) {}
    /// Before the top bar (an editor's frame begins).
    fn begin(&mut self, _ui: &mut Ui) {}
    /// Items in the top bar, left of the "panels" menu.
    fn bar(&mut self, _ui: &mut Ui) {}
    /// Whether the top bar (the app's items and the "panels" menu) is
    /// drawn at all. An app with its own menubar says no.
    fn bar_shown(&self) -> bool {
        true
    }
    /// Along the window's bottom, before the dock takes the rest (a
    /// status bar: `egui::Panel::bottom`).
    fn foot(&mut self, _ui: &mut Ui) {}
    /// One tab's body.
    fn tab(&mut self, ui: &mut Ui, tab: T);
    /// After the dock (an editor's frame ends; floating windows).
    fn end(&mut self, _ui: &mut Ui) {}
    /// Whether a tab's body is left unpainted, so what the app drew into
    /// the window before egui (a picture blitted straight to the surface)
    /// shows through. The tab bar and the outline are still drawn.
    fn see_through(&self, _tab: &T) -> bool {
        false
    }
    /// Whether a tab's body scrolls (a canvas says no).
    fn scroll_bars(&self, _tab: &T) -> [bool; 2] {
        [false, false]
    }
    /// Whether each leaf spends a row on its tab bar. Say no and the bar
    /// becomes a small patch ([`grip`]) the tab draws into its own first
    /// row of controls; a tab that never calls [`grip`] gets the patch
    /// floating at its top right. It drags, switches a stack and has the
    /// tab's menu, as the bar did.
    fn tab_bars(&self) -> bool {
        true
    }
    /// Every tab of the window in its top bar, right of "panels", always
    /// in the same place; the leaves get no bar and no patch. A tab's chip
    /// drags it, a click shows it in its leaf, hover lights its leaf.
    /// Needs the top bar (`bar_shown`); takes over from `tab_bars`.
    fn tabs_in_bar(&self) -> bool {
        false
    }
}

/// What a compact leaf hands its tab to size the patch (`grip`): the
/// stack's titles. Set only while a compact leaf draws its tab.
#[derive(Clone)]
struct GripSpec(Vec<&'static str>);

fn grip_spec_id() -> Id {
    Id::new("dock grip spec")
}
fn grip_rect_id() -> Id {
    Id::new("dock grip rect")
}

const GRIP_W: f32 = 12.0;
const CHIP_PAD: f32 = 10.0;

fn chip_font() -> FontId {
    FontId::proportional(11.0)
}

/// The patch that stands for a leaf's tab bar when the app turns the bars
/// off (`Panels::tab_bars`): call it first in a tab's top row of
/// controls. It reserves room; the dock draws it and takes its drags and
/// clicks. Outside a compact dock it does nothing, so a tab can call it
/// whatever the window.
pub fn grip(ui: &mut Ui) {
    let Some(GripSpec(titles)) = ui.ctx().data(|d| d.get_temp::<GripSpec>(grip_spec_id())) else { return };
    let h = ui.spacing().interact_size.y;
    let w = grip_width(ui, &titles);
    let (r, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
    ui.ctx().data_mut(|d| d.insert_temp(grip_rect_id(), r));
}

fn grip_width(ui: &Ui, titles: &[&'static str]) -> f32 {
    if titles.len() < 2 {
        return GRIP_W;
    }
    let chips: f32 = titles.iter().map(|t| ui.painter().layout_no_wrap(t.to_string(), chip_font(), Color32::WHITE).size().x + CHIP_PAD + 2.0).sum();
    GRIP_W + chips
}

/// The leaves' rects as drawn: on their way to where the tree puts them;
/// and the tab this window has in a drag.
pub struct Anim<T> {
    rects: HashMap<LeafId, Rect>,
    drag: Option<T>,
    /// A leaf shown alone (the rest folded away).
    pub solo: Option<LeafId>,
}

impl<T> Default for Anim<T> {
    fn default() -> Self {
        Self { rects: HashMap::new(), drag: None, solo: None }
    }
}

pub const BAR_H: f32 = 24.0;
pub const GAP: f32 = 4.0;
/// Within this of a leaf's edge the drop splits off that side.
const SIDE_FRAC: f32 = 0.25;
/// Within this of the window's edge the drop splits the whole window.
const EDGE_PX: f32 = 18.0;
/// How fast the leaves glide (per second).
const GLIDE: f32 = 14.0;

pub struct DockApp<'b, T: Tab, P: Panels<T>> {
    pub tree: &'b mut Tree<T>,
    pub anim: &'b mut Anim<T>,
    /// Its place among the windows, and the others (for "move to").
    pub here: usize,
    pub others: Vec<(usize, String)>,
    /// Which tabs are open in any window.
    pub open: Vec<T>,
    pub asks: &'b mut Vec<Ask<T>>,
    pub panels: &'b mut P,
    /// A tab this window has in a drag this frame (read by the dock after).
    pub dragged: Option<T>,
    /// A tab dragged from another window, over this one, the pointer in
    /// this window's points: shown landing here.
    pub dropping: Option<(T, Pos2)>,
    /// Where `dropping` would land (read by the dock after).
    pub drop_here: Option<Drop>,
    /// The leaf whose tab is hovered in the top bar (lit as it is drawn).
    pub lit: Option<LeafId>,
}

impl<T: Tab, P: Panels<T>> UiApp for DockApp<'_, T, P> {
    fn raw_input(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        let tabs = self.tree.tabs();
        self.panels.raw_input(ctx, raw, &tabs);
    }

    fn ui(&mut self, ui: &mut Ui) {
        self.panels.begin(ui);
        if self.panels.bar_shown() {
            egui::Panel::top(Id::new(("dock-bar", self.here))).show(ui, |ui| {
                ui.horizontal(|ui| {
                    self.panels.bar(ui);
                    ui.menu_button("panels", |ui| self.panels_menu(ui)).response.on_hover_text(
                        "every panel: open or close it; drag a tab to move it, right-click it to detach it or move it to another window",
                    );
                    if self.panels.tabs_in_bar() {
                        self.bar_tabs(ui);
                    }
                });
            });
        }
        self.panels.foot(ui);
        let area = ui.available_rect_before_wrap();
        self.dock(ui, area);
        self.panels.end(ui);
    }
}

impl<T: Tab, P: Panels<T>> DockApp<'_, T, P> {
    /// The window's tabs along the top bar (`Panels::tabs_in_bar`): leaf by
    /// leaf, a gap between leaves; each shown one lit.
    fn bar_tabs(&mut self, ui: &mut Ui) {
        let drag = self.anim.drag;
        let mut lit = None;
        let mut set_active = None;
        let leaves: Vec<(LeafId, Vec<T>, usize)> = self.tree.leaves().into_iter().map(|(l, t, a)| (l, t.to_vec(), a)).collect();
        for (n, (leaf, tabs, active)) in leaves.into_iter().enumerate() {
            if n > 0 {
                ui.add_space(10.0);
            }
            for (i, tab) in tabs.iter().copied().enumerate() {
                let on = i == active;
                let text = egui::RichText::new(tab.title());
                let text = if drag == Some(tab) { text.weak() } else { text };
                let resp = ui.add(egui::Button::selectable(on, text).sense(Sense::click_and_drag()));
                ui.ctx().data_mut(|d| d.insert_temp(super::viewer::tab_rect_id(tab), resp.rect));
                if resp.hovered() || resp.dragged() {
                    lit = Some(leaf);
                }
                if self.tab_handle(resp, tab) {
                    set_active = Some(tab);
                }
            }
        }
        if let Some(t) = set_active {
            self.tree.set_active(t);
        }
        self.lit = lit;
    }

    /// The "panels" menu: each tab, ticked if open; ticking opens it (in
    /// its home window), unticking closes it.
    fn panels_menu(&mut self, ui: &mut Ui) {
        for &tab in T::all() {
            let mut open = self.open.contains(&tab);
            let tip = match tab.key() {
                "" => tab.help().to_string(),
                k => format!("{} ({k})", tab.help()),
            };
            let r = ui.add_enabled(!tab.pinned(), egui::Checkbox::new(&mut open, tab.title())).on_hover_text(tip);
            if r.changed() {
                self.asks.push(if open { Ask::Open(tab) } else { Ask::Close(tab) });
            }
        }
    }

    fn dock(&mut self, ui: &mut Ui, area: Rect) {
        let ctx = ui.ctx().clone();
        let (ptr, down) = ctx.input(|i| (i.pointer.latest_pos(), i.pointer.primary_down()));

        // A drag of our own, or one handed in from another window.
        let mut mine: Option<T> = self.anim.drag;
        let (drag, at) = match (mine, self.dropping) {
            (Some(t), _) => (Some(t), ptr),
            (None, Some((t, p))) => (Some(t), Some(p)),
            _ => (None, None),
        };
        let bar_h = if self.panels.tab_bars() && !(self.panels.tabs_in_bar() && self.panels.bar_shown()) { BAR_H } else { 0.0 };
        let strip_h = |tabs: &[T]| strip_height(tabs, bar_h);
        let real = self.tree.layout_fixed(area, GAP, &strip_h);
        let inside = at.is_some_and(|p| area.contains(p));
        let drop = match (drag, at) {
            (Some(tab), Some(p)) if inside => drop_at(&real, area, p, self.tree.find(tab)),
            _ => None,
        };
        let proposal = match (drag, drop) {
            (Some(tab), Some(d)) => self.tree.proposed(tab, d),
            _ => None,
        };
        self.drop_here = if self.dropping.is_some() { drop.filter(|_| proposal.is_some()) } else { None };

        // Our drag let go: inside, it lands here; outside, the dock has it.
        if let Some(tab) = mine {
            if !down {
                self.anim.drag = None;
                mine = None;
                if inside {
                    if let Some(d) = drop {
                        if proposal.is_some() {
                            self.tree.remove(tab);
                            self.tree.insert(tab, d);
                        }
                    }
                }
            }
        }
        self.dragged = mine;

        // Where every leaf is heading, and the glide toward it. A leaf
        // shown alone takes the whole area; the rest fold away.
        if self.anim.solo.is_some_and(|l| !self.tree.has_leaf(l)) {
            self.anim.solo = None;
        }
        let solo = self.anim.solo.filter(|_| drag.is_none());
        let mut target = match &proposal {
            Some(p) => p.layout_fixed(area, GAP, &strip_h),
            None => self.tree.layout_fixed(area, GAP, &strip_h),
        };
        if let Some(l) = solo {
            target.seps.clear();
            target.leaves = vec![(l, area)];
        }
        let dt = ctx.input(|i| i.stable_dt).min(0.1);
        let k = 1.0 - (-GLIDE * dt).exp();
        let mut moving = false;
        let mut targets: HashMap<LeafId, Rect> = target.leaves.iter().copied().collect();
        // A leaf the proposal does without (the dragged tab was alone in
        // it) folds to nothing where it was.
        for (id, r) in &real.leaves {
            targets.entry(*id).or_insert(Rect::from_center_size(r.center(), vec2(0.0, 0.0)));
        }
        for (id, want) in &targets {
            let from = self.anim.rects.get(id).copied().unwrap_or_else(|| {
                // New: a ghost grows from the pointer, a first leaf is just there.
                match at {
                    Some(p) if proposal.is_some() => Rect::from_center_size(p, vec2(0.0, 0.0)),
                    _ => *want,
                }
            });
            let to = Rect::from_min_max(from.min + (want.min - from.min) * k, from.max + (want.max - from.max) * k);
            let done = (to.min - want.min).length() < 0.3 && (to.max - want.max).length() < 0.3;
            let to = if done { *want } else { to };
            moving |= !done;
            self.anim.rects.insert(*id, to);
        }
        self.anim.rects.retain(|id, _| targets.contains_key(id));
        if moving {
            ctx.request_repaint();
        }

        // Separators (from the tree as it is: not while a drag reshapes it,
        // nor while a leaf is alone).
        if drag.is_none() && solo.is_none() {
            for (path, band, whole, dir) in &real.seps {
                self.separator(ui, path, *band, *whole, *dir);
            }
        }

        // The leaves, where they are now.
        let ghost = proposal.as_ref().map(|_| self.tree.next_id());
        let drawn: Vec<(LeafId, Rect)> = self.anim.rects.iter().map(|(id, r)| (*id, *r)).collect();
        for (id, rect) in drawn {
            if rect.width() < 4.0 || rect.height() < 4.0 {
                continue;
            }
            if Some(id) == ghost {
                if let Some(tab) = drag {
                    ghost_leaf(ui, rect, tab.title());
                }
                continue;
            }
            // A centre drop shows as a ghost tab on that leaf's bar.
            let ghost_tab = match (drop, drag) {
                (Some(Drop::Leaf(l, Zone::Center)), Some(t)) if l == id && proposal.is_some() => Some(t),
                _ => None,
            };
            self.leaf(ui, id, rect, drag, ghost_tab);
        }

        if self.tree.is_empty() && drag.is_none() {
            ui.painter().text(area.center(), Align2::CENTER_CENTER, "nothing open: see panels", FontId::proportional(14.0), ui.visuals().weak_text_color());
        }

        // The tab in hand, riding the pointer.
        if let (Some(tab), Some(p)) = (mine, ptr) {
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, Id::new("dock drag label")));
            let font = FontId::proportional(13.0);
            let galley = painter.layout_no_wrap(tab.title().to_string(), font, ui.visuals().strong_text_color());
            let size = galley.size() + vec2(12.0, 6.0);
            // Beside the pointer, kept on screen (to its left near the right edge).
            let mut min = p + vec2(10.0, 10.0);
            if min.x + size.x > area.max.x {
                min.x = p.x - 10.0 - size.x;
            }
            if min.y + size.y > area.max.y {
                min.y = p.y - 10.0 - size.y;
            }
            let r = Rect::from_min_size(min, size);
            painter.rect_filled(r, 4.0, ui.visuals().widgets.active.bg_fill);
            painter.rect_stroke(r, 4.0, ui.visuals().widgets.active.fg_stroke, egui::StrokeKind::Inside);
            painter.galley(r.min + vec2(6.0, 3.0), galley, ui.visuals().strong_text_color());
            ctx.request_repaint();
        }
    }

    fn separator(&mut self, ui: &mut Ui, path: &[bool], band: Rect, whole: Rect, dir: Dir) {
        let id = Id::new(("dock sep", self.here, path));
        let hit = band.expand(2.0);
        let r = ui.interact(hit, id, Sense::drag());
        let cursor = match dir {
            Dir::Row => egui::CursorIcon::ResizeHorizontal,
            Dir::Col => egui::CursorIcon::ResizeVertical,
        };
        if r.hovered() || r.dragged() {
            ui.ctx().set_cursor_icon(cursor);
        }
        if r.dragged() {
            if let Some(p) = r.interact_pointer_pos() {
                let frac = match dir {
                    Dir::Row => (p.x - whole.min.x) / (whole.width() - GAP),
                    Dir::Col => (p.y - whole.min.y) / (whole.height() - GAP),
                };
                self.tree.set_frac(path, frac);
            }
        }
        let c = if r.hovered() || r.dragged() { ui.visuals().widgets.hovered.bg_stroke.color } else { ui.visuals().widgets.noninteractive.bg_stroke.color };
        ui.painter().rect_filled(band, 0.0, c);
    }

    /// One leaf: its tab bar (or, compact, its patch) and the active
    /// tab's body, in `rect`.
    fn leaf(&mut self, ui: &mut Ui, id: LeafId, rect: Rect, drag: Option<T>, ghost_tab: Option<T>) {
        let Some((_, tabs, active)) = self.tree.leaves().into_iter().find(|(l, ..)| *l == id) else { return };
        let tabs: Vec<T> = tabs.to_vec();
        let in_bar = self.panels.tabs_in_bar() && self.panels.bar_shown();
        let bars = self.panels.tab_bars() && !in_bar;
        let bar_h = if bars { BAR_H } else { 0.0 };
        let (bar, body) = (Rect::from_min_size(rect.min, vec2(rect.width(), bar_h)), Rect::from_min_max(pos2(rect.min.x, rect.min.y + bar_h), rect.max));
        let vis = ui.visuals().clone();
        let p = ui.painter_at(rect);
        let clear = tabs.get(active).is_some_and(|t| self.panels.see_through(t));
        if !clear {
            p.rect_filled(rect, 4.0, vis.window_fill);
        }
        if bars {
            p.rect_filled(bar, egui::CornerRadius { nw: 4, ne: 4, sw: 0, se: 0 }, vis.faint_bg_color);
        }
        // (A see-through body is a picture: it is its own edge.)
        if !clear {
            p.rect_stroke(rect, 4.0, vis.window_stroke, egui::StrokeKind::Inside);
        }

        let mut set_active = None;
        if bars {
            // The tabs.
            let font = FontId::proportional(13.0);
            let mut x = bar.min.x + 4.0;
            let all: Vec<(T, bool)> = tabs.iter().map(|t| (*t, false)).chain(ghost_tab.map(|t| (t, true))).collect();
            for (i, (tab, ghost)) in all.iter().enumerate() {
                let galley = p.layout_no_wrap(tab.title().to_string(), font.clone(), vis.text_color());
                let w = galley.size().x + 26.0;
                let r = Rect::from_min_size(pos2(x, bar.min.y + 2.0), vec2(w, BAR_H - 4.0));
                x += w + 2.0;
                if x > bar.max.x + w {
                    break;
                }
                let is_active = i == active && !ghost;
                let in_hand = drag == Some(*tab) && !ghost;
                if *ghost {
                    p.rect_stroke(r, 3.0, Stroke::new(1.0, vis.selection.stroke.color), egui::StrokeKind::Inside);
                    p.galley(r.min + vec2(6.0, 3.0), galley, vis.selection.stroke.color);
                    continue;
                }
                let resp = ui.interact(r, Id::new(("dock tab", self.here, *tab)), Sense::click_and_drag());
                ui.ctx().data_mut(|d| d.insert_temp(super::viewer::tab_rect_id(*tab), r));
                let fill = if is_active { vis.widgets.active.bg_fill } else if resp.hovered() { vis.widgets.hovered.bg_fill } else { Color32::TRANSPARENT };
                p.rect_filled(r, 3.0, fill);
                let tc = if in_hand { vis.weak_text_color() } else if is_active { vis.strong_text_color() } else { vis.text_color() };
                p.galley(r.min + vec2(6.0, 3.0), galley, tc);
                // The close cross (none on a pinned tab).
                if !tab.pinned() {
                    let cx = Rect::from_center_size(pos2(r.max.x - 10.0, r.center().y), vec2(12.0, 12.0));
                    let close = ui.interact(cx, Id::new(("dock close", self.here, *tab)), Sense::click());
                    let xc = if close.hovered() { vis.strong_text_color() } else { vis.weak_text_color() };
                    p.line_segment([cx.min + vec2(3.0, 3.0), cx.max - vec2(3.0, 3.0)], Stroke::new(1.2, xc));
                    p.line_segment([pos2(cx.max.x - 3.0, cx.min.y + 3.0), pos2(cx.min.x + 3.0, cx.max.y - 3.0)], Stroke::new(1.2, xc));
                    if close.clicked() {
                        self.asks.push(Ask::Close(*tab));
                    }
                }
                if self.tab_handle(resp, *tab) {
                    set_active = Some(*tab);
                }
            }
        } else if let Some(t) = ghost_tab {
            // A centre drop, with no bar to show a ghost tab on: the leaf lights.
            let c = vis.selection.stroke.color;
            p.rect_filled(rect, 4.0, c.gamma_multiply(0.10));
            p.rect_stroke(rect, 4.0, Stroke::new(1.5, c), egui::StrokeKind::Inside);
            p.text(rect.center(), Align2::CENTER_CENTER, t.title(), FontId::proportional(13.0), c);
        }

        // The body.
        let Some(tab) = tabs.get(active).copied() else { return };
        // (A see-through body takes the leaf to its edges: the picture
        // behind it is drawn there.)
        let inner = if clear { body } else { body.shrink2(vec2(4.0, 2.0)) };
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner).id_salt(("dock body", id)));
        child.set_clip_rect(inner);
        if !bars && !in_bar {
            let titles: Vec<&'static str> = tabs.iter().map(|t| t.title()).collect();
            ui.ctx().data_mut(|d| {
                d.insert_temp(grip_spec_id(), GripSpec(titles));
                d.remove::<Rect>(grip_rect_id());
            });
        }
        let scroll = self.panels.scroll_bars(&tab);
        if scroll[0] || scroll[1] {
            egui::ScrollArea::new(scroll).id_salt(("dock scroll", id)).auto_shrink([false, false]).show(&mut child, |ui| {
                self.panels.tab(ui, tab);
            });
        } else {
            self.panels.tab(&mut child, tab);
        }
        if self.lit == Some(id) {
            let c = vis.selection.stroke.color;
            ui.painter().rect_stroke(rect, 4.0, Stroke::new(1.5, c), egui::StrokeKind::Inside);
        }
        if !bars && !in_bar {
            let placed = ui.ctx().data_mut(|d| {
                d.remove::<GripSpec>(grip_spec_id());
                let r = d.get_temp::<Rect>(grip_rect_id());
                d.remove::<Rect>(grip_rect_id());
                r
            });
            // The tab drew no row for it: the patch floats at its top right.
            let r = placed.unwrap_or_else(|| {
                let w = grip_width(ui, &tabs.iter().map(|t| t.title()).collect::<Vec<_>>());
                Rect::from_min_size(pos2(inner.max.x - w - 2.0, inner.min.y + 2.0), vec2(w, ui.spacing().interact_size.y))
            });
            if let Some(t) = self.patch(ui, r.intersect(inner), &tabs, active, drag, placed.is_none()) {
                set_active = Some(t);
            }
        }
        if let Some(t) = set_active {
            self.tree.set_active(t);
        }
    }

    /// A compact leaf's patch at `r`: a grip (drags the tab on show) and,
    /// for a stack, a chip per tab. Returns a chip clicked.
    fn patch(&mut self, ui: &mut Ui, r: Rect, tabs: &[T], active: usize, drag: Option<T>, floating: bool) -> Option<T> {
        if r.width() < 2.0 || r.height() < 2.0 {
            return None;
        }
        let vis = ui.visuals().clone();
        let p = ui.painter_at(r.expand(1.0));
        if floating {
            p.rect_filled(r, 3.0, vis.faint_bg_color);
        }
        let mut clicked = None;
        let g = Rect::from_min_size(r.min, vec2(GRIP_W, r.height()));
        if let Some(&shown) = tabs.get(active) {
            let resp = ui.interact(g, Id::new(("dock tab", self.here, shown)), Sense::click_and_drag());
            if tabs.len() == 1 {
                ui.ctx().data_mut(|d| d.insert_temp(super::viewer::tab_rect_id(shown), g));
            }
            if resp.hovered() || resp.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                p.rect_filled(g, 3.0, vis.widgets.hovered.bg_fill);
            }
            // Six dots: a thing to take hold of.
            let c = if drag == Some(shown) { vis.strong_text_color() } else if resp.hovered() { vis.text_color() } else { vis.weak_text_color() };
            for row in -1..=1 {
                for col in [-1.0f32, 1.0] {
                    p.circle_filled(g.center() + vec2(col * 2.2, row as f32 * 4.0), 1.1, c);
                }
            }
            self.tab_handle(resp, shown);
        }
        if tabs.len() > 1 {
            let mut x = g.max.x;
            for (i, &tab) in tabs.iter().enumerate() {
                let galley = p.layout_no_wrap(tab.title().to_string(), chip_font(), vis.text_color());
                let w = galley.size().x + CHIP_PAD;
                let cr = Rect::from_min_size(pos2(x, r.min.y + 1.0), vec2(w, r.height() - 2.0));
                x += w + 2.0;
                let resp = ui.interact(cr, Id::new(("dock chip", self.here, tab)), Sense::click_and_drag());
                ui.ctx().data_mut(|d| d.insert_temp(super::viewer::tab_rect_id(tab), cr));
                let on = i == active;
                let fill = if on { vis.widgets.active.bg_fill } else if resp.hovered() { vis.widgets.hovered.bg_fill } else { Color32::TRANSPARENT };
                p.rect_filled(cr, 3.0, fill);
                let tc = if drag == Some(tab) { vis.weak_text_color() } else if on { vis.strong_text_color() } else { vis.weak_text_color() };
                p.galley(pos2(cr.min.x + CHIP_PAD * 0.5, cr.center().y - galley.size().y * 0.5), galley, tc);
                if self.tab_handle(resp, tab) {
                    clicked = Some(tab);
                }
            }
        }
        clicked
    }

    /// A tab's handle (its button on the bar, or its patch): drag moves
    /// it, right-click is its menu, hover says what it is. True on a click.
    fn tab_handle(&mut self, resp: egui::Response, tab: T) -> bool {
        if resp.drag_started() {
            self.anim.drag = Some(tab);
        }
        let key = match tab.key() {
            "" => String::new(),
            k => format!(" ({k} shows its window)"),
        };
        let resp = resp.on_hover_text(format!("{}: {}\n\ndrag it to move it: within the window, to another window, or anywhere else for a window of its own; right-click it for more{key}", tab.title(), tab.help()));
        let clicked = resp.clicked();
        let others = self.others.clone();
        let asks = &mut *self.asks;
        if tab.pinned() {
            return clicked;
        }
        resp.context_menu(|ui| {
            if ui.button("detach to its own window").clicked() {
                asks.push(Ask::Detach(tab));
                ui.close();
            }
            if !others.is_empty() {
                ui.menu_button("move to", |ui| {
                    for (i, name) in &others {
                        if ui.button(name).clicked() {
                            asks.push(Ask::Move(tab, *i, None));
                            ui.close();
                        }
                    }
                });
            }
            if ui.button("close").clicked() {
                asks.push(Ask::Close(tab));
                ui.close();
            }
        });
        clicked
    }
}

/// A leaf of nothing but strips (`Tab::strip`): its whole height, the
/// tab bar and the body's margins included.
fn strip_height<T: Tab>(tabs: &[T], bar_h: f32) -> Option<f32> {
    let mut h: Option<f32> = None;
    for t in tabs {
        h = Some(h.unwrap_or(0.0).max(t.strip()?));
    }
    h.map(|h| h + bar_h + 4.0)
}

/// The ghost: where the dragged tab's new leaf would be.
fn ghost_leaf(ui: &Ui, rect: Rect, title: &str) {
    let vis = ui.visuals();
    let p = ui.painter();
    let c = vis.selection.stroke.color;
    p.rect_filled(rect, 4.0, c.gamma_multiply(0.12));
    p.rect_stroke(rect, 4.0, Stroke::new(1.5, c), egui::StrokeKind::Inside);
    let bar = Rect::from_min_size(rect.min, vec2(rect.width(), BAR_H));
    p.text(bar.min + vec2(10.0, BAR_H * 0.5), Align2::LEFT_CENTER, title, FontId::proportional(13.0), c);
}

/// Where a drop at `p` lands: the window's edge splits the whole, a
/// leaf's middle is a tab there, a leaf's side a split off it. `from` is
/// the leaf the dragged tab is in (its own middle is nothing).
pub fn drop_at(layout: &super::tree::Layout, area: Rect, p: Pos2, from: Option<LeafId>) -> Option<Drop> {
    let edge = [
        (p.x - area.min.x, Side::Left),
        (area.max.x - p.x, Side::Right),
        (p.y - area.min.y, Side::Top),
        (area.max.y - p.y, Side::Bottom),
    ];
    if layout.leaves.len() > 1 {
        if let Some((_, s)) = edge.iter().filter(|(d, _)| *d < EDGE_PX).min_by(|a, b| a.0.total_cmp(&b.0)) {
            return Some(Drop::Root(*s));
        }
    }
    let (id, r) = layout.leaves.iter().find(|(_, r)| r.contains(p)).or_else(|| layout.leaves.first()).copied()?;
    if layout.leaves.is_empty() {
        return Some(Drop::Root(Side::Right));
    }
    let (fx, fy) = ((p.x - r.min.x) / r.width().max(1.0), (p.y - r.min.y) / r.height().max(1.0));
    let sides = [(fx, Side::Left), (1.0 - fx, Side::Right), (fy, Side::Top), (1.0 - fy, Side::Bottom)];
    let (d, s) = sides.iter().min_by(|a, b| a.0.total_cmp(&b.0)).copied()?;
    // (On its own leaf the middle is where it is: `proposed` says so.)
    let _ = from;
    if d < SIDE_FRAC {
        return Some(Drop::Leaf(id, Zone::Side(s)));
    }
    Some(Drop::Leaf(id, Zone::Center))
}
