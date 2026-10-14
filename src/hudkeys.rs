//! The floating hotkey strip a viewer HUD shows while it is up: one key cap
//! per gesture along the top of the viewer, its short description fanning
//! down from the cap at 45° while the hand hovers the row — the same strip
//! the node graph carries along its top edge.
//!
//! The general rule: a HUD with held-key or modifier gestures lists them
//! here, so the hands can find them without the hotkeys window. The words
//! are shadowed so they hold on any footage. A held key lights its cap.
//! `Prefs::hud_keys` hides it. Depends on egui only.

use egui::Rect;

/// One gesture: the key as drawn on the cap ("E", "alt-drag") and what it
/// does, in a few words.
#[derive(Clone, Copy, Debug)]
pub struct HudKey {
    pub key: &'static str,
    pub label: &'static str,
    /// The key that lights the cap while held, if it is a key at all.
    pub held: Option<egui::Key>,
}

pub const fn key(key: &'static str, label: &'static str, held: egui::Key) -> HudKey {
    HudKey { key, label, held: Some(held) }
}

pub const fn chord(key: &'static str, label: &'static str) -> HudKey {
    HudKey { key, label, held: None }
}

const PITCH: f32 = 28.0;
const CAP_H: f32 = 16.0;
const TEXT: f32 = 11.0;

/// The viewer's gestures as [`TopStrip`] items, each cap lit while its key
/// is held — so the viewer's strip is drawn exactly like the graph's.
pub fn items(ui: &egui::Ui, keys: &[HudKey]) -> Vec<(String, String, bool)> {
    keys.iter()
        .map(|k| {
            let lit = k.held.is_some_and(|h| ui.input(|i| i.key_down(h)));
            (k.key.to_string(), k.label.to_string(), lit)
        })
        .collect()
}

/// The TOP strip: caps along the very top edge of `area`, their words
/// fanning DOWN and to the right — revealed only while the hand hovers the
/// strip, so the top edge carries a row of small caps and nothing more
/// until you ask. The node graph's hint (`editor.rs`) and the viewer's.
///
/// `items`: (cap text, words, lit). Split in two passes because the caps
/// are a hint layer drawn UNDER the graph while the words, once asked for,
/// are read ON TOP of it.
pub struct TopStrip {
    caps: Vec<(Rect, String, String, bool)>,
}

/// How far below the top edge the caps sit.
const TOP_Y: f32 = 14.0;
/// Down and to the right.
const ANGLE_DOWN: f32 = std::f32::consts::FRAC_PI_4;

fn rot_by(v: egui::Vec2, a: f32) -> egui::Vec2 {
    let (s, c) = a.sin_cos();
    egui::vec2(v.x * c - v.y * s, v.x * s + v.y * c)
}

impl TopStrip {
    pub fn new(painter: &egui::Painter, area: Rect, items: &[(String, String, bool)]) -> Self {
        let cap_font = egui::FontId::monospace(TEXT);
        let mut x = area.min.x + 12.0;
        let y = area.min.y + TOP_Y;
        let caps = items
            .iter()
            .map(|(k, label, lit)| {
                let w = (painter
                    .layout_no_wrap(k.clone(), cap_font.clone(), egui::Color32::WHITE)
                    .size()
                    .x
                    + 8.0)
                    .max(CAP_H);
                let r = Rect::from_min_size(egui::pos2(x, y), egui::vec2(w, CAP_H));
                x += (w + 8.0).max(PITCH);
                (r, k.clone(), label.clone(), *lit)
            })
            .collect();
        Self { caps }
    }

    /// The row of caps (what hovering reveals the words from).
    pub fn row(&self) -> Rect {
        self.caps.iter().fold(Rect::NOTHING, |a, (r, ..)| a.union(*r))
    }

