//! A number field of FIXED size. egui sizes a `DragValue` to its text —
//! `add_sized` is only a minimum — so a long value widened its own box and
//! pushed its row, and every column under it, out of line. Here the box is
//! a fixed allocation and the text fits IT: decimals drop one at a time,
//! down to the field's own minimum, until the number fits; anything still
//! too long is clipped to the box rather than let grow. Typing edits the
//! full value — only the display is trimmed.
//!
//! Depends on egui only: the properties rows (`widgets::num_field`) and the
//! colour picker (shared with the colour designer) both use it.

/// `dv` drawn in exactly `w` x `h`.
pub fn fixed(ui: &mut egui::Ui, w: f32, h: f32, dv: egui::DragValue<'_>) -> egui::Response {
    let size = egui::vec2(w, h);
    let (id, rect) = ui.allocate_space(size);
    let pad = ui.spacing().button_padding.x * 2.0 + 2.0;
    let font = egui::TextStyle::Button.resolve(ui.style());
    let digit = ui
        .painter()
        .layout_no_wrap("0".to_owned(), font, egui::Color32::WHITE)
        .size()
        .x
        .max(1.0);
    let fit = (((w - pad) / digit).floor() as usize).max(1);
    let dv = dv.custom_formatter(move |v, dec| fit_number(v, dec, fit));
    // Its own id salt: children of one ui would otherwise all start the
    // same auto-id sequence and every field in a row would share one id.
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(id)
            .max_rect(rect)
            .layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight)),
    );
    child.shrink_clip_rect(rect);
    // The edit box fills the field too (egui sizes it off interact_size).
    child.spacing_mut().interact_size.x = w;
    child.add_sized(size, dv)
}

/// `v` with as many of `dec`'s decimals as fit in `fit` characters (digits
/// are the widest glyphs in the button font, so counting them is honest).
pub fn fit_number(v: f64, dec: std::ops::RangeInclusive<usize>, fit: usize) -> String {
    let (lo, hi) = (*dec.start(), *dec.end());
    for d in (lo..=hi.max(lo)).rev() {
        let s = egui::emath::format_with_decimals_in_range(v, lo.min(d)..=d);
        if s.chars().count() <= fit {
            return s;
        }
    }
    let s = format!("{v:.0}");
    if s.chars().count() <= fit { s } else { format!("{v:.1e}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimals_give_way_before_the_box_does() {
        assert_eq!(fit_number(0.123456, 0..=4, 8), "0.1235");
        assert_eq!(fit_number(12345.6789, 0..=3, 7), "12345.7");
        assert_eq!(fit_number(12345.6789, 0..=3, 5), "12346");
        assert!(fit_number(123456789.0, 0..=3, 5).contains('e'));
    }

    /// Whatever it shows, the field takes exactly its box.
    #[test]
    fn the_field_is_the_same_size_for_any_value() {
        let ctx = egui::Context::default();
        let mut widths = Vec::new();
        for v in [0.5f64, 123456.789, -98765432.1] {
            let mut v = v;
            crate::snapshot::discard(ctx.run_ui(egui::RawInput::default(), |ui| {
                ui.horizontal(|ui| {
                    let r = ui.scope(|ui| fixed(ui, 54.0, 18.0, egui::DragValue::new(&mut v).max_decimals(3)));
                    widths.push(r.response.rect.width());
                });
            }));
        }
        assert!(widths.iter().all(|w| (w - 54.0).abs() < 0.5), "{widths:?}");
    }
}
