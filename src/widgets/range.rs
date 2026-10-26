//! Re-aimable slider ranges, as in wobble: a slider's two bounds can be
//! shown as drag fields. Drag (or click and type) to change what the rail
//! spans; right-click a bound to go back to the built-in range. The value
//! itself is untouched: the range is soft.
//!
//! Two ways to bring the fields up:
//! - hold shift over one rail: its fields float over its ends
//! - ranges mode (A, bare, over any window; [`toggle`]): every slider shows
//!   its fields in a row of their own, so whatever holds the slider grows
//!
//! A labelled [`super::slider`] keeps its range here, by its composer key
//! (`"Light/Sun/strength"`), in one store for the whole app: load it with
//! [`load`] and save what [`take_edited`] hands back. A slider whose range
//! lives elsewhere (a node's parameters) calls [`range_slider`] itself.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use egui::{vec2, Key, Rect, Response, Ui};

use super::rate;
use super::slider::{self, SliderOut};

const BOUND_W: f32 = 50.0;
const BOUND_H: f32 = 18.0;
const ACCENT: egui::Color32 = egui::Color32::from_rgb(196, 132, 40);

/// Ranges mode, app-wide: one A shows every window's fields.
static SHOWN: AtomicBool = AtomicBool::new(false);

/// Each labelled slider's re-aimed range, by key.
static STORE: Mutex<Store> = Mutex::new(Store { map: BTreeMap::new(), edited: false });

struct Store {
    map: BTreeMap<String, [f32; 2]>,
    edited: bool,
}

fn store<R>(f: impl FnOnce(&mut Store) -> R) -> R {
    f(&mut STORE.lock().unwrap_or_else(|e| e.into_inner()))
}

/// Ranges mode: every range slider shows its bound fields.
pub fn shown() -> bool {
    SHOWN.load(Ordering::Relaxed)
}

pub fn toggle() {
    SHOWN.fetch_xor(true, Ordering::Relaxed);
}

/// A bare A in this window's pass toggles ranges mode, once a pass (every
/// slider drawn asks; the first to ask in a pass answers). Not while a text
/// field is typed in, and not for a key a panel took for itself.
pub fn poll_key(ctx: &egui::Context) {
    let id = egui::Id::new("imfram-range-key-pass");
    let pass = ctx.cumulative_pass_nr();
    if ctx.data(|d| d.get_temp::<u64>(id)) == Some(pass) {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(id, pass));
    if !ctx.text_edit_focused() && ctx.input(|i| i.key_pressed(Key::A) && i.modifiers.is_none()) {
        toggle();
    }
}

/// Replace the stored ranges (an app's saved file).
pub fn load(map: BTreeMap<String, [f32; 2]>) {
    store(|s| {
        s.map = map;
        s.edited = false;
    });
}

/// Every stored range, when one changed since the last call: save it.
pub fn take_edited() -> Option<BTreeMap<String, [f32; 2]>> {
    store(|s| std::mem::take(&mut s.edited).then(|| s.map.clone()))
}

/// The stored range for `key`, if it was re-aimed.
pub fn get(key: &str) -> Option<[f32; 2]> {
    store(|s| s.map.get(key).copied())
}

fn set(key: &str, range: Option<[f32; 2]>) {
    store(|s| {
        match range {
            Some(r) => s.map.insert(key.to_owned(), r),
            None => s.map.remove(key),
        };
        s.edited = true;
    });
}

/// A labelled slider's row, its range kept in the store under `key`.
pub(super) fn keyed_row(ui: &mut Ui, key: &str, value: &mut f32, default: (f32, f32), text: &str, log: bool) -> SliderOut {
    let mut custom = get(key);
    let out = range_slider(ui, egui::Id::new(("imfram-range", key)), value, default, &mut custom, text, log);
    if out.range_changed {
        set(key, custom);
    }
    out
}

/// A float slider with a user-adjustable range. `custom` is the stored
/// override of `default`, if any.
pub fn range_slider(
    ui: &mut Ui,
    id: egui::Id,
    value: &mut f32,
    default: (f32, f32),
    custom: &mut Option<[f32; 2]>,
    text: &str,
    log: bool,
) -> SliderOut {
    poll_key(ui.ctx());
    let cur = custom.map_or(default, |r| (r[0], r[1]));
    let rail_w = ui.spacing().slider_width;
    // A bound drags at the rail's own rate.
    let speed = rate::rated(ui, ((cur.1 - cur.0).abs() / rail_w.max(1.0)) as f64).max(1e-6);
    let (mut lo, mut hi) = cur;

    // Ranges mode: the fields get a row above the rail, one over each end.
    let inline = shown().then(|| ui.horizontal(|ui| bound_fields(ui, &mut lo, &mut hi, rail_w, speed)).inner);
    let row = slider::row(ui, value, cur, text, log);
    let rail = row.rail.rect;
    let fields = inline.or_else(|| {
        let hovered = row.rail.contains_pointer() || row.rail.hovered();
        floating_fields(ui, id, rail, hovered, &mut lo, &mut hi, speed)
    });

    slider::remember(ui, id, rail, fields.as_ref().map(|f| f.rects));
    let range_changed = fields.is_some_and(|f| apply(&f, lo, hi, cur, custom));
    SliderOut { value_changed: row.changed, range_changed }
}

