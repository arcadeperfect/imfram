//! The row's two dots: the reset dot at the left, the key dot at the right.

use egui::{vec2, Color32, Rect, Response, Stroke, Ui};

use super::{KeyState, DOT_R, KEY_R};

pub(super) fn key_tip(resp: &Response, state: KeyState) {
    let what = match state {
        KeyState::None => "not keyed",
        KeyState::Keyed => "keyed here",
        KeyState::Animated => "animated (keys elsewhere)",
        KeyState::Driven => "driven",
    };
    resp.clone().on_hover_text(format!("{what} · click to toggle a key"));
}

/// A filled dot as geometry. egui paints a small `circle_filled` as a
/// sprite from its font atlas, which a headless snapshot (and anything else
/// reading the mesh) cannot see; a 16-gon of this size is the same picture.
pub(super) fn disc(p: &egui::Painter, c: egui::Pos2, r: f32, colour: Color32) {
    let pts = (0..16).map(|i| {
        let a = i as f32 / 16.0 * std::f32::consts::TAU;
        c + vec2(a.cos(), a.sin()) * r
    });
    p.add(egui::Shape::convex_polygon(pts.collect(), colour, Stroke::NONE));
}

/// The key dot: hollow dot, filled diamond, hollow diamond, filled circle.
pub(super) fn paint_key(ui: &Ui, rect: Rect, state: KeyState, hovered: bool) {
    let v = ui.visuals();
    let (colour, filled, diamond) = match state {
        KeyState::None => (v.weak_text_color(), false, false),
        KeyState::Keyed => (v.selection.bg_fill, true, true),
        KeyState::Animated => (v.hyperlink_color, false, true),
        KeyState::Driven => (v.warn_fg_color, true, false),
    };
    let colour = if hovered && state == KeyState::None { v.widgets.hovered.fg_stroke.color } else { colour };
    let c = rect.center();
    let p = ui.painter();
    let stroke = Stroke::new(1.2, colour);
    if diamond {
        let pts = vec![c + vec2(0.0, -KEY_R), c + vec2(KEY_R, 0.0), c + vec2(0.0, KEY_R), c + vec2(-KEY_R, 0.0)];
        let fill = if filled { colour } else { Color32::TRANSPARENT };
        p.add(egui::Shape::convex_polygon(pts, fill, stroke));
    } else if filled {
        disc(p, c, KEY_R, colour);
    } else {
        p.circle_stroke(c, DOT_R, stroke);
    }
}
