//! The float slider row: a rail, a value box and a label.
//!
//! Dragging the rail jumps the knob to the pointer, as usual. With a rate
//! key held (see `rate`) the drag turns relative: the knob moves a fraction
//! (or a multiple) of the pointer's travel. The value box drags at the same
//! rate. The range is soft: a value typed past it is kept.

use egui::{Rect, Response, Sense, Ui};

use super::rate;

/// Width of the value box.
pub const NUM_W: f32 = 48.0;

#[derive(Clone, Copy, Default)]
pub struct SliderOut {
    pub value_changed: bool,
    /// The slider's range was edited (worth saving; not a value change).
    pub range_changed: bool,
}

/// Where a slider's parts were last drawn, in window coordinates. Only read
/// by the scripted UI self-test.
#[derive(Clone, Copy)]
pub struct SliderRects {
    pub rail: Rect,
    /// The range's bound fields, while they are showing.
    pub bounds: Option<[Rect; 2]>,
}

fn rects_id(id: egui::Id) -> egui::Id {
    id.with("rects")
}

pub fn rects(ctx: &egui::Context, id: egui::Id) -> Option<SliderRects> {
    ctx.data(|d| d.get_temp(rects_id(id)))
}

/// Remember where slider `id` was drawn. Rects are in the ui's own layer.
pub fn remember(ui: &Ui, id: egui::Id, rail: Rect, bounds: Option<[Rect; 2]>) {
    let to_window = ui.ctx().layer_transform_to_global(ui.layer_id()).unwrap_or_default();
    let rects = SliderRects { rail: to_window * rail, bounds: bounds.map(|b| b.map(|r| to_window * r)) };
    ui.data_mut(|d| d.insert_temp(rects_id(id), rects));
}

pub struct Row {
    pub rail: Response,
    pub changed: bool,
}

/// A float slider with a fixed range. True when the value changed.
/// `hint` is its hover text ("" for none).
pub fn slider(ui: &mut Ui, value: &mut f32, range: (f32, f32), text: &str, log: bool, hint: &str) -> bool {
    let row = ui.scope(|ui| row(ui, value, range, text, log));
    if !hint.is_empty() {
        row.response.on_hover_text(hint);
    }
    row.inner.changed
}

pub(super) fn row(ui: &mut Ui, value: &mut f32, (lo, hi): (f32, f32), text: &str, log: bool) -> Row {
    // A log rail needs a strictly positive span.
    let log = log && lo > 0.0 && hi > lo;
    let before = *value;
    let rate = rate::held(ui);
    ui.horizontal(|ui| {
        let mut shown = before;
        let mut rail = egui::Slider::new(&mut shown, lo..=hi).show_value(false).clamping(egui::SliderClamping::Never);
        if log {
            rail = rail.logarithmic(true);
        }
        let rail = ui.add(rail);
        // Our own value box rather than the slider's: it drags at the rail's
        // per-point rate, so the rate keys mean the same on both.
        let speed = rate::per_point(before, rail.rect.width(), lo, hi, log) as f64 * rate.unwrap_or(1.0) as f64;
        ui.add_sized(
            [NUM_W, rail.rect.height()],
            egui::DragValue::new(&mut shown).speed(speed.max(1e-7)).max_decimals(6),
        );
        ui.label(text);
        match rate_drag(ui, &rail, rate) {
            // The slider's own jump-to-pointer this frame is dropped.
            Some(travel) if travel != 0.0 => *value = rate::dragged(before, travel, lo, hi, log),
            Some(_) => {}
            None => *value = shown,
        }
        Row { rail, changed: *value != before }
    })
    .inner
}

/// Relative rail drag while a rate key is down (wobble's `sens_drag`).
///
/// A drag surface registered over the rail owns the gesture, so the
/// slider's jump-to-pointer never runs: a press with the key already held
/// lands on it, and a drag already running on the rail moves over to it the
/// frame a key goes down. Either way it keeps the drag to the end of the
/// gesture (letting go of the key mid-drag must not snap the value to the
/// pointer), and the rate is read live.
///
/// Returns the knob's travel, in rail-lengths, while it owns the gesture.
fn rate_drag(ui: &mut Ui, rail: &Response, rate: Option<f32>) -> Option<f32> {
    let id = rail.id.with("rate");
    let stolen = ui.ctx().dragged_id() == Some(id);
    if rate.is_none() && !stolen {
        return None;
    }
    let surface = ui.interact(rail.rect, id, Sense::drag());
    if !stolen && !surface.dragged() {
        if rail.dragged() {
            // The key went down mid-drag: take the drag over.
            ui.ctx().set_dragged_id(id);
            return Some(0.0);
        }
        return None;
    }
    Some(surface.drag_delta().x / rail.rect.width().max(1.0) * rate.unwrap_or(1.0))
}
