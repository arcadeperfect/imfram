//! The sheet headless: the grid, the dots, the events.

use super::*;
use crate::snapshot::{discard, shoot};

const W: f32 = 320.0;
type Events = Vec<(RowId, RowEvent)>;

/// A frame with the pointer doing `events`; what the rows reported.
fn frame(ctx: &egui::Context, events: Vec<egui::Event>, build: impl FnOnce(&mut Ui) -> Events) -> Events {
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(W, 200.0))),
        events,
        ..Default::default()
    };
    let mut out = Vec::new();
    let mut once = Some(build);
    discard(ctx.run_ui(input, |ui| {
        if let Some(b) = once.take() {
            out = b(ui);
        }
    }));
    out
}

/// Press and release at `at`, one frame each, around `build`.
fn click(ctx: &egui::Context, at: egui::Pos2, mut build: impl FnMut(&mut Ui) -> Events) -> Events {
    let button = |pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
    frame(ctx, vec![egui::Event::PointerMoved(at)], &mut build);
    frame(ctx, vec![button(true)], &mut build);
    frame(ctx, vec![button(false)], &mut build)
}

fn lit(shot: &crate::snapshot::Shot, x: std::ops::Range<f32>) -> bool {
    let xs = (x.start * shot.ss) as usize..(x.end * shot.ss) as usize;
    (0..shot.h).any(|y| xs.clone().any(|x| shot.px(x, y) != [18, 18, 18]))
}

#[test]
fn every_control_starts_at_the_same_x_whatever_its_label() {
    let mut xs = Vec::new();
    let mut on = true;
    let mut n = 2;
    shoot(W, 160.0, |ui| {
        let mut s = Sheet::new(ui).label_width(80.0);
        s.toggle("x", &mut on);
        s.int("a much longer label than the column holds", &mut n, (0, 9));
        s.custom("custom", |ui| {
            xs.push(ui.max_rect().min.x);
            false
        });
        drop(s);
        for l in ["x", "a much longer label than the column holds"] {
            xs.push(rects(ui.ctx(), Id::new(l)).unwrap().control.min.x);
        }
    });
    assert_eq!(xs.len(), 3);
    assert!(xs.iter().all(|x| (x - xs[0]).abs() < 0.5), "{xs:?}");
}

#[test]
fn the_reset_dot_shows_only_while_the_value_is_off_its_default() {
    let dot_lit = |value: f32| {
        let mut v = value;
        let shot = shoot(W, 40.0, |ui| {
            Sheet::new(ui).slider("gain", &mut v, (0.0, 2.0), false).default(1.0);
        });
        lit(&shot, 0.0..DOT_W)
    };
    assert!(dot_lit(1.5), "away from the default: a dot");
    assert!(!dot_lit(1.0), "at the default: nothing");
}

#[test]
fn clicking_the_reset_dot_restores_the_default_and_says_so() {
    let ctx = egui::Context::default();
    let mut v = 1.5f32;
    let mut build = |ui: &mut Ui| {
        let mut s = Sheet::new(ui);
        s.slider("gain", &mut v, (0.0, 2.0), false).default(1.0);
        s.finish()
    };
    frame(&ctx, vec![], &mut build);
    let dot = rects(&ctx, Id::new("gain")).unwrap().dot.center();
    let events = click(&ctx, dot, &mut build);
    assert_eq!(v, 1.0);
    assert_eq!(events, vec![(Id::new("gain"), RowEvent::Reset)]);
}

#[test]
fn a_defaults_table_feeds_rows_by_label_and_by_caller_id() {
    let ctx = egui::Context::default();
    let defaults = Defaults::new().set("size", 4).set(7u32, "seven");
    let (mut n, mut t) = (9i32, String::from("x"));
    let mut build = |ui: &mut Ui| {
        let mut s = Sheet::new(ui).defaults(&defaults);
        s.int("size", &mut n, (0, 10));
        s.text("name", &mut t).id(7u32);
        s.finish()
    };
    frame(&ctx, vec![], &mut build);
    let size_dot = rects(&ctx, Id::new("size")).unwrap().dot.center();
    let name_dot = rects(&ctx, Id::new(7u32)).unwrap().dot.center();
    click(&ctx, size_dot, &mut build);
    click(&ctx, name_dot, &mut build);
    assert_eq!((n, t.as_str()), (4, "seven"));
}

#[test]
fn the_key_dot_reports_a_toggle_and_a_changed_row_names_itself() {
    let ctx = egui::Context::default();
    let mut on = false;
    let mut build = |ui: &mut Ui| {
        let mut s = Sheet::new(ui);
        s.toggle("solo", &mut on).key(KeyState::Keyed);
        s.finish()
    };
    frame(&ctx, vec![], &mut build);
    let key = rects(&ctx, Id::new("solo")).unwrap().key.expect("a key slot").center();
    let events = click(&ctx, key, &mut build);
    assert_eq!(events, vec![(Id::new("solo"), RowEvent::ToggleKey)]);
    // A control that says it changed is reported under the row's id.
    let events = frame(&ctx, vec![], |ui| {
        let mut s = Sheet::new(ui);
        s.custom("anything", |_| true).id("ch");
        s.finish()
    });
    assert_eq!(events, vec![(Id::new("ch"), RowEvent::Changed)]);
}

