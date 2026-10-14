//! A canvas for anything placed in 2D: vector shapes, pictures, video,
//! cards of text, or an app's own kinds. The foundation a vector editor, a
//! photo editor or a Milanote-style board stands on: the camera, the hands
//! and the stack are here; what an item IS belongs to the app (`Scene`).
//!
//! The hands are Blender's (manifesto 11), through `crate::nav`:
//!
//! - MMB / alt+LMB pan, ctrl+MMB / ctrl+alt+LMB drag-zoom, wheel zooms
//!   about the pointer, two fingers pan, pinch zooms
//! - Home frames everything, F (or `.`) frames the selection; the camera
//!   glides there rather than jumping
//! - click selects, shift-click toggles, click empty space deselects;
//!   drag empty space for a box (shift adds, ctrl / cmd takes away)
//! - drag an item to move what is selected; ctrl snaps to the grid
//! - G / R / S grab, rotate, scale with no button held, X / Y lock an
//!   axis, ctrl snaps, shift is precise; click or Enter keeps it, right
//!   click or Esc puts everything back
//! - the selection box: drag a corner to scale about the opposite one
//!   (shift: about the centre), the knob above it to rotate
//! - A selects all, alt+A none, X / Delete deletes, shift+D duplicates
//!   and grabs the copies, ] / [ bring to front / send to back
//! - double-click a note (any editable item) to edit it in place
//! - files dropped on the canvas come back with the world point they
//!   landed on (decode is the app's)
//!
//! Every gesture is computed from its PRESS (a snapshot of the placements
//! and the pointer's start), never summed per frame, so a drag lands where
//! the hand is however many frames it took.
//!
//! | module   | what it owns                                               |
//! |----------|------------------------------------------------------------|
//! | `camera` | world <-> screen, zoom about a point, framing, the glide   |
//! | `xform`  | an item's placement: translate, rotate, scale              |
//! | `scene`  | the trait an app's document implements                     |
//! | `pen`    | drawing in local coordinates: meshes, paths, text          |
//! | `path`   | vector paths: béziers, flattening, ear-clip fill, hits     |
//! | `board`  | a ready-made scene: shapes, pictures, clips, notes         |
//! | `grid`   | the decade grid behind it all                              |
//!
//! This file is the widget: `Canvas::show`.

pub mod board;
pub mod camera;
pub mod grid;
pub mod path;
pub mod pen;
pub mod scene;
pub mod xform;

use egui::{vec2, Align2, Color32, FontId, Key, PointerButton, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};

pub use board::{Board, Clip, Content, FrameSource, ItemId, Note, Picture, Shape};
pub use camera::Camera;
pub use path::{Path, Seg};
pub use pen::Pen;
pub use scene::Scene;
pub use xform::Xform;

use crate::nav::{self, DragNav};

/// Screen points of slack for hitting an item.
const HIT_PX: f32 = 4.0;
/// A selection-box handle's half size, and how far from it a press counts.
const HANDLE_PX: f32 = 4.0;
const HANDLE_HIT_PX: f32 = 9.0;
/// The rotate knob sits this far above the box.
const KNOB_PX: f32 = 22.0;
/// Margin round what Home / F frame, and how far in they will zoom.
const FRAME_MARGIN: f32 = 40.0;
const FRAME_MAX_ZOOM: f32 = 4.0;
const GLIDE_S: f64 = 0.22;
const SNAP_DEG: f32 = 5.0;
const SNAP_SCALE: f32 = 0.1;
const PRECISE: f32 = 0.1;

/// The canvas's single-letter keys, so an app can route them through its
/// own key map. Delete and Backspace always delete too; Esc / Enter end a
/// G / R / S.
#[derive(Clone, Copy, Debug)]
pub struct Keys {
    pub grab: Key,
    pub rotate: Key,
    pub scale: Key,
    pub axis_x: Key,
    pub axis_y: Key,
    pub all: Key,
    pub delete: Key,
    pub duplicate: Key,
    pub frame_all: Key,
    pub frame_selected: Key,
    pub frame_selected_alt: Key,
    pub front: Key,
    pub back: Key,
}

