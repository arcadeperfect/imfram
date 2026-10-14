//! Tidy: the whole graph laid out again as a layered top-to-bottom flow,
//! wobble's way. Pure: it moves the nodes in the document; the canvas
//! glides them there (`glide`).
//!
//! | module  | what it owns                                              |
//! |---------|-----------------------------------------------------------|
//! | `rank`  | which row each node goes in                               |
//! | `order` | the order along each row that crosses fewest wires         |
//! | `place` | where along its row each goes                             |
//! | `glide` | the canvas' motion to the new places (a staggered ease)   |
//!
//! Here: corner dots are taken out first (so tidying twice does not pile
//! them up), each family of connected nodes is laid out alone and the
//! families set side by side, the whole kept where it stood; a wire that
//! spans rows bends round the rows between at new corner dots (when the
//! graph has dots).

mod glide;
mod order;
mod place;
mod rank;

use std::collections::HashMap;

use egui::{pos2, vec2, Pos2, Rect, Vec2};

pub use glide::Glide;

use super::graph::Graph;
use super::style::DOT_R;

/// Room between rows, and between families set side by side.
const ROW_GAP: f32 = 56.0;
const FAMILY_GAP: f32 = 3.0 * place::COL_GAP;
/// How wide a long wire's step is, for spacing.
const CHAN_W: f32 = 20.0;
/// How far a wire may stray from straight before it keeps a corner.
const BEND: f32 = 8.0;

/// A wire between two nodes (by their places in `ids`).
struct Wire {
    a: usize,
    b: usize,
    from_port: String,
    to_port: String,
}

/// One end of a wire, seen from the other: the vertex it reaches, where
/// along that vertex its port is (`far`), and where along this one its
/// own port is (`near`); each a share of the width from the middle,
/// -0.5..0.5 (0 for a long wire's steps).
#[derive(Clone, Copy, Debug)]
pub struct Link {
    pub n: usize,
    pub far: f32,
    pub near: f32,
}

/// A long wire's way down: from its output, through the rows it crosses,
/// to its input.
struct Way<Id> {
    from: Id,
    from_port: String,
    to: Id,
    points: Vec<Pos2>,
}

/// Lay the graph out. `size` is each node's size on the canvas. True if
/// anything moved.
pub fn tidy<G: Graph>(g: &mut G, size: &dyn Fn(&G, &G::Id) -> Vec2) -> bool {
    let before: Vec<(G::Id, Pos2)> = g.nodes().iter().map(|id| (id.clone(), g.pos(id))).collect();
    loop {
        let Some(dot) = g.nodes().into_iter().find(|id| g.is_dot(id)) else { break };
        g.remove(&dot);
    }
    let ids = g.nodes();
    let n = ids.len();
    if n == 0 {
        return false;
    }
    let sizes: Vec<Vec2> = ids.iter().map(|id| size(g, id)).collect();
    let old_centre = (0..n).map(|i| Rect::from_min_size(g.pos(&ids[i]), sizes[i])).reduce(|a, b| a.union(b)).expect("nodes").center();
    let index: HashMap<&G::Id, usize> = ids.iter().enumerate().map(|(i, id)| (id, i)).collect();
    let links: Vec<Wire> = g
        .edges()
        .iter()
        .filter_map(|e| Some(Wire { a: *index.get(&e.from)?, b: *index.get(&e.to)?, from_port: e.from_port.clone(), to_port: e.to_port.clone() }))
        .filter(|w| w.a != w.b)
        .collect();

    // Families: nodes joined by wires, biggest first.
    let mut family: Vec<usize> = (0..n).collect();
    fn root(f: &mut [usize], i: usize) -> usize {
        let mut r = i;
        while f[r] != r {
            r = f[r];
        }
        f[i] = r;
        r
    }
    for w in &links {
        let (ra, rb) = (root(&mut family, w.a), root(&mut family, w.b));
        family[ra] = rb;
    }
    let mut families: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..n {
        let r = root(&mut family, i);
        families.entry(r).or_default().push(i);
    }
    let mut families: Vec<Vec<usize>> = families.into_values().collect();
    families.sort_by(|a, b| b.len().cmp(&a.len()).then(a[0].cmp(&b[0])));

    // Each laid out alone, then set side by side, tops aligned.
    let mut centres = vec![Pos2::ZERO; n];
    let mut ways: Vec<Way<G::Id>> = Vec::new();
    let mut cursor = 0.0;
    for members in &families {
        let (placed, mut family_ways, bounds) = family_layout(g, &ids, members, &links, &sizes);
        let shift = vec2(cursor - bounds.min.x, -bounds.min.y);
        for (i, c) in placed {
            centres[i] = c + shift;
        }
        for w in &mut family_ways {
            for p in &mut w.points {
                *p += shift;
            }
        }
        ways.extend(family_ways);
        cursor += bounds.width() + FAMILY_GAP;
    }
    // The whole kept where it stood.
    let new_bounds = (0..n).map(|i| Rect::from_center_size(centres[i], sizes[i])).reduce(|a, b| a.union(b)).expect("nodes");
    let d = old_centre - new_bounds.center();
    for (i, id) in ids.iter().enumerate() {
        g.set_pos(id, centres[i] + d - 0.5 * sizes[i]);
    }
    for w in &mut ways {
        for p in &mut w.points {
            *p += d;
        }
        corners(g, w);
    }
    let after: HashMap<G::Id, Pos2> = g.nodes().iter().map(|id| (id.clone(), g.pos(id))).collect();
    before.len() != after.len() || before.iter().any(|(id, p)| after.get(id) != Some(p))
}

