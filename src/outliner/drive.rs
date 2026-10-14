//! A scripted pointer and keyboard for headless tests: a `Context` run a
//! frame at a time with the events a hand would make. egui decides hits
//! from the previous frame's layout, so a click is three frames (move,
//! press, release) and every frame's output is collected.

use egui::{Context, Event, Modifiers, Pos2, PointerButton, RawInput, Rect, Ui, Vec2};

pub(crate) struct Drive {
    pub ctx: Context,
    size: Vec2,
    t: f64,
}

impl Drive {
    pub fn new(w: f32, h: f32) -> Self {
        Self { ctx: Context::default(), size: Vec2::new(w, h), t: 0.0 }
    }

    /// One frame with `events`, returning what `build` returned.
    pub fn frame<R>(&mut self, events: Vec<Event>, build: &mut dyn FnMut(&mut Ui) -> R) -> R {
        self.t += 1.0 / 60.0;
        let input = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)), time: Some(self.t), events, ..Default::default() };
        let mut got = None;
        let out = self.ctx.run_ui(input, |ui| got = Some(build(ui)));
        crate::snapshot::discard(out);
        got.unwrap()
    }

    /// Settle: frames with nothing happening.
    pub fn idle<R>(&mut self, n: usize, build: &mut dyn FnMut(&mut Ui) -> R) -> Vec<R> {
        (0..n).map(|_| self.frame(vec![], build)).collect()
    }

    pub fn click<R>(&mut self, at: Pos2, mods: Modifiers, build: &mut dyn FnMut(&mut Ui) -> R) -> Vec<R> {
        let press = |pressed| Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: mods };
        vec![
            self.frame(vec![Event::PointerMoved(at), Event::ModifiersChanged(mods)], build),
            self.frame(vec![press(true)], build),
            self.frame(vec![press(false)], build),
            self.frame(vec![Event::ModifiersChanged(Modifiers::NONE)], build),
        ]
    }

    pub fn double_click<R>(&mut self, at: Pos2, build: &mut dyn FnMut(&mut Ui) -> R) -> Vec<R> {
        let mut r = self.click(at, Modifiers::NONE, build);
        r.extend(self.click(at, Modifiers::NONE, build));
        r
    }

    /// Press at `from`, move past the click threshold, land on `to`, let go.
    pub fn drag<R>(&mut self, from: Pos2, to: Pos2, build: &mut dyn FnMut(&mut Ui) -> R) -> Vec<R> {
        let press = |pos, pressed| Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE };
        vec![
            self.frame(vec![Event::PointerMoved(from)], build),
            self.frame(vec![press(from, true)], build),
            self.frame(vec![Event::PointerMoved(from + Vec2::new(0.0, 10.0))], build),
            self.frame(vec![Event::PointerMoved(to)], build),
            self.frame(vec![Event::PointerMoved(to)], build),
            self.frame(vec![press(to, false)], build),
        ]
    }

    pub fn key<R>(&mut self, key: egui::Key, mods: Modifiers, build: &mut dyn FnMut(&mut Ui) -> R) -> Vec<R> {
        let k = |pressed| Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: mods };
        vec![self.frame(vec![Event::ModifiersChanged(mods), k(true)], build), self.frame(vec![k(false), Event::ModifiersChanged(Modifiers::NONE)], build)]
    }

    pub fn text<R>(&mut self, s: &str, build: &mut dyn FnMut(&mut Ui) -> R) -> R {
        self.frame(vec![Event::Text(s.to_string())], build)
    }
}