impl Default for Keys {
    fn default() -> Self {
        Self {
            grab: Key::G,
            rotate: Key::R,
            scale: Key::S,
            axis_x: Key::X,
            axis_y: Key::Y,
            all: Key::A,
            delete: Key::X,
            duplicate: Key::D,
            frame_all: Key::Home,
            frame_selected: Key::F,
            frame_selected_alt: Key::Period,
            front: Key::CloseBracket,
            back: Key::OpenBracket,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Op {
    Grab,
    Rotate,
    Scale,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Axis {
    Free,
    X,
    Y,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum BoxMode {
    Replace,
    Add,
    Remove,
}

enum Gesture<Id> {
    Pan(PointerButton),
    Zoom(PointerButton, Pos2),
    Box { button: PointerButton, from: Pos2, mode: BoxMode },
    /// A drag on an item: the selection follows the pointer.
    Tweak { from: Pos2, snap: Vec<(Id, Xform)> },
    /// G / R / S, no button held. `press` is in screen space (R and S
    /// measure angles and distances there, as Blender does).
    Modal { op: Op, axis: Axis, pivot: Pos2, press: Pos2, snap: Vec<(Id, Xform)> },
    /// A selection-box handle: a corner scales about `pivot`, the knob
    /// rotates about it.
    Handle { rotate: bool, pivot: Pos2, from: Pos2, snap: Vec<(Id, Xform)> },
}

struct Glide {
    from: Camera,
    to: Camera,
    start: f64,
}

/// What the frame did to the scene.
pub struct Out<Id> {
    pub response: Response,
    /// The scene changed this frame (redraw what depends on it).
    pub changed: bool,
    /// A gesture finished with a change: one undo step's worth.
    pub committed: bool,
    pub selection_changed: bool,
    /// The world point under the pointer.
    pub pointer: Option<Pos2>,
    /// Files dropped on the canvas this frame, and where they landed.
    pub drops: Vec<(egui::DroppedFileHandle, Pos2)>,
    /// Items the frame deleted (already gone from the scene).
    pub removed: Vec<Id>,
}

pub struct Canvas<Id> {
    pub cam: Camera,
    /// In the order picked; the last is the active one.
    pub selection: Vec<Id>,
    pub keys: Keys,
    pub show_grid: bool,
    /// Frame everything on the next frame (set at start).
    pub fit_pending: bool,
    gesture: Option<Gesture<Id>>,
    glide: Option<Glide>,
    editing: Option<Id>,
    hover: Option<Id>,
    /// A press that confirmed a G / R / S: its drag must not start a box.
    swallow: bool,
    /// The G / R / S readout along the bottom.
    hint: Option<String>,
}

impl<Id> Default for Canvas<Id> {
    fn default() -> Self {
        Self {
            cam: Camera::default(),
            selection: vec![],
            keys: Keys::default(),
            show_grid: true,
            fit_pending: true,
            gesture: None,
            glide: None,
            editing: None,
            hover: None,
            swallow: false,
            hint: None,
        }
    }
}

impl<Id: Copy + Eq + std::hash::Hash + std::fmt::Debug> Canvas<Id> {
    /// Is a G / R / S or a drag in progress (an app may hold its own keys).
    pub fn busy(&self) -> bool {
        self.gesture.is_some() || self.editing.is_some()
    }

    pub fn editing(&self) -> Option<Id> {
        self.editing
    }

    /// Glide the camera to frame `bounds` (world).
    pub fn frame(&mut self, ui: &Ui, view: Rect, bounds: Rect) {
        let to = Camera::framing(view, bounds, FRAME_MARGIN, FRAME_MAX_ZOOM);
        self.glide = Some(Glide { from: self.cam, to, start: ui.input(|i| i.time) });
    }

    /// Takes all the space it is given.
    pub fn show<S: Scene<Id = Id>>(&mut self, ui: &mut Ui, scene: &mut S) -> Out<Id> {
        let (view, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let mut out = Out { response, changed: false, committed: false, selection_changed: false, pointer: None, drops: vec![], removed: vec![] };
        let ids = scene.ids();
        let before = self.selection.clone();
        self.selection.retain(|s| ids.contains(s));
        if self.editing.is_some_and(|e| !ids.contains(&e)) {
            self.editing = None;
        }

        if self.fit_pending && view.is_positive() {
            if let Some(b) = union(ids.iter().map(|&id| scene::bounds(scene, id))) {
                self.cam = Camera::framing(view, b, FRAME_MARGIN, FRAME_MAX_ZOOM);
            }
            self.fit_pending = false;
        }
        self.navigate(ui, &out.response, view);
        self.step_glide(ui);

        let pointer = ui.input(|i| i.pointer.latest_pos());
        let world = pointer.map(|p| self.cam.to_world(view, p));
        out.pointer = world.filter(|_| out.response.hovered());

        if self.editing.is_none() {
            let r = out.response.clone();
            self.keys_and_modal(ui, scene, view, &r, pointer, &mut out);
            self.select_and_drag(ui, scene, view, &r, &mut out);
        }

        // Files dropped while the pointer is over us.
        if out.response.contains_pointer() {
            let at = world.unwrap_or(self.cam.centre);
            out.drops = ui.input(|i| i.raw.dropped_files.clone()).into_iter().map(|f| (f, at)).collect();
        }

        self.hover = match (&self.gesture, world) {
            (None, Some(w)) if out.response.hovered() => self.pick(scene, w),
            _ => None,
        };

        self.paint(ui, scene, view, world);

        if let Some(id) = self.editing {
            let screen = self.screen_bounds(scene, view, &[id]).unwrap_or(view);
            out.changed |= scene.edit_ui(id, ui, screen);
            let done = ui.input(|i| i.key_pressed(Key::Escape)) || (ui.input(|i| i.pointer.any_pressed()) && pointer.is_some_and(|p| !screen.contains(p)));
            if done {
                self.editing = None;
                out.committed = true;
            }
        }

        if scene.animating() || self.glide.is_some() || matches!(self.gesture, Some(Gesture::Modal { .. })) {
            ui.ctx().request_repaint();
        }
        out.selection_changed = before != self.selection;
        out
    }

    fn navigate(&mut self, ui: &Ui, r: &Response, view: Rect) {
        let mods = ui.input(|i| i.modifiers);
        for b in [PointerButton::Primary, PointerButton::Secondary, PointerButton::Middle] {
            if r.drag_started_by(b) && !matches!(self.gesture, Some(Gesture::Modal { .. })) {
                let at = ui.input(|i| i.pointer.press_origin()).unwrap_or(view.center());
                match nav::drag_nav(b, mods) {
                    Some(DragNav::Pan) => self.gesture = Some(Gesture::Pan(b)),
                    Some(DragNav::Zoom) => self.gesture = Some(Gesture::Zoom(b, at)),
                    None => {}
                }
            }
        }
        match self.gesture {
            Some(Gesture::Pan(b)) => {
                self.glide = None;
                self.cam.pan(r.drag_delta());
                if !r.dragged_by(b) {
                    self.gesture = None;
                }
            }
            Some(Gesture::Zoom(b, at)) => {
                self.glide = None;
                self.cam.zoom_about(view, at, nav::drag_zoom(r.drag_delta()));
                if !r.dragged_by(b) {
                    self.gesture = None;
                }
            }
            _ => {}
        }
        if r.hovered() {
            let w = nav::wheel_nav(ui);
            if w.pan != Vec2::ZERO || w.zoom != 1.0 {
                self.glide = None;
            }
            self.cam.pan(w.pan);
            if w.zoom != 1.0 {
                let at = r.hover_pos().unwrap_or(view.center());
                self.cam.zoom_about(view, at, w.zoom);
            }
        }
    }

    fn step_glide(&mut self, ui: &Ui) {
        let Some(g) = &self.glide else { return };
        let t = ((ui.input(|i| i.time) - g.start) / GLIDE_S).clamp(0.0, 1.0) as f32;
        let t = t * t * (3.0 - 2.0 * t);
        self.cam = g.from.lerp(&g.to, t);
        if t >= 1.0 {
            self.glide = None;
        }
    }

    /// The front-most item under a world point.
    pub fn pick<S: Scene<Id = Id>>(&self, scene: &S, world: Pos2) -> Option<Id> {
        scene.ids().into_iter().rev().find(|&id| {
            let xf = scene.xform(id);
            scene.hit(id, xf.inverse(world), HIT_PX / (self.cam.zoom * xf.unit()))
        })
    }

    fn snapshot<S: Scene<Id = Id>>(&self, scene: &S) -> Vec<(Id, Xform)> {
        self.selection.iter().map(|&id| (id, scene.xform(id))).collect()
    }

    fn keys_and_modal<S: Scene<Id = Id>>(&mut self, ui: &Ui, scene: &mut S, view: Rect, r: &Response, pointer: Option<Pos2>, out: &mut Out<Id>) {
        let typing = ui.ctx().memory(|m| m.focused().is_some());
        let k = self.keys;
        let (mods, pressed) = ui.input(|i| {
            let keys: Vec<Key> = i.events.iter().filter_map(|e| match e {
                egui::Event::Key { key, pressed: true, repeat: false, .. } => Some(*key),
                _ => None,
            }).collect();
            (i.modifiers, keys)
        });
        let hit = |key: Key| !typing && pressed.contains(&key);

        // A G / R / S in progress takes the keys first.
        if let Some(Gesture::Modal { op, axis, pivot, press, snap }) = &mut self.gesture {
            if hit(k.axis_x) {
                *axis = if *axis == Axis::X { Axis::Free } else { Axis::X };
            }
            if hit(k.axis_y) {
                *axis = if *axis == Axis::Y { Axis::Free } else { Axis::Y };
            }
            let (op, axis, pivot, press) = (*op, *axis, *pivot, *press);
            let snap = snap.clone();
            if let Some(p) = pointer {
                let (shift, rot, scale, hint) = modal_move(op, axis, &self.cam, view, pivot, press, p, mods);
                for &(id, xf) in &snap {
                    scene.set_xform(id, xf.moved(pivot, shift, rot, scale));
                }
                self.hint = Some(hint);
                out.changed = true;
            }
            let (lmb, rmb) = ui.input(|i| (i.pointer.button_pressed(PointerButton::Primary), i.pointer.button_pressed(PointerButton::Secondary)));
            let inside = pointer.is_some_and(|p| view.contains(p));
            if hit(Key::Escape) || (rmb && inside) {
                for &(id, xf) in &snap {
                    scene.set_xform(id, xf);
                }
                self.gesture = None;
                self.hint = None;
                self.swallow = rmb;
                out.changed = true;
            } else if hit(Key::Enter) || (lmb && inside) {
                self.gesture = None;
                self.hint = None;
                self.swallow = lmb;
                out.committed = true;
            }
            return;
        }
        if self.gesture.is_some() || !r.hovered() {
            return;
        }

        let start = |canvas: &mut Self, scene: &S, op: Op| {
            if canvas.selection.is_empty() {
                return;
            }
            let Some(p) = pointer else { return };
            let pivot = union(canvas.selection.iter().map(|&id| scene::bounds(scene, id))).map_or(p, |b| b.center());
            canvas.gesture = Some(Gesture::Modal { op, axis: Axis::Free, pivot, press: p, snap: canvas.snapshot(scene) });
        };
        if mods.shift && hit(k.duplicate) {
            let copies = scene.duplicate(&self.selection);
            if !copies.is_empty() {
                self.selection = copies;
                out.changed = true;
                start(self, scene, Op::Grab);
            }
        } else if hit(k.grab) {
            start(self, scene, Op::Grab);
        } else if hit(k.rotate) {
            start(self, scene, Op::Rotate);
        } else if hit(k.scale) {
            start(self, scene, Op::Scale);
        } else if hit(k.all) {
            self.selection = if mods.alt { vec![] } else { scene.ids() };
        } else if hit(k.delete) || hit(Key::Delete) || hit(Key::Backspace) {
            if !self.selection.is_empty() {
                let gone = std::mem::take(&mut self.selection);
                scene.remove(&gone);
                out.removed = gone;
                out.changed = true;
                out.committed = true;
            }
        } else if hit(k.front) || hit(k.back) {
            if !self.selection.is_empty() {
                scene.restack(&self.selection, hit(k.front));
                out.changed = true;
                out.committed = true;
            }
        } else if hit(k.frame_all) {
            if let Some(b) = union(scene.ids().into_iter().map(|id| scene::bounds(scene, id))) {
                self.frame(ui, view, b);
            }
        } else if hit(k.frame_selected) || hit(k.frame_selected_alt) {
            let pick = if self.selection.is_empty() { scene.ids() } else { self.selection.clone() };
            if let Some(b) = union(pick.into_iter().map(|id| scene::bounds(scene, id))) {
                self.frame(ui, view, b);
            }
        }
    }

    fn select_and_drag<S: Scene<Id = Id>>(&mut self, ui: &Ui, scene: &mut S, view: Rect, r: &Response, out: &mut Out<Id>) {
        let mods = ui.input(|i| i.modifiers);
        let primary = PointerButton::Primary;
        if self.swallow {
            if !ui.input(|i| i.pointer.any_down()) {
                self.swallow = false;
            }
            return;
        }
        let to_world = |p: Pos2| self.cam.to_world(view, p);

        if r.double_clicked() {
            if let Some(id) = r.interact_pointer_pos().and_then(|p| self.pick(scene, to_world(p))) {
                if scene.editable(id) {
                    self.selection = vec![id];
                    self.editing = Some(id);
                    return;
                }
            }
        }
        if r.clicked() && !mods.alt {
            let hit = r.interact_pointer_pos().and_then(|p| self.pick(scene, to_world(p)));
            match hit {
                Some(id) if mods.shift => {
                    if let Some(i) = self.selection.iter().position(|&s| s == id) {
                        self.selection.remove(i);
                    } else {
                        self.selection.push(id);
                    }
                }
                Some(id) => self.selection = vec![id],
                None if !mods.shift => self.selection.clear(),
                None => {}
            }
        }

        if r.drag_started_by(primary) && self.gesture.is_none() && !mods.alt {
            let press = ui.input(|i| i.pointer.press_origin()).unwrap_or(view.center());
            let from = to_world(press);
            if let Some((rotate, pivot)) = self.handle_at(scene, view, press, mods.shift) {
                self.gesture = Some(Gesture::Handle { rotate, pivot, from, snap: self.snapshot(scene) });
            } else if let Some(id) = self.pick(scene, from) {
                if !self.selection.contains(&id) {
                    if mods.shift {
                        self.selection.push(id);
                    } else {
                        self.selection = vec![id];
                    }
                }
                self.gesture = Some(Gesture::Tweak { from, snap: self.snapshot(scene) });
            } else {
                let mode = if mods.shift {
                    BoxMode::Add
                } else if mods.ctrl || mods.command {
                    BoxMode::Remove
                } else {
                    BoxMode::Replace
                };
                self.gesture = Some(Gesture::Box { button: primary, from, mode });
            }
        }

        let cur = ui.input(|i| i.pointer.latest_pos()).map(to_world);
        let released = !r.dragged_by(primary);
        match &self.gesture {
            Some(Gesture::Tweak { from, snap }) => {
                if let Some(c) = cur {
                    let mut d = c - *from;
                    if mods.ctrl || mods.command {
                        let step = grid::step(self.cam.zoom).0;
                        d = (d / step).round() * step;
                    }
                    for &(id, xf) in snap {
                        scene.set_xform(id, xf.moved(xf.pos, d, 0.0, Vec2::splat(1.0)));
                    }
                    out.changed = true;
                }
                if released {
                    self.gesture = None;
                    out.committed = true;
                }
            }
            Some(Gesture::Handle { rotate, pivot, from, snap }) => {
                if let Some(c) = cur {
                    let (rot, k) = if *rotate {
                        let mut a = angle(c - *pivot) - angle(*from - *pivot);
                        if mods.ctrl || mods.command {
                            a = snap_to(a.to_degrees(), SNAP_DEG).to_radians();
                        }
                        (a, 1.0)
                    } else {
                        let mut k = (c - *pivot).length() / (*from - *pivot).length().max(1e-6);
                        if mods.ctrl || mods.command {
                            k = snap_to(k, SNAP_SCALE);
                        }
                        (0.0, k)
                    };
                    for &(id, xf) in snap {
                        scene.set_xform(id, xf.moved(*pivot, Vec2::ZERO, rot, Vec2::splat(k)));
                    }
                    out.changed = true;
                }
                if released {
                    self.gesture = None;
                    out.committed = true;
                }
            }
            Some(Gesture::Box { button, from, mode }) => {
                if !r.dragged_by(*button) {
                    if let Some(c) = cur {
                        let area = Rect::from_two_pos(*from, c);
                        let inside: Vec<Id> = scene.ids().into_iter().filter(|&id| scene::bounds(scene, id).intersects(area)).collect();
                        match mode {
                            BoxMode::Replace => self.selection = inside,
                            BoxMode::Add => inside.into_iter().for_each(|id| {
                                if !self.selection.contains(&id) {
                                    self.selection.push(id)
                                }
                            }),
                            BoxMode::Remove => self.selection.retain(|s| !inside.contains(s)),
                        }
                    }
                    self.gesture = None;
                }
            }
            _ => {}
        }
    }

    /// The selection's box on screen: an oriented rect for one item, the
    /// world-aligned bounds of many. Corners clockwise from top-left.
    fn select_box<S: Scene<Id = Id>>(&self, scene: &S, view: Rect) -> Option<[Pos2; 4]> {
        let world = match self.selection.as_slice() {
            [] => return None,
            [one] => scene::corners(scene, *one),
            many => {
                let b = union(many.iter().map(|&id| scene::bounds(scene, id)))?;
                [b.left_top(), b.right_top(), b.right_bottom(), b.left_bottom()]
            }
        };
        Some(world.map(|p| self.cam.to_screen(view, p)))
    }

    /// Where the rotate knob sits: above the top edge's middle, along the
    /// box's own "up".
    fn knob(q: &[Pos2; 4]) -> Pos2 {
        let top = q[0] + (q[1] - q[0]) * 0.5;
        let up = (q[0] - q[3]).normalized();
        let up = if up.is_finite() { up } else { vec2(0.0, -1.0) };
        top + up * KNOB_PX
    }

    /// A handle under a screen point: (rotate?, world pivot).
    fn handle_at<S: Scene<Id = Id>>(&self, scene: &S, view: Rect, at: Pos2, about_centre: bool) -> Option<(bool, Pos2)> {
        let q = self.select_box(scene, view)?;
        let centre = self.cam.to_world(view, q[0] + (q[2] - q[0]) * 0.5);
        if (Self::knob(&q) - at).length() <= HANDLE_HIT_PX {
            return Some((true, centre));
        }
        let i = (0..4).find(|&i| (q[i] - at).length() <= HANDLE_HIT_PX)?;
        Some((false, if about_centre { centre } else { self.cam.to_world(view, q[(i + 2) % 4]) }))
    }

    fn screen_bounds<S: Scene<Id = Id>>(&self, scene: &S, view: Rect, ids: &[Id]) -> Option<Rect> {
        let b = union(ids.iter().map(|&id| scene::bounds(scene, id)))?;
        Some(Rect::from_two_pos(self.cam.to_screen(view, b.min), self.cam.to_screen(view, b.max)))
    }

    fn paint<S: Scene<Id = Id>>(&self, ui: &Ui, scene: &S, view: Rect, world: Option<Pos2>) {
        let p = ui.painter_at(view);
        let vis = ui.visuals();
        p.rect_filled(view, 0.0, vis.extreme_bg_color);
        if self.show_grid {
            let line = vis.weak_text_color().gamma_multiply(0.18);
            grid::paint(&p, view, &self.cam, line, vis.weak_text_color().gamma_multiply(0.4));
        }
        let shown = self.cam.visible(view);
        let time = scene.time();
        for id in scene.ids() {
            if !scene::bounds(scene, id).intersects(shown) || self.editing == Some(id) {
                continue;
            }
            scene.paint(id, &Pen { painter: &p, cam: &self.cam, view, xf: scene.xform(id), time });
        }

        let sel = vis.selection.stroke.color;
        let outline = |id: Id, stroke: Stroke| {
            let c = scene::corners(scene, id).map(|q| self.cam.to_screen(view, q));
            p.add(egui::Shape::closed_line(c.to_vec(), stroke));
        };
        if let Some(h) = self.hover.filter(|h| !self.selection.contains(h)) {
            outline(h, Stroke::new(1.0, sel.gamma_multiply(0.6)));
        }
        for &id in &self.selection {
            outline(id, Stroke::new(1.0, sel));
        }
        if let Some(q) = self.select_box(scene, view) {
            let active = !matches!(self.gesture, Some(Gesture::Modal { .. }) | Some(Gesture::Tweak { .. }));
            if self.selection.len() > 1 {
                p.add(egui::Shape::dashed_line(&[q[0], q[1], q[2], q[3], q[0]], Stroke::new(1.0, sel), 4.0, 3.0));
            }
            if active {
                let knob = Self::knob(&q);
                p.line_segment([q[0] + (q[1] - q[0]) * 0.5, knob], Stroke::new(1.0, sel));
                p.circle(knob, HANDLE_PX, vis.extreme_bg_color, Stroke::new(1.5, sel));
                for c in q {
                    p.rect(Rect::from_center_size(c, Vec2::splat(HANDLE_PX * 2.0)), 1.0, vis.extreme_bg_color, Stroke::new(1.5, sel), egui::StrokeKind::Middle);
                }
            }
        }
        if let (Some(Gesture::Box { from, mode, .. }), Some(w)) = (&self.gesture, world) {
            let r = Rect::from_two_pos(self.cam.to_screen(view, *from), self.cam.to_screen(view, w));
            let c = if *mode == BoxMode::Remove { vis.error_fg_color } else { sel };
            p.rect(r, 0.0, c.gamma_multiply(0.08), Stroke::new(1.0, c), egui::StrokeKind::Inside);
        }
        if let Some(Gesture::Modal { pivot, axis, .. }) = &self.gesture {
            let pv = self.cam.to_screen(view, *pivot);
            match axis {
                Axis::X => {
                    p.hline(view.x_range(), pv.y, Stroke::new(1.0, Color32::from_rgb(230, 70, 80)));
                }
                Axis::Y => {
                    p.vline(pv.x, view.y_range(), Stroke::new(1.0, Color32::from_rgb(120, 200, 60)));
                }
                Axis::Free => {}
            }
            p.circle_stroke(pv, 3.0, Stroke::new(1.0, sel));
        }
        if let Some(h) = &self.hint {
            p.text(view.left_bottom() + vec2(8.0, -8.0), Align2::LEFT_BOTTOM, h, FontId::monospace(11.0), vis.text_color());
        }
    }
}

/// G / R / S from the press to `cur`: (shift, rotation, scale, the words
/// for the status line).
#[allow(clippy::too_many_arguments)]
fn modal_move(op: Op, axis: Axis, cam: &Camera, view: Rect, pivot: Pos2, press: Pos2, cur: Pos2, mods: egui::Modifiers) -> (Vec2, f32, Vec2, String) {
    let snap = mods.ctrl || mods.command;
    let fine = if mods.shift { PRECISE } else { 1.0 };
    let lock = match axis {
        Axis::Free => "",
        Axis::X => "  along X",
        Axis::Y => "  along Y",
    };
    match op {
        Op::Grab => {
            let mut d = (cur - press) / cam.zoom * fine;
            match axis {
                Axis::X => d.y = 0.0,
                Axis::Y => d.x = 0.0,
                Axis::Free => {}
            }
            if snap {
                let step = grid::step(cam.zoom).0;
                d = (d / step).round() * step;
            }
            (d, 0.0, Vec2::splat(1.0), format!("move  {:.1}  {:.1}{lock}", d.x, d.y))
        }
        Op::Rotate => {
            let pv = cam.to_screen(view, pivot);
            let mut a = (angle(cur - pv) - angle(press - pv)) * fine;
            if snap {
                a = snap_to(a.to_degrees(), SNAP_DEG).to_radians();
            }
            (Vec2::ZERO, a, Vec2::splat(1.0), format!("rotate  {:.1}°", a.to_degrees()))
        }
        Op::Scale => {
            let pv = cam.to_screen(view, pivot);
            let mut k = (cur - pv).length() / (press - pv).length().max(1.0);
            k = 1.0 + (k - 1.0) * fine;
            if snap {
                k = snap_to(k, SNAP_SCALE);
            }
            let s = match axis {
                Axis::Free => Vec2::splat(k),
                Axis::X => vec2(k, 1.0),
                Axis::Y => vec2(1.0, k),
            };
            (Vec2::ZERO, 0.0, s, format!("scale  {k:.3}{lock}"))
        }
    }
}

fn angle(v: Vec2) -> f32 {
    v.y.atan2(v.x)
}

fn snap_to(v: f32, step: f32) -> f32 {
    (v / step).round() * step
}

fn union(rects: impl Iterator<Item = Rect>) -> Option<Rect> {
    rects.reduce(|a, b| a.union(b))
}

/// A world rect centred on `c`, `size` across: shorthand for apps placing
/// content.
pub fn around(c: Pos2, size: Vec2) -> Rect {
    Rect::from_center_size(c, size)
}

#[cfg(test)]
mod tests;