/// One family laid out: each member's centre, its long wires' ways, and
/// the bounds of it all.
fn family_layout<G: Graph>(g: &G, ids: &[G::Id], members: &[usize], links: &[Wire], sizes: &[Vec2]) -> (Vec<(usize, Pos2)>, Vec<Way<G::Id>>, Rect) {
    let local: HashMap<usize, usize> = members.iter().enumerate().map(|(k, &i)| (i, k)).collect();
    let m = members.len();
    let (mut parents, mut children) = (vec![Vec::new(); m], vec![Vec::new(); m]);
    for w in links {
        let (Some(&a), Some(&b)) = (local.get(&w.a), local.get(&w.b)) else { continue };
        if !parents[b].contains(&a) {
            parents[b].push(a);
            children[a].push(b);
        }
    }
    let rank = rank::ranks(m, &parents, &children);
    let rows_n = rank.iter().max().map_or(1, |r| r + 1);

    // Vertices: the members, then a stand-in for each row a long wire crosses.
    let mut verts: Vec<place::Vertex> = (0..m).map(|k| place::Vertex { width: sizes[members[k]].x, real: true }).collect();
    let mut vrank: Vec<usize> = rank.clone();
    let (mut up, mut down) = (vec![Vec::new(); m], vec![Vec::new(); m]);
    // (Long wires, one way for each output into each node.)
    let mut long: Vec<(usize, usize, String, Vec<usize>)> = Vec::new();
    for w in links {
        let (Some(&a), Some(&b)) = (local.get(&w.a), local.get(&w.b)) else { continue };
        if rank[b] <= rank[a] {
            continue; // a wire back up (a loop) is left out of the layout
        }
        let out = port_at(g, &ids[w.a], &w.from_port, true);
        let inn = port_at(g, &ids[w.b], &w.to_port, false);
        if rank[b] == rank[a] + 1 {
            link(a, b, out, inn, &mut up, &mut down);
            continue;
        }
        let port = &w.from_port;
        if long.iter().any(|l| l.0 == a && l.1 == b && l.2 == *port) {
            continue;
        }
        let mut chain = Vec::new();
        let (mut prev, mut from_at) = (a, out);
        for r in rank[a] + 1..rank[b] {
            let v = verts.len();
            verts.push(place::Vertex { width: CHAN_W, real: false });
            vrank.push(r);
            up.push(Vec::new());
            down.push(Vec::new());
            link(prev, v, from_at, 0.0, &mut up, &mut down);
            chain.push(v);
            (prev, from_at) = (v, 0.0);
        }
        link(prev, b, from_at, inn, &mut up, &mut down);
        long.push((a, b, port.clone(), chain));
    }

    // Rows, first in the order things stand now (a stand-in where its
    // wire would cross that row), then uncrossed.
    let now_x = |k: usize| g.pos(&ids[members[k]]).x + 0.5 * sizes[members[k]].x;
    let mut seed = vec![0.0f32; verts.len()];
    for k in 0..m {
        seed[k] = now_x(k);
    }
    for (a, b, _, chain) in &long {
        for (s, &v) in chain.iter().enumerate() {
            let t = (s + 1) as f32 / (chain.len() + 1) as f32;
            seed[v] = now_x(*a) + (now_x(*b) - now_x(*a)) * t;
        }
    }
    let mut rows: Vec<Vec<usize>> = vec![Vec::new(); rows_n];
    for v in 0..verts.len() {
        rows[vrank[v]].push(v);
    }
    for row in &mut rows {
        row.sort_by(|a, b| seed[*a].total_cmp(&seed[*b]).then(a.cmp(b)));
    }
    order::reduce(&mut rows, &up, &down);
    let x = place::xs(&rows, &verts, &up, &down);

    // Rows top to bottom, each as tall as its tallest node, tops aligned.
    let mut row_top = vec![0.0f32; rows_n];
    let mut row_h = vec![20.0f32; rows_n];
    for k in 0..m {
        row_h[rank[k]] = row_h[rank[k]].max(sizes[members[k]].y);
    }
    let mut y = 0.0;
    for r in 0..rows_n {
        row_top[r] = y;
        y += row_h[r] + ROW_GAP;
    }
    let placed: Vec<(usize, Pos2)> = (0..m).map(|k| (members[k], pos2(x[k], row_top[rank[k]] + 0.5 * sizes[members[k]].y))).collect();
    let mut bounds = placed.iter().map(|(i, c)| Rect::from_center_size(*c, sizes[*i])).reduce(|a, b| a.union(b)).expect("members");
    let ways = long
        .into_iter()
        .map(|(a, b, port, chain)| {
            let (ca, cb) = (placed[a].1, placed[b].1);
            let mut points = vec![pos2(ca.x, ca.y + 0.5 * sizes[members[a]].y)];
            points.extend(chain.iter().map(|&v| pos2(x[v], row_top[vrank[v]] + 0.5 * row_h[vrank[v]])));
            points.push(pos2(cb.x, cb.y - 0.5 * sizes[members[b]].y));
            for p in &points {
                bounds = bounds.union(Rect::from_center_size(*p, Vec2::splat(1.0)));
            }
            Way { from: ids[members[a]].clone(), from_port: port, to: ids[members[b]].clone(), points }
        })
        .collect();
    (placed, ways, bounds)
}

