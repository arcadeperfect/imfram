//! One tile a frame: which picture (the poster, or the frame under the
//! pointer while scrubbing), its paint, and what a click or a drag on it means.

use std::collections::HashMap;

use egui::{vec2, Id, LayerId, Order, Pos2, Rect, Sense, Ui};

use super::{paint, Dragged, Gallery, GalleryEvent, Items, Layout, Shown, Thumbs};

/// Frames asked for across a clip as soon as it is scrubbed, so a fast
/// sweep always has something near the pointer to show.
const SPREAD: u32 = 16;
/// Frames either side of the pointer's frame asked for next.
const AHEAD: u32 = 2;
/// How far either side to look for a loaded frame to stand in.
const NEAR: u32 = 96;

/// The picture item i shows in `area`: its poster, or, with the pointer at
/// `scrub_at`, the frame that far along the clip. While that frame loads
/// the nearest frame already here stands in (else the last one shown, held
/// in `hold`), so a scrub never blanks and never lags far behind the
/// pointer. A scrub also asks for frames spread across the clip and its
/// neighbours, at lower priorities, so the next move finds them ready.
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
            let shown = match thumbs.want(&src, f, px, prio + 3.0) {
                Some(s) => {
                    hold.insert(iid, f);
                    Some(s)
                }
                None => {
                    let near = nearest(frames, f, |g| thumbs.has(&src, g));
                    let held = hold.get(&iid).copied().unwrap_or(poster);
                    let stand = near.filter(|&g| g.abs_diff(f) <= held.abs_diff(f)).unwrap_or(held);
                    thumbs.want(&src, stand, px, prio).or_else(|| thumbs.want(&src, poster, px, prio + 1.0))
                }
            };
            for d in 1..=AHEAD {
                for g in [f.checked_sub(d), Some(f + d).filter(|&g| g < frames)].into_iter().flatten() {
                    if !thumbs.has(&src, g) {
                        let _ = thumbs.want(&src, g, px, prio + 2.0 - d as f32 * 0.1);
                    }
                }
            }
            for g in spread(frames) {
                if !thumbs.has(&src, g) {
                    let _ = thumbs.want(&src, g, px, prio + 1.5);
                }
            }
            (shown, scrub)
        }
        None => {
            hold.remove(&iid);
            (thumbs.want(&src, poster, px, prio), None)
        }
    }
}

/// The loaded frame nearest `f` (within `NEAR`), by `here`.
pub(super) fn nearest(frames: u32, f: u32, here: impl Fn(u32) -> bool) -> Option<u32> {
    (1..=NEAR.min(frames)).find_map(|d| [f.checked_sub(d), Some(f + d).filter(|&g| g < frames)].into_iter().flatten().find(|&g| here(g)))
}

/// `SPREAD` frames evenly across a clip (all of a short one).
pub(super) fn spread(frames: u32) -> impl Iterator<Item = u32> {
    let n = SPREAD.min(frames);
    (0..n).map(move |k| if n <= 1 { 0 } else { k * (frames - 1) / (n - 1) })
}

#[cfg(test)]
mod scrub_tests {
    use super::*;

    #[test]
    fn the_nearest_loaded_frame_stands_in_either_side() {
        let loaded = [10u32, 40];
        assert_eq!(nearest(100, 13, |g| loaded.contains(&g)), Some(10));
        assert_eq!(nearest(100, 30, |g| loaded.contains(&g)), Some(40));
        assert_eq!(nearest(100, 99, |_| false), None);
    }

    #[test]
    fn a_spread_reaches_both_ends() {
        let s: Vec<u32> = spread(100).collect();
        assert_eq!((s.len(), s[0], *s.last().unwrap()), (16, 0, 99));
        assert_eq!(spread(3).collect::<Vec<_>>(), vec![0, 1, 2]);
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
        drop_zone(ui, &resp, cell, items, i, look, ev);
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

/// A drop target under a drag: outline it while a payload hovers, and
/// report a release there.
pub(super) fn drop_zone<I: Items>(ui: &Ui, resp: &egui::Response, rect: Rect, items: &I, i: usize, look: &paint::Look, ev: &mut GalleryEvent) {
    if !items.drop_target(i) {
        return;
    }
    let iid = items.id(i);
    let own = |d: &Dragged| d.0.contains(&iid);
    if resp.dnd_hover_payload::<Dragged>().is_some_and(|d| !own(&d)) {
        ui.painter().rect_stroke(rect.shrink(1.0), egui::CornerRadius::same(paint::RADIUS), egui::Stroke::new(2.0, look.sel_fill), egui::StrokeKind::Inside);
    }
    if let Some(d) = resp.dnd_release_payload::<Dragged>() {
        if !own(&d) {
            ev.dropped = Some((Some(iid), (*d).clone()));
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
