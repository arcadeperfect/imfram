//! A ratio lock: a small padlock between two (or more) values that go
//! together — width and height, scale x and y. Locked, editing one moves
//! the others by the same FACTOR, so their ratio holds.
//!
//! The widget is two halves: [`button`] draws and toggles the padlock, and
//! [`keep_ratio`] / [`keep_ratio_u32`] are the maths, applied by the caller
//! after its fields have run. They are separate because the fields are the
//! caller's (keyed rows, driven values, number boxes of its own), and the
//! maths is the part worth a test.

use egui::{Rect, Response, Sense, Stroke, Ui, Vec2};

/// Width the button takes in a row.
pub const W: f32 = 12.0;

/// The padlock, toggled by a click. Closed and lit when locked; open and
/// dim when not. Returns the click response.
pub fn button(ui: &mut Ui, locked: &mut bool) -> Response {
    let h = ui.spacing().interact_size.y.min(18.0);
    let (rect, mut r) = ui.allocate_exact_size(Vec2::new(W, h), Sense::click());
    if r.clicked() {
        *locked = !*locked;
        r.mark_changed();
    }
    if ui.is_rect_visible(rect) {
        paint(ui, rect, *locked, r.hovered());
    }
    r.on_hover_text(if *locked {
        "ratio locked: editing one keeps the ratio (click to unlock)"
    } else {
        "lock the ratio"
    })
}

fn paint(ui: &Ui, rect: Rect, locked: bool, hot: bool) {
    let v = ui.visuals();
    let col = if locked {
        v.selection.stroke.color
    } else if hot {
        v.widgets.hovered.fg_stroke.color
    } else {
        v.widgets.inactive.fg_stroke.color.gamma_multiply(0.6)
    };
    let p = ui.painter();
    let c = rect.center();
    // Body: a small filled block in the lower half.
    let body = Rect::from_center_size(c + Vec2::new(0.0, 2.5), Vec2::new(8.0, 6.0));
    if locked {
        p.rect_filled(body, 1.0, col);
    } else {
        p.rect_stroke(body, 1.0, Stroke::new(1.0, col), egui::StrokeKind::Inside);
    }
    // Shackle: an arch over the body. Open, its right leg lifts clear.
    let r = 2.5;
    let top = body.top() - 3.0;
    let (lx, rx) = (c.x - r, c.x + r);
    let lift = if locked { 0.0 } else { 2.5 };
    let n = 10;
    let arch: Vec<egui::Pos2> = (0..=n)
        .map(|i| {
            let a = std::f32::consts::PI * (1.0 - i as f32 / n as f32);
            egui::pos2(c.x + r * a.cos(), top - lift - r * a.sin())
        })
        .collect();
    let s = Stroke::new(1.2, col);
    p.line_segment([egui::pos2(lx, body.top()), egui::pos2(lx, top - lift)], s);
    p.line_segment([egui::pos2(rx, top - lift), egui::pos2(rx, body.top() - lift)], s);
    p.add(egui::Shape::line(arch, s));
}

/// After an edit, carry the edited value's FACTOR to the others: `before`
/// are the values as they were shown, `after` as they came out of the
/// fields. If exactly one moved, every other is scaled by `new / old` and
/// its index is returned. A value moved off 0 has no factor, so nothing
/// follows it (a ratio through zero is not a ratio).
pub fn keep_ratio(before: &[f64], after: &mut [f64]) -> Option<usize> {
    let moved: Vec<usize> = (0..before.len().min(after.len()))
        .filter(|&i| before[i] != after[i])
        .collect();
    let &[i] = moved.as_slice() else { return None };
    let old = before[i];
    if old == 0.0 || !old.is_finite() || !after[i].is_finite() {
        return None;
    }
    let f = after[i] / old;
    for (j, v) in after.iter_mut().enumerate() {
        if j != i {
            *v = before[j] * f;
        }
    }
    Some(i)
}

/// [`keep_ratio`] for whole numbers (a format's width × height): the
/// others round to the nearest, and never below `min`.
pub fn keep_ratio_u32(before: &[u32], after: &mut [u32], min: u32) -> Option<usize> {
    let b: Vec<f64> = before.iter().map(|&v| v as f64).collect();
    let mut a: Vec<f64> = after.iter().map(|&v| v as f64).collect();
    let i = keep_ratio(&b, &mut a)?;
    for (j, v) in after.iter_mut().enumerate() {
        if j != i {
            *v = (a[j].round() as u32).max(min);
        }
    }
    Some(i)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_edit_carries_its_factor() {
        let mut a = [3.0, 1.0];
        assert_eq!(keep_ratio(&[2.0, 1.0], &mut a), Some(0));
        assert_eq!(a, [3.0, 1.5]);
        let mut a = [1920, 540];
        assert_eq!(keep_ratio_u32(&[1920, 1080], &mut a, 16), Some(1));
        assert_eq!(a, [960, 540]);
        let mut a = [2.0, 4.0, 6.0];
        assert_eq!(keep_ratio(&[1.0, 2.0, 3.0], &mut a), None, "three moved: not one edit");
    }

    #[test]
    fn zero_and_no_edit_do_nothing() {
        let mut a = [1.0, 5.0];
        assert_eq!(keep_ratio(&[0.0, 5.0], &mut a), None);
        assert_eq!(a, [1.0, 5.0]);
        let mut a = [1.0, 5.0];
        assert_eq!(keep_ratio(&[1.0, 5.0], &mut a), None);
        let mut a = [17, 1080];
        keep_ratio_u32(&[1920, 1080], &mut a, 16);
        assert_eq!(a, [17, 16], "clamped to the minimum");
    }

    #[test]
    fn the_button_toggles() {
        let ctx = egui::Context::default();
        let mut locked = false;
        for _ in 0..2 {
            crate::snapshot::discard(ctx.run_ui(egui::RawInput::default(), |ui| {
                let r = button(ui, &mut locked);
                assert!(r.rect.width() >= W);
            }));
        }
        assert!(!locked, "no click, no toggle");
    }
}

#[cfg(test)]
mod shot {
    /// Both states, drawn: `target/ratiolock.png`.
    #[test]
    #[ignore]
    fn ratiolock_render() {
        let s = crate::snapshot::shoot(60.0, 24.0, |ui| {
            ui.horizontal(|ui| {
                super::button(ui, &mut true);
                ui.add_space(12.0);
                super::button(ui, &mut false);
            });
        });
        s.save("ratiolock");
    }
}
