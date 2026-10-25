//! One tile a frame: which picture (the poster, or the frame under the
//! pointer while scrubbing), its paint, and what a click or a drag on it means.

use std::collections::HashMap;

use egui::{vec2, Id, LayerId, Order, Pos2, Rect, Sense, Ui};

use super::{paint, Dragged, Gallery, GalleryEvent, Items, Layout, Shown, Thumbs};

/// The picture item i shows in `area`: its poster, or, with the pointer at
/// `scrub_at`, the frame that far along the clip. While that frame loads
/// the last one shown is held (in `hold`), so a scrub never blanks.
/// Also answers where the scrub is (0..1) and the frame's name.
#[allow(clippy::too_many_arguments)]
pub(super) fn picture<I: Items>(thumbs: &mut Thumbs<I::Src>, hold: &mut HashMap<u64, u32>, items: &I, i: usize, area: Rect, scrub_at: Option<Pos2>, px: u32, prio: f32) -> (Option<Shown>, Option<(f32, String)>) {
    let iid = items.id(i);
    let src = items.src(i);
    let frames = items.frames(i).max(1);
    let poster = items.poster(i).min(frames - 1);
    if !items.has_picture(i) {
        return (None, None);
    }
    match scrub_at.filter(|_| frames > 1) {
        Some(p) => {
            let frac = ((p.x - area.left()) / area.width()).clamp(0.0, 0.9999);
            let f = (frac * frames as f32) as u32;
            let scrub = Some((frac, items.frame_name(i, f)));
            let held = hold.get(&iid).copied().unwrap_or(poster);
            let shown = match thumbs.want(&src, f, px, prio + 2.0) {
                Some(s) => {
                    hold.insert(iid, f);
                    Some(s)
                }
                None => thumbs.want(&src, held, px, prio).or_else(|| thumbs.want(&src, poster, px, prio + 1.0)),
            };
            (shown, scrub)
        }
        None => {
            hold.remove(&iid);
            (thumbs.want(&src, poster, px, prio), None)
        }
    }
}

impl Gallery {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn tile<I: Items>(&mut self, ui: &mut Ui, id: Id, i: usize, items: &I, thumbs: &mut Thumbs<I::Src>, layout: &Layout, origin: egui::Vec2, px: u32, prio: f32, look: &paint::Look, ev: &mut GalleryEvent) {
        let iid = items.id(i);
        let cell = layout.cell_rect(i).translate(origin);
        let area = layout.tile_rect(i).translate(origin);
        let resp = ui.interact(cell, id.with(("tile", iid)), Sense::click_and_drag());
        let src = items.src(i);
        let frames = items.frames(i).max(1);
        let poster = items.poster(i).min(frames - 1);
        let scrub_at = resp.hover_pos().filter(|_| self.scrub && frames > 1 && self.boxsel.is_none() && !resp.dragged());
        let (shown, scrub) = picture(thumbs, &mut self.hold, items, i, area, scrub_at, px, prio);
        let (label, kind, badge) = (items.label(i), items.kind(i), items.badge(i));
        let t = paint::Tile {
            label: &label,
            kind: kind.as_deref(),
            badge: badge.as_deref(),
            selected: self.selection.contains(&iid),
            hovered: resp.hovered(),
            cursor: self.cursor == Some(i) && ui.memory(|m| m.has_focus(id)),
            dim: items.dim(i),
            failed: thumbs.failed(&src, poster).is_some(),
            fill: self.fill,
            scrub,
        };
        if ui.is_rect_visible(cell) {
            paint::tile(ui.painter(), cell, area, shown.as_ref(), &t, look);
        }
        if resp.clicked() || resp.double_clicked() || resp.secondary_clicked() || resp.drag_started() {
            ui.memory_mut(|m| m.request_focus(id));
        }
        let mods = ui.input(|inp| inp.modifiers);
        if resp.double_clicked() {
            ev.activated = Some(iid);
        } else if resp.clicked() {
            self.click(items, i, mods, ev);
        }
        if resp.secondary_clicked() {
            if !self.selection.contains(&iid) {
                self.click(items, i, egui::Modifiers::NONE, ev);
            }
            ev.context = Some((Some(iid), resp.interact_pointer_pos().unwrap_or(cell.center())));
        }
        if resp.drag_started() {
            if !self.selection.contains(&iid) {
                self.click(items, i, egui::Modifiers::NONE, ev);
            }
            let ids: Vec<u64> = (0..items.len()).map(|j| items.id(j)).filter(|j| self.selection.contains(j)).collect();
            resp.dnd_set_drag_payload(Dragged(ids));
            ev.drag_started = true;
        }
    }
}

/// While a [`Dragged`] payload is in the air: how many, by the pointer.
pub(super) fn drag_badge(ui: &Ui, id: Id, look: &paint::Look) {
    let Some(d) = egui::DragAndDrop::payload::<Dragged>(ui.ctx()) else { return };
    let Some(at) = ui.ctx().pointer_latest_pos() else { return };
    let p = ui.ctx().layer_painter(LayerId::new(Order::Tooltip, id.with("dnd")));
    let text = if d.0.len() == 1 { "1 item".to_owned() } else { format!("{} items", d.0.len()) };
    let g = p.layout_no_wrap(text, look.small.clone(), look.sel_text);
    let rect = Rect::from_min_size(at + vec2(14.0, 10.0), g.size()).expand2(vec2(6.0, 3.0));
    p.rect_filled(rect, egui::CornerRadius::same(paint::RADIUS), look.sel_fill);
    p.galley(rect.min + vec2(6.0, 3.0), g, look.sel_text);
}
