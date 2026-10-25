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

use std::sync::Arc;

use winit::window::Window;

use crate::shell::{Gpu, UiWindow};

pub use drag::Drag;
use serde::{Deserialize, Serialize};
pub use frame::{grip, Anim, DockApp, Panels, BAR_H};
pub use HostWin as Win;
pub use tree::{Drop, LeafId, Side, Tree, Zone};
pub use viewer::tab_rect;

/// A panel a window can hold.
pub trait Tab: Copy + Eq + Hash + Debug + Serialize + serde::de::DeserializeOwned + 'static {
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
    /// A strip: a body this many points tall, whatever the window's size
    /// (a timeline, a status bar). A leaf of nothing but strips keeps that
    /// height in a split above or below it, and its divider stays put.
    fn strip(self) -> Option<f32> {
        None
    }
    /// It stays in the window it is in: dragged, it moves about within
    /// that window only, and its menu has no detach, move or close (an
    /// app's main view, drawn by that window alone).
    fn pinned(self) -> bool {
        false
    }
    /// The panel it shows, as `all()` lists it: itself, unless it is a
    /// copy (`copy`).
    fn panel(self) -> Self {
        self
    }
    /// The `n`th copy of this panel (0: the panel itself), when the same
    /// panel may be open in more than one place at once; each copy is a
    /// tab of its own (its own scroll, its own place).
    fn copy(self, n: u32) -> Option<Self> {
        (n == 0).then_some(self)
    }
}

/// What a window is for: its title and size. One role is "a window a tab
/// was detached into"; the rest are the app's home windows.
pub trait Role: Copy + Eq + Debug + Serialize + serde::de::DeserializeOwned + 'static {
    fn title(self) -> &'static str;
    fn size(self) -> (u32, u32);
    fn detached() -> Self;
    fn is_detached(self) -> bool {
        self == Self::detached()
    }
}

/// A layout as saved: every window, its tree and where it stood.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Layout<T, R> {
    pub windows: Vec<Saved<T, R>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Saved<T, R> {
    pub role: R,
    pub tree: Tree<T>,
    /// Outer position, physical pixels.
    pub at: Option<[f64; 2]>,
    /// Inner size, logical points.
    pub size: Option<[u32; 2]>,
    pub visible: bool,
}

/// A host's window: one of ours (`UiWindow`, drawn by `Docks::frame`), or
/// the app's own (its main window, whose egui frame the app runs itself
/// and draws the dock into with `Docks::draw_external`).
pub enum HostWin {
    Own(UiWindow),
    External { window: Arc<Window>, ctx: egui::Context },
}

impl HostWin {
    pub fn window(&self) -> &Window {
        match self {
            HostWin::Own(w) => &w.window,
            HostWin::External { window, .. } => window,
        }
    }
    pub fn ctx(&self) -> &egui::Context {
        match self {
            HostWin::Own(w) => w.ctx(),
            HostWin::External { ctx, .. } => ctx,
        }
    }
    pub fn id(&self) -> WindowId {
        self.window().id()
    }
    pub fn is_external(&self) -> bool {
        matches!(self, HostWin::External { .. })
    }
    pub fn is_visible(&self) -> bool {
        self.window().is_visible().unwrap_or(true)
    }
    pub fn request_repaint(&mut self) {
        match self {
            HostWin::Own(w) => w.request_repaint(),
            HostWin::External { window, ctx } => {
                ctx.request_repaint();
                window.request_redraw();
            }
        }
    }
    pub fn show(&mut self) {
        match self {
            HostWin::Own(w) => w.show(),
            HostWin::External { window, .. } => {
                window.set_visible(true);
                window.set_minimized(false);
                window.focus_window();
            }
        }
    }
    pub fn toggle_visible(&mut self) {
        match self {
            HostWin::Own(w) => w.toggle_visible(),
            HostWin::External { window, .. } => {
                let show = !window.is_visible().unwrap_or(true);
                window.set_visible(show);
            }
        }
    }
    pub fn set_visible(&mut self, show: bool) {
        match self {
            HostWin::Own(w) => w.set_visible(show),
            HostWin::External { window, .. } => window.set_visible(show),
        }
    }
    /// Our own window, to draw or inject into (none for the app's).
    pub fn own(&mut self) -> Option<&mut UiWindow> {
        match self {
            HostWin::Own(w) => Some(w),
            HostWin::External { .. } => None,
        }
    }
}

