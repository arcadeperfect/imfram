//! Hold E over a node and move the pointer sideways: right widens it,
//! left narrows it, about its centre (each edge moves as far as the
//! pointer does).

use egui::{vec2, Pos2};

use super::graph::Graph;
use super::laid::Laid;
use super::style::{NODE_W_MAX, NODE_W_MIN};

/// A stretch in progress: the node, and where the pointer was.
pub struct Stretch<Id> {
    node: Id,
    /// The pointer's x last frame, in canvas units.
    last_x: f32,
}

impl<Id: Clone + Eq + std::hash::Hash> Stretch<Id> {
    /// E went down: take the node under the pointer.
    pub fn start(ids: &[Id], laid: &Laid<Id>, p: Pos2) -> Option<Self> {
        let i = laid.node_at(p)?;
        Some(Self { node: ids[i].clone(), last_x: p.x })
    }

    /// E is still held: widen or narrow by however far the pointer moved.
    /// True when the node changed.
    pub fn update<G: Graph<Id = Id>>(&mut self, g: &mut G, p: Pos2) -> bool {
        let dx = p.x - self.last_x;
        self.last_x = p.x;
        dx != 0.0 && stretch(g, &self.node, dx)
    }
}

/// Move both edges of `node` out by `dx` (in, when negative), within the
/// widths a node may have; its centre stays put. True if it changed.
pub fn stretch<G: Graph>(g: &mut G, node: &G::Id, dx: f32) -> bool {
    if g.is_dot(node) {
        return false;
    }
    let before = g.width(node);
    let width = (before + 2.0 * dx).clamp(NODE_W_MIN, NODE_W_MAX);
    if width == before {
        return false;
    }
    g.set_pos(node, g.pos(node) - vec2((width - before) * 0.5, 0.0));
    g.set_width(node, width);
    true
}
