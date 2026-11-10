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
    // egui (0.36+) keeps a DragValue's half-typed text in a private type,
    // so the field's own parser records it: egui hands it every edit.
    let seen = std::rc::Rc::new(std::cell::RefCell::new(None::<String>));
    let saw = seen.clone();
    // Typing COMMITS on Enter or focus loss, never per keystroke. A value
    // set mid-typing reaches the caller, and a caller that clamps it (a
    // range's last frame held at or past its first) hands back a different
    // number — egui then throws the typed text away for the value's, with
    // the caret left where it was, and the next digit lands IN FRONT of
    // the old value. So while the field has focus the parser only records
    // the text; egui parses once more when focus goes, and that one counts.
    let wid_key = id.with("numfield-wid");
    let typing = ui
        .data(|d| d.get_temp::<egui::Id>(wid_key))
        .is_some_and(|wid| ui.memory(|m| m.has_focus(wid)));
    let dv = dv
        .update_while_editing(true)
        .custom_formatter(move |v, dec| fit_number(v, dec, fit))
        .custom_parser(move |t| {
            *saw.borrow_mut() = Some(t.to_owned());
            if typing { None } else { default_parse(t) }
        });
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
    // The wheel over the field steps it (`wheel_steps`), handed to the
    // DragValue as accessibility Increment/Decrement requests aimed at
    // last frame's id: that is egui's own step path, so the caller's
    // speed, range, clamp and rounding all apply exactly as for a drag.
    let typing_now = ui.data(|d| d.get_temp::<egui::Id>(wid_key)).is_some_and(|wid| ui.memory(|m| m.has_focus(wid)));
    let steps = if typing_now || !ui.is_enabled() { 0 } else { wheel_steps(ui, id, rect) };
    if steps != 0 {
        if let Some(wid) = ui.data(|d| d.get_temp::<egui::Id>(wid_key)) {
            let action = if steps > 0 { egui::accesskit::Action::Increment } else { egui::accesskit::Action::Decrement };
            ui.input_mut(|i| {
                for _ in 0..steps.unsigned_abs().min(400) {
                    i.events.push(egui::Event::AccessKitActionRequest(egui::accesskit::ActionRequest {
                        action,
                        target_tree: egui::accesskit::TreeId::ROOT,
                        target_node: wid.accesskit_id(),
                        data: None,
                    }));
                }
            });
        }
    }
    let r = child.add_sized(size, dv);
    ui.data_mut(|d| d.insert_temp(wid_key, r.id));
    let key = text_key(r.id);
    let typed = seen.borrow_mut().take();
    if !r.has_focus() {
        ui.data_mut(|d| d.remove::<String>(key));
    } else if let Some(t) = typed {
        ui.data_mut(|d| d.insert_temp(key, t));
    }
    r
}

/// Who owns the current wheel gesture. A gesture is a run of wheel events
/// with no gap over `WHEEL_GAP`; it belongs to the field the pointer was
/// over when it STARTED, or to nobody. So a scroll that begins on a
/// panel's empty space keeps scrolling the panel as fields pass under the
/// pointer, and only a scroll begun ON a field changes it.
#[derive(Clone, Copy)]
struct Wheel {
    /// The frame `owner` and `last` were brought up to date for.
    frame: u64,
    /// Time of the last wheel event.
    last: f64,
    owner: Option<egui::Id>,
    /// The field under the pointer, and the frame it was drawn.
    hover: Option<(egui::Id, u64)>,
    /// Fraction of a step carried to the next event.
    carry: f32,
}

impl Default for Wheel {
    fn default() -> Self {
        // Long ago, so the first wheel event (time starts at 0) begins a gesture.
        Wheel { frame: u64::MAX, last: f64::NEG_INFINITY, owner: None, hover: None, carry: 0.0 }
    }
}

const WHEEL_GAP: f64 = 0.35;
/// A wheel notch is four points of drag (an integer field's default speed
/// is a quarter, so one notch is one unit); a trackpad's points are drag
/// points one for one.
const LINE_STEPS: f32 = 4.0;

fn wheel_key() -> egui::Id {
    egui::Id::new("numfield-wheel")
}

