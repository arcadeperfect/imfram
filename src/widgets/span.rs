//! A frame span on a bar: an in and an out over a wider stretch of frames,
//! the clip's in/out picture you drag rather than type.
//!
//! Hands: drag the left or right edge to move that end, drag inside to
//! slide the whole span (its length held). A drag is measured from the
//! PRESS against the bar's layout as it was at the press, so the bar may
//! re-fit itself to the new span every frame without the drag running
//! away. Shift held while sliding is reported (`SpanOut::shift_slide`), so
//! a caller can let shift carry something else along (a Load moves its
//! clip with it). Typed frames are the caller's own number fields; this is
//! the picture.
//!
//! Colours come from `Visuals`: the bar is the extreme background, the
//! `solid` band (what exists, e.g. the files on disk) a widget fill, the
//! span the selection colour, the marker (a playhead) strong text unless the
//! caller names one.

use egui::{pos2, vec2, Align2, CursorIcon, FontId, Rect, Sense, Stroke, Ui};

const BAR_H: f32 = 18.0;
/// How far from an end a press still takes that end, in points.
const EDGE_HIT: f32 = 6.0;

/// What the bar shows besides the span.
#[derive(Clone, Copy, Debug, Default)]
pub struct SpanOpts {
    /// The frames the bar spans, first and last. `None` fits the span and
    /// `solid` with a little room either side.
    pub view: Option<(i64, i64)>,
    /// A band under the span: what exists (a clip's files).
    pub solid: Option<(i64, i64)>,
    /// A frame marked with a line (the playhead).
    pub marker: Option<i64>,
    /// The marker's colour (the strong text colour when unset).
    pub marker_color: Option<egui::Color32>,
    /// The longest the span may be (`i64::MAX` when unset).
    pub max_len: Option<i64>,
    /// Label the two ends with their frame numbers.
    pub labels: bool,
    /// The span's colour (the selection colour when unset).
    pub color: Option<egui::Color32>,
    /// A line added to the hover text (what shift does, say).
    pub hint: Option<&'static str>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grab {
    Lo,
    Hi,
    Body,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SpanOut {
    /// The span moved this frame.
    pub changed: bool,
    /// What is being dragged, if anything.
    pub grab: Option<Grab>,
    /// The span slid with shift held. With `changed`, `delta` is how far.
    pub shift_slide: bool,
    /// How far `lo` moved this frame.
    pub delta: i64,
    /// A double-click on the bar.
    pub double_clicked: bool,
}

/// The press, kept in egui memory for the life of a drag.
#[derive(Clone, Copy)]
struct Press {
    grab: Grab,
    span: (i64, i64),
    frame: f64,
    view: (i64, i64),
    rect: Rect,
}

/// The fitted view: the span and `solid` with a twentieth either side.
pub fn fit(span: (i64, i64), solid: Option<(i64, i64)>) -> (i64, i64) {
    let (mut a, mut b) = (span.0.min(span.1), span.0.max(span.1));
    if let Some((s0, s1)) = solid {
        a = a.min(s0);
        b = b.max(s1);
    }
    let pad = ((b - a) / 20).max(1);
    (a - pad, b + pad)
}

/// Frame -> x on a bar spanning `view` across `rect`. A frame is a cell:
/// `view.0` starts at the left edge and `view.1` ends at the right.
pub fn x_of(rect: Rect, view: (i64, i64), f: f64) -> f32 {
    let n = (view.1 - view.0 + 1).max(1) as f64;
    rect.min.x + ((f - view.0 as f64) / n) as f32 * rect.width()
}

/// x -> frame (continuous), the inverse of [`x_of`].
pub fn frame_at(rect: Rect, view: (i64, i64), x: f32) -> f64 {
    let n = (view.1 - view.0 + 1).max(1) as f64;
    view.0 as f64 + ((x - rect.min.x) / rect.width().max(1.0)) as f64 * n
}

/// Where a drag from `p` lands with the pointer at frame `now`.
fn dragged(p: &Press, now: f64, max_len: i64) -> (i64, i64) {
    let d = (now - p.frame).round() as i64;
    let (a, b) = p.span;
    match p.grab {
        Grab::Lo => ((a + d).min(b).max(b.saturating_sub(max_len)), b),
        Grab::Hi => (a, (b + d).max(a).min(a.saturating_add(max_len))),
        Grab::Body => (a + d, b + d),
    }
}

/// Which part of the bar a press at `x` takes. Ends win inside their hit
/// band; a span too narrow to slide by its middle still slides from inside.
fn grab_at(rect: Rect, view: (i64, i64), span: (i64, i64), x: f32) -> Option<Grab> {
    let xa = x_of(rect, view, span.0 as f64);
    let xb = x_of(rect, view, span.1 as f64 + 1.0);
    let (da, db) = ((x - xa).abs(), (x - xb).abs());
    if da <= EDGE_HIT && da <= db {
        Some(Grab::Lo)
    } else if db <= EDGE_HIT {
        Some(Grab::Hi)
    } else if x > xa && x < xb {
        Some(Grab::Body)
    } else {
        None
    }
}

/// Draw the bar the available width and let it be dragged. `span` is
/// inclusive, first and last.
pub fn span_bar(ui: &mut Ui, id: impl std::hash::Hash + std::fmt::Debug, span: &mut (i64, i64), opts: SpanOpts) -> SpanOut {
    let id = ui.make_persistent_id(id);
    let w = ui.available_width().max(40.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, BAR_H), Sense::click_and_drag());
    let max_len = opts.max_len.unwrap_or(i64::MAX);
    let mut out = SpanOut { double_clicked: resp.double_clicked(), ..Default::default() };

