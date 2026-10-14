//! The drawn keyboard: its caps, their layout and geometry, and the colour
//! arithmetic the window paints with.

use egui::{Color32, Key, Modifiers, Rect};

/// One keycap: a real egui key, a modifier (which is not a `Key` in egui —
/// it can only be part of a chord), or a dead cap (caps lock) that exists
/// so the rows read as the keyboard on the desk.
pub(super) struct Cap {
    pub key: Option<Key>,
    pub modifier: Option<ModKey>,
    pub label: &'static str,
    /// Width in key units (a letter is 1.0).
    pub w: f32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum ModKey {
    Cmd,
    Shift,
    Alt,
    Ctrl,
}

fn kcap(key: Key, label: &'static str, w: f32) -> Cap {
    Cap { key: Some(key), modifier: None, label, w }
}
fn mcap(m: ModKey, label: &'static str, w: f32) -> Cap {
    Cap { key: None, modifier: Some(m), label, w }
}
fn dcap(label: &'static str, w: f32) -> Cap {
    Cap { key: None, modifier: None, label, w }
}

/// The ANSI rows, 15 units wide each, esc kept (the usual fixed back /
/// cancel synonym) and the arrow cluster flattened onto the bottom row.
/// `keyboard_rows_line_up` pins the width; an app's test should pin that
/// its every default is drawable here (see the toy one in `tests.rs`).
pub(super) fn keyboard_rows() -> Vec<Vec<Cap>> {
    use Key::*;
    use ModKey::*;
    vec![
        vec![kcap(Escape, "esc", 1.3), kcap(F1, "F1", 1.0), kcap(F2, "F2", 1.0), kcap(F3, "F3", 1.0)],
        vec![
            kcap(Backtick, "`", 1.0),
            kcap(Num1, "1", 1.0),
            kcap(Num2, "2", 1.0),
            kcap(Num3, "3", 1.0),
            kcap(Num4, "4", 1.0),
            kcap(Num5, "5", 1.0),
            kcap(Num6, "6", 1.0),
            kcap(Num7, "7", 1.0),
            kcap(Num8, "8", 1.0),
            kcap(Num9, "9", 1.0),
            kcap(Num0, "0", 1.0),
            kcap(Minus, "-", 1.0),
            kcap(Equals, "=", 1.0),
            kcap(Backspace, "delete", 2.0),
        ],
        vec![
            kcap(Tab, "tab", 1.5),
            kcap(Q, "Q", 1.0),
            kcap(W, "W", 1.0),
            kcap(E, "E", 1.0),
            kcap(R, "R", 1.0),
            kcap(T, "T", 1.0),
            kcap(Y, "Y", 1.0),
            kcap(U, "U", 1.0),
            kcap(I, "I", 1.0),
            kcap(O, "O", 1.0),
            kcap(P, "P", 1.0),
            kcap(OpenBracket, "[", 1.0),
            kcap(CloseBracket, "]", 1.0),
            kcap(Backslash, "\\", 1.5),
        ],
        vec![
            dcap("caps", 1.75),
            kcap(A, "A", 1.0),
            kcap(S, "S", 1.0),
            kcap(D, "D", 1.0),
            kcap(F, "F", 1.0),
            kcap(G, "G", 1.0),
            kcap(H, "H", 1.0),
            kcap(J, "J", 1.0),
            kcap(K, "K", 1.0),
            kcap(L, "L", 1.0),
            kcap(Semicolon, ";", 1.0),
            kcap(Quote, "'", 1.0),
            kcap(Enter, "return", 2.25),
        ],
        vec![
            mcap(Shift, "shift", 2.25),
            kcap(Z, "Z", 1.0),
            kcap(X, "X", 1.0),
            kcap(C, "C", 1.0),
            kcap(V, "V", 1.0),
            kcap(B, "B", 1.0),
            kcap(N, "N", 1.0),
            kcap(M, "M", 1.0),
            kcap(Comma, ",", 1.0),
            kcap(Period, ".", 1.0),
            kcap(Slash, "/", 1.0),
            mcap(Shift, "shift", 2.75),
        ],
        vec![
            mcap(Ctrl, "ctrl", 1.25),
            mcap(Alt, "alt", 1.25),
            mcap(Cmd, "⌘", 1.5),
            kcap(Space, "", 5.5),
            mcap(Cmd, "⌘", 1.5),
            kcap(ArrowLeft, "⏴", 1.0),
            kcap(ArrowUp, "⏶", 1.0),
            kcap(ArrowDown, "⏷", 1.0),
            kcap(ArrowRight, "⏵", 1.0),
        ],
    ]
}

/// A word ("tab", "return") small; one glyph ("Q", "⌘", "⏶") larger.
/// Proportional for both: the monospace face has no backtick.
pub(super) fn cap_font(label: &str) -> egui::FontId {
    if label.chars().count() > 1 {
        egui::FontId::proportional(9.5 * KB_SCALE)
    } else {
        egui::FontId::proportional(13.0 * KB_SCALE)
    }
}

pub(super) fn mod_in(m: Modifiers, which: ModKey) -> bool {
    match which {
        ModKey::Cmd => m.command || m.mac_cmd,
        ModKey::Shift => m.shift,
        ModKey::Alt => m.alt,
        ModKey::Ctrl => m.ctrl,
    }
}

/// The board's one size knob: caps, gaps, labels, dots, strokes and the
/// legend all scale with it. The list and the inspector do not.
pub(super) const KB_SCALE: f32 = 2.0;
pub(super) const KB_U: f32 = 30.0 * KB_SCALE;
pub(super) const KB_GAP: f32 = 3.0 * KB_SCALE;
pub(super) const KB_H: f32 = 28.0 * KB_SCALE;
pub(super) const LIST_W: f32 = 300.0;
pub(super) const ROW_H: f32 = 22.0;
pub(super) const INSPECT_H: f32 = 132.0;

pub(super) fn cap_w(units: f32) -> f32 {
    units * KB_U + (units - 1.0) * KB_GAP
}

pub(super) fn board_size(rows: &[Vec<Cap>]) -> egui::Vec2 {
    egui::vec2(cap_w(15.0), rows.len() as f32 * (KB_H + KB_GAP) - KB_GAP)
}

/// Where cap (row, col) sits, relative to the board's top-left. A wide cap
/// absorbs the gaps it spans, so the columns line up across rows. Shared
/// by the painter and the pixel test, so the test looks where the paint
/// went.
pub(super) fn cap_rect(rows: &[Vec<Cap>], ri: usize, ci: usize) -> Rect {
    let x: f32 = rows[ri][..ci].iter().map(|c| cap_w(c.w) + KB_GAP).sum();
    let y = ri as f32 * (KB_H + KB_GAP);
    Rect::from_min_size(egui::pos2(x, y), egui::vec2(cap_w(rows[ri][ci].w), KB_H))
}

/// An OPAQUE blend. `gamma_multiply(..).to_opaque()` looks like a tint but
/// is not one: `to_opaque` un-premultiplies, which hands back the colour
/// at full strength.
pub(super) fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

/// A colour a few steps lighter (or darker) in every channel.
pub(super) fn lift(c: Color32, by: i32) -> Color32 {
    let l = |x: u8| (x as i32 + by).clamp(0, 255) as u8;
    Color32::from_rgb(l(c.r()), l(c.g()), l(c.b()))
}
