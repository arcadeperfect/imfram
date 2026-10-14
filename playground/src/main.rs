//! The playground: every hands widget in a tab, the tabs in docks across
//! two windows, a tilt menubar along the top. Nothing behind any of it.
//!
//! `cargo run -p playground`. `--shot DIR` renders each window once
//! offscreen to `DIR/<role>.png` and exits (no window is shown).
//! `--video DIR` scripts a tab drag through every kind of drop and writes
//! each frame to `DIR/frame-NNNN.png` (ffmpeg stitches them).
//!
//! | module  | what it owns                                    |
//! |---------|-------------------------------------------------|
//! | `tabs`  | the tabs and windows, and what each tab draws   |
//! | `menus` | the tilt menubar and what its items do          |

mod menus;
mod tabs;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use imfram::dock::{Docks, Side, Tree};
use imfram::shell::Gpu;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::WindowId;

use tabs::{Playground, Role, Tab};

struct State {
    gpu: Gpu,
    docks: Docks<Tab>,
    panels: Playground,
}

struct App {
    state: Option<State>,
    shot: Option<PathBuf>,
    video: Option<PathBuf>,
}

impl App {
    fn open(&mut self, el: &ActiveEventLoop) {
        let gpu = Gpu::standalone().expect("a GPU");
        let mut docks = Docks::new("playground", self.shot.is_some() || self.video.is_some());
        let mut main = Tree::new(vec![Tab::Widgets, Tab::Text]);
        main.split(Side::Right, 0.55, vec![Tab::Colour, Tab::Files]);
        docks.add(el, &gpu, Role::Main, main);
        let mut side = Tree::new(vec![Tab::Canvas]);
        side.split(Side::Bottom, 0.7, vec![Tab::Keys]);
        docks.add(el, &gpu, Role::Side, side);
        docks.add_hidden(el, &gpu, Role::Spare, Tree::new(vec![Tab::Nothing]));
        self.state = Some(State { gpu, docks, panels: Playground::default() });
    }

    /// Each window once, offscreen, to PNGs; then a scripted drag of the
    /// Widgets tab toward the Colour leaf's right edge, a frame of the
    /// glide at a time; then out.
    fn shoot(&mut self, dir: &PathBuf) {
        let s = self.state.as_mut().expect("opened");
        std::fs::create_dir_all(dir).expect("shot dir");
        let save = |gpu: &Gpu, tex: &wgpu::Texture, name: &str| {
            let path = dir.join(format!("{name}.png"));
            imfram::shell::capture::save_png(gpu, tex, &path).expect("png");
            eprintln!("wrote {}", path.display());
        };
        for i in 0..s.docks.hosts.len() {
            // Two frames: egui sizes some things from the pass before.
            let _ = s.docks.frame(i, &s.gpu, &mut s.panels, Some(vec![]));
            let tex = s.docks.frame(i, &s.gpu, &mut s.panels, Some(vec![])).expect("offscreen");
            save(&s.gpu, &tex, &format!("{:?}", s.docks.hosts[i].role).to_lowercase());
        }
        // The drag: press on the Widgets tab, move to the far right of the
        // window (the Colour leaf's right edge), hold there; let go.
        let ctx = s.docks.hosts[0].win.ctx().clone();
        let from = imfram::dock::tab_rect(&ctx, Tab::Widgets).expect("the tab was drawn").center();
        let size = s.docks.hosts[0].win.size();
        let ppp = ctx.pixels_per_point();
        let to = egui::pos2(size[0] as f32 / ppp - 40.0, size[1] as f32 / ppp * 0.5);
        let press = vec![
            egui::Event::PointerMoved(from),
            egui::Event::PointerButton { pos: from, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() },
        ];
        let _ = s.docks.frame(0, &s.gpu, &mut s.panels, Some(press));
        for step in 1..=12 {
            let t = (step as f32 / 6.0).min(1.0);
            let at = from + (to - from) * t;
            let tex = s.docks.frame(0, &s.gpu, &mut s.panels, Some(vec![egui::Event::PointerMoved(at)])).expect("offscreen");
            if step % 3 == 0 {
                save(&s.gpu, &tex, &format!("drag-{step:02}"));
            }
        }
        let release = vec![egui::Event::PointerButton { pos: to, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() }];
        let _ = s.docks.frame(0, &s.gpu, &mut s.panels, Some(release));
        for step in 0..12 {
            let tex = s.docks.frame(0, &s.gpu, &mut s.panels, Some(vec![])).expect("offscreen");
            if step == 11 {
                save(&s.gpu, &tex, "dropped");
            }
        }
    }
}

