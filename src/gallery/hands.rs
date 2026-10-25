//! What the hands do: clicks, the keyboard, a box drawn on empty space.

use std::collections::BTreeSet;

use egui::{Id, Key, Modifiers, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};

use super::{BoxSel, Gallery, GalleryEvent, Items, Layout};

/// How close to the view's edge a box drag scrolls, and how fast.
const EDGE: f32 = 24.0;
const EDGE_SPEED: f32 = 12.0;

/// What a click on item i does to a selection, the same in every view:
/// plain picks it alone, shift picks the range from the anchor, cmd/ctrl
/// toggles it. Answers whether the selection changed.
pub(crate) fn pick<I: Items>(sel: &mut BTreeSet<u64>, anchor: &mut Option<usize>, cursor: &mut Option<usize>, items: &I, i: usize, mods: Modifiers) -> bool {
    let before = sel.clone();
    let iid = items.id(i);
    match *anchor {
        Some(a) if mods.shift => {
            let a = a.min(items.len() - 1);
            *sel = (a.min(i)..=a.max(i)).map(|j| items.id(j)).collect();
        }
        _ if mods.command => {
            if !sel.remove(&iid) {
                sel.insert(iid);
            }
            *anchor = Some(i);
        }
        _ => {
            *sel = BTreeSet::from([iid]);
            *anchor = Some(i);
        }
    }
    *cursor = Some(i);
    *sel != before
}

/// The selection keys every view shares: A / cmd-A all, alt-A / Esc none,
/// ctrl-I invert, Enter activates the cursor's item, Space previews it.
/// True when one of them was taken.
pub(crate) fn select_keys<I: Items>(ui: &mut Ui, sel: &mut BTreeSet<u64>, cursor: Option<usize>, items: &I, ev: &mut GalleryEvent) -> bool {
    let n = items.len();
    let press = |ui: &mut Ui, mods: Modifiers, k: Key| ui.input_mut(|i| i.consume_key(mods, k));
    let all = || (0..n).map(|j| items.id(j)).collect::<BTreeSet<u64>>();
    let set = |sel: &mut BTreeSet<u64>, s: BTreeSet<u64>, ev: &mut GalleryEvent| {
        ev.selection_changed |= s != *sel;
        *sel = s;
    };
    if press(ui, Modifiers::COMMAND, Key::A) || press(ui, Modifiers::NONE, Key::A) {
        set(sel, all(), ev);
        return true;
    }
    if press(ui, Modifiers::ALT, Key::A) || press(ui, Modifiers::NONE, Key::Escape) {
        set(sel, BTreeSet::new(), ev);
        return true;
    }
    if press(ui, Modifiers::CTRL, Key::I) {
        let inv = all().difference(sel).copied().collect();
        set(sel, inv, ev);
        return true;
    }
    if let Some(c) = cursor {
        if press(ui, Modifiers::NONE, Key::Enter) {
            ev.activated = Some(items.id(c));
            return true;
        }
        if press(ui, Modifiers::NONE, Key::Space) {
            ev.preview = Some(items.id(c));
            return true;
        }
    }
    false
}

impl Gallery {
    pub(crate) fn click<I: Items>(&mut self, items: &I, i: usize, mods: Modifiers, ev: &mut GalleryEvent) {
        ev.selection_changed |= pick(&mut self.selection, &mut self.anchor, &mut self.cursor, items, i, mods);
    }

    fn set(&mut self, sel: BTreeSet<u64>, ev: &mut GalleryEvent) {
        ev.selection_changed |= sel != self.selection;
        self.selection = sel;
    }

