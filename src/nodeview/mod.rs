//! A node canvas for any graph: fixed-width nodes with the app's controls
//! inline, data flowing top to bottom (inputs on a node's top edge,
//! outputs on its bottom edge). The hands are graav's, which are wobble's:
//!
//! - drag an output dot onto an input dot to connect (one wire per input)
//! - drag a wired input dot away (or right-click it) to disconnect
//! - ctrl-drag across wires to cut them
//! - G over a port (or a node, for its output) picks a wire up and carries
//!   it with no button held; click a port to land it, G / Esc to drop it
//! - Tab (or right-click the background) to add a node: browse the
//!   submenus, or type to search
//! - drop a loose node on a wire to insert it there
//! - drag a node near another: a free input and an output in reach court
//!   (a dashed line); shift wires them (wobble's magnet)
//! - shift-click a wire to put a corner dot on it; drag the dot to bend
//!   the wire, right-click it to take it out
//! - hold E over a node and move right to widen it, left to narrow it
//! - T tidies: wobble's layered layout, the nodes gliding there
//! - click a node to select it (shift-click adds / takes it out), drag
//!   the background to select in a box (shift adds); dragging a selected
//!   node moves them all; Delete / Backspace deletes them
//! - hover a node's title or a port for what it does
//! - pan / zoom with the shared navigation hands (`crate::nav`)
//!
//! | module    | what it owns                                              |
//! |-----------|-----------------------------------------------------------|
//! | `graph`   | the trait the app's document implements                   |
//! | `demo`    | a toy graph for the playground and the tests              |
//! | `view`    | pan / zoom, the canvas layer, claimed gestures            |
//! | `node`    | drawing one node: frame, title, the app's body, ports     |
//! | `dot`     | drawing a corner dot                                      |
//! | `laid`    | drawing all nodes; where nodes and ports ended up         |
//! | `wires`   | wire curve geometry and cut tests (pure)                  |
//! | `wiring`  | the wire in hand (drag, G-carry) and the cutter           |
//! | `magnet`  | free inputs courting outputs; shift wires them            |
//! | `splice`  | which wire a dragged node would join                      |
//! | `stretch` | hold E to widen or narrow a node (pure)                   |
//! | `search`  | the add-node popup                                        |
//! | `tidy`    | the layered layout, then a staggered glide                |
//! | `hotkeys` | the keys and chords, the box listing them, the key strip  |
//! | `style`   | sizes                                                     |
//!
//! This file ties them into one frame: `NodeView::show`.

pub mod demo;
mod dot;
pub mod graph;
pub mod hotkeys;
mod laid;
mod magnet;
mod node;
pub mod search;
mod splice;
mod stretch;
pub mod style;
pub mod tidy;
mod view;
pub mod wires;
mod wiring;

use std::collections::{HashMap, HashSet};

use egui::{vec2, Align2, FontId, Modifiers, PointerButton, Pos2, Rect, Sense, Shape, Ui, Vec2};

pub use graph::{Body, Edge, Entry, Graph, Port, WireStyle};
pub use node::PortKey;
use search::Search;
use stretch::Stretch;
use view::View;
use wiring::Wiring;

use crate::nav::{self, Press};

/// What a graph edit means for the caller.
#[derive(Default, Clone, Copy, Debug, PartialEq)]
pub struct Edit {
    /// Output changed: re-evaluate.
    pub changed: bool,
    /// Layout only: save, no re-evaluation.
    pub moved: bool,
}

impl std::ops::BitOrAssign for Edit {
    fn bitor_assign(&mut self, other: Self) {
        self.changed |= other.changed;
        self.moved |= other.moved;
    }
}

/// Input the canvas claimed before egui's widgets saw it. The app fills
/// it from its `GestureFilter` and its own key handling (so a text field
/// elsewhere can keep G and Tab).
#[derive(Default)]
pub struct Input<'a> {
    pub presses: &'a [Press],
    pub releases: &'a [PointerButton],
    pub tab: bool,
    /// G was pressed (and no text field wanted it).
    pub grab: bool,
    /// E is held: stretch the node under the pointer.
    pub stretch: bool,
    /// Shift is held (alone): wire the pairs courting.
    pub shift: bool,
    /// T was pressed: tidy the graph.
    pub tidy: bool,
    /// Delete / Backspace was pressed: delete the selected nodes.
    pub delete: bool,
}