/// The frame's plain (unmodified, bar shift) wheel travel in steps, as a
/// float. Ctrl / cmd / alt wheels are navigation and never count.
fn wheel_travel(events: &[egui::Event]) -> Option<f32> {
    let mut any = false;
    let mut t = 0.0;
    for e in events {
        if let egui::Event::MouseWheel { unit, delta, modifiers, .. } = e {
            if modifiers.ctrl || modifiers.command || modifiers.alt || modifiers.mac_cmd {
                continue;
            }
            any = true;
            // macOS turns shift+wheel into a sideways scroll.
            let d = if delta.y != 0.0 { delta.y } else { delta.x };
            t += match unit {
                egui::MouseWheelUnit::Point => d,
                egui::MouseWheelUnit::Line => d * LINE_STEPS,
                egui::MouseWheelUnit::Page => d * LINE_STEPS * 10.0,
            };
        }
    }
    any.then_some(t)
}

/// Bring the gesture up to date for this frame (once, whoever asks first).
fn wheel_frame(ctx: &egui::Context) -> (Wheel, Option<f32>) {
    // Read before taking the input lock: egui's lock is not re-entrant.
    let frame = ctx.cumulative_frame_nr();
    let (now, travel) = ctx.input(|i| (i.time, wheel_travel(&i.events)));
    let mut w = ctx.data(|d| d.get_temp::<Wheel>(wheel_key())).unwrap_or_default();
    if w.frame != frame {
        if travel.is_some() {
            if now - w.last > WHEEL_GAP {
                // A new gesture: the field the pointer was over last frame.
                w.owner = w.hover.filter(|&(_, f)| f + 1 >= frame).map(|(id, _)| id);
                w.carry = 0.0;
            }
            w.last = now;
        } else if now - w.last > WHEEL_GAP {
            w.owner = None;
        }
        w.frame = frame;
        ctx.data_mut(|d| d.insert_temp(wheel_key(), w));
    }
    (w, travel)
}

/// True while a field owns the wheel: a canvas reading raw wheel events
/// (`nav::wheel_nav`) stands down, or it would zoom as the field steps.
pub fn wheel_claimed(ctx: &egui::Context) -> bool {
    wheel_frame(ctx).0.owner.is_some()
}

/// Whole steps the wheel moves the field `id` at `rect` this frame; when
/// it owns the gesture the wheel is swallowed so no scroll area under it
/// moves too.
fn wheel_steps(ui: &egui::Ui, id: egui::Id, rect: egui::Rect) -> i32 {
    if !ui.rect_contains_pointer(rect) {
        return 0;
    }
    let ctx = ui.ctx();
    let (mut w, travel) = wheel_frame(ctx);
    w.hover = Some((id, w.frame));
    let mut steps = 0;
    if w.owner == Some(id) {
        if let Some(t) = travel {
            let t = t + w.carry;
            steps = t.trunc() as i32;
            w.carry = t.fract();
        }
        ui.ctx().input_mut(|i| {
            i.events.retain(|e| !matches!(e, egui::Event::MouseWheel { .. }));
            // The smoothed scroll keeps spending a notch for a few frames.
            i.smooth_scroll_delta = egui::Vec2::ZERO;
        });
    }
    ctx.data_mut(|d| d.insert_temp(wheel_key(), w));
    steps
}

fn text_key(id: egui::Id) -> egui::Id {
    id.with("numfield-typed")
}

/// What has been typed into the field `id` so far, while it is being typed
/// into; `None` until the first keystroke and once it loses focus.
pub fn typed_text(ctx: &egui::Context, id: egui::Id) -> Option<String> {
    ctx.data(|d| d.get_temp::<String>(text_key(id)))
}

/// Drop the field's typed text (its typing went somewhere else).
pub fn forget_typed(ctx: &egui::Context, id: egui::Id) {
    ctx.data_mut(|d| d.remove::<String>(text_key(id)));
}