    /// Where the words land when fully out — the hover zone once they are
    /// showing, so moving down to read one does not put it away.
    pub fn fan(&self, painter: &egui::Painter) -> Rect {
        let font = egui::FontId::proportional(TEXT);
        let mut bb = self.row();
        for (r, _, label, _) in &self.caps {
            let w = painter
                .layout_no_wrap(label.clone(), font.clone(), egui::Color32::WHITE)
                .size()
                .x;
            let from = r.right_bottom() + egui::vec2(3.0, 2.0);
            bb = bb.union(Rect::from_two_pos(from, from + rot_by(egui::vec2(w, 0.0), ANGLE_DOWN)));
        }
        bb.expand(8.0)
    }

    /// The caps, lit while held. Hint layer: call before the wires.
    pub fn draw_caps(&self, painter: &egui::Painter) {
        let cap_font = egui::FontId::monospace(TEXT);
        for (r, k, _, lit) in &self.caps {
            let (fg, bg) = if *lit {
                (egui::Color32::WHITE, egui::Color32::from_gray(0x30))
            } else {
                (egui::Color32::from_gray(0xd4), egui::Color32::from_black_alpha(90))
            };
            painter.rect(
                *r,
                3.0,
                bg,
                egui::Stroke::new(
                    if *lit { 1.6 } else { 1.0 },
                    fg.gamma_multiply(if *lit { 1.0 } else { 0.55 }),
                ),
                egui::StrokeKind::Outside,
            );
            painter.text(r.center(), egui::Align2::CENTER_CENTER, k, cap_font.clone(), fg);
        }
    }

    /// The words, `reveal` 0..1 of the way out: each slides out of its own
    /// cap along the down-right ray and fades in, a beat after its left
    /// neighbour, so the row unfolds left to right. Call on top.
    pub fn draw_words(&self, painter: &egui::Painter, reveal: f32) {
        if reveal <= 0.001 {
            return;
        }
        let font = egui::FontId::proportional(TEXT);
        let n = self.caps.len().max(1) as f32;
        const STAGGER: f32 = 0.06;
        for (i, (r, _, label, lit)) in self.caps.iter().enumerate() {
            let p = (reveal * (1.0 + STAGGER * n) - STAGGER * i as f32).clamp(0.0, 1.0);
            if p <= 0.0 {
                continue;
            }
            // Ease out: quick to leave the cap, gentle to settle.
            let e = 1.0 - (1.0 - p) * (1.0 - p);
            let a = (255.0 * e) as u8;
            let fg = if *lit {
                egui::Color32::from_white_alpha(a)
            } else {
                egui::Color32::from_rgba_unmultiplied(0xd4, 0xd4, 0xd4, a)
            };
            let bg = egui::Color32::from_black_alpha((200.0 * e) as u8);
            let g = painter.layout_no_wrap(label.clone(), font.clone(), fg);
            let from = r.right_bottom() + egui::vec2(3.0, 2.0)
                - rot_by(egui::vec2((1.0 - e) * 14.0, 0.0), ANGLE_DOWN);
            let pos = from - rot_by(egui::vec2(0.0, g.size().y * 0.5), ANGLE_DOWN);
            let shadow = painter.layout_no_wrap(label.clone(), font.clone(), bg);
            painter.add(
                egui::epaint::TextShape::new(pos + egui::vec2(1.0, 1.0), shadow, bg)
                    .with_angle(ANGLE_DOWN),
            );
            painter.add(egui::epaint::TextShape::new(pos, g, fg).with_angle(ANGLE_DOWN));
        }
    }
}

/// How a legend dot draws.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DotStyle {
    Plain,
    /// With a halo of its own colour: a LIT wire.
    Glow,
    /// Bigger: the fat multi-channel cable.
    Fat,
}

/// The TOP-RIGHT twin of [`TopStrip`]: a row of coloured dots on the top
/// edge, right-aligned, each one a thing the graph's colour can mean. The
/// words appear only while the hand hovers the row, and they are the key
/// strip's mirror image: they hang down and to the LEFT of the dots and
/// read UP toward them, so nothing runs off the right edge of the pane.
/// LEFT-JUSTIFIED: every label starts on one line (the first letters form
/// a tidy edge, like rotated column headers), so a short label ends short
/// of its dot — a faint leader in the dot's own colour closes the gap.
pub struct DotStrip {
    dots: Vec<(egui::Pos2, egui::Color32, DotStyle, String)>,
}