/// Chords the canvas takes for itself: navigation, and ctrl-drag to cut
/// (macOS delivers ctrl+left as a right-button press). For the app's
/// `GestureFilter::process`.
pub fn claims(button: PointerButton, mods: Modifiers) -> bool {
    nav::drag_nav(button, mods).is_some() || (mods.ctrl && matches!(button, PointerButton::Primary | PointerButton::Secondary))
}

/// This frame's key commands that act at the pointer.
#[derive(Clone, Copy)]
struct Keys {
    grab: bool,
    stretch: bool,
    shift: bool,
    /// Shift-clicked the background here (canvas): a dot, if on a wire.
    dot_at: Option<Pos2>,
}

pub struct NodeView<Id> {
    view: View,
    wiring: Wiring<Id>,
    search: Option<Search>,
    /// The key strip's words: which row's are out, how far.
    strip: crate::widgets::keystrip::Reveal,
    /// Nodes gliding to where a tidy put them.
    glide: tidy::Glide<Id>,
    /// Each node's height as last drawn (for tidy's rows).
    heights: HashMap<Id, f32>,
    /// The node E is stretching.
    stretch: Option<Stretch<Id>>,
    /// The node whose ports court (`magnet`): the one being dragged, or
    /// just dropped while the pointer is still on it.
    magnet_node: Option<Id>,
    /// The selected nodes.
    selected: HashSet<Id>,
    /// A box select in progress: its corner (canvas), and what was
    /// selected before it (kept when shift began it).
    marquee: Option<(Pos2, HashSet<Id>)>,
    /// Draw the key strip along the top.
    pub show_strip: bool,
    /// Last frame's ports in window coordinates (scripted self-tests).
    pub ports_on_screen: Vec<(Id, String, bool, Pos2)>,
}

impl<Id> Default for NodeView<Id> {
    fn default() -> Self {
        Self {
            view: View::default(),
            wiring: Wiring::default(),
            search: None,
            strip: Default::default(),
            glide: tidy::Glide::default(),
            heights: HashMap::new(),
            stretch: None,
            magnet_node: None,
            selected: HashSet::new(),
            marquee: None,
            show_strip: true,
            ports_on_screen: Vec::new(),
        }
    }
}

impl<Id: Clone + Eq + std::hash::Hash + std::fmt::Debug + Ord> NodeView<Id> {
    /// Re-frame the whole graph next frame.
    pub fn fit(&mut self) {
        self.view.request_fit();
    }

    /// Canvas zoom and pan (scripted UI self-tests).
    pub fn transform(&self) -> (f32, Vec2) {
        self.view.transform()
    }

    pub fn to_canvas(&self, window: Pos2) -> Pos2 {
        self.view.to_canvas(window)
    }

    /// The selected nodes.
    pub fn selected(&self) -> &HashSet<Id> {
        &self.selected
    }

    /// The add-node popup is open.
    pub fn searching(&self) -> bool {
        self.search.is_some()
    }

    /// Tidy the graph (wobble's): laid out again where it stands, the
    /// nodes gliding to their new places.
    pub fn tidy<G: Graph<Id = Id>>(&mut self, g: &mut G) {
        let at = |g: &G| g.nodes().iter().map(|id| (id.clone(), g.pos(id))).collect::<Vec<_>>();
        let before = at(g);
        let heights = &self.heights;
        let size = |g: &G, id: &Id| vec2(g.width(id), heights.get(id).copied().unwrap_or(60.0));
        tidy::tidy(g, &size);
        self.glide.start(&before, &at(g));
    }

    /// A graph that arrived without positions (every node at the origin)
    /// is laid out and framed.
    pub fn place_if_unplaced<G: Graph<Id = Id>>(&mut self, g: &mut G) -> bool {
        let ids = g.nodes();
        if ids.len() > 1 && ids.iter().all(|id| g.pos(id) == Pos2::ZERO) {
            tidy::tidy(g, &|g, id| vec2(g.width(id), 60.0));
            self.fit();
            return true;
        }
        false
    }

