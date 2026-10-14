//! Drag rates, as in wobble: hold a key on the bottom row while dragging a
//! value to change how far it moves. Z X C slow the drag down (Z finest),
//! V B N speed it up (N coarsest). Held, not toggled, so the rate can be
//! stepped in the middle of a drag.

use egui::{Key, Ui};

/// Mirrored in log space around 1 (C x V = 1, X x B = 1). Same table as
/// wobble's `rgbslider::RATE_KEYS`.
pub const RATE_KEYS: [(Key, f32); 6] = [
    (Key::Z, 0.01),
    (Key::X, 0.1),
    (Key::C, 1.0 / 3.0),
    (Key::V, 3.0),
    (Key::B, 10.0),
    (Key::N, 30.0),
];

/// The held rate, if any. Bare keys only (cmd+Z is undo, not "finest"),
/// and not while a text field is being typed in.
pub fn held(ui: &Ui) -> Option<f32> {
    if ui.ctx().text_edit_focused() {
        return None;
    }
    ui.input(|i| {
        let m = i.modifiers;
        if m.command || m.mac_cmd || m.ctrl || m.alt {
            return None;
        }
        RATE_KEYS.iter().find(|(k, _)| i.key_down(*k)).map(|&(_, rate)| rate)
    })
}

/// What to scale a drag's travel by: the held rate, or 1.
pub fn drag_rate(ui: &Ui) -> f32 {
    held(ui).unwrap_or(1.0)
}

/// A drag speed with the held rate folded in: what a bare `DragValue`
/// should be given, so the rate keys work on it too.
pub fn rated(ui: &Ui, base: f64) -> f64 {
    base * held(ui).unwrap_or(1.0) as f64
}

/// Where a value lands when its knob moves `travel` rail-lengths (the rate
/// already applied). On a log rail equal travel is an equal ratio.
pub fn dragged(value: f32, travel: f32, lo: f32, hi: f32, log: bool) -> f32 {
    if log && lo > 0.0 && hi > lo && value > 0.0 {
        value * (hi / lo).powf(travel)
    } else {
        value + travel * (hi - lo)
    }
}

/// Value units per point of pointer travel along a rail `width` points long.
pub fn per_point(value: f32, width: f32, lo: f32, hi: f32, log: bool) -> f32 {
    let width = width.max(1.0);
    if log && lo > 0.0 && hi > lo {
        value.abs().max(lo) * (hi / lo).ln() / width
    } else {
        (hi - lo).abs() / width
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rates_mirror_around_one() {
        let rate = |k| RATE_KEYS.iter().find(|(key, _)| *key == k).unwrap().1;
        assert!((rate(Key::C) * rate(Key::V) - 1.0).abs() < 1e-6);
        assert!((rate(Key::X) * rate(Key::B) - 1.0).abs() < 1e-6);
        assert!(rate(Key::Z) < rate(Key::X) && rate(Key::B) < rate(Key::N), "Z finest, N coarsest");
    }

    #[test]
    fn a_full_rail_of_travel_spans_the_range() {
        assert_eq!(dragged(0.0, 1.0, 0.0, 4.0, false), 4.0);
        assert_eq!(dragged(1.0, 0.25, 0.0, 4.0, false), 2.0);
        // At the finest rate the same pointer travel moves 1/100 as far.
        assert!((dragged(1.0, 0.25 * 0.01, 0.0, 4.0, false) - 1.01).abs() < 1e-6);
    }

    #[test]
    fn on_a_log_rail_travel_is_a_ratio() {
        let end = dragged(0.001, 1.0, 0.001, 0.1, true);
        assert!((end - 0.1).abs() < 1e-6, "{end}");
        let mid = dragged(0.001, 0.5, 0.001, 0.1, true);
        assert!((mid - 0.01).abs() < 1e-6, "halfway along is the geometric mean, got {mid}");
        // A value that cannot sit on a log rail falls back to linear.
        assert_eq!(dragged(0.0, 0.5, 0.001, 0.1, true), 0.0495);
    }

    #[test]
    fn the_value_box_drags_at_the_rails_rate() {
        assert_eq!(per_point(0.0, 100.0, 0.0, 4.0, false), 0.04);
        let slow = per_point(0.001, 100.0, 0.001, 0.1, true);
        let fast = per_point(0.1, 100.0, 0.001, 0.1, true);
        assert!(fast > slow * 50.0, "a log rail moves faster where values are bigger");
    }
}
