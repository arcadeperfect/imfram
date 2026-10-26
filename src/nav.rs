//! Blender's navigation hands, one implementation for every 2D canvas
//! (manifesto 11: we are a Blender companion — muscle memory carries).
//!
//! Drags:
//!   MMB                  pan   (shift+MMB too — Blender's pan chord, so a
//!                               3D view can one day split them into
//!                               tumble vs pan without retraining anyone)
//!   ctrl+MMB             drag-zoom about the press point
//!   alt+LMB              pan   ("emulate 3 button mouse": the tablet path)
//!   shift+alt+LMB        pan
//!   ctrl+alt+LMB         drag-zoom
//!   alt/cmd+RMB          drag-zoom (the Houdini dolly this app always had;
//!                               macOS ALSO lands ctrl+alt+LMB here, because
//!                               the OS turns ctrl+LMB into a right-click)
//!
//! Wheel and trackpad (Blender's 2D-editor rules):
//!   wheel                zoom about the cursor
//!   ctrl+wheel           pan sideways
//!   shift+wheel          pan vertically
//!   trackpad two-finger  pan (the canvas follows the fingers)
//!   ctrl+trackpad        zoom
//!   pinch                zoom
//!
//! Every canvas — node graph, viewer, curve editors, patch bay — routes
//! through these two classifiers, so the hands are identical everywhere.
//! Canvases keep their own coordinate maths; this module only answers
//! "what does this input MEAN".

use egui::{Modifiers, PointerButton, Vec2};

/// What a drag with this button + modifier chord is, if it is navigation
/// at all. `None` = not nav; the canvas's own gestures may have it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DragNav {
    Pan,
    Zoom,
}

/// Classify a drag. Pure, so the table above is testable without a window.
pub fn drag_nav(button: PointerButton, m: Modifiers) -> Option<DragNav> {
    let ctrl = m.ctrl || m.command;
    match button {
        PointerButton::Middle => Some(if ctrl { DragNav::Zoom } else { DragNav::Pan }),
        PointerButton::Primary if m.alt => {
            Some(if ctrl { DragNav::Zoom } else { DragNav::Pan })
        }
        // The dolly we always had. Kept: it is also how macOS delivers
        // ctrl(+alt)+LMB, and Houdini hands are welcome where they don't
        // collide with Blender's.
        PointerButton::Secondary if m.alt || m.command => Some(DragNav::Zoom),
        _ => None,
    }
}

/// Multiplicative zoom for a drag-zoom step. Up or right zooms in: the
/// vertical axis is Blender's dolly, the horizontal is the Houdini habit —
/// they never disagree, because each gesture only moves one of them.
pub fn drag_zoom(delta: Vec2) -> f32 {
    ((delta.x - delta.y) * 0.005).exp()
}

/// Per-axis drag-zoom, for the curve editors: Blender's graph editor
/// stretches the view axis by axis under ctrl+MMB — drag right to stretch
/// time, up to stretch values.
pub fn drag_zoom_xy(delta: Vec2) -> (f32, f32) {
    ((delta.x * 0.005).exp(), (-delta.y * 0.005).exp())
}

/// What the wheel/trackpad meant this frame: screen-pixel pan plus a
/// multiplicative zoom (1.0 = none). `alt_wheel_zoom_y`: the curve editors
/// keep their old alt+wheel = value-axis-only zoom, which Blender does not
/// use for anything — those canvases split the returned zoom themselves,
/// so it is reported, not interpreted, here.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct WheelNav {
    pub pan: Vec2,
    pub zoom: f32,
    /// Alt was held on the zooming wheel events (for the curve editors'
    /// value-axis-only zoom). Meaningless when `zoom == 1`.
    pub alt: bool,
}

/// One wheel notch ("line") in pixels. ~16% zoom per notch lands where
/// Blender's per-notch step feels.
pub const LINE_PX: f32 = 50.0;
const WHEEL_ZOOM: f32 = 0.003;
/// Zoom per wheel notch, as the exponent (`exp(NOTCH_LN)` a notch).
pub const NOTCH_LN: f32 = LINE_PX * WHEEL_ZOOM;