/// egui's own number parser (private there): whitespace ignored, the
/// typographic minus read as a minus.
fn default_parse(text: &str) -> Option<f64> {
    let t: String = text
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| if c == '\u{2212}' { '-' } else { c })
        .collect();
    t.parse().ok()
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

    /// Drive `field` through a click, then type `typed`, then Enter.
    fn click_and_type(typed: &str, field: impl Fn(&mut egui::Ui, &mut f64) -> egui::Response) -> f64 {
        let ctx = egui::Context::default();
        let mut v = 1001.0f64;
        let mut centre = egui::Pos2::ZERO;
        let frame = |events: Vec<egui::Event>, v: &mut f64, centre: &mut egui::Pos2| {
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 200.0))),
                events,
                ..Default::default()
            };
            crate::snapshot::discard(ctx.run_ui(raw, |ui| {
                let r = field(ui, v);
                *centre = r.rect.center();
            }));
        };
        frame(vec![], &mut v, &mut centre);
        let c = centre;
        let btn = |pressed| egui::Event::PointerButton {
            pos: c,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(vec![egui::Event::PointerMoved(c)], &mut v, &mut centre);
        frame(vec![btn(true)], &mut v, &mut centre);
        frame(vec![btn(false)], &mut v, &mut centre);
        frame(vec![], &mut v, &mut centre);
        frame(vec![], &mut v, &mut centre);
        for ch in typed.chars() {
            frame(vec![egui::Event::Text(ch.to_string())], &mut v, &mut centre);
        }
        let enter = |pressed| egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        frame(vec![enter(true), enter(false)], &mut v, &mut centre);
        frame(vec![], &mut v, &mut centre);
        v
    }

    /// One click selects every digit, so typing REPLACES the value.
    #[test]
    fn typing_over_a_clicked_field_replaces_it() {
        let plain = click_and_type("57", |ui, v| ui.add(egui::DragValue::new(v)));
        let fixed_v = click_and_type("57", |ui, v| fixed(ui, 54.0, 18.0, egui::DragValue::new(v)));
        assert_eq!((plain, fixed_v), (57.0, 57.0));
    }

    /// A caller that clamps what it is handed (a range's last frame held at
    /// or past its first) must not turn typing into prepending: "1100"
    /// typed over a selected 1001 is 1100, not 11001001.
    #[test]
    fn a_clamping_caller_does_not_scramble_the_typing() {
        let clamp = |ui: &mut egui::Ui, v: &mut f64| {
            let mut x = *v;
            let r = fixed(ui, 54.0, 18.0, egui::DragValue::new(&mut x));
            *v = x.max(1001.0);
            r
        };
        assert_eq!(click_and_type("1100", clamp), 1100.0);
        let raw = |ui: &mut egui::Ui, v: &mut f64| {
            let mut x = *v;
            let r = ui.add(egui::DragValue::new(&mut x).update_while_editing(false));
            *v = x.max(1001.0);
            r
        };
        assert_eq!(click_and_type("1100", raw), 1100.0);
    }

    /// Run frames with `wheel` notches arriving on the given frames, the
    /// pointer at `at`, a column of fields inside a short scroll area.
    fn wheel_run(at: impl Fn(&[egui::Rect]) -> egui::Pos2, notches: &[(usize, f32)]) -> (Vec<f64>, f32) {
        let ctx = egui::Context::default();
        let mut vals = vec![10.0f64; 8];
        let mut rects = Vec::new();
        let mut offset = 0.0;
        let mut time = 0.0;
        for frame in 0..40 {
            let mut events = Vec::new();
            if !rects.is_empty() {
                events.push(egui::Event::PointerMoved(at(&rects)));
            }
            for &(f, n) in notches {
                if f == frame {
                    events.push(egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Line,
                        delta: egui::vec2(0.0, n),
                        modifiers: egui::Modifiers::NONE,
                        phase: egui::TouchPhase::Move,
                    });
                }
            }
            time += 1.0 / 60.0;
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(300.0, 120.0))),
                time: Some(time),
                events,
                ..Default::default()
            };
            crate::snapshot::discard(ctx.run_ui(raw, |ui| {
                rects.clear();
                let out = egui::ScrollArea::vertical().max_height(100.0).show(ui, |ui| {
                    ui.add_space(30.0);
                    for v in vals.iter_mut() {
                        let r = fixed(ui, 54.0, 18.0, egui::DragValue::new(v).speed(0.25).range(0..=100));
                        rects.push(r.rect);
                        ui.add_space(12.0);
                    }
                });
                offset = out.state.offset.y;
            }));
        }
        (vals, offset)
    }

    /// A wheel begun on a field steps it (a notch is one unit of an
    /// integer-speed field) and the panel under it does not move.
    #[test]
    fn the_wheel_steps_the_field_it_starts_on() {
        let (vals, offset) = wheel_run(|r| r[0].center(), &[(5, 1.0), (6, 1.0), (7, -1.0), (8, 1.0)]);
        assert_eq!(vals[0], 12.0, "{vals:?}");
        assert!(vals[1..].iter().all(|&v| v == 10.0), "{vals:?}");
        assert_eq!(offset, 0.0);
    }

    /// A scroll begun off every field keeps scrolling the panel: the
    /// fields passing under the pointer are left alone.
    #[test]
    fn a_scroll_begun_off_a_field_scrolls_the_panel() {
        let gap = |r: &[egui::Rect]| egui::pos2(r[0].center().x, 24.0);
        let (vals, offset) = wheel_run(gap, &[(5, -1.0), (6, -1.0), (7, -1.0), (8, -1.0)]);
        assert!(vals.iter().all(|&v| v == 10.0), "{vals:?}");
        assert!(offset > 0.0);
    }
}
