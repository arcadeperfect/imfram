//! The composer: a panel the user fills at run time with controls from
//! anywhere else, so the few they keep reaching for sit side by side.
//!
//! Hold Alt over any composable control (it lights) and drag it into a
//! composer; or turn on the composer's pick mode and click controls in any
//! window. Drag a row's grip to reorder it, × to take it out.
//!
//! A row in a composer is the real control, not a copy: the composer runs
//! the app's own panel code (`sources`) with every control drawn nowhere
//! except the ones it holds, which are drawn into its rows. So a composed
//! slider edits the same value, through the same code (saving, undo,
//! greying), as the one on its page.
//!
//! What makes a control composable is [`item`]: `widgets::slider` (with a
//! label), `widgets::switched`'s switch and every `props` row are items
//! already; anything else is wrapped by the app. A control is known by its
//! key: the [`scope`]s around it (every `widgets::group` is one) and its
//! name, `"Sky/Gradient/gradient follow"`. Keys are what a [`Composer`]
//! saves, so they stay put while the code around them does.
//!
//! ```ignore
//! fn pages(ui: &mut Ui, app: &mut App) {          // every page, in order
//!     composer::scope(ui, "Sky", |ui| sky::tab(ui, app));
//!     composer::scope(ui, "Light", |ui| light::tab(ui, app));
//! }
//! // the page tab draws one page the same way: composer::scope(ui, "Sky", ..)
//! if app.composer.show(ui, |ui| pages(ui, app)) { /* save app.composer */ }
//! ```
//!
//! A control the sources did not reach this frame (its page hides it now,
//! or the code lost it) keeps its row, greyed, until it comes back or is
//! taken out.

mod source;
#[cfg(test)]
mod tests;

use std::collections::HashMap;

use egui::{vec2, Align2, DragAndDrop, Id, Rect, Sense, Ui};
use serde::{Deserialize, Serialize};

pub use source::{armed, carrying, harvesting, item, item_with, pick_mode, scope, set_pick_mode, set_screen_origin, ItemAt, Pick};
use source::Slot;

/// Width of a row's grip.
const GRIP_W: f32 = 14.0;
/// Width kept for a row's ×.
const CLOSE_W: f32 = 18.0;
/// Height a row is given before its control has been drawn once.
const FIRST_H: f32 = 20.0;
/// Height of the drop area under the rows (all of it, when there are none).
const TAIL_H: f32 = 28.0;

/// One control in a composer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub key: String,
    /// Shown while the control is not there.
    pub name: String,
    /// What the user renamed the row to (right-click its grip), shown in
    /// place of the control's own name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// A composer's rows, in order. Save it (serde) to keep a view.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Composer {
    pub entries: Vec<Entry>,
}

/// A row dragged by its grip, within the composer `from`.
#[derive(Clone, Copy, Debug)]
struct Moving {
    from: Id,
    index: usize,
}

fn height_id(key: &str) -> Id {
    Id::new(("composer-row-h", key))
}

impl Composer {
    pub fn contains(&self, key: &str) -> bool {
        self.entries.iter().any(|e| e.key == key)
    }

    /// Add at `at` (clamped); a key already here moves there instead.
    pub fn insert(&mut self, at: usize, entry: Entry) {
        let mut at = at.min(self.entries.len());
        if let Some(i) = self.entries.iter().position(|e| e.key == entry.key) {
            self.entries.remove(i);
            if i < at {
                at -= 1;
            }
        }
        self.entries.insert(at, entry);
    }

    /// Add at the end, unless it is here already. True when added.
    pub fn push(&mut self, entry: Entry) -> bool {
        if self.contains(&entry.key) {
            return false;
        }
        self.entries.push(entry);
        true
    }

    pub fn remove(&mut self, key: &str) -> bool {
        let n = self.entries.len();
        self.entries.retain(|e| e.key != key);
        self.entries.len() != n
    }

    /// Move the row at `from` to sit before what is now at `to`.
    pub fn reorder(&mut self, from: usize, to: usize) {
        if from >= self.entries.len() {
            return;
        }
        let e = self.entries.remove(from);
        let to = if to > from { to - 1 } else { to };
        self.entries.insert(to.min(self.entries.len()), e);
    }