    let press: Option<Press> = ui.data(|d| d.get_temp(id));
    // Frozen while a drag runs: the caller may re-fit to the new span.
    let view = press.map(|p| p.view).unwrap_or_else(|| opts.view.unwrap_or_else(|| fit(*span, opts.solid)));

    if resp.drag_started() {
        // From where the button went DOWN: drag_started fires past the
        // threshold, by which time the pointer has left the edge.
        if let Some(p0) = ui.input(|i| i.pointer.press_origin()) {
            if let Some(g) = grab_at(rect, view, *span, p0.x) {
                let p = Press { grab: g, span: *span, frame: frame_at(rect, view, p0.x), view, rect };
                ui.data_mut(|d| d.insert_temp(id, p));
            }
        }
    }
    let press: Option<Press> = ui.data(|d| d.get_temp(id));
    if let Some(p) = press {
        if resp.dragged() {
            if let Some(ptr) = resp.interact_pointer_pos() {
                let next = dragged(&p, frame_at(p.rect, p.view, ptr.x), max_len);
                if next != *span {
                    out.changed = true;
                    out.delta = next.0 - span.0;
                    *span = next;
                }
            }
            out.grab = Some(p.grab);
            out.shift_slide = p.grab == Grab::Body && ui.input(|i| i.modifiers.shift);
        }
        if !resp.dragged() || resp.drag_stopped() {
            ui.data_mut(|d| d.remove::<Press>(id));
        }
    }