/// Classify raw wheel/pinch events. Pure for the same reason as `drag_nav`.
pub fn classify_wheel(events: &[egui::Event]) -> WheelNav {
    let mut out = WheelNav { pan: Vec2::ZERO, zoom: 1.0, alt: false };
    for e in events {
        match e {
            egui::Event::MouseWheel { unit, delta, modifiers, .. } => {
                let line = !matches!(unit, egui::MouseWheelUnit::Point);
                let px = if line { *delta * LINE_PX } else { *delta };
                let ctrl = modifiers.ctrl || modifiers.command;
                if line {
                    if ctrl {
                        // ctrl+wheel: sideways. A wheel only has a y axis,
                        // so its clicks become x travel.
                        out.pan.x += px.y;
                    } else if modifiers.shift {
                        out.pan.y += px.y;
                    } else {
                        out.zoom *= (px.y * WHEEL_ZOOM).exp();
                        out.alt |= modifiers.alt;
                    }
                } else if ctrl {
                    // ctrl + two fingers: zoom, per Blender (and macOS).
                    out.zoom *= (px.y * WHEEL_ZOOM).exp();
                    out.alt |= modifiers.alt;
                } else {
                    // Two fingers move the canvas. Both axes, unscaled.
                    out.pan += px;
                }
            }
            egui::Event::Zoom(f) => {
                out.zoom *= f;
            }
            _ => {}
        }
    }
    out
}

/// The frame's wheel/trackpad navigation for a hovered canvas. Reads RAW
/// events on purpose: egui's `smooth_scroll_delta`/`zoom_delta` bake in
/// their own ctrl-scroll-is-zoom opinion, which is not Blender's.
pub fn wheel_nav(ui: &egui::Ui) -> WheelNav {
    ui.input(|i| classify_wheel(&i.events))
}

/// A pointer press or release that a canvas claims before egui's widgets
/// see it (so a middle-drag over a slider pans instead of scrubbing it).
#[derive(Clone, Copy, Debug)]
pub struct Press {
    pub button: PointerButton,
    pub pos: egui::Pos2,
    pub modifiers: Modifiers,
}

/// Pulls canvas gestures out of the raw input. `claims` decides, at press
/// time, whether a chord belongs to the canvas; the matching release is
/// taken too. (graav's.)
#[derive(Default)]
pub struct GestureFilter {
    held: [bool; 5],
    pub presses: Vec<Press>,
    pub releases: Vec<PointerButton>,
}

impl GestureFilter {
    pub fn process(&mut self, raw: &mut egui::RawInput, claims: impl Fn(PointerButton, Modifiers) -> bool) {
        self.process_at(raw, |_, button, mods| claims(button, mods));
    }

    /// [`process`](Self::process), `claims` told where the press is too:
    /// a canvas that shares its window with panels claims only over itself.
    pub fn process_at(&mut self, raw: &mut egui::RawInput, claims: impl Fn(egui::Pos2, PointerButton, Modifiers) -> bool) {
        self.presses.clear();
        self.releases.clear();
        let slot = |b: PointerButton| b as usize;
        raw.events.retain(|e| match e {
            egui::Event::PointerButton { pos, button, pressed: true, modifiers } => {
                if claims(*pos, *button, *modifiers) {
                    self.held[slot(*button)] = true;
                    self.presses.push(Press { button: *button, pos: *pos, modifiers: *modifiers });
                    false
                } else {
                    true
                }
            }
            egui::Event::PointerButton { button, pressed: false, .. } if self.held[slot(*button)] => {
                self.held[slot(*button)] = false;
                self.releases.push(*button);
                false
            }
            _ => true,
        });
        // A release lost to another window must not leave a gesture stuck.
        let lost = raw.events.iter().any(|e| matches!(e, egui::Event::WindowFocused(false)));
        if lost {
            for (i, held) in self.held.iter_mut().enumerate() {
                if std::mem::take(held) {
                    self.releases.push(BUTTONS[i]);
                }
            }
        }
    }
}