const DOT_R: f32 = 5.0;
/// Parallel 45° labels sit `pitch · sin 45°` apart: 20 clears 11px type.
const DOT_PITCH: f32 = 20.0;
/// Up and to the right — the direction the words READ, into their dot.
const ANGLE_UP: f32 = -std::f32::consts::FRAC_PI_4;

impl DotStrip {
    /// `avoid`: the key strip's cap row. On a pane too narrow for both on
    /// one line the dots drop to a row of their own under it — sharing a
    /// line, hovering one would also reveal the other.
    pub fn new(
        area: Rect,
        items: &[(egui::Color32, DotStyle, String)],
        avoid: Option<Rect>,
    ) -> Self {
        let n = items.len();
        let place = |y: f32| -> Vec<egui::Pos2> {
            (0..n)
                .map(|i| egui::pos2(area.max.x - 16.0 - (n - 1 - i) as f32 * DOT_PITCH, y))
                .collect()
        };
        let top = area.min.y + TOP_Y + CAP_H * 0.5;
        let mut at = place(top);
        if let (Some(av), Some(first)) = (avoid, at.first()) {
            if first.x - DOT_R - 10.0 < av.max.x {
                at = place(av.max.y + 12.0 + DOT_R);
            }
        }
        let dots = items
            .iter()
            .zip(at)
            .map(|((c, st, label), p)| (p, *c, *st, label.clone()))
            .collect();
        Self { dots }
    }

    pub fn row(&self) -> Rect {
        self.dots.iter().fold(Rect::NOTHING, |a, (p, ..)| {
            a.union(Rect::from_center_size(*p, egui::Vec2::splat(DOT_R * 2.0 + 4.0)))
        })
    }

    /// Where the words land when fully out: the hover zone once they show.
    /// The longest label's width: every label starts where it would.
    fn span(&self, painter: &egui::Painter) -> f32 {
        let font = egui::FontId::proportional(TEXT);
        self.dots
            .iter()
            .map(|(_, _, _, l)| {
                painter.layout_no_wrap(l.clone(), font.clone(), egui::Color32::WHITE).size().x
            })
            .fold(0.0, f32::max)
    }

    pub fn fan(&self, painter: &egui::Painter) -> Rect {
        let span = self.span(painter);
        let mut bb = self.row();
        for (p, ..) in &self.dots {
            let end = *p + egui::vec2(-DOT_R - 3.0, DOT_R + 3.0);
            bb = bb.union(Rect::from_two_pos(end, end - rot_by(egui::vec2(span, 0.0), ANGLE_UP)));
        }
        bb.expand(8.0)
    }

    /// The dots. Hint layer: call before the wires.
    pub fn draw_dots(&self, painter: &egui::Painter) {
        for (p, c, st, _) in &self.dots {
            match st {
                DotStyle::Glow => {
                    for (r, a) in [(DOT_R * 2.4, 0.18f32), (DOT_R * 1.7, 0.35)] {
                        let k = |v: u8| (v as f32 * a) as u8;
                        painter.circle_filled(
                            *p,
                            r,
                            egui::Color32::from_rgba_premultiplied(k(c.r()), k(c.g()), k(c.b()), 0),
                        );
                    }
                    painter.circle_filled(*p, DOT_R, *c);
                }
                DotStyle::Fat => {
                    painter.circle_filled(*p, DOT_R * 1.35, *c);
                }
                DotStyle::Plain => {
                    painter.circle_filled(*p, DOT_R, *c);
                }
            }
        }
    }

