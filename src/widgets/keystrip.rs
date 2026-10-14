//! A row of small key caps along the top of a view; hover it and what
//! each does fans out under it, the words slanting down at 45°, one after
//! another left to right. A key that is held lights its cap. Too many for
//! one row, they go on in another under it, and only the row hovered
//! fans out its words. (wobble's `hudkeys::TopStrip`; the node canvas and
//! the game window each have one.)

use std::f32::consts::FRAC_PI_4;

use egui::{pos2, vec2, Align2, Color32, FontId, LayerId, Order, Painter, Pos2, Rect, Stroke, StrokeKind, Ui, Vec2};

/// Caps: how far apart at least, how tall, the room between rows, their
/// text's size.
const PITCH: f32 = 28.0;
const CAP_H: f32 = 16.0;
const ROW_GAP: f32 = 8.0;
const TEXT: f32 = 11.0;
/// The words: their slant, how much later each starts than the one before
/// (a share of the reveal), how far each slides out.
const ANGLE: f32 = FRAC_PI_4;
const STAGGER: f32 = 0.06;
const SLIDE: f32 = 14.0;
/// How fast the reveal follows the pointer (a second's share, as a rate).
const RATE: f32 = 12.0;
/// How near the words the pointer keeps them out.
const HOLD: f32 = 6.0;

const FG: Color32 = Color32::from_gray(0xd4);

/// A strip's state: which row's words are out, and how far (0..1).
#[derive(Default, Clone, Copy)]
pub struct Reveal {
    pub row: usize,
    pub amount: f32,
}

/// One key: what its cap says, what it does, and whether it is held.
pub struct Cap {
    pub cap: &'static str,
    pub does: &'static str,
    pub held: bool,
}

/// The strip from the top left of `area` (window coordinates), across it
/// and down a row at a time as needed; drawn over `clip`. `reveal`: which
/// row's words are out and how far, followed toward the pointer here (or
/// held out, `hold`).
pub fn show(ui: &Ui, area: Rect, clip: Rect, caps: &[Cap], reveal: &mut Reveal, hold: bool) {
    let painter = ui.ctx().layer_painter(LayerId::new(Order::Foreground, ui.id().with("keystrip"))).with_clip_rect(clip);
    let rects = lay_out(&painter, area, caps);
    // Rows, by their caps' tops.
    let mut tops: Vec<f32> = rects.iter().map(|r| r.top()).collect();
    tops.dedup();
    let row_of = |r: &Rect| tops.iter().position(|t| *t == r.top()).unwrap_or(0);
    let in_row = |k: usize| rects.iter().enumerate().filter(move |(_, r)| row_of(r) == k).map(|(i, _)| i);
    let pointer = ui.input(|i| i.pointer.hover_pos());
    let dragging = ui.ctx().dragged_id().is_some();
    // The row the pointer is on; or, while its words are out, over them.
    let on_row = (0..tops.len()).find(|&k| pointer.is_some_and(|p| in_row(k).map(|i| rects[i]).reduce(Rect::union).is_some_and(|b| b.expand(6.0).contains(p))));
    let shown: Vec<usize> = in_row(reveal.row).collect();
    let over_words = reveal.amount > 0.3
        && pointer.is_some_and(|p| shown.iter().any(|&i| on_words(&painter, &rects[i], &caps[i], p)));
    let hot = !dragging && (on_row.is_some() || over_words);
    if let Some(k) = on_row.filter(|k| *k != reveal.row) {
        *reveal = Reveal { row: k, amount: 0.0 };
    }
    let target = if hot || hold { 1.0 } else { 0.0 };
    let dt = ui.input(|i| i.stable_dt).min(0.1);
    reveal.amount += (target - reveal.amount) * (1.0 - (-dt * RATE).exp());
    if (target - reveal.amount).abs() > 0.004 {
        ui.ctx().request_repaint();
    } else {
        reveal.amount = target;
    }
    for (r, c) in rects.iter().zip(caps) {
        let (fg, bg, stroke) = if c.held { (Color32::WHITE, Color32::from_gray(0x30), 1.6) } else { (FG, Color32::from_black_alpha(90), 1.0) };
        // (The other rows step back while one row's words are out.)
        let back = if row_of(r) == reveal.row { 1.0 } else { 1.0 - 0.8 * reveal.amount };
        let (fg, bg) = (fg.gamma_multiply(back), bg.gamma_multiply(back));
        let edge = if c.held { fg } else { fg.gamma_multiply(0.55) };
        painter.rect(*r, 3.0, bg, Stroke::new(stroke, edge), StrokeKind::Outside);
        painter.text(r.center(), Align2::CENTER_CENTER, c.cap, FontId::monospace(TEXT), fg);
    }
    if reveal.amount > 0.0 {
        let row: Vec<usize> = in_row(reveal.row).collect();
        let (r, c): (Vec<Rect>, Vec<&Cap>) = row.iter().map(|&i| (rects[i], &caps[i])).unzip();
        words(&painter, &r, &c, reveal.amount);
    }
}

