//! The hands, driven through real egui passes with synthetic input.

use super::*;
use egui::{pos2, Event, Modifiers, RawInput};

struct Rig {
    ctx: egui::Context,
    canvas: Canvas<ItemId>,
    board: Board,
    view: Rect,
    time: f64,
    pointer: Pos2,
    mods: Modifiers,
    last: Option<(bool, bool, bool)>,
}

impl Rig {
    fn new() -> Self {
        let mut board = Board::default();
        // Two cards side by side, one triangle further off.
        board.add(Xform::at(pos2(-100.0, 0.0)), Note::new("a", vec2(80.0, 60.0), Color32::YELLOW));
        board.add(Xform::at(pos2(100.0, 0.0)), Note::new("b", vec2(80.0, 60.0), Color32::LIGHT_BLUE));
        let tri = Path::polygon(&[pos2(0.0, -30.0), pos2(30.0, 30.0), pos2(-30.0, 30.0)]);
        board.add(Xform::at(pos2(0.0, 200.0)), Shape { path: tri, fill: Color32::RED, stroke: Stroke::NONE });
        let canvas = Canvas { fit_pending: false, ..Default::default() };
        let mut r = Rig { ctx: egui::Context::default(), canvas, board, view: Rect::NOTHING, time: 0.0, pointer: pos2(400.0, 300.0), mods: Modifiers::NONE, last: None };
        r.frame(vec![]);
        r
    }

    fn frame(&mut self, mut events: Vec<Event>) {
        self.time += 1.0 / 60.0;
        events.insert(0, Event::PointerMoved(self.pointer));
        events.insert(0, Event::ModifiersChanged(self.mods));
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let (c, b) = (&mut self.canvas, &mut self.board);
        let mut view = Rect::NOTHING;
        let mut last = None;
        let mut out = self.ctx.run_ui(input, |ui| {
            let o = c.show(ui, b);
            view = o.response.rect;
            last = Some((o.changed, o.committed, o.selection_changed));
        });
        out.textures_delta.clear();
        self.view = view;
        self.last = last;
    }

    fn screen(&self, world: Pos2) -> Pos2 {
        self.canvas.cam.to_screen(self.view, world)
    }

    fn move_to(&mut self, world: Pos2) {
        self.pointer = self.screen(world);
        self.frame(vec![]);
    }

    fn button(&mut self, b: PointerButton, pressed: bool) {
        let e = Event::PointerButton { pos: self.pointer, button: b, pressed, modifiers: self.mods };
        self.frame(vec![e]);
    }

    fn click(&mut self, world: Pos2) {
        self.move_to(world);
        self.button(PointerButton::Primary, true);
        self.button(PointerButton::Primary, false);
        // Let a double-click window lapse.
        self.time += 1.0;
        self.frame(vec![]);
    }

    fn key(&mut self, key: Key) {
        let down = Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: self.mods };
        let up = Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers: self.mods };
        self.frame(vec![down]);
        self.frame(vec![up]);
    }

    /// A press at `from`, a path of hops to `to`, a release there.
    fn drag(&mut self, from: Pos2, to: Pos2) {
        self.move_to(from);
        self.button(PointerButton::Primary, true);
        for i in 1..=10 {
            self.move_to(from + (to - from) * (i as f32 / 10.0));
        }
        self.button(PointerButton::Primary, false);
    }

    fn pos(&self, i: usize) -> Pos2 {
        self.board.items[i].xf.pos
    }

    fn id(&self, i: usize) -> ItemId {
        self.board.items[i].id
    }
}

#[test]
fn a_click_selects_and_a_click_on_nothing_clears() {
    let mut r = Rig::new();
    r.click(pos2(-100.0, 0.0));
    assert_eq!(r.canvas.selection, vec![r.id(0)]);
    r.mods = Modifiers::SHIFT;
    r.click(pos2(100.0, 0.0));
    assert_eq!(r.canvas.selection, vec![r.id(0), r.id(1)], "shift adds");
    r.mods = Modifiers::NONE;
    r.click(pos2(0.0, -250.0));
    assert!(r.canvas.selection.is_empty());
}

#[test]
fn a_drag_on_an_item_moves_it_by_the_travel() {
    let mut r = Rig::new();
    r.canvas.cam.zoom = 2.0;
    r.frame(vec![]);
    r.drag(pos2(-100.0, 0.0), pos2(-60.0, 25.0));
    assert!((r.pos(0) - pos2(-60.0, 25.0)).length() < 0.01, "landed at {:?}", r.pos(0));
    assert_eq!(r.pos(1), pos2(100.0, 0.0), "the other card stays");
    assert_eq!(r.last.map(|l| l.1), Some(true), "the release commits");
}