    /// The words, `reveal` 0..1 out, fading in and sliding along their ray
    /// a beat after the one to their right (the row unfolds from the
    /// corner). Left-justified on a common start line; a short label's gap
    /// to its dot is a faint leader in the dot's colour. Call on top.
    pub fn draw_words(&self, painter: &egui::Painter, reveal: f32) {
        if reveal <= 0.001 {
            return;
        }
        let font = egui::FontId::proportional(TEXT);
        let span = self.span(painter);
        let n = self.dots.len().max(1) as f32;
        const STAGGER: f32 = 0.06;
        for (i, (p, c, _, label)) in self.dots.iter().enumerate() {
            let order = (self.dots.len() - 1 - i) as f32;
            let t = (reveal * (1.0 + STAGGER * n) - STAGGER * order).clamp(0.0, 1.0);
            if t <= 0.0 {
                continue;
            }
            let e = 1.0 - (1.0 - t) * (1.0 - t);
            let a = (255.0 * e) as u8;
            let fg = egui::Color32::from_rgba_unmultiplied(0xd8, 0xd8, 0xd8, a);
            let bg = egui::Color32::from_black_alpha((200.0 * e) as u8);
            let g = painter.layout_no_wrap(label.clone(), font.clone(), fg);
            let w = g.size().x;
            // The ray's near end (just below-left of the dot) and the
            // common START, where the longest label would begin; sliding
            // out = starting further down the ray and settling.
            let anchor = *p + egui::vec2(-DOT_R - 3.0, DOT_R + 3.0);
            let slide = rot_by(egui::vec2(-(1.0 - e) * 14.0, 0.0), ANGLE_UP);
            let start = anchor - rot_by(egui::vec2(span, 0.0), ANGLE_UP) + slide;
            let pos = start - rot_by(egui::vec2(0.0, g.size().y * 0.5), ANGLE_UP);
            let shadow = painter.layout_no_wrap(label.clone(), font.clone(), bg);
            painter.add(
                egui::epaint::TextShape::new(pos + egui::vec2(1.0, 1.0), shadow, bg)
                    .with_angle(ANGLE_UP),
            );
            painter.add(egui::epaint::TextShape::new(pos, g, fg).with_angle(ANGLE_UP));
            // The leader: from just past the word to just short of the dot.
            if span - w > 10.0 {
                let from = start + rot_by(egui::vec2(w + 4.0, 0.0), ANGLE_UP);
                let to = anchor + slide;
                let k = 0.55 * e;
                let lead = egui::Color32::from_rgba_unmultiplied(
                    c.r(),
                    c.g(),
                    c.b(),
                    (255.0 * k) as u8,
                );
                painter.line_segment([from, to], egui::Stroke::new(1.0, lead));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `cargo test -p app hudkeys_render -- --ignored` writes
    /// `target/hudkeys.png`: the viewer's strip with its words out.
    #[test]
    #[ignore]
    fn hudkeys_render() {
        let keys = [
            key("S", "click: add point", egui::Key::S),
            key("E", "scale about click", egui::Key::E),
            key("R", "rotate about click", egui::Key::R),
            key("T", "move pick", egui::Key::T),
            chord("ctrl", "drag corner: distort"),
        ];
        let out = crate::uishot::render("hudkeys", 520.0, 180.0, 2.0, 3, |ui, _| {
            let area = ui.max_rect();
            ui.painter().rect_filled(area, 0.0, egui::Color32::from_gray(0x50));
            let strip = TopStrip::new(ui.painter(), area, &items(ui, &keys));
            strip.draw_caps(ui.painter());
            strip.draw_words(ui.painter(), 1.0);
        });
        if out.is_none() {
            println!("SKIP: no adapter");
        }
    }

    #[test]
    fn caps_run_left_to_right_along_the_top() {
        let items: Vec<_> = ["S", "ctrl", "E"]
            .iter()
            .map(|k| (k.to_string(), "x".to_string(), false))
            .collect();
        let ctx = egui::Context::default();
        crate::snapshot::discard(ctx.run_ui(Default::default(), |ui| {
            let p = ui.painter().clone();
            let area = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 400.0));
            let s = TopStrip::new(&p, area, &items);
            for w in s.caps.windows(2) {
                assert!(w[1].0.min.x > w[0].0.max.x, "caps must run left to right");
            }
            assert!(s.row().min.y == area.min.y + TOP_Y);
        }));
    }
}