const BUTTONS: [PointerButton; 5] = [
    PointerButton::Primary,
    PointerButton::Secondary,
    PointerButton::Middle,
    PointerButton::Extra1,
    PointerButton::Extra2,
];

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, MouseWheelUnit};

    fn wheel(unit: MouseWheelUnit, d: [f32; 2], m: Modifiers) -> Event {
        Event::MouseWheel {
            unit,
            delta: egui::vec2(d[0], d[1]),
            modifiers: m,
            phase: egui::TouchPhase::Move,
        }
    }

    #[test]
    fn the_drag_table_is_blenders() {
        use DragNav::*;
        use PointerButton::*;
        let m = |ctrl: bool, alt: bool, shift: bool, cmd: bool| Modifiers {
            ctrl,
            alt,
            shift,
            command: cmd,
            mac_cmd: false,
        };
        assert_eq!(drag_nav(Middle, m(false, false, false, false)), Some(Pan));
        assert_eq!(drag_nav(Middle, m(false, false, true, false)), Some(Pan));
        assert_eq!(drag_nav(Middle, m(true, false, false, false)), Some(Zoom));
        assert_eq!(drag_nav(Primary, m(false, true, false, false)), Some(Pan));
        assert_eq!(drag_nav(Primary, m(false, true, true, false)), Some(Pan));
        assert_eq!(drag_nav(Primary, m(true, true, false, false)), Some(Zoom));
        // Plain, shift and bare-ctrl left-drags belong to the canvas
        // (select, scrub, slice) — nav must not eat them.
        assert_eq!(drag_nav(Primary, m(false, false, false, false)), None);
        assert_eq!(drag_nav(Primary, m(false, false, true, false)), None);
        assert_eq!(drag_nav(Primary, m(true, false, false, false)), None);
        // The legacy dolly.
        assert_eq!(drag_nav(Secondary, m(false, true, false, false)), Some(Zoom));
        assert_eq!(drag_nav(Secondary, m(false, false, false, true)), Some(Zoom));
        assert_eq!(drag_nav(Secondary, m(false, false, false, false)), None);
    }

    #[test]
    fn a_wheel_zooms_and_a_trackpad_pans() {
        let none = Modifiers::default();
        let w = classify_wheel(&[wheel(MouseWheelUnit::Line, [0.0, 1.0], none)]);
        assert_eq!(w.pan, Vec2::ZERO);
        assert!(w.zoom > 1.1, "a notch up zooms in, got {}", w.zoom);
        let t = classify_wheel(&[wheel(MouseWheelUnit::Point, [30.0, -12.0], none)]);
        assert_eq!(t.zoom, 1.0);
        assert_eq!(t.pan, egui::vec2(30.0, -12.0));
    }

    #[test]
    fn modifiers_reroute_the_wheel() {
        let ctrl = Modifiers { ctrl: true, ..Default::default() };
        let shift = Modifiers { shift: true, ..Default::default() };
        let w = classify_wheel(&[wheel(MouseWheelUnit::Line, [0.0, 2.0], ctrl)]);
        assert_eq!(w.zoom, 1.0);
        assert!(w.pan.x > 0.0 && w.pan.y == 0.0, "ctrl+wheel pans sideways");
        let w = classify_wheel(&[wheel(MouseWheelUnit::Line, [0.0, 2.0], shift)]);
        assert_eq!(w.zoom, 1.0);
        assert!(w.pan.y > 0.0 && w.pan.x == 0.0, "shift+wheel pans vertically");
        let w = classify_wheel(&[wheel(MouseWheelUnit::Point, [0.0, 40.0], ctrl)]);
        assert!(w.zoom > 1.0 && w.pan == Vec2::ZERO, "ctrl+trackpad zooms");
        let w = classify_wheel(&[Event::Zoom(1.3)]);
        assert!((w.zoom - 1.3).abs() < 1e-6, "pinch is zoom");
    }

    #[test]
    fn drag_zoom_curves_match_the_axes() {
        assert!(drag_zoom(egui::vec2(0.0, -80.0)) > 1.0, "up zooms in");
        assert!(drag_zoom(egui::vec2(80.0, 0.0)) > 1.0, "right zooms in");
        let (fx, fy) = drag_zoom_xy(egui::vec2(80.0, 0.0));
        assert!(fx > 1.0 && (fy - 1.0).abs() < 1e-6, "right stretches time only");
        let (fx, fy) = drag_zoom_xy(egui::vec2(0.0, -80.0));
        assert!(fy > 1.0 && (fx - 1.0).abs() < 1e-6, "up stretches value only");
    }

    #[test]
    fn a_press_outside_the_canvas_is_left_to_the_panels() {
        let canvas = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(100.0, 100.0));
        let press = |x: f32| Event::PointerButton { pos: egui::pos2(x, 50.0), button: PointerButton::Primary, pressed: true, modifiers: Modifiers::ALT };
        let mut raw = egui::RawInput { events: vec![press(50.0), press(150.0)], ..Default::default() };
        let mut f = GestureFilter::default();
        f.process_at(&mut raw, |pos, b, m| canvas.contains(pos) && drag_nav(b, m).is_some());
        assert_eq!(f.presses.len(), 1, "the one over the canvas is claimed");
        assert_eq!(raw.events.len(), 1, "the one over a panel goes on to egui");
    }
}