#[test]
fn grab_follows_the_pointer_and_esc_puts_it_back() {
    let mut r = Rig::new();
    r.click(pos2(100.0, 0.0));
    r.key(Key::G);
    r.move_to(pos2(130.0, 10.0));
    assert!((r.pos(1) - pos2(130.0, 10.0)).length() < 0.01, "grabbed to {:?}", r.pos(1));
    r.key(Key::X);
    r.move_to(pos2(150.0, 40.0));
    assert!((r.pos(1) - pos2(150.0, 0.0)).length() < 0.01, "X locks the axis: {:?}", r.pos(1));
    r.key(Key::Escape);
    assert_eq!(r.pos(1), pos2(100.0, 0.0));
    assert!(!r.canvas.busy());

    r.move_to(pos2(110.0, 0.0));
    r.key(Key::G);
    r.move_to(pos2(80.0, 20.0));
    r.button(PointerButton::Primary, true);
    r.button(PointerButton::Primary, false);
    assert!((r.pos(1) - pos2(70.0, 20.0)).length() < 0.01, "a click keeps it: {:?}", r.pos(1));
    assert_eq!(r.canvas.selection, vec![r.id(1)], "the confirming click selected nothing else");
}

#[test]
fn rotate_and_scale_turn_about_the_selection() {
    let mut r = Rig::new();
    r.click(pos2(100.0, 0.0));
    r.move_to(pos2(160.0, 0.0));
    r.key(Key::R);
    r.move_to(pos2(100.0, 60.0));
    let xf = r.board.items[1].xf;
    assert!((xf.rot - std::f32::consts::FRAC_PI_2).abs() < 1e-3, "a quarter turn: {}", xf.rot);
    assert!((xf.pos - pos2(100.0, 0.0)).length() < 1e-3, "about its own centre");
    r.key(Key::Enter);
    r.move_to(pos2(130.0, 0.0));
    r.key(Key::S);
    r.move_to(pos2(160.0, 0.0));
    assert!((r.board.items[1].xf.scale - Vec2::splat(2.0)).length() < 1e-3);
    r.key(Key::Escape);
    assert_eq!(r.board.items[1].xf.scale, Vec2::splat(1.0));
}

#[test]
fn a_box_picks_what_it_touches() {
    let mut r = Rig::new();
    r.drag(pos2(-200.0, -100.0), pos2(150.0, 20.0));
    assert_eq!(r.canvas.selection, vec![r.id(0), r.id(1)]);
    r.mods = Modifiers::CTRL;
    r.drag(pos2(50.0, -100.0), pos2(150.0, 100.0));
    r.mods = Modifiers::NONE;
    assert_eq!(r.canvas.selection, vec![r.id(0)], "ctrl takes away");
}

#[test]
fn a_shape_is_picked_by_its_outline() {
    let mut r = Rig::new();
    // Inside the triangle's box, outside the triangle.
    r.click(pos2(-27.0, 175.0));
    assert!(r.canvas.selection.is_empty());
    r.click(pos2(0.0, 210.0));
    assert_eq!(r.canvas.selection, vec![r.id(2)]);
}

#[test]
fn delete_and_duplicate() {
    let mut r = Rig::new();
    r.click(pos2(-100.0, 0.0));
    r.mods = Modifiers::SHIFT;
    r.key(Key::D);
    r.mods = Modifiers::NONE;
    assert_eq!(r.board.items.len(), 4);
    assert!(r.canvas.busy(), "a duplicate is grabbed");
    r.move_to(pos2(-100.0, 100.0));
    r.key(Key::Enter);
    assert!((r.pos(3) - pos2(-100.0, 100.0)).length() < 0.01);
    assert_eq!(r.pos(0), pos2(-100.0, 0.0));
    r.key(Key::Delete);
    assert_eq!(r.board.items.len(), 3);
    assert!(r.canvas.selection.is_empty());
}

#[test]
fn a_corner_handle_scales_about_the_opposite_corner() {
    let mut r = Rig::new();
    r.click(pos2(100.0, 0.0));
    // The card spans 60..140 x -30..30; drag its bottom-right corner out
    // so the card doubles, its top-left staying put.
    r.drag(pos2(140.0, 30.0), pos2(220.0, 90.0));
    let xf = r.board.items[1].xf;
    assert!((xf.scale - Vec2::splat(2.0)).length() < 1e-3, "{:?}", xf.scale);
    assert!((xf.pos - pos2(140.0, 30.0)).length() < 1e-3, "{:?}", xf.pos);
}

#[test]
fn home_glides_to_everything() {
    let mut r = Rig::new();
    r.canvas.cam = Camera { centre: pos2(5000.0, 5000.0), zoom: 0.1 };
    r.frame(vec![]);
    r.key(Key::Home);
    r.time += 1.0;
    r.frame(vec![]);
    let shown = r.canvas.cam.visible(r.view);
    for i in 0..3 {
        assert!(shown.contains_rect(scene::bounds(&r.board, r.id(i))), "item {i} in view");
    }
}