/// One tool window and the dock of tabs in it.
pub struct Host<T: Tab> {
    pub role: T::Role,
    pub win: HostWin,
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
    /// Into the leaf holding the second (a group dragged out: the rest of
    /// its leaf follows the one that led it).
    Follow(T, T),
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
    /// The drag out (`drag`) carries its tab's whole leaf.
    pub drag_group: bool,
}

impl<T: Tab> Docks<T> {
    /// No windows yet: `add` the home windows next.
    pub fn new(app: &'static str, headless: bool) -> Self {
        Self { app, hosts: Vec::new(), asks: Vec::new(), drag: None, headless, drag_group: false }
    }

    /// A window of `role` holding `dock`, shown. Returns its place among
    /// the hosts.
    pub fn add(&mut self, el: &ActiveEventLoop, gpu: &Gpu, role: T::Role, tree: Tree<T>) -> usize {
        let win = UiWindow::new(el, gpu, role.title(), role.size(), !self.headless);
        self.hosts.push(Host { role, win: HostWin::Own(win), tree, anim: Anim::default() });
        let i = self.hosts.len() - 1;
        self.retitle(i);
        i
    }

    /// The app's own window as a host: the app runs its egui frame and
    /// draws the dock into it with `draw_external` (and feeds `raw_input`
    /// itself). Never retitled, never closed by the dock.
    pub fn add_external(&mut self, role: T::Role, window: Arc<Window>, ctx: egui::Context, tree: Tree<T>) -> usize {
        self.hosts.push(Host { role, win: HostWin::External { window, ctx }, tree, anim: Anim::default() });
        self.hosts.len() - 1
    }

    pub fn is_external(&self, i: usize) -> bool {
        self.hosts[i].win.is_external()
    }

    /// Every one of our own windows clears to this colour.
    pub fn set_clear(&mut self, color: wgpu::Color) {
        for h in &mut self.hosts {
            if let Some(w) = h.win.own() {
                w.clear = color;
            }
        }
    }

    /// Show `tab`'s leaf alone in its window (the rest fold away), or put
    /// the layout back if it already is. The leaves glide either way.
    pub fn solo(&mut self, tab: T) {
        let Some(i) = self.host_of(tab) else { return };
        let leaf = self.hosts[i].tree.find(tab);
        let anim = &mut self.hosts[i].anim;
        anim.solo = if anim.solo == leaf { None } else { leaf };
        self.hosts[i].win.request_repaint();
    }

    /// The tab shown alone in window `i`, if one is.
    pub fn soloed(&self, i: usize) -> Option<T> {
        let h = &self.hosts[i];
        h.anim.solo.and_then(|l| h.tree.active(l))
    }

    /// Put window `i`'s layout back (nothing alone).
    pub fn unsolo(&mut self, i: usize) {
        if self.hosts[i].anim.solo.take().is_some() {
            self.hosts[i].win.request_repaint();
        }
    }