/// Each cap's rect, left to right from the corner, on down a row when
/// the next would not fit.
fn lay_out(painter: &Painter, area: Rect, caps: &[Cap]) -> Vec<Rect> {
    let (mut x, mut y) = (area.min.x, area.min.y);
    caps.iter()
        .map(|c| {
            let w = (painter.layout_no_wrap(c.cap.to_string(), FontId::monospace(TEXT), FG).size().x + 8.0).max(CAP_H);
            if x > area.min.x && x + w > area.max.x {
                (x, y) = (area.min.x, y + CAP_H + ROW_GAP);
            }
            let r = Rect::from_min_size(pos2(x, y), vec2(w, CAP_H));
            x += (w + 8.0).max(PITCH);
            r
        })
        .collect()
}

/// Turn `v` by the slant.
fn slant(v: Vec2) -> Vec2 {
    let (s, c) = ANGLE.sin_cos();
    vec2(v.x * c - v.y * s, v.x * s + v.y * c)
}

/// Where a cap's words start, slid back `back` along their slant.
fn start(r: &Rect, back: f32) -> Pos2 {
    r.right_bottom() + vec2(3.0, 2.0) - slant(vec2(back, 0.0))
}

/// Whether `p` is on a cap's words, all the way out (the hover keeps them
/// out while it is). Along their slant only: the box round slanted words
/// takes in most of the view, and kept them out wherever the pointer went.
fn on_words(painter: &Painter, r: &Rect, c: &Cap, p: Pos2) -> bool {
    let size = painter.layout_no_wrap(c.does.to_string(), FontId::proportional(TEXT), FG).size();
    on_slant(start(r, 0.0), size.x, size.y * 0.5 + HOLD, p)
}

/// Whether `p` is within `half` of the slanted line `len` long from
/// `from` (or as far past either end).
fn on_slant(from: Pos2, len: f32, half: f32, p: Pos2) -> bool {
    let (u, d) = (slant(vec2(1.0, 0.0)), p - from);
    let along = d.x * u.x + d.y * u.y;
    let across = d.x * u.y - d.y * u.x;
    (-half..=len + half).contains(&along) && across.abs() <= half
}

/// The words, slanting down from each cap, each a little after the one
/// before, easing out and sliding into place.
fn words(painter: &Painter, rects: &[Rect], caps: &[&Cap], reveal: f32) {
    let n = caps.len() as f32;
    for (i, (r, c)) in rects.iter().zip(caps).enumerate() {
        let p = (reveal * (1.0 + STAGGER * n) - STAGGER * i as f32).clamp(0.0, 1.0);
        if p <= 0.0 {
            continue;
        }
        let e = 1.0 - (1.0 - p) * (1.0 - p);
        let fg = if c.held { Color32::WHITE } else { FG }.gamma_multiply(e);
        let galley = painter.layout_no_wrap(c.does.to_string(), FontId::proportional(TEXT), fg);
        let at = start(r, (1.0 - e) * SLIDE) - slant(vec2(0.0, galley.size().y * 0.5));
        let shadow = painter.layout_no_wrap(c.does.to_string(), FontId::proportional(TEXT), Color32::from_black_alpha((200.0 * e) as u8));
        painter.add(egui::epaint::TextShape::new(at + vec2(1.0, 1.0), shadow, Color32::BLACK).with_angle(ANGLE));
        painter.add(egui::epaint::TextShape::new(at, galley, fg).with_angle(ANGLE));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_words_hold_the_pointer_only_along_their_slant() {
        let from = pos2(0.0, 0.0);
        assert!(on_slant(from, 100.0, 8.0, from + slant(vec2(50.0, 0.0))), "on the words");
        assert!(on_slant(from, 100.0, 8.0, from + slant(vec2(50.0, 6.0))), "beside them, near");
        // Inside the box round the words, far off their line: let go.
        assert!(!on_slant(from, 100.0, 8.0, pos2(60.0, 10.0)));
        assert!(!on_slant(from, 100.0, 8.0, from + slant(vec2(130.0, 0.0))), "past their end");
    }
}