#[test]
fn the_key_slot_is_only_there_when_asked_for() {
    let ctx = egui::Context::default();
    let (mut a, mut b) = (0.0f32, 0.0f32);
    let mut two = |ui: &mut Ui, column: bool| {
        let mut s = Sheet::new(ui).key_column(column);
        s.slider("plain", &mut a, (0.0, 1.0), false);
        s.slider("keyed", &mut b, (0.0, 1.0), false).key(KeyState::Animated);
        s.finish()
    };
    frame(&ctx, vec![], |ui| two(ui, false));
    let plain = rects(&ctx, Id::new("plain")).unwrap();
    let keyed = rects(&ctx, Id::new("keyed")).unwrap();
    assert!(plain.key.is_none() && keyed.key.is_some());
    assert!(plain.control.max.x > keyed.control.max.x + KEY_W * 0.5, "the key slot comes out of the control");
    // With a key column every row keeps the slot, so controls line up.
    frame(&ctx, vec![], |ui| two(ui, true));
    let plain = rects(&ctx, Id::new("plain")).unwrap();
    let keyed = rects(&ctx, Id::new("keyed")).unwrap();
    assert!(plain.key.is_some());
    assert_eq!(plain.control.max.x, keyed.control.max.x);
}

#[test]
fn every_key_state_draws_something_in_its_slot() {
    for state in [KeyState::None, KeyState::Keyed, KeyState::Animated, KeyState::Driven] {
        let mut on = false;
        let mut slot = None;
        let shot = shoot(W, 40.0, |ui| {
            Sheet::new(ui).toggle("k", &mut on).key(state);
            slot = rects(ui.ctx(), Id::new("k")).unwrap().key;
        });
        let slot = slot.unwrap();
        assert!(lit(&shot, slot.min.x..slot.max.x), "{state:?} drew nothing");
    }
}

#[test]
fn rows_in_a_section_report_through_the_sheet() {
    let ctx = egui::Context::default();
    let events = frame(&ctx, vec![], |ui| {
        let mut s = Sheet::new(ui);
        s.section("fold", Start::Open, |s| {
            s.custom("inner", |_| true);
        });
        s.finish()
    });
    assert_eq!(events, vec![(Id::new("inner"), RowEvent::Changed)]);
}

#[test]
fn a_switched_section_lines_up_with_the_rows_round_it_and_greys_its_own() {
    let ctx = egui::Context::default();
    let (mut a, mut b, mut on) = (true, true, false);
    let events = frame(&ctx, vec![], |ui| {
        let mut s = Sheet::new(ui);
        s.toggle("outside", &mut a);
        s.switched(&mut on, "fold", "", Start::Open, |s| {
            s.custom("inner", |ui| {
                assert!(!ui.is_enabled(), "a switched-off section's rows are greyed");
                false
            });
            s.toggle("inside", &mut b);
        });
        s.finish()
    });
    assert!(events.is_empty());
    let outside = rects(&ctx, Id::new("outside")).unwrap();
    let inside = rects(&ctx, Id::new("inside")).unwrap();
    // The group indents its body, and the label column gives it back.
    assert!((outside.control.min.x - inside.control.min.x).abs() < 0.5, "{outside:?} {inside:?}");
    assert_eq!(outside.control.max.x, inside.control.max.x);
}

#[test]
fn no_row_is_wider_than_its_room() {
    // A row past its room makes a resizable panel grow every frame.
    let (mut f, mut i, mut t, mut on, mut v2) = (0.5f32, 3, String::from("name"), true, [1.0f32, 2.0]);
    let mut used = Vec::new();
    for room in [320.0f32, 640.0] {
        let ctx = egui::Context::default();
        frame(&ctx, vec![], |ui| {
            ui.set_max_width(room);
            let left = ui.max_rect().left();
            let mut s = Sheet::new(ui);
            s.slider("slider", &mut f, (0.0, 1.0), false);
            s.int("int", &mut i, (0, 9));
            s.text("text", &mut t);
            s.vec2("vec2", &mut v2);
            s.switched(&mut on, "fold", "", Start::Open, |s| {
                s.slider("inner", &mut f, (0.0, 1.0), false);
            });
            let ev = s.finish();
            used.push((room, ui.min_rect().right() - left));
            ev
        });
    }
    for (room, w) in used {
        assert!(w <= room + 0.5, "{w} used of {room}");
    }
}

#[test]
fn every_control_ends_at_the_same_x() {
    let (mut f, mut i, mut t) = (0.5f32, 3, String::from("name"));
    let ctx = egui::Context::default();
    frame(&ctx, vec![], |ui| {
        let mut s = Sheet::new(ui);
        s.slider("slider", &mut f, (0.0, 1.0), false);
        s.int("int", &mut i, (0, 9));
        s.text("text", &mut t);
        s.finish()
    });
    let end = |r: &str| rects(&ctx, Id::new(r)).unwrap().control.right();
    // (The slider's value box once ran a gap past the others.)
    let ends = ["slider", "int", "text"].map(end);
    assert!(ends.iter().all(|e| (e - ends[0]).abs() < 0.5), "{ends:?}");
}