    /// Draw the composer: its header, its rows (each the real control,
    /// drawn by running `sources`), and the drop line while something is
    /// dragged over it. True when its rows changed (worth saving).
    pub fn show(&mut self, ui: &mut Ui, sources: impl FnOnce(&mut Ui)) -> bool {
        let me = ui.id().with("composer");
        let mut changed = false;
        ui.horizontal(|ui| {
            let mut on = pick_mode();
            if ui.toggle_value(&mut on, "pick").on_hover_text("on: click a control anywhere to add it here (or hold Alt and drag one in, any time)").changed() {
                set_pick_mode(on);
            }
            if !self.entries.is_empty() && ui.small_button("clear").on_hover_text("take every row out").clicked() {
                self.entries.clear();
                changed = true;
            }
        });
        for p in source::take_picked() {
            changed |= self.push(Entry { key: p.key, name: p.name, label: None });
        }

        // The rows, laid out at their last heights; the controls go in after.
        let top = ui.cursor().min.y;
        let mut slots = HashMap::new();
        let mut rows: Vec<Rect> = Vec::with_capacity(self.entries.len());
        let mut close = None;
        let mut moving = None;
        let mut renamed = None;
        for (i, e) in self.entries.iter().enumerate() {
            let h = ui.data(|d| d.get_temp::<f32>(height_id(&e.key))).unwrap_or(FIRST_H);
            let row = ui.horizontal(|ui| {
                let (grip, g) = ui.allocate_exact_size(vec2(GRIP_W, h), Sense::drag());
                let w = (ui.available_width() - CLOSE_W - ui.spacing().item_spacing.x).max(40.0);
                let (slot, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
                let x = ui.add_sized(vec2(CLOSE_W, h.min(FIRST_H)), egui::Button::new("×").frame(false)).on_hover_text("take it out");
                (grip, g, slot, x)
            });
            let (grip, g, slot, x) = row.inner;
            let g = g.on_hover_cursor(egui::CursorIcon::Grab).on_hover_text(format!("{}\nright-click: rename", e.key));
            g.context_menu(|ui| {
                if let Some(label) = rename_menu(ui, me.with(("rename", &e.key)), e) {
                    renamed = Some((i, label));
                }
            });
            let c = if g.hovered() || g.dragged() { ui.visuals().strong_text_color() } else { ui.visuals().weak_text_color() };
            for (dx, dy) in [(-2.5, -4.0), (2.5, -4.0), (-2.5, 0.0), (2.5, 0.0), (-2.5, 4.0), (2.5, 4.0)] {
                ui.painter().circle_filled(grip.center() + vec2(dx, dy), 1.2, c);
            }
            if g.drag_started() {
                DragAndDrop::set_payload(ui.ctx(), Moving { from: me, index: i });
            }
            if g.dragged() {
                moving = Some(i);
            }
            if x.clicked() {
                close = Some(i);
            }
            slots.insert(
                e.key.clone(),
                Slot { id: me.with(("slot", &e.key)), rect: slot, layer: ui.layer_id(), clip: ui.clip_rect(), style: ui.style().clone(), label: e.label.clone() },
            );
            rows.push(row.response.rect);
        }
        let tail_h = if self.entries.is_empty() { TAIL_H * 2.0 } else { TAIL_H };
        let (tail, _) = ui.allocate_exact_size(vec2(ui.available_width(), tail_h), Sense::hover());
        if self.entries.is_empty() {
            ui.painter().text(
                tail.center(),
                Align2::CENTER_CENTER,
                "hold Alt and drag a control here",
                egui::TextStyle::Body.resolve(ui.style()),
                ui.visuals().weak_text_color(),
            );
        }

        // Draw the controls into their rows.
        let drawn = if slots.is_empty() { HashMap::new() } else { source::harvest(ui, slots, sources) };
        for (e, row) in self.entries.iter().zip(&rows) {
            match drawn.get(&e.key) {
                Some(&h) => {
                    let was = ui.data(|d| d.get_temp::<f32>(height_id(&e.key)));
                    if was.is_none_or(|w| (w - h).abs() > 0.5) {
                        ui.data_mut(|d| d.insert_temp(height_id(&e.key), h.max(1.0)));
                        ui.ctx().request_discard("a composer row's height changed");
                    }
                }
                None => {
                    let at = egui::pos2(row.left() + GRIP_W + ui.spacing().item_spacing.x, row.center().y);
                    let font = egui::TextStyle::Body.resolve(ui.style());
                    let name = e.label.as_deref().unwrap_or(&e.name);
                    ui.painter().text(at, Align2::LEFT_CENTER, format!("{name} (not here now)"), font, ui.visuals().weak_text_color());
                }
            }
        }
        if let Some(i) = moving {
            ui.painter().rect_filled(rows[i], 2.0, ui.visuals().selection.bg_fill.gamma_multiply(0.25));
        }

        // A drop: where the pointer is between rows.
        let area = Rect::from_min_max(egui::pos2(ui.min_rect().left(), top), tail.max);
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let picked = DragAndDrop::payload::<Pick>(ui.ctx());
        let moved = DragAndDrop::payload::<Moving>(ui.ctx()).filter(|m| m.from == me);
        if let Some(at) = pointer.filter(|p| area.contains(*p) && (picked.is_some() || moved.is_some())) {
            let to = rows.iter().position(|r| at.y < r.center().y).unwrap_or(rows.len());
            let y = rows.get(to).map_or_else(|| rows.last().map_or(tail.top(), |r| r.bottom()), |r| r.top());
            ui.painter().hline(area.x_range(), y, (2.0, ui.visuals().selection.stroke.color));
            if ui.input(|i| i.pointer.any_released()) {
                if let Some(p) = DragAndDrop::take_payload::<Pick>(ui.ctx()) {
                    self.insert(to, Entry { key: p.key.clone(), name: p.name.clone(), label: None });
                    source::end_carry();
                    changed = true;
                } else if let Some(m) = DragAndDrop::take_payload::<Moving>(ui.ctx()) {
                    if to != m.index && to != m.index + 1 {
                        self.reorder(m.index, to);
                        changed = true;
                    }
                }
            }
        }
        // A control carried from another window: dropped where it was let go.
        if picked.is_none() && moved.is_none() {
            if let Some((p, at, released)) = source::carried_here(ui.ctx()).filter(|(_, at, _)| area.contains(*at)) {
                let to = rows.iter().position(|r| at.y < r.center().y).unwrap_or(rows.len());
                let y = rows.get(to).map_or_else(|| rows.last().map_or(tail.top(), |r| r.bottom()), |r| r.top());
                ui.painter().hline(area.x_range(), y, (2.0, ui.visuals().selection.stroke.color));
                if released {
                    self.insert(to, Entry { key: p.key, name: p.name, label: None });
                    source::end_carry();
                    changed = true;
                }
            }
        }
        if let Some((i, label)) = renamed {
            self.entries[i].label = label;
            changed = true;
        }
        if let Some(i) = close {
            self.entries.remove(i);
            changed = true;
        }
        changed
    }
}

/// A row's rename menu: a field holding its label (Enter or the button
/// takes it; empty, or "own name", goes back to the control's own).
/// `Some` when the row's label changes.
fn rename_menu(ui: &mut Ui, id: Id, e: &Entry) -> Option<Option<String>> {
    let mut text: String = ui.data(|d| d.get_temp(id)).unwrap_or_else(|| e.label.clone().unwrap_or_else(|| e.name.clone()));
    ui.label("label");
    let field = ui.text_edit_singleline(&mut text);
    if !field.has_focus() && !field.lost_focus() && ui.data(|d| d.get_temp::<bool>(id.with("focused"))).is_none() {
        field.request_focus();
        ui.data_mut(|d| d.insert_temp(id.with("focused"), true));
    }
    let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    let mut out = None;
    ui.horizontal(|ui| {
        if ui.button("rename").clicked() || enter {
            let t = text.trim();
            out = Some((!t.is_empty() && t != e.name).then(|| t.to_owned()));
        }
        if e.label.is_some() && ui.button("own name").on_hover_text(format!("back to \"{}\"", e.name)).clicked() {
            out = Some(None);
        }
    });
    if out.is_some() {
        ui.data_mut(|d| {
            d.remove::<String>(id);
            d.remove::<bool>(id.with("focused"));
        });
        ui.close();
    } else {
        ui.data_mut(|d| d.insert_temp(id, text));
    }
    out.filter(|l| *l != e.label)
}