impl App {
    /// A drag of the Widgets tab, filmed: to the right of Colour, under
    /// Text, onto Colour's bar, to the window's left edge; let go there.
    /// Every frame to `dir`.
    fn film(&mut self, dir: &PathBuf) {
        let s = self.state.as_mut().expect("opened");
        std::fs::create_dir_all(dir).expect("video dir");
        let mut n = 0usize;
        let mut frame = |s: &mut State, events: Vec<egui::Event>| {
            let tex = s.docks.frame(0, &s.gpu, &mut s.panels, Some(events)).expect("offscreen");
            imfram::shell::capture::save_png(&s.gpu, &tex, &dir.join(format!("frame-{n:04}.png"))).expect("png");
            n += 1;
        };
        for _ in 0..3 {
            frame(s, vec![]);
        }
        let ctx = s.docks.hosts[0].win.ctx().clone();
        let from = imfram::dock::tab_rect(&ctx, Tab::Widgets).expect("the tab was drawn").center();
        let size = s.docks.hosts[0].win.size();
        let ppp = ctx.pixels_per_point();
        let (w, h) = (size[0] as f32 / ppp, size[1] as f32 / ppp);
        let stops = [
            (egui::pos2(w - 40.0, h * 0.5), 24, 30),  // right of Colour
            (egui::pos2(w * 0.25, h * 0.9), 24, 30),  // under Text
            (egui::pos2(w * 0.8, h * 0.45), 24, 30), // onto Colour (a tab)
            (egui::pos2(8.0, h * 0.5), 24, 30),      // the window's left edge
        ];
        let mut at = from;
        let press = vec![
            egui::Event::PointerMoved(from),
            egui::Event::PointerButton { pos: from, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() },
        ];
        frame(s, press);
        for (to, travel, hold) in stops {
            let start = at;
            for i in 1..=travel {
                let t = i as f32 / travel as f32;
                let t = t * t * (3.0 - 2.0 * t);
                at = start + (to - start) * t;
                frame(s, vec![egui::Event::PointerMoved(at)]);
            }
            for _ in 0..hold {
                frame(s, vec![]);
            }
        }
        frame(s, vec![egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() }]);
        for _ in 0..40 {
            frame(s, vec![]);
        }
        eprintln!("{n} frames in {}", dir.display());
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.state.is_none() {
            self.open(el);
            if let Some(dir) = self.shot.clone() {
                self.shoot(&dir);
                el.exit();
            }
            if let Some(dir) = self.video.clone() {
                self.film(&dir);
                el.exit();
            }
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(s) = &mut self.state else { return };
        let Some(i) = s.docks.by_window(id) else { return };
        if s.docks.event(&s.gpu, i, &event) {
            s.docks.frame(i, &s.gpu, &mut s.panels, None);
        }
        if s.panels.quit {
            el.exit();
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        let Some(s) = &mut self.state else { return };
        for tab in s.panels.take_toggles() {
            s.docks.toggle(tab);
        }
        s.docks.apply(Some(el), &s.gpu);
        s.docks.poll();
        if s.docks.hosts.iter().all(|h| !h.win.is_visible()) && self.shot.is_none() && self.video.is_none() {
            el.exit();
        }
        el.set_control_flow(ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(4)));
    }
}

fn main() {
    env_logger::init();
    let mut args = std::env::args().skip(1);
    let mut shot = None;
    let mut video = None;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--shot" => shot = args.next().map(PathBuf::from),
            "--video" => video = args.next().map(PathBuf::from),
            _ => {}
        }
    }
    let el = EventLoop::new().expect("event loop");
    let mut app = App { state: None, shot, video };
    el.run_app(&mut app).expect("run");
}