    // Cursor: what a press here would take.
    let hover = out.grab.or_else(|| resp.hover_pos().and_then(|p| grab_at(rect, view, *span, p.x)));
    match hover {
        Some(Grab::Lo | Grab::Hi) => ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal),
        Some(Grab::Body) => ui.ctx().set_cursor_icon(if out.grab.is_some() {
            CursorIcon::Grabbing
        } else {
            CursorIcon::Grab
        }),
        None => {}
    }

    // --- paint --------------------------------------------------------------
    let v = ui.visuals();
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, v.extreme_bg_color);
    let band = |a: i64, b: i64| {
        Rect::from_min_max(
            pos2(x_of(rect, view, a as f64).max(rect.min.x), rect.min.y),
            pos2(x_of(rect, view, b as f64 + 1.0).min(rect.max.x), rect.max.y),
        )
    };
    if let Some((s0, s1)) = opts.solid {
        let r = band(s0, s1).shrink2(vec2(0.0, 4.0));
        painter.rect_filled(r, 1.0, v.widgets.inactive.bg_fill);
    }
    let sr = band(span.0, span.1);
    let sel = opts.color.unwrap_or(v.selection.bg_fill);
    painter.rect_filled(sr, 2.0, sel.gamma_multiply(0.55));
    painter.rect_stroke(sr, 2.0, Stroke::new(1.0, sel), egui::StrokeKind::Inside);
    // The ends: a bar each, brighter when held or hovered.
    for (g, x) in [(Grab::Lo, sr.min.x), (Grab::Hi, sr.max.x)] {
        let hot = hover == Some(g);
        let c = if hot { v.strong_text_color() } else { sel.lerp_to_gamma(egui::Color32::WHITE, 0.45) };
        let x = if g == Grab::Lo { x + 1.5 } else { x - 1.5 };
        painter.line_segment([pos2(x, rect.min.y + 1.0), pos2(x, rect.max.y - 1.0)], Stroke::new(if hot { 3.0 } else { 2.0 }, c));
    }
    if opts.labels {
        let font = FontId::monospace(10.0);
        let c = v.text_color();
        let la = format!("{}", span.0);
        let lb = format!("{}", span.1);
        // Inside the span when it is wide enough, else outside it.
        let room = sr.width() > 70.0;
        let (pa, aa) = if room { (pos2(sr.min.x + 5.0, rect.center().y), Align2::LEFT_CENTER) } else { (pos2(sr.min.x - 4.0, rect.center().y), Align2::RIGHT_CENTER) };
        let (pb, ab) = if room { (pos2(sr.max.x - 5.0, rect.center().y), Align2::RIGHT_CENTER) } else { (pos2(sr.max.x + 4.0, rect.center().y), Align2::LEFT_CENTER) };
        painter.text(pa, aa, la, font.clone(), c);
        painter.text(pb, ab, lb, font, c);
    }
    if let Some(m) = opts.marker {
        let x = x_of(rect, view, m as f64 + 0.5);
        if rect.x_range().contains(x) {
            painter.line_segment([pos2(x, rect.min.y), pos2(x, rect.max.y)], Stroke::new(1.0, opts.marker_color.unwrap_or(v.strong_text_color())));
        }
    }
    resp.on_hover_text(format!(
        "{}–{} ({} frames)\ndrag an end to move it · drag inside to slide{}",
        span.0,
        span.1,
        span.1 - span.0 + 1,
        opts.hint.map(|h| format!("\n{h}")).unwrap_or_default()
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> Rect {
        Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, BAR_H))
    }

    #[test]
    fn frames_and_x_round_trip() {
        let v = (0, 99);
        for f in [0.0, 12.5, 99.0] {
            assert!((frame_at(rect(), v, x_of(rect(), v, f)) - f).abs() < 1e-4);
        }
    }

    #[test]
    fn a_press_picks_the_nearest_end_then_the_body() {
        let v = (0, 99);
        let s = (20, 59); // x 20..60
        assert_eq!(grab_at(rect(), v, s, 21.0), Some(Grab::Lo));
        assert_eq!(grab_at(rect(), v, s, 58.0), Some(Grab::Hi));
        assert_eq!(grab_at(rect(), v, s, 40.0), Some(Grab::Body));
        assert_eq!(grab_at(rect(), v, s, 90.0), None);
    }

    #[test]
    fn a_drag_is_measured_from_the_press_and_ends_never_cross() {
        let p = |grab| Press { grab, span: (20, 59), frame: 40.0, view: (0, 99), rect: rect() };
        assert_eq!(dragged(&p(Grab::Body), 50.2, i64::MAX), (30, 69));
        assert_eq!(dragged(&p(Grab::Lo), 30.0, i64::MAX), (10, 59));
        // Past the other end: pinned at it, never inverted.
        assert_eq!(dragged(&p(Grab::Lo), 200.0, i64::MAX), (59, 59));
        assert_eq!(dragged(&p(Grab::Hi), -100.0, i64::MAX), (20, 20));
        // The longest it may be.
        assert_eq!(dragged(&p(Grab::Hi), 100.0, 50), (20, 70));
    }

    #[test]
    fn the_fit_covers_the_span_and_the_solid_band() {
        let (a, b) = fit((10, 20), Some((0, 100)));
        assert!(a < 0 && b > 100);
    }
}