/// Join `a` to `b` in the row below it, from a port `out` along `a` to
/// one `inn` along `b` (once for each pair of ports).
fn link(a: usize, b: usize, out: f32, inn: f32, up: &mut [Vec<Link>], down: &mut [Vec<Link>]) {
    if !down[a].iter().any(|l| l.n == b && l.near == out && l.far == inn) {
        down[a].push(Link { n: b, far: inn, near: out });
        up[b].push(Link { n: a, far: out, near: inn });
    }
}

/// Where port `port` is along node `node` (an output if `output`), as a
/// share of its width from the middle.
fn port_at<G: Graph>(g: &G, node: &G::Id, port: &str, output: bool) -> f32 {
    let ports = if output { g.outputs(node) } else { g.inputs(node) };
    match ports.iter().position(|p| p.id == port) {
        Some(i) => (i as f32 + 0.5) / ports.len() as f32 - 0.5,
        None => 0.0,
    }
}

/// Bend a long wire round the rows it crosses: a corner dot at each of
/// its way's bends (straight stretches need none), and every wire from
/// that output into that node led through them. Nothing, for a graph
/// without dots.
fn corners<G: Graph>(g: &mut G, way: &Way<G::Id>) {
    let bends = simplify(&way.points, BEND);
    let inner = &bends[1..bends.len() - 1];
    if inner.is_empty() {
        return;
    }
    let ports: Vec<String> = g.edges().iter().filter(|e| e.from == way.from && e.from_port == way.from_port && e.to == way.to).map(|e| e.to_port.clone()).collect();
    let mut from = (way.from.clone(), way.from_port.clone());
    for p in inner {
        let Some(dot) = g.add_dot(*p - vec2(DOT_R, DOT_R)) else { return };
        g.connect(&from.0, &from.1, &dot, "in");
        from = (dot, "out".to_string());
    }
    for port in ports {
        g.connect(&from.0, &from.1, &way.to, &port);
    }
}

/// The points of a polyline that matter (Douglas-Peucker): its ends, and
/// every one the rest strays further than `eps` from.
fn simplify(points: &[Pos2], eps: f32) -> Vec<Pos2> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let (a, b) = (points[0], points[points.len() - 1]);
    let off = |p: Pos2| {
        let ab = b - a;
        let t = if ab.length_sq() > 0.0 { ((p - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0) } else { 0.0 };
        p.distance(a + ab * t)
    };
    let (k, far) = points[1..points.len() - 1].iter().enumerate().map(|(k, p)| (k + 1, off(*p))).fold((0, 0.0), |m, x| if x.1 > m.1 { x } else { m });
    if far <= eps {
        return vec![a, b];
    }
    let mut left = simplify(&points[..=k], eps);
    left.pop();
    left.extend(simplify(&points[k..], eps));
    left
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_straight_run_keeps_no_corners() {
        let pts = [pos2(0.0, 0.0), pos2(0.5, 50.0), pos2(0.0, 100.0)];
        assert_eq!(simplify(&pts, BEND).len(), 2);
        let bent = [pos2(0.0, 0.0), pos2(80.0, 50.0), pos2(0.0, 100.0)];
        assert_eq!(simplify(&bent, BEND).len(), 3);
    }
}
