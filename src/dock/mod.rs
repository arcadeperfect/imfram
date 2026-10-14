//! Tool windows as docks, in the way of an editing app: each whole panel
//! is a tab ([`Tab`]), and each tool window holds a dock of them
//! (egui_dock): side by side, split, dragged about within the window. Drag
//! a tab out past its window's edge and let go: over another tool window
//! it docks there, over the desktop it becomes a window of its own (a real
//! OS window). A tab's menu (right-click it) detaches or moves it too;
//! closing a detached window puts its tabs back in their home windows. A
//! closed tab comes back from the "panels" menu along each window's top.
//!
//! | module   | what it owns                                               |
//! |----------|------------------------------------------------------------|
//! | `tree`   | the split tree: layout, remove / insert, a drag's proposal |
//! | `frame`  | one window's frame: top bar, the leaves gliding to their   |
//! |          | places, tab bars, the drag, the drop                       |
//! | `drag`   | a tab dragged out of its window: where it is let go        |
//! | `viewer` | where a tab's button was (self-tests)                      |
//!
//! Here: the windows, and the asks (open, close, move, detach) taken up
//! between frames, where windows can be made.
//!
//! The app supplies its tabs (an enum implementing [`Tab`]) and its windows
//! (an enum implementing [`Role`]), adds its home windows with
//! [`Docks::add`], and draws each tab through [`Panels`].

mod drag;
mod frame;
pub mod tree;
mod viewer;

use std::fmt::Debug;
use std::hash::Hash;

use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

use crate::shell::{Gpu, UiWindow};

pub use drag::Drag;
pub use frame::{Anim, DockApp, Panels, BAR_H};
pub use tree::{Drop, LeafId, Side, Tree, Zone};
pub use viewer::tab_rect;

/// A panel a window can hold.
pub trait Tab: Copy + Eq + Hash + Debug + 'static {
    type Role: Role;
    /// Every tab, for the "panels" menu.
    fn all() -> &'static [Self];
    fn title(self) -> &'static str;
    /// A line on hover: what the panel is for.
    fn help(self) -> &'static str {
        ""
    }
    /// The hot key that shows its window, as shown to the user ("1 / F1").
    fn key(self) -> &'static str {
        ""
    }
    /// The window it goes back to: when opened again, or when its own
    /// window is closed.
    fn home(self) -> Self::Role;
}

/// What a window is for: its title and size. One role is "a window a tab
/// was detached into"; the rest are the app's home windows.
pub trait Role: Copy + Eq + Debug + 'static {
    fn title(self) -> &'static str;
    fn size(self) -> (u32, u32);
    fn detached() -> Self;
    fn is_detached(self) -> bool {
        self == Self::detached()
    }
}

/// One tool window and the dock of tabs in it.
pub struct Host<T: Tab> {
    pub role: T::Role,
    pub win: UiWindow,
    pub tree: Tree<T>,
    pub anim: Anim<T>,
}

/// Something asked of the windows in a frame, done between frames.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Ask<T> {
    /// Into a window of its own.
    Detach(T),
    /// Into a window of its own, at this screen point (where it was let go).
    DetachAt(T, [f64; 2]),
    /// Into the window at this place among the hosts, where a drag put
    /// it (or, none, its first leaf).
    Move(T, usize, Option<Drop>),
    /// Back where it was closed from, or its home; its window shown.
    Open(T),
    Close(T),
}

pub struct Docks<T: Tab> {
    /// The app's name, a detached window's title prefix ("app — Objects").
    pub app: &'static str,
    pub hosts: Vec<Host<T>>,
    pub asks: Vec<Ask<T>>,
    /// A tab being dragged out of its window.
    pub drag: Option<Drag<T>>,
    /// Windows are made hidden and stay so (captures, scripted tests).
    pub headless: bool,
}

impl<T: Tab> Docks<T> {
    /// No windows yet: `add` the home windows next.
    pub fn new(app: &'static str, headless: bool) -> Self {
        Self { app, hosts: Vec::new(), asks: Vec::new(), drag: None, headless }
    }

    /// A window of `role` holding `dock`, shown. Returns its place among
    /// the hosts.
    pub fn add(&mut self, el: &ActiveEventLoop, gpu: &Gpu, role: T::Role, tree: Tree<T>) -> usize {
        let win = UiWindow::new(el, gpu, role.title(), role.size(), !self.headless);
        self.hosts.push(Host { role, win, tree, anim: Anim::default() });
        let i = self.hosts.len() - 1;
        self.retitle(i);
        i
    }

    /// `add`, but the window starts hidden (until its key or a tab opens).
    pub fn add_hidden(&mut self, el: &ActiveEventLoop, gpu: &Gpu, role: T::Role, tree: Tree<T>) -> usize {
        let i = self.add(el, gpu, role, tree);
        self.hosts[i].win.window.set_visible(false);
        i
    }

    /// The first window of a role.
    pub fn find(&self, role: T::Role) -> Option<usize> {
        self.hosts.iter().position(|h| h.role == role)
    }

