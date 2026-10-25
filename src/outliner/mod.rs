//! A tree view over the app's data. The app implements [`TreeModel`] (roots,
//! children, labels); an [`Outliner`] keeps what is folded, selected, being
//! renamed or dragged, and answers each frame with [`Event`]s. It never
//! touches the model: a drop says `Move`, a committed rename says `Rename`,
//! and the app applies them (or not).
//!
//! Hands: click selects, shift-click a range, cmd/ctrl-click toggles;
//! double-click or F2 renames in place (Enter commits, Esc cancels); drag a
//! row between rows (a line marks the slot) or onto one (an outline:
//! reparent, appended); arrows walk the rows, left folds or climbs, right
//! unfolds or descends, Enter activates. Rows are virtual: only the visible
//! slice is laid out. Colours come from `Visuals`.
//!
//! A row may also carry a checkbox (`TreeModel::check`; a click on it says
//! `Check`), a colour swatch in the icon's column (`swatch`) and a dimmed
//! label (`dimmed`), which is what a flat list of switchable things needs
//! (graav's biomes). `can_move` false keeps a row where it is.
//! `show_fit` sizes the view to its rows, for a list inside a scrolling panel.

use std::collections::HashSet;
use std::fmt::Debug;
use std::hash::Hash;
use std::ops::Range;

use egui::{CursorIcon, Key, Modifiers, Rect, Sense, Ui};

mod row;
#[cfg(test)]
pub(crate) mod drive;
#[cfg(test)]
mod tests;

pub use row::{Row, CHECK_W, GAP, ICON_W, INDENT, ROW_H, TRI_W};
use row::{Lead, Look, Paint, RenameOut, Slot};

/// The app's tree, read-only. Children are asked for on demand, so a
/// model may be lazy; ids must be stable across frames.
pub trait TreeModel {
    type Id: Copy + Eq + Hash + Debug;
    fn roots(&self) -> Vec<Self::Id>;
    fn children(&self, id: Self::Id) -> Vec<Self::Id>;
    fn label(&self, id: Self::Id) -> String;
    fn icon(&self, _id: Self::Id) -> Option<&'static str> {
        None
    }
    fn can_rename(&self, _id: Self::Id) -> bool {
        true
    }
    /// A checkbox at the row's left: whether it is on, and whether it may
    /// be clicked now. None: no box.
    fn check(&self, _id: Self::Id) -> Option<Check> {
        None
    }
    /// A colour swatch in the icon's column (instead of an icon).
    fn swatch(&self, _id: Self::Id) -> Option<egui::Color32> {
        None
    }
    /// The label drawn weak (a thing switched off).
    fn dimmed(&self, _id: Self::Id) -> bool {
        false
    }
    /// Whether the row can be dragged elsewhere.
    fn can_move(&self, _id: Self::Id) -> bool {
        true
    }
}

/// A row's checkbox.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Check {
    pub on: bool,
    /// False: drawn greyed, and a click does nothing.
    pub enabled: bool,
}

/// What the user asked for this frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event<Id> {
    /// The selection changed; this is all of it, in click order.
    Select(Vec<Id>),
    Rename(Id, String),
    /// Put `id` under `parent` (`None`: a root) at `index` among the
    /// siblings *as they are once `id` is taken out*: remove, then insert.
    Move { id: Id, parent: Option<Id>, index: usize },
    /// Enter on a row, or a double-click on one that cannot be renamed.
    Activate(Id),
    /// The row's checkbox was clicked: what it asks to be now.
    Check(Id, bool),
}

struct Rename<Id> {
    id: Id,
    text: String,
    fresh: bool,
}

struct Drag<Id> {
    id: Id,
    /// The node and its subtree: nowhere to drop.
    forbidden: HashSet<Id>,
    slot: Option<Slot>,
}

pub struct Outliner<Id> {
    pub expanded: HashSet<Id>,
    pub selection: Vec<Id>,
    /// The row the arrow keys move from.
    cursor: Option<Id>,
    /// Where a shift-range starts.
    anchor: Option<Id>,
    rename: Option<Rename<Id>>,
    drag: Option<Drag<Id>>,
    scroll_to: Option<Id>,
    visible: Range<usize>,
}