    /// The keys, while the gallery has focus.
    pub(crate) fn keys<I: Items>(&mut self, ui: &mut Ui, l: &Layout, items: &I, view_h: f32, ev: &mut GalleryEvent) {
        let n = items.len();
        if n == 0 {
            return;
        }
        if select_keys(ui, &mut self.selection, self.cursor, items, ev) {
            return;
        }
        let press = |ui: &mut Ui, mods: Modifiers, k: Key| ui.input_mut(|i| i.consume_key(mods, k));
        let mods = ui.input(|i| i.modifiers);
        let mut go = None;
        let cur = self.cursor;
        let page = l.rows_per(view_h);
        for (k, step) in [(Key::ArrowLeft, -1i64), (Key::ArrowRight, 1)] {
            if press(ui, mods, k) {
                go = Some(cur.map_or(0, |c| (c as i64 + step).clamp(0, n as i64 - 1) as usize));
            }
        }
        for (k, dir, rows) in [(Key::ArrowUp, -1, 1), (Key::ArrowDown, 1, 1), (Key::PageUp, -1, page), (Key::PageDown, 1, page)] {
            if press(ui, mods, k) {
                let mut at = cur.unwrap_or(0);
                for _ in 0..rows {
                    match l.vertical(at, dir) {
                        Some(j) => at = j,
                        None => {
                            at = if dir < 0 { 0 } else { n - 1 };
                            break;
                        }
                    }
                }
                go = Some(at);
            }
        }
        if press(ui, mods, Key::Home) {
            go = Some(0);
        }
        if press(ui, mods, Key::End) {
            go = Some(n - 1);
        }
        if let Some(i) = go {
            if mods.command && !mods.shift {
                self.cursor = Some(i);
            } else {
                self.click(items, i, Modifiers { shift: mods.shift, ..Modifiers::NONE }, ev);
            }
            self.scroll_to = Some(i);
        }
    }

    /// Empty space: a click clears (and takes focus), a drag draws a box.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn background<I: Items>(&mut self, ui: &mut Ui, id: Id, content: Rect, origin: Vec2, vp: Rect, l: &Layout, items: &I, ev: &mut GalleryEvent) {
        let r = ui.interact(content, id.with("bg"), Sense::click_and_drag());
        let mods = ui.input(|i| i.modifiers);
        if r.clicked() || r.drag_started() || r.secondary_clicked() {
            ui.memory_mut(|m| m.request_focus(id));
        }
        if r.clicked() && !mods.shift && !mods.command {
            self.set(BTreeSet::new(), ev);
        }
        if r.secondary_clicked() {
            ev.context = Some((None, r.interact_pointer_pos().unwrap_or(content.center())));
        }
        if r.drag_started() {
            if let Some(p) = r.interact_pointer_pos() {
                let base = if mods.shift || mods.command { self.selection.clone() } else { BTreeSet::new() };
                self.boxsel = Some(BoxSel { from: p - origin, base, toggle: mods.command, add: mods.shift });
            }
        }
        if let (Some(b), Some(p)) = (&self.boxsel, ui.ctx().pointer_latest_pos()) {
            let to = p - origin;
            let rect = Rect::from_two_pos(b.from, to);
            let hit: BTreeSet<u64> = l.items_meeting(rect).into_iter().map(|i| items.id(i)).collect();
            let sel = if b.toggle {
                b.base.symmetric_difference(&hit).copied().collect()
            } else if b.add {
                b.base.union(&hit).copied().collect()
            } else {
                hit
            };
            let v = ui.visuals();
            let (fill, stroke) = (v.selection.bg_fill.gamma_multiply(0.15), Stroke::new(1.0, v.selection.bg_fill));
            ui.painter().rect(rect.translate(origin), 0.0, fill, stroke, StrokeKind::Inside);
            // Near the view's top or bottom edge the view scrolls on.
            let screen_vp = vp.translate(origin);
            let dy = if p.y < screen_vp.top() + EDGE { EDGE_SPEED } else if p.y > screen_vp.bottom() - EDGE { -EDGE_SPEED } else { 0.0 };
            if dy != 0.0 {
                ui.scroll_with_delta(Vec2::new(0.0, dy));
                ui.ctx().request_repaint();
            }
            self.set(sel, ev);
            if r.drag_stopped() || !ui.input(|i| i.pointer.primary_down()) {
                self.boxsel = None;
            }
        }
    }
}