/// The two bound fields, as drawn this frame.
struct Fields {
    lo: Response,
    hi: Response,
    /// In the layer of the slider they belong to.
    rects: [Rect; 2],
}

/// The fields side by side, `rail_w` apart end to end when there is room.
fn bound_fields(ui: &mut Ui, lo: &mut f32, hi: &mut f32, rail_w: f32, speed: f64) -> Fields {
    // Accent: these are the rail's bounds, not the row's value.
    let v = ui.visuals_mut();
    v.override_text_color = Some(egui::Color32::BLACK);
    for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active] {
        w.bg_fill = ACCENT;
        w.weak_bg_fill = ACCENT;
    }
    // (Clicked to type, the field is a text edit, which has its own
    // background: left dark, the black digits were unreadable.)
    v.text_edit_bg_color = Some(ACCENT);
    v.selection.bg_fill = egui::Color32::from_rgb(120, 80, 20);
    ui.spacing_mut().item_spacing.x = 0.0;
    let field = |ui: &mut Ui, v: &mut f32, what: &str| {
        ui.add_sized([BOUND_W, BOUND_H], egui::DragValue::new(v).speed(speed).max_decimals(4))
            .on_hover_text(format!("slider range {what}: drag or type\nright-click: built-in range"))
    };
    let lo = field(ui, lo, "minimum");
    ui.add_space((rail_w - 2.0 * BOUND_W).max(2.0));
    let hi = field(ui, hi, "maximum");
    Fields { rects: [lo.rect, hi.rect], lo, hi }
}

/// Shift-hover: the fields float over the rail's ends. They stay while the
/// pointer is on them, or one is being dragged or typed into (shift may
/// well come up mid-edit).
fn floating_fields(ui: &mut Ui, id: egui::Id, rail: Rect, hovered: bool, lo: &mut f32, hi: &mut f32, speed: f64) -> Option<Fields> {
    let live_id = id.with("range-live");
    let shift = ui.input(|i| i.modifiers.shift);
    let keep: bool = ui.data(|d| d.get_temp(live_id)).unwrap_or(false);
    if !(keep || (shift && hovered)) {
        return None;
    }
    // The fields float in window space; the slider may sit on a zoomed canvas.
    let to_window = ui.ctx().layer_transform_to_global(ui.layer_id()).unwrap_or_default();
    let rail_win = to_window * rail;
    let mut fields = egui::Area::new(id.with("range-edit"))
        .order(egui::Order::Tooltip)
        .fade_in(false)
        // Flush against the rail's top: a gap would be a strip where the
        // pointer hovers nothing on its way up, and the editor blinks out.
        .fixed_pos(rail_win.left_top() - vec2(0.0, BOUND_H))
        .show(ui.ctx(), |ui| ui.horizontal(|ui| bound_fields(ui, lo, hi, rail_win.width(), speed)).inner)
        .inner;
    // Read every response BEFORE data_mut: egui's lock is not re-entrant.
    let live = fields.lo.dragged()
        || fields.hi.dragged()
        || fields.lo.has_focus()
        || fields.hi.has_focus()
        || (shift && (hovered || fields.lo.hovered() || fields.hi.hovered()));
    ui.data_mut(|d| d.insert_temp(live_id, live));
    // `remember` wants the slider's own layer.
    let from_window = to_window.inverse();
    fields.rects = fields.rects.map(|r| from_window * r);
    Some(fields)
}

/// Take this frame's edits to the bounds. True when the range changed.
fn apply(fields: &Fields, mut lo: f32, mut hi: f32, cur: (f32, f32), custom: &mut Option<[f32; 2]>) -> bool {
    if fields.lo.secondary_clicked() || fields.hi.secondary_clicked() {
        return custom.take().is_some();
    }
    if !(fields.lo.changed() || fields.hi.changed()) {
        return false;
    }
    // Bounds stay ordered; the edited one gives way.
    let eps = ((cur.1 - cur.0).abs() * 1e-3).max(1e-6);
    if fields.lo.changed() {
        lo = lo.min(hi - eps);
    }
    if fields.hi.changed() {
        hi = hi.max(lo + eps);
    }
    *custom = Some([lo, hi]);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stored_range_is_handed_back_once_for_saving() {
        load(BTreeMap::new());
        assert_eq!(take_edited(), None);
        set("Light/strength", Some([0.0, 10.0]));
        assert_eq!(get("Light/strength"), Some([0.0, 10.0]));
        assert_eq!(take_edited().and_then(|m| m.get("Light/strength").copied()), Some([0.0, 10.0]));
        assert_eq!(take_edited(), None, "only once");
        set("Light/strength", None);
        assert_eq!(get("Light/strength"), None);
        load(BTreeMap::new());
    }
}