    /// Draw the graph and handle one frame of interaction.
    pub fn show<G: Graph<Id = Id>>(&mut self, ui: &mut Ui, g: &mut G, input: &Input) -> Edit {
        let mut edit = Edit::default();
        if self.place_if_unplaced(g) {
            edit.moved = true;
        }
        if input.tidy && ui.input(|i| i.pointer.latest_pos()).is_some_and(|p| ui.max_rect().contains(p)) && self.search.is_none() {
            self.tidy(g);
            edit.moved = true;
        }
        let dt = ui.input(|i| i.stable_dt).clamp(0.001, 0.05);
        if self.glide.step(dt) {
            ui.ctx().request_repaint();
        }

        let (outer, background) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let pointer_win = ui.input(|i| i.pointer.latest_pos());
        let hovered = pointer_win.is_some_and(|p| outer.contains(p));

        let nav = self.view.navigate(ui, outer, hovered, input.presses, input.releases);
        if nav.gesture_started {
            self.wiring.drop_wire();
        }

        let mut canvas = self.view.canvas_ui(ui, outer);
        let pointer = pointer_win.map(|p| self.view.to_canvas(p));
        let keys_here = hovered && self.search.is_none();
        // (Read off the pointer, not `background`: egui gives a click near
        // a wire to whatever else is in reach. `frame` skips nodes and ports.)
        let clicked = ui.input(|i| i.pointer.primary_clicked());
        let dot_at = pointer.filter(|_| clicked && input.shift && keys_here);
        let keys = Keys { grab: input.grab && keys_here, stretch: input.stretch && keys_here, shift: input.shift && keys_here, dot_at };
        // Selection: a click on the background clears it, a drag there
        // boxes it (shift keeps what was selected).
        let shift_held = ui.input(|i| i.modifiers.shift);
        if background.clicked() && !shift_held {
            self.selected.clear();
        }
        if background.drag_started_by(PointerButton::Primary) {
            if let Some(p) = ui.input(|i| i.pointer.press_origin()) {
                let base = if shift_held { self.selected.clone() } else { HashSet::new() };
                self.marquee = Some((self.view.to_canvas(p), base));
            }
        }
        let bounds = self.frame(&mut canvas, g, pointer, keys, &mut edit);
        if !background.dragged() {
            self.marquee = None;
        }
        if input.delete && keys_here {
            for id in std::mem::take(&mut self.selected) {
                if g.nodes().contains(&id) && g.deletable(&id) {
                    g.remove(&id);
                    edit.changed = true;
                }
            }
        }
        self.view.fit_if_pending(ui, outer, bounds);
        if self.show_strip {
            let strip = outer.shrink2(egui::vec2(12.0, 14.0));
            crate::widgets::keystrip::show(ui, strip, outer, &hotkeys::caps(ui), &mut self.strip, false);
        }

        // Tab, or a right-click on the background, opens the node search.
        let open_at = if input.tab && hovered {
            pointer_win
        } else if background.secondary_clicked() {
            background.interact_pointer_pos().or(pointer_win)
        } else {
            None
        };
        if input.tab && self.search.is_some() {
            self.search = None;
        } else if let Some(p) = open_at {
            self.search = Some(Search::open(self.view.to_canvas(p), p));
        }
        if let Some(search) = &mut self.search {
            let catalog = g.catalog();
            match search.show(ui, &catalog) {
                search::Outcome::Open => {}
                search::Outcome::Closed => self.search = None,
                search::Outcome::Picked(ty) => {
                    let at = search.canvas - vec2(style::NODE_W * 0.5, 12.0);
                    if g.add(&ty, at).is_some() {
                        edit.changed = true;
                    }
                    self.search = None;
                }
            }
        }
        edit
    }