    /// The window holding `tab`, if it is open anywhere.
    pub fn host_of(&self, tab: T) -> Option<usize> {
        self.hosts.iter().position(|h| h.tree.find(tab).is_some())
    }

    pub fn by_window(&self, id: WindowId) -> Option<usize> {
        self.hosts.iter().position(|h| h.win.id() == id)
    }

    /// The egui context of the first window of a role (scripted tests).
    pub fn ctx(&self, role: T::Role) -> &egui::Context {
        self.hosts[self.find(role).unwrap_or(0)].win.ctx()
    }

    /// The other windows, by place and title (a tab's "move to").
    pub fn others(&self, here: usize) -> Vec<(usize, String)> {
        self.hosts.iter().enumerate().filter(|(i, _)| *i != here).map(|(i, h)| (i, self.title(h))).collect()
    }

    /// Which tabs are open in any window.
    pub fn open_tabs(&self) -> Vec<T> {
        self.hosts.iter().flat_map(|h| h.tree.tabs()).collect()
    }

    pub fn poll(&mut self) {
        for h in &mut self.hosts {
            h.win.poll();
        }
    }

    /// Draw again the window holding `tab` (its numbers changed).
    pub fn repaint(&mut self, tab: T) {
        if let Some(i) = self.host_of(tab) {
            self.hosts[i].win.request_repaint();
        }
    }

    /// Show `tab`: opened if it was closed, its window brought up, and it
    /// the tab shown there.
    pub fn show(&mut self, tab: T) {
        self.asks.push(Ask::Open(tab));
    }

    /// A window's hot key: its window shown or hidden; opened if closed.
    pub fn toggle(&mut self, tab: T) {
        match self.host_of(tab) {
            Some(i) => self.hosts[i].win.toggle_visible(),
            None => self.asks.push(Ask::Open(tab)),
        }
    }

    /// Open or close a tab at once (scripted tests; no window is made).
    pub fn set_open(&mut self, tab: T, open: bool) {
        if open {
            self.open(tab);
        } else {
            self.take(tab);
        }
    }

    /// Take up what was asked. `el` makes windows for detached tabs
    /// (without it, a detached tab goes home).
    pub fn apply(&mut self, el: Option<&ActiveEventLoop>, gpu: &Gpu) {
        for ask in std::mem::take(&mut self.asks) {
            match ask {
                Ask::Close(tab) => {
                    self.take(tab);
                }
                Ask::Open(tab) => self.open(tab),
                Ask::Move(tab, to, at) if to < self.hosts.len() => {
                    if self.take(tab) {
                        match at {
                            Some(d) => self.hosts[to].tree.insert(tab, d),
                            None => self.hosts[to].tree.push_first(tab),
                        }
                        self.hosts[to].win.show();
                    }
                }
                Ask::Move(..) => {}
                Ask::Detach(tab) | Ask::DetachAt(tab, _) => {
                    let Some(el) = el else { continue };
                    if self.take(tab) {
                        let i = self.add(el, gpu, T::Role::detached(), Tree::new(vec![tab]));
                        if let Ask::DetachAt(_, at) = ask {
                            // (Its title bar under the pointer, where it was let go.)
                            let at = winit::dpi::PhysicalPosition::new(at[0] - 60.0, at[1] - 12.0);
                            self.hosts[i].win.window.set_outer_position(at);
                        }
                        self.hosts[i].win.show();
                    }
                }
            }
        }
        // A detached window left with nothing in it goes; the others keep
        // their titles in step with what they hold.
        self.hosts.retain(|h| !h.role.is_detached() || !h.tree.is_empty());
        for i in 0..self.hosts.len() {
            self.retitle(i);
        }
    }

    /// After window `i`'s frame: `dragged` is a tab its dock had in hand.
    /// Followed past the window's edge: the window under the pointer is
    /// lit; let go there, the drop is asked for.
    pub fn track(&mut self, i: usize, dragged: Option<T>) {
        if self.drag.is_none() {
            self.drag = dragged.map(|tab| Drag { tab, from: i, over: None, at: None, drop: None });
        }
        let Some(d) = self.drag.filter(|d| d.from == i) else { return };
        let (win, ctx) = (&self.hosts[i].win.window, self.hosts[i].win.ctx());
        let (pos, up) = ctx.input(|inp| (inp.pointer.latest_pos(), !inp.pointer.primary_down()));
        let Some(pos) = pos else { return };
        let outside = !ctx.content_rect().contains(pos);
        let screen = win.inner_position().ok().map(|o| {
            let k = win.scale_factor();
            [o.x as f64 + pos.x as f64 * k, o.y as f64 + pos.y as f64 * k]
        });
        let over = if outside { screen.and_then(|p| drag::under(&self.frames(), p, i)) } else { None };
        // (The window under the pointer draws the landing as it moves.)
        for w in [d.over, over].into_iter().flatten() {
            self.hosts[w].win.request_repaint();
        }
        let drop = if over == d.over { d.drop } else { None };
        self.drag = Some(Drag { over, at: screen, drop, ..d });
        if up {
            self.drag = None;
            if let Some(w) = over {
                self.hosts[w].win.request_repaint();
            }
            if let (true, Some(p)) = (outside, screen) {
                self.asks.push(drag::drop(d.tab, over, drop, p));
            }
        }
    }

