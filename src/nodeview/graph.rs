//! What a node view needs of the app's graph: the document stays the
//! app's (its ids, its node kinds, its parameters), and the view reads
//! and edits it through this trait. The body of a node (its controls)
//! is the app's to draw: the view hands it the room.

use egui::{Color32, Pos2, Ui};
use std::fmt::Debug;
use std::hash::Hash;

/// One port of a node.
#[derive(Clone, Debug, PartialEq)]
pub struct Port {
    pub id: String,
    pub label: String,
    /// The kind of thing it carries, the app's own number: ports connect
    /// only when `Graph::can_connect` says so, but equal kinds draw alike.
    pub kind: u8,
}

/// A wire from an output to an input.
#[derive(Clone, Debug, PartialEq)]
pub struct Edge<Id> {
    pub from: Id,
    pub from_port: String,
    pub to: Id,
    pub to_port: String,
}

/// How a wire (and the ports at its ends) is drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct WireStyle {
    pub color: Color32,
    pub width: f32,
    /// Written small halfway along it (a range, a type).
    pub label: Option<String>,
}

/// A node that can be added from the search popup.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// The app's type key, handed back to `Graph::add`.
    pub ty: String,
    pub title: String,
    /// Its submenu in the browse list.
    pub menu: String,
}

/// What drawing a node's body did.
#[derive(Default, Clone, Copy)]
pub struct Body {
    /// A value changed: the app re-evaluates.
    pub changed: bool,
    /// Layout only (a range, a fold): save, no re-evaluation.
    pub moved: bool,
}

pub trait Graph {
    type Id: Clone + Eq + Hash + Debug + Ord;

    fn nodes(&self) -> Vec<Self::Id>;
    fn edges(&self) -> Vec<Edge<Self::Id>>;

    fn pos(&self, id: &Self::Id) -> Pos2;
    fn set_pos(&mut self, id: &Self::Id, pos: Pos2);
    /// The node's width on the canvas (E stretches it: `set_width`).
    fn width(&self, id: &Self::Id) -> f32;
    fn set_width(&mut self, _id: &Self::Id, _width: f32) {}

    fn title(&self, id: &Self::Id) -> String;
    /// The title's hover text.
    fn about(&self, _id: &Self::Id) -> String {
        String::new()
    }
    fn inputs(&self, id: &Self::Id) -> Vec<Port>;
    fn outputs(&self, id: &Self::Id) -> Vec<Port>;
    /// A corner dot: a node that only bends a wire (both ports at its
    /// centre). `add_dot` makes one; `None` if the app has no such node.
    fn is_dot(&self, _id: &Self::Id) -> bool {
        false
    }
    fn add_dot(&mut self, _pos: Pos2) -> Option<Self::Id> {
        None
    }
    /// Drawn with the accent outline (a sink).
    fn is_sink(&self, _id: &Self::Id) -> bool {
        false
    }
    fn deletable(&self, id: &Self::Id) -> bool {
        !self.is_sink(id)
    }

    /// The node's controls, in `width` of room. The view has drawn the
    /// title and port labels; wired inputs are the app's to show as such.
    fn body(&mut self, ui: &mut Ui, id: &Self::Id, width: f32) -> Body;

    /// What a wire from this output looks like.
    fn wire_style(&self, from: &Self::Id, from_port: &str) -> WireStyle;
    /// A port's colour: by default an output's wire, an input's kind
    /// matched to any output of the same kind on the graph, else plain.
    fn port_color(&self, id: &Self::Id, port: &str, output: bool, plain: Color32) -> Color32 {
        if output {
            return self.wire_style(id, port).color;
        }
        let kind = self.inputs(id).iter().find(|p| p.id == port).map(|p| p.kind);
        for n in self.nodes() {
            for o in self.outputs(&n) {
                if Some(o.kind) == kind {
                    return self.wire_style(&n, &o.id).color;
                }
            }
        }
        plain
    }
    /// A port's hover text.
    fn port_tip(&self, _id: &Self::Id, port: &Port, output: bool) -> String {
        let what = if output { "out" } else { "in" };
        if port.label.is_empty() { what.into() } else { format!("{}: {what}", port.label) }
    }

    /// May this wire be made? Types, loops: the app's rules. The default
    /// refuses self-wires, loops, and mismatched kinds.
    fn can_connect(&self, from: &Self::Id, from_port: &str, to: &Self::Id, to_port: &str) -> bool {
        if from == to || would_cycle(self, from, to) {
            return false;
        }
        let a = self.outputs(from).into_iter().find(|p| p.id == from_port).map(|p| p.kind);
        let b = self.inputs(to).into_iter().find(|p| p.id == to_port).map(|p| p.kind);
        a.is_some() && a == b
    }
    /// Make the wire (an input holds one: the old one goes). False if
    /// refused. The app may put a converter between (graav's ToUnit).
    fn connect(&mut self, from: &Self::Id, from_port: &str, to: &Self::Id, to_port: &str) -> bool;
    /// Take the wire off an input, returning it.
    fn disconnect(&mut self, to: &Self::Id, to_port: &str) -> Option<Edge<Self::Id>>;
    /// Remove a node and its wires (a dot's wire joins back up).
    fn remove(&mut self, id: &Self::Id);

    /// The ports `node` would use in the middle of `edge`: its first free
    /// input that takes the wire, and its first output the far end takes.
    /// None when it cannot go there.
    fn splice_ports(&self, node: &Self::Id, edge: &Edge<Self::Id>) -> Option<(String, String)> {
        if &edge.from == node || &edge.to == node || self.edges().iter().any(|e| &e.from == node) {
            return None;
        }
        let wired: Vec<String> = self.edges().iter().filter(|e| &e.to == node).map(|e| e.to_port.clone()).collect();
        let input = self.inputs(node).into_iter().find(|p| !wired.contains(&p.id) && self.can_connect(&edge.from, &edge.from_port, node, &p.id))?;
        let output = self.outputs(node).into_iter().find(|p| self.can_connect(node, &p.id, &edge.to, &edge.to_port))?;
        Some((input.id, output.id))
    }

    /// What the search popup offers, and adding one at a canvas point.
    fn catalog(&self) -> Vec<Entry>;
    fn add(&mut self, ty: &str, pos: Pos2) -> Option<Self::Id>;
}

/// Would wiring `from -> to` close a loop? (True if `from` is downstream of `to`.)
pub fn would_cycle<G: Graph + ?Sized>(g: &G, from: &G::Id, to: &G::Id) -> bool {
    let edges = g.edges();
    let mut stack = vec![to.clone()];
    let mut seen = std::collections::HashSet::new();
    while let Some(n) = stack.pop() {
        if &n == from {
            return true;
        }
        if seen.insert(n.clone()) {
            stack.extend(edges.iter().filter(|e| e.from == n).map(|e| e.to.clone()));
        }
    }
    false
}