    /// Everything on the canvas layer: nodes, wires, and what the pointer
    /// does to them. Returns the nodes' bounding rect.
    fn frame<G: Graph<Id = Id>>(&mut self, ui: &mut Ui, g: &mut G, pointer: Option<Pos2>, keys: Keys, edit: &mut Edit) -> Rect {
        let hot = ui.visuals().selection.stroke.color;
        let ids = g.nodes();
        // Wires sit under the nodes but are only known after layout.
        let wires_slot = ui.painter().add(Shape::Noop);
        self.selected.retain(|id| ids.contains(id));
        let selected = &self.selected;
        let laid::Drawn { laid, actions, delete, dragged, dropped, drag_delta, clicked, edit: drawn } =
            laid::draw_nodes(ui, g, &ids, self.wiring.in_hand(), keys.stretch, &|id| self.glide.offset(id), &|id| selected.contains(id));
        if dragged.is_some() {
            self.glide.stop(); // (a hand on a node: everything where it is)
        }
        self.select(g, &ids, &laid, clicked, dragged, drag_delta, pointer, ui.input(|i| i.modifiers.shift), edit);
        self.heights = ids.iter().enumerate().map(|(i, id)| (id.clone(), laid.rect(i).height())).filter(|(_, h)| h.is_finite() && *h > 0.0).collect();
        *edit |= drawn;

        // E held: the node under the pointer is stretched. Layout only.
        match pointer.filter(|_| keys.stretch) {
            Some(p) => {
                if self.stretch.is_none() {
                    self.stretch = Stretch::start(&ids, &laid, p);
                }
                if let Some(s) = &mut self.stretch {
                    edit.moved |= s.update(g, p);
                }
            }
            None => self.stretch = None,
        }
        // Shift-click on a wire: a corner dot there.
        if let Some(p) = keys.dot_at.filter(|&p| laid.node_at(p).is_none() && laid.nearest_port(p, |_| true).is_none()) {
            let edges = g.edges();
            let near = edges
                .iter()
                .filter_map(|e| Some((e, wires::distance(laid.wire_ends(e)?.0, laid.wire_ends(e)?.1, p))))
                .filter(|&(_, d)| d <= style::DOT_HIT)
                .min_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((e, _)) = near {
                if let Some(dot) = g.add_dot(p - vec2(style::DOT_R, style::DOT_R)) {
                    if splice::splice(g, &dot, e) {
                        edit.changed = true;
                    } else {
                        g.remove(&dot);
                    }
                }
            }
        }
        for action in actions {
            edit.changed |= self.wiring.apply(g, action);
        }
        if keys.grab {
            edit.changed |= self.wiring.grab(g, &ids, &laid, pointer);
        }
        // A node dropped on a wire joins it; while it hovers, that wire
        // lights up. (One node alone: a selection moves as it is.)
        let alone = self.selected.len() <= 1;
        if let Some((node, wire)) = dropped.filter(|_| alone).and_then(|i| Some((i, splice::target(g, &ids, &laid, i)?))) {
            let e = g.edges()[wire].clone();
            edit.changed |= splice::splice(g, &ids[node], &e);
        }
        let joining = dragged.filter(|_| alone).and_then(|i| splice::target(g, &ids, &laid, i));
        let courting = self.courting(g, &ids, &laid, dragged, pointer);
        if keys.shift {
            edit.changed |= magnet::wire(g, &courting);
        }
        if let Some(cutter) = self.view.cutter() {
            edit.changed |= cutter.slice(g, &laid, pointer);
        }

        let mut shapes: Vec<Shape> = Vec::new();
        for (i, e) in g.edges().iter().enumerate() {
            let Some((a, b)) = laid.wire_ends(e) else { continue };
            let st = g.wire_style(&e.from, &e.from_port);
            let (color, width) = if joining == Some(i) { (hot, st.width + 2.0) } else { (st.color, st.width) };
            shapes.push(wires::shape(a, b, color, width));
            // (A wire into a corner dot goes on out of it: one label, on the last stretch.)
            if let Some(label) = st.label.filter(|_| !g.is_dot(&e.to)) {
                shapes.extend(wire_label(ui, wires::midpoint(a, b), &label));
            }
        }
        shapes.extend(courting.iter().filter(|_| !keys.shift).flat_map(|c| magnet::tether(c, hot)));
        let (preview, landed) = self.wiring.update(ui, g, &laid, pointer);
        edit.changed |= landed;
        shapes.extend(preview);
        ui.painter().set(wires_slot, Shape::Vec(shapes));

        // The selection box, and the cutter's trail, go over everything.
        let scale = self.view.scale();
        if let (Some((corner, _)), Some(p)) = (&self.marquee, pointer) {
            let r = Rect::from_two_pos(*corner, p);
            ui.painter().rect(r, 2.0, hot.gamma_multiply(0.08), egui::Stroke::new(1.0 / scale, hot), egui::StrokeKind::Inside);
        }
        let cut = ui.visuals().error_fg_color;
        if let Some(cutter) = self.view.cutter() {
            ui.painter().add(cutter.trail(scale, cut));
        }

        self.ports_on_screen = laid.ports().map(|(k, p)| (k.node.clone(), k.port.clone(), k.output, self.view.to_window(p))).collect();

        if let Some(index) = delete {
            g.remove(&ids[index]);
            edit.changed = true;
        }
        laid.bounds
    }

