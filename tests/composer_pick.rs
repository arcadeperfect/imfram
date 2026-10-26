//! Pick mode is process-wide (a click in one OS window reaches a composer in
//! another), so it is tested in a binary of its own: inside the lib tests it
//! would arm the controls of every test running beside it.

use egui::{pos2, vec2, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Ui, UiBuilder};
use imfram::composer::{self, Composer, Entry};

const W: f32 = 600.0;

fn frame(ctx: &egui::Context, events: Vec<Event>, mut build: impl FnMut(&mut Ui)) {
    let input = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(W, 400.0))), events, ..Default::default() };
    imfram::snapshot::discard(ctx.run_ui(input, |ui| build(ui)));
}

fn press(at: Pos2, pressed: bool) -> Event {
    Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE }
}

#[test]
fn in_pick_mode_a_click_on_a_page_control_adds_it_and_leaves_its_value_alone() {
    let ctx = egui::Context::default();
    let mut c = Composer::default();
    let mut gain = 0.5;
    let mut at = None;
    let run = |events: Vec<Event>, c: &mut Composer, gain: &mut f32, at: &mut Option<Rect>| {
        frame(&ctx, events, |ui| {
            let (left, right) = ui.max_rect().split_left_right_at_fraction(0.5);
            let mut l = ui.new_child(UiBuilder::new().max_rect(left).id_salt("page"));
            composer::scope(&mut l, "Page", |ui| {
                imfram::widgets::slider(ui, gain, (0.0, 1.0), "gain", false, "");
            });
            *at = Some(l.min_rect());
            let mut r = ui.new_child(UiBuilder::new().max_rect(right).id_salt("composer"));
            c.show(&mut r, |ui| {
                composer::scope(ui, "Page", |ui| {
                    imfram::widgets::slider(ui, gain, (0.0, 1.0), "gain", false, "");
                });
            });
        });
    };
    run(vec![], &mut c, &mut gain, &mut at);
    composer::set_pick_mode(true);
    // A tenth of the way along the rail: a plain click there would move it.
    let r = at.expect("the slider was drawn");
    let p = pos2(r.left() + r.width() * 0.1, r.center().y);
    run(vec![Event::PointerMoved(p)], &mut c, &mut gain, &mut at);
    run(vec![press(p, true)], &mut c, &mut gain, &mut at);
    run(vec![press(p, false)], &mut c, &mut gain, &mut at);
    run(vec![], &mut c, &mut gain, &mut at);
    composer::set_pick_mode(false);
    assert_eq!(c.entries, vec![Entry { key: "Page/gain".into(), name: "gain".into(), label: None }]);
    assert_eq!(gain, 0.5, "the pick click must not also set the slider");
}
