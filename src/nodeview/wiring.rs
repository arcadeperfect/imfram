//! Wires in motion: dragging one out of a port, carrying one with G, and
//! cutting with ctrl-drag.

use egui::{Pos2, Shape, Stroke, Ui};

use super::graph::Graph;
use super::laid::{Laid, PortAction};
use super::wires;

/// A wire being pulled from an output port.
struct InHand<Id> {
    from: Id,
    from_port: String,
    /// Picked up with G: it follows the cursor with no button held, and
    /// lands on a click instead of a release.
    carried: bool,
}

/// The wire in the user's hand, if any.
pub struct Wiring<Id> {
    wire: Option<InHand<Id>>,
}

impl<Id> Default for Wiring<Id> {
    fn default() -> Self {
        Self { wire: None }
    }
}

impl<Id: Clone + Eq + std::hash::Hash> Wiring<Id> {
    pub fn in_hand(&self) -> bool {
        self.wire.is_some()
    }

    pub fn drop_wire(&mut self) {
        self.wire = None;
    }

    /// A port was dragged or right-clicked. True when the graph changed.
    pub fn apply<G: Graph<Id = Id>>(&mut self, g: &mut G, action: PortAction<Id>) -> bool {
        match action {
            PortAction::StartWire(key) => {
                self.wire = Some(InHand { from: key.node, from_port: key.port, carried: false });
                false
            }
            PortAction::Detach { key, redrag } => self.detach(g, &key.node, &key.port, redrag.then_some(false)),
        }
    }

    /// Take the wire off an input. `keep` holds its free end: `Some(carried)`.
    fn detach<G: Graph<Id = Id>>(&mut self, g: &mut G, node: &Id, port: &str, keep: Option<bool>) -> bool {
        let Some(e) = g.disconnect(node, port) else { return false };
        if let Some(carried) = keep {
            self.wire = Some(InHand { from: e.from, from_port: e.from_port, carried });
        }
        true
    }

    /// G: drop the wire in hand, or pick one up under the pointer and carry
    /// it. Over an output (or a node body, for its first output) a new wire
    /// starts; over a wired input, that wire comes off. True when the graph
    /// changed.
    pub fn grab<G: Graph<Id = Id>>(&mut self, g: &mut G, ids: &[Id], laid: &Laid<Id>, pointer: Option<Pos2>) -> bool {
        if self.wire.take().is_some() {
            return false;
        }
        let Some(p) = pointer else { return false };
        match laid.nearest_port(p, |_| true) {
            Some((k, _)) if k.output => {
                self.wire = Some(InHand { from: k.node, from_port: k.port, carried: true });
                false
            }
            Some((k, _)) => self.detach(g, &k.node, &k.port, Some(true)),
            None => {
                self.wire = laid.node_at(p).and_then(|i| {
                    let id = &ids[i];
                    let port = g.outputs(id).first()?.id.clone();
                    Some(InHand { from: id.clone(), from_port: port, carried: true })
                });
                false
            }
        }
    }

    /// Advance the wire in hand: its preview shape, and whether landing it
    /// changed the graph. A dragged wire ends on release; a carried one on a
    /// click that lands it (a miss keeps carrying) or Esc.
    pub fn update<G: Graph<Id = Id>>(&mut self, ui: &Ui, g: &mut G, laid: &Laid<Id>, pointer: Option<Pos2>) -> (Option<Shape>, bool) {
        let Some(wire) = &self.wire else { return (None, false) };
        let from = laid.port(&wire.from, &wire.from_port, true);
        // Only inputs this wire may legally land on attract it.
        let target = pointer.and_then(|p| laid.nearest_port(p, |k| !k.output && k.node != wire.from && g.can_connect(&wire.from, &wire.from_port, &k.node, &k.port)));
        let hot = ui.visuals().selection.stroke.color;
        let preview = match (from, pointer) {
            (Some(from), Some(p)) => {
                let width = g.wire_style(&wire.from, &wire.from_port).width;
                Some(wires::shape(from, target.as_ref().map_or(p, |t| t.1), hot, width))
            }
            _ => None,
        };

        let (released, clicked, esc) = ui.input(|i| (!i.pointer.any_down(), i.pointer.primary_clicked(), i.key_pressed(egui::Key::Escape)));
        let landing = if wire.carried { clicked && target.is_some() } else { released };
        let mut changed = false;
        if landing {
            if let Some((to, _)) = target {
                changed = g.connect(&wire.from, &wire.from_port, &to.node, &to.port);
            }
            self.wire = None;
        } else if wire.carried && esc {
            self.wire = None;
        }
        (preview, changed)
    }
}

/// The ctrl-drag wire cutter: it slices as it goes.
pub struct Cutter {
    last: Pos2,
    trail: Vec<Pos2>,
}

impl Cutter {
    pub fn start(p: Pos2) -> Self {
        Self { last: p, trail: vec![p] }
    }

    /// Cut every wire the newest stretch of the stroke crosses. True when
    /// the graph changed.
    pub fn slice<G: Graph>(&mut self, g: &mut G, laid: &Laid<G::Id>, pointer: Option<Pos2>) -> bool {
        let Some(p) = pointer.filter(|p| p.distance(self.last) > 1.0) else { return false };
        let (a, b) = (self.last, p);
        let mut changed = false;
        for e in g.edges() {
            if laid.wire_ends(&e).is_some_and(|(w0, w1)| wires::crosses(w0, w1, a, b)) {
                changed |= g.disconnect(&e.to, &e.to_port).is_some();
            }
        }
        self.trail.push(p);
        self.last = p;
        changed
    }

    /// The stroke so far, at a constant on-screen width.
    pub fn trail(&self, canvas_scale: f32, color: egui::Color32) -> Shape {
        Shape::line(self.trail.clone(), Stroke::new(1.5 / canvas_scale, color))
    }
}