    /// This frame's selecting: a click on a node selects it (shift: in or
    /// out), dragging an unselected node selects it, and the rest of a
    /// selection follows the node dragged. A box select in progress
    /// selects what it touches.
    #[allow(clippy::too_many_arguments)]
    fn select<G: Graph<Id = Id>>(&mut self, g: &mut G, ids: &[Id], laid: &laid::Laid<Id>, clicked: Option<usize>, dragged: Option<usize>, delta: Vec2, pointer: Option<Pos2>, shift: bool, edit: &mut Edit) {
        if let Some(i) = clicked {
            let id = &ids[i];
            if !shift {
                self.selected.clear();
                self.selected.insert(id.clone());
            } else if !self.selected.remove(id) {
                self.selected.insert(id.clone());
            }
        }
        if let Some(i) = dragged {
            let id = &ids[i];
            if !self.selected.contains(id) {
                if !shift {
                    self.selected.clear();
                }
                self.selected.insert(id.clone());
            }
            if delta != Vec2::ZERO {
                for other in self.selected.iter().filter(|o| *o != id) {
                    g.set_pos(other, g.pos(other) + delta);
                    edit.moved = true;
                }
            }
        }
        if let (Some((corner, base)), Some(p)) = (&self.marquee, pointer) {
            let r = Rect::from_two_pos(*corner, p);
            let mut sel = base.clone();
            sel.extend(ids.iter().enumerate().filter(|&(i, _)| laid.rect(i).intersects(r)).map(|(_, id)| id.clone()));
            self.selected = sel;
        }
    }

    /// The pairs courting this frame (`magnet`). The node in hand is the
    /// one `dragged`, or the last one dragged while `pointer` is still on it.
    fn courting<G: Graph<Id = Id>>(&mut self, g: &G, ids: &[Id], laid: &laid::Laid<Id>, dragged: Option<usize>, pointer: Option<Pos2>) -> Vec<magnet::Courting<Id>> {
        if let Some(i) = dragged {
            self.magnet_node = Some(ids[i].clone());
        }
        let under = pointer.and_then(|p| laid.node_at(p)).map(|i| &ids[i]);
        if dragged.is_none() && under != self.magnet_node.as_ref() {
            self.magnet_node = None;
        }
        match &self.magnet_node {
            Some(id) => magnet::courting(g, laid.ports(), id),
            None => Vec::new(),
        }
    }
}

/// A wire's label, written small at `at` on a dark pill.
fn wire_label(ui: &Ui, at: Pos2, text: &str) -> [Shape; 2] {
    let vis = ui.visuals();
    let galley = ui.painter().layout_no_wrap(text.to_string(), FontId::proportional(style::LABEL_FONT), vis.strong_text_color());
    let rect = Align2::CENTER_CENTER.anchor_size(at, galley.size());
    [Shape::rect_filled(rect.expand2(vec2(3.0, 1.0)), 3.0, vis.extreme_bg_color.gamma_multiply(0.85)), Shape::galley(rect.min, galley, vis.strong_text_color())]
}