    /// Window `i`'s frame, the usual way: its dock drawn through `panels`
    /// and presented (or, with `offscreen` events injected, rendered to a
    /// texture for a scripted test), then a tab dragged out tracked.
    pub fn frame<P: Panels<T>>(&mut self, i: usize, gpu: &Gpu, panels: &mut P, offscreen: Option<Vec<egui::Event>>) -> Option<wgpu::Texture> {
        let others = self.others(i);
        let open = self.open_tabs();
        let dropping = self.dropping_into(i);
        let Docks { hosts, asks, .. } = self;
        let Host { win, tree, anim, .. } = &mut hosts[i];
        let mut app = DockApp { tree, anim, here: i, others, open, asks, panels, dragged: None, dropping, drop_here: None };
        let tex = match offscreen {
            Some(events) => {
                win.inject(events);
                Some(win.offscreen(gpu, &mut app))
            }
            None => {
                win.draw(gpu, &mut app);
                None
            }
        };
        let dragged = app.dragged;
        let drop_here = app.drop_here;
        if let Some(d) = &mut self.drag {
            if d.over == Some(i) {
                d.drop = drop_here;
            }
        }
        self.track(i, dragged);
        tex
    }

    /// A window event for host `i`: redraws are the caller's (they need
    /// the panels: call `frame`); a close goes through `close_requested`;
    /// the rest reach egui. True if the caller should draw the window.
    pub fn event(&mut self, gpu: &Gpu, i: usize, event: &winit::event::WindowEvent) -> bool {
        use winit::event::WindowEvent as E;
        match event {
            E::RedrawRequested => return true,
            E::CloseRequested => {
                self.close_requested(i);
            }
            _ => self.hosts[i].win.on_event(gpu, event),
        }
        false
    }

    /// The tab a drag would drop into window `i` while it is over it,
    /// with the pointer in that window's points.
    pub fn dropping_into(&self, i: usize) -> Option<(T, egui::Pos2)> {
        let d = self.drag.filter(|d| d.over == Some(i))?;
        let at = d.at?;
        let win = &self.hosts[i].win.window;
        let o = win.inner_position().ok()?;
        let k = win.scale_factor();
        Some((d.tab, egui::pos2(((at[0] - o.x as f64) / k) as f32, ((at[1] - o.y as f64) / k) as f32)))
    }

    /// Where each shown window is on the screen.
    fn frames(&self) -> Vec<(usize, drag::Frame)> {
        self.hosts
            .iter()
            .enumerate()
            .filter(|(_, h)| h.win.is_visible())
            .filter_map(|(i, h)| {
                let (o, size) = (h.win.window.outer_position().ok()?, h.win.window.outer_size());
                let min = [o.x as f64, o.y as f64];
                Some((i, drag::Frame { min, max: [min[0] + size.width as f64, min[1] + size.height as f64] }))
            })
            .collect()
    }

    /// A window's close button: a detached window's tabs go home and it
    /// goes; any other is hidden. True if it was taken away.
    pub fn close_requested(&mut self, i: usize) -> bool {
        if !self.hosts[i].role.is_detached() {
            self.hosts[i].win.window.set_visible(false);
            return false;
        }
        let host = self.hosts.remove(i);
        for tab in host.tree.tabs() {
            self.home(tab);
        }
        true
    }

    /// Take `tab` out of whichever window holds it. False if none did.
    fn take(&mut self, tab: T) -> bool {
        let Some(i) = self.host_of(tab) else { return false };
        self.hosts[i].tree.remove(tab)
    }

    /// Put `tab` in its home window (the first window, if that is gone).
    fn home(&mut self, tab: T) {
        if self.hosts.is_empty() {
            return;
        }
        let i = self.find(tab.home()).unwrap_or(0);
        self.hosts[i].tree.push_first(tab);
        self.hosts[i].win.request_repaint();
    }

    /// Open `tab` (home, if it was closed), its window shown, it in front.
    fn open(&mut self, tab: T) {
        if self.host_of(tab).is_none() {
            self.home(tab);
        }
        let Some(i) = self.host_of(tab) else { return };
        self.hosts[i].tree.set_active(tab);
        self.hosts[i].win.show();
    }

    fn retitle(&mut self, i: usize) {
        let t = self.title(&self.hosts[i]);
        self.hosts[i].win.window.set_title(&t);
    }

    /// A window's title: its role's, or for a detached one what it holds.
    fn title(&self, h: &Host<T>) -> String {
        if !h.role.is_detached() {
            return h.role.title().to_string();
        }
        let tabs: Vec<&str> = h.tree.tabs().into_iter().map(|t| t.title()).collect();
        format!("{} — {}", self.app, tabs.join(", "))
    }
}
