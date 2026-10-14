//! Wobble's magnet: bring a node's free input near an output it could take
//! (or its output near such an input) and the two court: a dashed line
//! between them, brighter as they close. Shift wires the close ones.
//!
//! Only the node in hand courts: the one being dragged, or the one just
//! dropped while the pointer is still on it.

use std::collections::HashSet;

use egui::{Color32, Pos2, Shape, Stroke};

use super::graph::Graph;
use super::node::PortKey;
use super::style::{MAGNET_CLOSE, MAGNET_REACH};

/// A free input and the output reaching for it.
#[derive(Debug, Clone, PartialEq)]
pub struct Courting<Id> {
    pub from: PortKey<Id>,
    pub to: PortKey<Id>,
    /// Where the output and the input are (canvas units).
    pub a: Pos2,
    pub b: Pos2,
    /// 0 at the edge of reach, 1 touching.
    pub pull: f32,
}

impl<Id> Courting<Id> {
    /// Close enough that shift wires it.
    pub fn close(&self) -> bool {
        self.pull >= MAGNET_CLOSE
    }
}

/// The pairs courting this frame, those with an end on node `active`. Each
/// free input courts its nearest output that it may legally take; an output
/// courts only one input (the nearest), and never a second port of a node
/// it already feeds.
pub fn courting<'a, G: Graph>(g: &G, ports: impl Iterator<Item = (&'a PortKey<G::Id>, Pos2)>, active: &G::Id) -> Vec<Courting<G::Id>>
where
    G::Id: 'a,
{
    let ports: Vec<(&PortKey<G::Id>, Pos2)> = ports.collect();
    let edges = g.edges();
    let wired: HashSet<(&G::Id, &str)> = edges.iter().map(|e| (&e.to, e.to_port.as_str())).collect();
    let feeds = |from: &PortKey<G::Id>, to: &G::Id| edges.iter().any(|e| e.from == from.node && e.from_port == from.port && &e.to == to);
    let mut pairs: Vec<Courting<G::Id>> = Vec::new();
    for &(input, b) in ports.iter().filter(|(k, _)| !k.output && !wired.contains(&(&k.node, k.port.as_str()))) {
        let best = ports
            .iter()
            .filter(|(k, _)| k.output && k.node != input.node && (&k.node == active || &input.node == active))
            .filter(|(k, a)| a.distance(b) < MAGNET_REACH && !feeds(k, &input.node))
            .filter(|(k, _)| g.can_connect(&k.node, &k.port, &input.node, &input.port))
            .min_by(|x, y| x.1.distance(b).total_cmp(&y.1.distance(b)));
        if let Some(&(output, a)) = best {
            let pull = 1.0 - a.distance(b) / MAGNET_REACH;
            pairs.push(Courting { from: output.clone(), to: input.clone(), a, b, pull });
        }
    }
    // One suitor per output: the nearest input wins it.
    pairs.sort_by(|x, y| y.pull.total_cmp(&x.pull));
    let mut taken = HashSet::new();
    pairs.retain(|c| taken.insert(c.from.clone()));
    pairs
}

/// Shift: wire every close pair. True when the graph changed.
pub fn wire<G: Graph>(g: &mut G, pairs: &[Courting<G::Id>]) -> bool {
    let mut changed = false;
    for c in pairs.iter().filter(|c| c.close()) {
        changed |= g.connect(&c.from.node, &c.from.port, &c.to.node, &c.to.port);
    }
    changed
}

/// The dashed line between a courting pair: faint at the edge of reach,
/// bright (and thicker) once shift would wire it.
pub fn tether<Id>(c: &Courting<Id>, hot: Color32) -> Vec<Shape> {
    const DASHES: usize = 7;
    let alpha = if c.close() { 1.0 } else { 0.2 + 0.4 * c.pull };
    let color = Color32::from(hot.gamma_multiply(alpha));
    let stroke = Stroke::new(if c.close() { 2.5 } else { 1.5 }, color);
    (0..DASHES)
        .step_by(2)
        .map(|k| {
            let t0 = k as f32 / DASHES as f32;
            let t1 = (k + 1) as f32 / DASHES as f32;
            Shape::line_segment([c.a.lerp(c.b, t0), c.a.lerp(c.b, t1)], stroke)
        })
        .collect()
}
