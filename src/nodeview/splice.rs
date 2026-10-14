//! Dropping a node onto a wire inserts it there. While a node is dragged,
//! `target` says which wire it would join, so the canvas can show it.

use super::graph::{Edge, Graph};
use super::laid::Laid;
use super::wires;

/// The wire node `node` would join if dropped now: of the wires passing
/// through its body that it could legally join (`Graph::splice_ports`),
/// the one running nearest its centre.
pub fn target<G: Graph>(g: &G, ids: &[G::Id], laid: &Laid<G::Id>, node: usize) -> Option<usize> {
    let id = ids.get(node)?;
    let body = laid.rect(node);
    g.edges()
        .iter()
        .enumerate()
        .filter(|(_, e)| g.splice_ports(id, e).is_some())
        .filter_map(|(i, e)| {
            let (a, b) = laid.wire_ends(e)?;
            Some((i, wires::through(a, b, body)?))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

/// Put `node` in the middle of the wire `edge`: a -> b becomes
/// a -> node -> b. False (and nothing changes) if it cannot go there.
pub fn splice<G: Graph>(g: &mut G, node: &G::Id, edge: &Edge<G::Id>) -> bool {
    let Some((input, output)) = g.splice_ports(node, edge) else { return false };
    // The second `connect` replaces the wire being spliced.
    if !g.connect(&edge.from, &edge.from_port, node, &input) {
        return false;
    }
    if !g.connect(node, &output, &edge.to, &edge.to_port) {
        // Put it back as it was.
        g.disconnect(node, &input);
        g.connect(&edge.from, &edge.from_port, &edge.to, &edge.to_port);
        return false;
    }
    true
}
