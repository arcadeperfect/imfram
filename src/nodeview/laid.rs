//! One frame's layout: draws every node and records where each node and
//! port ended up, so the wiring code can hit-test against it.

use std::collections::{HashMap, HashSet};

use egui::{PointerButton, Pos2, Rect, Ui};

use super::graph::{Body, Edge, Graph};
use super::node::{self, Look, PortKey};
use super::style::PORT_HIT;
use super::{dot, Edit};

/// Where things are this frame, in canvas space.
pub struct Laid<Id> {
    ports: HashMap<PortKey<Id>, Pos2>,
    /// Node rects, by node index (in `Graph::nodes` order).
    rects: Vec<Rect>,
    /// Node indices in draw order, bottom to top.
    order: Vec<usize>,
    /// Bounding rect of every node.
    pub bounds: Rect,
}

/// Something a port asked for this frame.
pub enum PortAction<Id> {
    /// Start dragging a new wire out of this output.
    StartWire(PortKey<Id>),
    /// Take the wire off this input; `redrag` keeps hold of its free end.
    Detach { key: PortKey<Id>, redrag: bool },
}

pub struct Drawn<Id> {
    pub laid: Laid<Id>,
    pub actions: Vec<PortAction<Id>>,
    /// Index of a node whose delete button was clicked.
    pub delete: Option<usize>,
    /// Index of the node being dragged by its body, and of one whose drag
    /// ended this frame.
    pub dragged: Option<usize>,
    pub dropped: Option<usize>,
    /// How far the dragged node moved this frame.
    pub drag_delta: egui::Vec2,
    /// Index of a node clicked this frame.
    pub clicked: Option<usize>,
    pub edit: Edit,
}

impl<Id: Clone + Eq + std::hash::Hash> Laid<Id> {
    pub fn rect(&self, node: usize) -> Rect {
        self.rects[node]
    }

    /// The topmost node under a point.
    pub fn node_at(&self, p: Pos2) -> Option<usize> {
        self.order.iter().rev().copied().find(|&i| self.rects[i].contains(p))
    }

    pub fn port(&self, node: &Id, port: &str, output: bool) -> Option<Pos2> {
        self.ports.get(&PortKey { node: node.clone(), port: port.to_string(), output }).copied()
    }

    /// The closest port within reach of `p` that passes `accept`. (An
    /// output wins a tie: a corner dot's two ports share one spot.)
    pub fn nearest_port(&self, p: Pos2, accept: impl Fn(&PortKey<Id>) -> bool) -> Option<(PortKey<Id>, Pos2)> {
        self.ports
            .iter()
            .filter(|(k, _)| accept(k))
            .map(|(k, &pos)| (k, pos, pos.distance(p)))
            .filter(|&(_, _, d)| d <= PORT_HIT)
            .min_by(|a, b| a.2.total_cmp(&b.2).then(b.0.output.cmp(&a.0.output)))
            .map(|(k, pos, _)| (k.clone(), pos))
    }

    /// Both ends of a wire, if both its ports are on screen.
    pub fn wire_ends(&self, e: &Edge<Id>) -> Option<(Pos2, Pos2)> {
        Some((self.port(&e.from, &e.from_port, true)?, self.port(&e.to, &e.to_port, false)?))
    }

    pub fn ports(&self) -> impl Iterator<Item = (&PortKey<Id>, Pos2)> {
        self.ports.iter().map(|(k, &p)| (k, p))
    }
}

/// Draw every node. Wider nodes go later so their bodies sit over their
/// neighbours, and corner dots last of all. `hold_still`: E is held, so
/// nodes are stretched rather than moved. `offset`: where a tidy's glide
/// has each node, against where it is. `selected`: drawn outlined.
pub fn draw_nodes<G: Graph>(ui: &mut Ui, g: &mut G, ids: &[G::Id], wire_in_hand: bool, hold_still: bool, offset: &dyn Fn(&G::Id) -> egui::Vec2, selected: &dyn Fn(&G::Id) -> bool) -> Drawn<G::Id> {
    let edges = g.edges();
    let wired: HashSet<(G::Id, String)> = edges.iter().map(|e| (e.to.clone(), e.to_port.clone())).collect();
    let is_dot = |i: usize| g.is_dot(&ids[i]);
    let mut order: Vec<usize> = (0..ids.len()).collect();
    order.sort_by(|&a, &b| is_dot(a).cmp(&is_dot(b)).then(g.width(&ids[a]).total_cmp(&g.width(&ids[b]))));
    // A dot is the colour of what it carries.
    let dot_colors: HashMap<G::Id, egui::Color32> = ids
        .iter()
        .filter(|id| g.is_dot(id))
        .map(|id| {
            let feed = edges.iter().find(|e| &e.to == id);
            let color = match feed {
                Some(e) => g.wire_style(&e.from, &e.from_port).color,
                None => g.wire_style(id, "out").color,
            };
            (id.clone(), color)
        })
        .collect();

    let mut drawn = Drawn {
        laid: Laid { ports: HashMap::new(), rects: vec![Rect::NOTHING; ids.len()], order, bounds: Rect::NOTHING },
        actions: Vec::new(),
        delete: None,
        dragged: None,
        dropped: None,
        drag_delta: egui::Vec2::ZERO,
        clicked: None,
        edit: Edit::default(),
    };
    for i in drawn.laid.order.clone() {
        let id = &ids[i];
        // (Drawn where a tidy's glide has it, which is not yet where it is.)
        let off = offset(id);
        let pos = g.pos(id);
        g.set_pos(id, pos + off);
        let sel = selected(id);
        let look = Look { wired: &wired, wire_in_hand, hold_still, selected: sel };
        let out = match dot_colors.get(id) {
            Some(&color) => dot::show(ui, g, id, color, wire_in_hand, hold_still, sel),
            None => node::show(ui, g, id, &look),
        };
        let pos = g.pos(id);
        g.set_pos(id, pos - off);
        drawn.laid.bounds = drawn.laid.bounds.union(out.rect);
        drawn.laid.rects[i] = out.rect;
        drawn.edit |= Edit { changed: out.body.changed, moved: out.body.moved || out.moved };
        if out.delete {
            drawn.delete = Some(i);
        }
        if out.dragging {
            drawn.dragged = Some(i);
            drawn.drag_delta = out.drag_delta;
        }
        if out.clicked {
            drawn.clicked = Some(i);
        }
        if out.dropped {
            drawn.dropped = Some(i);
        }
        for (key, pos, resp) in out.ports {
            let is_wired = !key.output && wired.contains(&(key.node.clone(), key.port.clone()));
            if key.output {
                if resp.drag_started_by(PointerButton::Primary) {
                    drawn.actions.push(PortAction::StartWire(key.clone()));
                }
            } else if is_wired {
                if resp.drag_started_by(PointerButton::Primary) {
                    drawn.actions.push(PortAction::Detach { key: key.clone(), redrag: true });
                } else if resp.secondary_clicked() {
                    drawn.actions.push(PortAction::Detach { key: key.clone(), redrag: false });
                }
            }
            drawn.laid.ports.insert(key, pos);
        }
    }
    let _ = Body::default();
    drawn
}