    /// `add`, but the window starts hidden (until its key or a tab opens).
    pub fn add_hidden(&mut self, el: &ActiveEventLoop, gpu: &Gpu, role: T::Role, tree: Tree<T>) -> usize {
        let i = self.add(el, gpu, role, tree);
        self.hosts[i].win.window().set_visible(false);
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
            if let Some(w) = h.win.own() {
                w.poll();
            }
        }
    }

    /// Draw again every window showing `tab`'s panel (its numbers changed).
    pub fn repaint(&mut self, tab: T) {
        for h in &mut self.hosts {
            if h.tree.tabs().iter().any(|t| t.panel() == tab.panel()) {
                h.win.request_repaint();
            }
        }
    }

    /// The window holding `tab`, or else any copy of its panel.
    pub fn host_of_panel(&self, tab: T) -> Option<usize> {
        self.host_of(tab).or_else(|| self.hosts.iter().position(|h| h.tree.tabs().iter().any(|t| t.panel() == tab.panel())))
    }

    /// Show `tab`: opened if it was closed, its window brought up, and it
    /// the tab shown there.
    pub fn show(&mut self, tab: T) {
        self.asks.push(Ask::Open(tab));
    }

    /// A window's hot key: its window shown or hidden; opened if closed.
    pub fn toggle(&mut self, tab: T) {
        match self.host_of_panel(tab) {
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
                Ask::Follow(tab, lead) => {
                    let Some(to) = self.host_of(lead).filter(|_| tab != lead && !tab.pinned()) else { continue };
                    if self.take(tab) {
                        let leaf = self.hosts[to].tree.find(lead);
                        match leaf {
                            Some(l) => self.hosts[to].tree.insert(tab, Drop::Leaf(l, Zone::Center)),
                            None => self.hosts[to].tree.push_first(tab),
                        }
                        self.hosts[to].tree.set_active(lead);
                    }
                }
                Ask::Detach(tab) | Ask::DetachAt(tab, _) => {
                    let Some(el) = el else { continue };
                    if self.take(tab) {
                        let i = self.add(el, gpu, T::Role::detached(), Tree::new(vec![tab]));
                        if let Ask::DetachAt(_, at) = ask {
                            // (Its title bar under the pointer, where it was let go.)
                            let at = winit::dpi::PhysicalPosition::new(at[0] - 60.0, at[1] - 12.0);
                            self.hosts[i].win.window().set_outer_position(at);
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
        let (win, ctx) = (self.hosts[i].win.window(), self.hosts[i].win.ctx());
        let (pos, up) = ctx.input(|inp| (inp.pointer.latest_pos(), !inp.pointer.primary_down()));
        let Some(pos) = pos else { return };
        let outside = !ctx.content_rect().contains(pos);
        let screen = win.inner_position().ok().map(|o| {
            let k = win.scale_factor();
            [o.x as f64 + pos.x as f64 * k, o.y as f64 + pos.y as f64 * k]
        });
        // (A pinned tab never leaves its window: past the edge it lands nowhere.)
        let outside = outside && !d.tab.pinned();
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
                // (a group: the rest of its leaf follows)
                if self.drag_group {
                    let h = &self.hosts[i].tree;
                    let leaf = h.find(d.tab);
                    let rest: Vec<T> = h.leaves().into_iter().find(|(l, ..)| Some(*l) == leaf).map(|(_, tabs, _)| tabs.iter().copied().filter(|t| *t != d.tab && !t.pinned()).collect()).unwrap_or_default();
                    self.asks.extend(rest.into_iter().map(|t| Ask::Follow(t, d.tab)));
                }
            }
            self.drag_group = false;
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
        let Some(win) = win.own() else { return None };
        let mut app = DockApp { tree, anim, here: i, others, open, asks, panels, dragged: None, dragged_group: false, dropping, drop_here: None };
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
        let (dragged, group) = (app.dragged, app.dragged_group);
        let drop_here = app.drop_here;
        self.after_frame(i, dragged, group, drop_here);
        tex
    }

    /// The dock drawn into the app's own egui frame (an external host):
    /// `ui` is the room it gets. Call `raw_input` before the frame.
    pub fn draw_external<P: Panels<T>>(&mut self, i: usize, ui: &mut egui::Ui, panels: &mut P) {
        let others = self.others(i);
        let open = self.open_tabs();
        let dropping = self.dropping_into(i);
        let Docks { hosts, asks, .. } = self;
        let Host { tree, anim, .. } = &mut hosts[i];
        let mut app = DockApp { tree, anim, here: i, others, open, asks, panels, dragged: None, dragged_group: false, dropping, drop_here: None };
        crate::shell::UiApp::ui(&mut app, ui);
        let (dragged, group) = (app.dragged, app.dragged_group);
        let drop_here = app.drop_here;
        self.after_frame(i, dragged, group, drop_here);
    }

    /// Before an external host's egui frame: the panels' claim on the raw
    /// input (a canvas' chords).
    pub fn raw_input<P: Panels<T>>(&mut self, i: usize, raw: &mut egui::RawInput, panels: &mut P) {
        let tabs = self.hosts[i].tree.tabs();
        let ctx = self.hosts[i].win.ctx().clone();
        panels.raw_input(&ctx, raw, &tabs);
        let h = &self.hosts[i];
        frame::keys_to_owner(&h.tree, &h.anim, &ctx, raw, &*panels);
    }

    fn after_frame(&mut self, i: usize, dragged: Option<T>, group: bool, drop_here: Option<Drop>) {
        if let Some(d) = &mut self.drag {
            if d.over == Some(i) {
                d.drop = drop_here;
            }
        }
        if self.drag.is_none() {
            self.drag_group = group;
        }
        self.track(i, dragged);
    }

    /// A window event for host `i`: redraws are the caller's (they need
    /// the panels: call `frame`); a close goes through `close_requested`;
    /// the rest reach egui. True if the caller should draw the window.
    pub fn event(&mut self, gpu: &Gpu, i: usize, event: &winit::event::WindowEvent) -> bool {
        use winit::event::WindowEvent as E;
        let Some(win) = self.hosts[i].win.own() else { return false };
        match event {
            E::RedrawRequested => return true,
            E::CloseRequested => {
                self.close_requested(i);
            }
            _ => win.on_event(gpu, event),
        }
        false
    }

    /// Every window as it stands: for prefs. Detached windows included.
    pub fn save(&self) -> Layout<T, T::Role> {
        let windows = self
            .hosts
            .iter()
            .map(|h| {
                let w = h.win.window();
                let at = w.outer_position().ok().map(|p| [p.x as f64, p.y as f64]);
                let size = w.inner_size();
                let k = w.scale_factor();
                Saved { role: h.role, tree: h.tree.clone(), at, size: Some([(size.width as f64 / k) as u32, (size.height as f64 / k) as u32]), visible: h.win.is_visible() }
            })
            .collect();
        Layout { windows }
    }

    /// Windows as saved: one per entry, placed and sized as they were. A
    /// tab a saved layout does not know goes to its home window after.
    pub fn restore(&mut self, el: &ActiveEventLoop, gpu: &Gpu, layout: Layout<T, T::Role>) {
        for s in layout.windows {
            let i = self.add(el, gpu, s.role, s.tree);
            let w = self.hosts[i].win.window();
            if let Some(size) = s.size {
                let _ = w.request_inner_size(winit::dpi::LogicalSize::new(size[0].max(100), size[1].max(80)));
            }
            if let Some(at) = s.at {
                w.set_outer_position(winit::dpi::PhysicalPosition::new(at[0], at[1]));
            }
            if !s.visible {
                w.set_visible(false);
            }
        }
    }

    /// The tab a drag would drop into window `i` while it is over it,
    /// with the pointer in that window's points.
    pub fn dropping_into(&self, i: usize) -> Option<(T, egui::Pos2)> {
        let d = self.drag.filter(|d| d.over == Some(i))?;
        let at = d.at?;
        let win = self.hosts[i].win.window();
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
                let (o, size) = (h.win.window().outer_position().ok()?, h.win.window().outer_size());
                let min = [o.x as f64, o.y as f64];
                Some((i, drag::Frame { min, max: [min[0] + size.width as f64, min[1] + size.height as f64] }))
            })
            .collect()
    }

    /// A window's close button: a detached window's tabs go home and it
    /// goes; any other is hidden. True if it was taken away.
    pub fn close_requested(&mut self, i: usize) -> bool {
        if !self.hosts[i].role.is_detached() {
            self.hosts[i].win.set_visible(false);
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
        // (a copy of its panel open somewhere does)
        let tab = self.hosts.iter().flat_map(|h| h.tree.tabs()).find(|t| *t == tab).or_else(|| self.hosts.iter().flat_map(|h| h.tree.tabs()).find(|t| t.panel() == tab.panel())).unwrap_or(tab);
        if self.host_of(tab).is_none() {
            self.home(tab);
        }
        let Some(i) = self.host_of(tab) else { return };
        self.hosts[i].tree.set_active(tab);
        self.hosts[i].win.show();
    }

    fn retitle(&mut self, i: usize) {
        if self.hosts[i].win.is_external() {
            return;
        }
        let t = self.title(&self.hosts[i]);
        self.hosts[i].win.window().set_title(&t);
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
