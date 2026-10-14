//! A tooltip for a stretch of UI rather than one widget: a slider row with
//! its value box and label, or a node's title. egui only shows a tooltip
//! for a widget it counts as hovered, and a row of several widgets (or a
//! hover-only widget under a draggable one) is not one; this shows the
//! text by hand once the pointer has rested over it for egui's usual
//! delay.

use egui::{Id, PopupAnchor, Response, Ui};

/// How wide a tip may grow before it wraps (window points).
const WIDTH: f32 = 280.0;

/// Show `text` at the pointer while it rests on `row` (all of it).
pub fn tip_row(ui: &Ui, row: &Response, text: &str) {
    show(ui, row.id.with("tip"), ui.rect_contains_pointer(row.rect), text);
}

/// Lay out a row with `add`, and give the whole of it the tip `text`.
pub fn row<R>(ui: &mut Ui, text: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    let r = ui.scope(add);
    tip_row(ui, &r.response, text);
    r.inner
}

/// Show `text` at the pointer if `over` has held while the pointer rested
/// (and nothing is being dragged); otherwise ask to look again when it
/// would be due.
pub fn show(ui: &Ui, id: Id, over: bool, text: &str) {
    if !over || text.is_empty() || ui.ctx().dragged_id().is_some() {
        return;
    }
    let (still, wait, down) = ui.input(|i| (i.pointer.time_since_last_movement(), ui.style().interaction.tooltip_delay, i.pointer.any_down()));
    if down {
        return;
    }
    if still < wait {
        ui.ctx().request_repaint_after_secs(wait - still);
        return;
    }
    egui::Tooltip::always_open(ui.ctx().clone(), ui.layer_id(), id, PopupAnchor::Pointer).gap(12.0).show(|ui| {
        ui.set_max_width(WIDTH);
        ui.label(text);
    });
}