impl<Id: Copy + Eq + Hash + Debug> Default for Outliner<Id> {
    fn default() -> Self {
        Self { expanded: HashSet::new(), selection: vec![], cursor: None, anchor: None, rename: None, drag: None, scroll_to: None, visible: 0..0 }
    }
}

impl<Id: Copy + Eq + Hash + Debug> Outliner<Id> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn expand_all<M: TreeModel<Id = Id>>(&mut self, m: &M) {
        let mut stack = m.roots();
        while let Some(n) = stack.pop() {
            let kids = m.children(n);
            if !kids.is_empty() {
                self.expanded.insert(n);
                stack.extend(kids);
            }
        }
    }

    /// Start renaming `id` (a menu's Rename, say); the field appears next frame.
    pub fn rename<M: TreeModel<Id = Id>>(&mut self, m: &M, id: Id) {
        if m.can_rename(id) {
            self.rename = Some(Rename { id, text: m.label(id), fresh: true });
        }
    }

    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// The rows laid out last frame (the virtual window).
    pub fn visible(&self) -> Range<usize> {
        self.visible.clone()
    }

    /// `show` in a box as tall as the rows on show, `max_rows` at most
    /// (past that it scrolls), and as wide as there is: a list among other
    /// controls rather than a pane of its own.
    pub fn show_fit<M: TreeModel<Id = Id>>(&mut self, ui: &mut Ui, m: &M, max_rows: usize) -> Vec<Event<Id>> {
        let n = row::flatten(m, &self.expanded).len().clamp(1, max_rows.max(1));
        let size = egui::vec2(ui.available_width(), n as f32 * ROW_H);
        let r = ui.allocate_ui(size, |ui| {
            ui.set_min_size(size);
            self.show(ui, m)
        });
        r.inner
    }

    pub fn show<M: TreeModel<Id = Id>>(&mut self, ui: &mut Ui, m: &M) -> Vec<Event<Id>> {
        let mut ev = Vec::new();
        let rows = row::flatten(m, &self.expanded);
        let id = ui.make_persistent_id("imfram.outliner");
        let full = ui.available_rect_before_wrap();
        // The tree holds keyboard focus under `id` (a widget egui knows, so
        // the focus sticks); empty space clicked takes it and clears the selection.
        ui.interact(full, id, Sense::focusable_noninteractive());
        if ui.interact(full, id.with("bg"), Sense::click()).clicked() {
            ui.memory_mut(|mem| mem.request_focus(id));
            self.set_selection(vec![], &mut ev);
        }
        if ui.memory(|mem| mem.has_focus(id)) && self.rename.is_none() {
            self.keys(ui, m, &rows, &mut ev);
        }
        let pt = Paint::from(ui);
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            egui::ScrollArea::vertical().id_salt(id).auto_shrink([false, false]).min_scrolled_height(ROW_H).show_rows(ui, ROW_H, rows.len(), |ui, range| {
                self.visible = range.clone();
                // Row i's rect, for any i: the content's top is `range.start` rows above the viewport.
                let top0 = ui.max_rect().top() - range.start as f32 * ROW_H;
                let x = ui.max_rect().x_range();
                let rect_of = |i: usize| Rect::from_x_y_ranges(x, top0 + i as f32 * ROW_H..=top0 + (i + 1) as f32 * ROW_H);
                if let Some(want) = self.scroll_to.take() {
                    if let Some(i) = rows.iter().position(|r| r.id == want) {
                        ui.scroll_to_rect(rect_of(i), None);
                    }
                }
                let mut rects = Vec::with_capacity(range.len());
                for i in range.clone() {
                    let rect = rect_of(i);
                    rects.push((i, rect));
                    self.row(ui, m, &rows, i, rect, id, &pt, &mut ev);
                }
                self.drop(ui, m, &rows, &rects, &pt, &mut ev);
            });
        });
        // A rename whose row scrolled away has nowhere to live.
        if let Some(r) = &self.rename {
            if !rows[self.visible.clone()].iter().any(|row| row.id == r.id) {
                self.rename = None;
            }
        }
        ev
    }

    #[allow(clippy::too_many_arguments)]
    fn row<M: TreeModel<Id = Id>>(&mut self, ui: &mut Ui, m: &M, rows: &[Row<Id>], i: usize, rect: Rect, base: egui::Id, pt: &Paint, ev: &mut Vec<Event<Id>>) {
        let row = rows[i];
        let resp = ui.interact(rect, base.with(row.id), Sense::click_and_drag());
        let mods = ui.input(|inp| inp.modifiers);
        let renaming = self.rename.as_ref().is_some_and(|r| r.id == row.id);
        let look = Look {
            selected: self.selection.contains(&row.id),
            hovered: resp.hovered() && self.drag.is_none(),
            dragged: self.drag.as_ref().is_some_and(|d| d.forbidden.contains(&row.id)),
        };
        let icon = m.icon(row.id);
        let check = m.check(row.id);
        let lead = Lead { check, swatch: m.swatch(row.id), icon, dimmed: m.dimmed(row.id) };
        let label = m.label(row.id);
        row::paint_row(ui.painter(), rect, &row, &label, &lead, self.expanded.contains(&row.id), &look, pt, renaming);
        if renaming {
            let r = self.rename.as_mut().unwrap();
            let fresh = std::mem::replace(&mut r.fresh, false);
            match row::rename_field(ui, row::label_rect(rect, &row, &lead), base.with(("rename", row.id)), &mut r.text, fresh) {
                RenameOut::Editing => {}
                RenameOut::Cancel => {
                    self.rename = None;
                    ui.memory_mut(|mem| mem.request_focus(base));
                }
                RenameOut::Commit => {
                    ui.memory_mut(|mem| mem.request_focus(base));
                    let r = self.rename.take().unwrap();
                    let text = r.text.trim().to_string();
                    if !text.is_empty() && text != label {
                        ev.push(Event::Rename(row.id, text));
                    }
                }
            }
            return;
        }
        // The checkbox takes every click on it, a quick second one too
        // (which would otherwise be a double-click: a rename).
        let on_check = check.is_some() && resp.interact_pointer_pos().is_some_and(|p| row::check_rect(rect, &row).contains(p));
        if on_check {
            if let Some(c) = check.filter(|c| resp.clicked() && c.enabled) {
                ui.memory_mut(|mem| mem.request_focus(base));
                ev.push(Event::Check(row.id, !c.on));
            }
            return;
        }
        if resp.double_clicked() || resp.triple_clicked() {
            if m.can_rename(row.id) {
                self.rename(m, row.id);
            } else {
                ev.push(Event::Activate(row.id));
            }
        } else if resp.clicked() {
            ui.memory_mut(|mem| mem.request_focus(base));
            let on_tri = row.has_children && resp.interact_pointer_pos().is_some_and(|p| row::tri_rect(rect, &row).contains(p));
            if on_tri {
                self.toggle(row.id);
            } else {
                self.click(rows, i, mods, ev);
            }
        }
        if resp.drag_started() && m.can_move(row.id) {
            if !self.selection.contains(&row.id) {
                self.set_selection(vec![row.id], ev);
                self.cursor = Some(row.id);
                self.anchor = Some(row.id);
            }
            self.drag = Some(Drag { id: row.id, forbidden: row::subtree(m, row.id), slot: None });
        }
    }

    /// Track the drag over the laid-out rows, draw the slot, drop on release.
    fn drop<M: TreeModel<Id = Id>>(&mut self, ui: &mut Ui, m: &M, rows: &[Row<Id>], rects: &[(usize, Rect)], pt: &Paint, ev: &mut Vec<Event<Id>>) {
        let Some(d) = &mut self.drag else { return };
        ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
        let p = ui.input(|inp| inp.pointer.interact_pos());
        d.slot = p.and_then(|p| row::slot_at(p, rects)).filter(|s| {
            let t = match s {
                Slot::Before(i) | Slot::After(i) | Slot::Onto(i) => rows[*i],
            };
            !d.forbidden.contains(&t.id)
        });
        let into_expanded = |i: usize| rows[i].has_children && self.expanded.contains(&rows[i].id);
        if let Some(s) = d.slot {
            let into = matches!(s, Slot::After(i) if into_expanded(i));
            row::paint_slot(ui.painter(), s, rects, rows, into, pt.accent);
        }
        if ui.input(|inp| inp.pointer.any_released()) {
            let d = self.drag.take().unwrap();
            let Some(s) = d.slot else { return };
            let (parent, mut index) = match s {
                Slot::Before(i) => (rows[i].parent, rows[i].index),
                Slot::After(i) if into_expanded(i) => (Some(rows[i].id), 0),
                Slot::After(i) => (rows[i].parent, rows[i].index + 1),
                Slot::Onto(i) => (Some(rows[i].id), m.children(rows[i].id).len()),
            };
            let now = row::locate(m, d.id);
            if let Some((cur_parent, cur_index)) = now {
                if cur_parent == parent && cur_index < index {
                    index -= 1;
                }
                if (cur_parent, cur_index) == (parent, index) {
                    return;
                }
            }
            if let Some(pid) = parent {
                self.expanded.insert(pid);
            }
            ev.push(Event::Move { id: d.id, parent, index });
        }
    }

    fn toggle(&mut self, id: Id) {
        if !self.expanded.remove(&id) {
            self.expanded.insert(id);
        }
    }

    fn set_selection(&mut self, sel: Vec<Id>, ev: &mut Vec<Event<Id>>) {
        if sel != self.selection {
            self.selection = sel;
            ev.push(Event::Select(self.selection.clone()));
        }
    }

    /// A click on row `i` with `mods` held.
    fn click(&mut self, rows: &[Row<Id>], i: usize, mods: Modifiers, ev: &mut Vec<Event<Id>>) {
        let id = rows[i].id;
        let anchor_i = self.anchor.and_then(|a| rows.iter().position(|r| r.id == a));
        if mods.shift && anchor_i.is_some() {
            let a = anchor_i.unwrap();
            let (lo, hi) = (a.min(i), a.max(i));
            self.set_selection(rows[lo..=hi].iter().map(|r| r.id).collect(), ev);
        } else if mods.command {
            let mut sel = self.selection.clone();
            if let Some(k) = sel.iter().position(|s| *s == id) {
                sel.remove(k);
            } else {
                sel.push(id);
            }
            self.set_selection(sel, ev);
            self.anchor = Some(id);
        } else {
            self.set_selection(vec![id], ev);
            self.anchor = Some(id);
        }
        self.cursor = Some(id);
    }

    /// The arrow keys, F2 and Enter, while the tree has focus.
    fn keys<M: TreeModel<Id = Id>>(&mut self, ui: &mut Ui, m: &M, rows: &[Row<Id>], ev: &mut Vec<Event<Id>>) {
        if rows.is_empty() {
            return;
        }
        let pressed = |ui: &mut Ui, k: Key| ui.input_mut(|inp| {
            let mods = inp.modifiers;
            inp.consume_key(mods, k)
        });
        let mods = ui.input(|inp| inp.modifiers);
        let cur = self.cursor.and_then(|c| rows.iter().position(|r| r.id == c));
        let mut go = None;
        if pressed(ui, Key::ArrowDown) {
            go = Some(cur.map_or(0, |c| (c + 1).min(rows.len() - 1)));
        } else if pressed(ui, Key::ArrowUp) {
            go = Some(cur.map_or(rows.len() - 1, |c| c.saturating_sub(1)));
        } else if let Some(c) = cur {
            let row = rows[c];
            if pressed(ui, Key::ArrowLeft) {
                let folded = row.has_children && self.expanded.remove(&row.id);
                if !folded {
                    go = row.parent.and_then(|p| rows.iter().position(|r| r.id == p));
                }
            } else if pressed(ui, Key::ArrowRight) {
                if row.has_children && !self.expanded.contains(&row.id) {
                    self.expanded.insert(row.id);
                } else if row.has_children {
                    go = Some(c + 1);
                }
            } else if pressed(ui, Key::F2) {
                self.rename(m, row.id);
            } else if pressed(ui, Key::Enter) {
                ev.push(Event::Activate(row.id));
            }
        }
        if let Some(i) = go {
            let mods = Modifiers { shift: mods.shift, ..Modifiers::NONE };
            self.click(rows, i, mods, ev);
            self.scroll_to = Some(rows[i].id);
        }
    }
}
