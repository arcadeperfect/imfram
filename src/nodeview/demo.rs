//! A small graph for the playground and the tests: two kinds of value
//! (scalar, image), a handful of node types with sliders in their
//! bodies, corner dots. Nothing is evaluated.

use std::collections::BTreeMap;

use egui::{Color32, Pos2, Ui};

use super::graph::{Body, Edge, Entry, Graph, Port, WireStyle};
use super::style::{DOT_R, NODE_W};

pub const SCALAR: u8 = 0;
pub const IMAGE: u8 = 1;

#[derive(Clone, Debug)]
pub struct Node {
    pub ty: String,
    pub pos: Pos2,
    pub width: f32,
    pub params: Vec<(String, f32)>,
}

#[derive(Clone, Debug, Default)]
pub struct Demo {
    pub nodes: BTreeMap<u64, Node>,
    pub edges: Vec<Edge<u64>>,
    next: u64,
}

/// What each type is: title, menu, inputs, outputs, params.
fn def(ty: &str) -> Option<(&'static str, &'static str, &'static [(&'static str, u8)], &'static [(&'static str, u8)], &'static [&'static str])> {
    Some(match ty {
        "noise" => ("Noise", "Inputs", &[], &[("out", SCALAR)], &["frequency", "amplitude"]),
        "const" => ("Constant", "Inputs", &[], &[("out", SCALAR)], &["value"]),
        "image" => ("Image", "Inputs", &[], &[("out", IMAGE)], &[]),
        "abs" => ("Abs", "Maths", &[("in", SCALAR)], &[("out", SCALAR)], &[]),
        "mix" => ("Mix", "Maths", &[("a", SCALAR), ("b", SCALAR), ("t", SCALAR)], &[("out", SCALAR)], &["t"]),
        "curve" => ("Curve", "Shaping", &[("in", SCALAR)], &[("out", SCALAR)], &["gain", "bias"]),
        "blur" => ("Blur", "Image", &[("tex", IMAGE), ("amount", SCALAR)], &[("out", IMAGE)], &["radius"]),
        "output" => ("Output", "", &[("in", SCALAR)], &[], &[]),
        "dot" => ("", "", &[("in", SCALAR)], &[("out", SCALAR)], &[]),
        _ => return None,
    })
}

impl Demo {
    pub fn add_node(&mut self, ty: &str, pos: Pos2) -> u64 {
        let params = def(ty).map_or(&[][..], |d| d.4).iter().map(|p| (p.to_string(), 0.5)).collect();
        let id = self.next;
        self.next += 1;
        self.nodes.insert(id, Node { ty: ty.into(), pos, width: NODE_W, params });
        id
    }

    /// A noise and a constant into a mix into the output, with a curve
    /// on the way, and an image blurred by the noise.
    pub fn sample() -> Self {
        let mut g = Self::default();
        let n = g.add_node("noise", Pos2::new(0.0, 0.0));
        let c = g.add_node("const", Pos2::new(260.0, 0.0));
        let m = g.add_node("mix", Pos2::new(130.0, 160.0));
        let cv = g.add_node("curve", Pos2::new(130.0, 320.0));
        let o = g.add_node("output", Pos2::new(130.0, 480.0));
        let im = g.add_node("image", Pos2::new(520.0, 0.0));
        let b = g.add_node("blur", Pos2::new(520.0, 200.0));
        g.connect(&n, "out", &m, "a");
        g.connect(&c, "out", &m, "b");
        g.connect(&m, "out", &cv, "in");
        g.connect(&cv, "out", &o, "in");
        g.connect(&im, "out", &b, "tex");
        g.connect(&n, "out", &b, "amount");
        g
    }
}

fn ports(list: &[(&str, u8)]) -> Vec<Port> {
    list.iter().map(|(id, kind)| Port { id: id.to_string(), label: if list.len() > 1 { id.to_string() } else { String::new() }, kind: *kind }).collect()
}

impl Graph for Demo {
    type Id = u64;

    fn nodes(&self) -> Vec<u64> {
        self.nodes.keys().copied().collect()
    }
    fn edges(&self) -> Vec<Edge<u64>> {
        self.edges.clone()
    }
    fn pos(&self, id: &u64) -> Pos2 {
        self.nodes[id].pos
    }
    fn set_pos(&mut self, id: &u64, pos: Pos2) {
        if let Some(n) = self.nodes.get_mut(id) {
            n.pos = pos;
        }
    }
    fn width(&self, id: &u64) -> f32 {
        if self.is_dot(id) { 2.0 * DOT_R } else { self.nodes[id].width }
    }
    fn set_width(&mut self, id: &u64, width: f32) {
        if let Some(n) = self.nodes.get_mut(id) {
            n.width = width;
        }
    }
    fn title(&self, id: &u64) -> String {
        def(&self.nodes[id].ty).map_or(self.nodes[id].ty.clone(), |d| d.0.to_string())
    }
    fn about(&self, id: &u64) -> String {
        format!("a {} node (nothing behind it)", self.title(id).to_lowercase())
    }
    fn inputs(&self, id: &u64) -> Vec<Port> {
        def(&self.nodes[id].ty).map_or(vec![], |d| ports(d.2))
    }
    fn outputs(&self, id: &u64) -> Vec<Port> {
        def(&self.nodes[id].ty).map_or(vec![], |d| ports(d.3))
    }
    fn is_dot(&self, id: &u64) -> bool {
        self.nodes.get(id).is_some_and(|n| n.ty == "dot")
    }
    fn add_dot(&mut self, pos: Pos2) -> Option<u64> {
        Some(self.add_node("dot", pos))
    }
    fn is_sink(&self, id: &u64) -> bool {
        self.nodes[id].ty == "output"
    }
    fn body(&mut self, ui: &mut Ui, id: &u64, _width: f32) -> Body {
        let mut body = Body::default();
        let Some(n) = self.nodes.get_mut(id) else { return body };
        for (name, v) in &mut n.params {
            body.changed |= crate::widgets::slider(ui, v, (0.0, 1.0), name, false, "");
        }
        body
    }
    fn wire_style(&self, from: &u64, from_port: &str) -> WireStyle {
        // A dot carries what comes into it.
        if self.is_dot(from) {
            if let Some(e) = self.edges.iter().find(|e| &e.to == from) {
                return self.wire_style(&e.from, &e.from_port);
            }
        }
        let kind = self.outputs(from).into_iter().find(|p| p.id == from_port).map_or(SCALAR, |p| p.kind);
        match kind {
            IMAGE => WireStyle { color: Color32::from_rgb(110, 200, 165), width: 4.0, label: Some("image".into()) },
            _ => WireStyle { color: Color32::from_rgb(225, 145, 110), width: 2.0, label: Some("-1..1".into()) },
        }
    }
    fn can_connect(&self, from: &u64, from_port: &str, to: &u64, to_port: &str) -> bool {
        if from == to || super::graph::would_cycle(self, from, to) {
            return false;
        }
        // A dot takes anything and gives what it took.
        let a = if self.is_dot(from) {
            self.edges.iter().find(|e| &e.to == from).map(|e| self.outputs(&e.from).into_iter().find(|p| p.id == e.from_port).map_or(SCALAR, |p| p.kind))
        } else {
            self.outputs(from).into_iter().find(|p| p.id == from_port).map(|p| p.kind)
        };
        if self.is_dot(to) {
            return true;
        }
        let b = self.inputs(to).into_iter().find(|p| p.id == to_port).map(|p| p.kind);
        b.is_some() && (a.is_none() || a == b)
    }
    fn connect(&mut self, from: &u64, from_port: &str, to: &u64, to_port: &str) -> bool {
        if !self.can_connect(from, from_port, to, to_port) {
            return false;
        }
        self.edges.retain(|e| !(&e.to == to && e.to_port == to_port));
        self.edges.push(Edge { from: *from, from_port: from_port.into(), to: *to, to_port: to_port.into() });
        true
    }
    fn disconnect(&mut self, to: &u64, to_port: &str) -> Option<Edge<u64>> {
        let i = self.edges.iter().position(|e| &e.to == to && e.to_port == to_port)?;
        Some(self.edges.remove(i))
    }
    fn remove(&mut self, id: &u64) {
        let feed = self.edges.iter().find(|e| &e.to == id).filter(|_| self.is_dot(id)).cloned();
        self.nodes.remove(id);
        self.edges.retain(|e| &e.to != id);
        for e in self.edges.iter_mut().filter(|e| &e.from == id) {
            if let Some(f) = &feed {
                e.from = f.from;
                e.from_port = f.from_port.clone();
            }
        }
        self.edges.retain(|e| &e.from != id);
    }
    fn catalog(&self) -> Vec<Entry> {
        ["noise", "const", "image", "abs", "mix", "curve", "blur"]
            .iter()
            .map(|ty| {
                let d = def(ty).unwrap();
                Entry { ty: ty.to_string(), title: d.0.into(), menu: d.1.into() }
            })
            .collect()
    }
    fn add(&mut self, ty: &str, pos: Pos2) -> Option<u64> {
        def(ty)?;
        Some(self.add_node(ty, pos))
    }
}

#[cfg(test)]
mod tests {
    use super::super::{magnet, splice, tidy};
    use super::*;
    use egui::{pos2, vec2};

    fn by_ty(g: &Demo, ty: &str) -> u64 {
        *g.nodes.iter().find(|(_, n)| n.ty == ty).unwrap().0
    }

    #[test]
    fn a_tidy_flows_down_overlaps_nothing_and_stays_where_it_stood() {
        let mut g = Demo::sample();
        let size = |g: &Demo, id: &u64| vec2(g.width(id), 80.0);
        let centre = |g: &Demo| g.nodes().iter().map(|id| egui::Rect::from_min_size(g.pos(id), size(g, id))).reduce(|a, b| a.union(b)).unwrap().center();
        let before = centre(&g);
        assert!(tidy::tidy(&mut g, &size));
        let rect = |id: u64| egui::Rect::from_min_size(g.pos(&id), size(&g, &id));
        let (n, m, cv, o) = (by_ty(&g, "noise"), by_ty(&g, "mix"), by_ty(&g, "curve"), by_ty(&g, "output"));
        assert!(rect(n).bottom() < rect(m).top() && rect(m).bottom() < rect(cv).top() && rect(cv).bottom() < rect(o).top());
        let ids = g.nodes();
        for (k, a) in ids.iter().enumerate() {
            for b in &ids[k + 1..] {
                assert!(!rect(*a).intersects(rect(*b)), "{a} and {b} overlap");
            }
        }
        assert!((centre(&g) - before).length() < 1.0);
    }

    #[test]
    fn a_free_input_near_an_output_courts_it_and_shift_wires_it() {
        let mut g = Demo::sample();
        let abs = g.add_node("abs", pos2(0.0, 0.0));
        let n = by_ty(&g, "noise");
        let key = |node, port: &str, output| super::super::PortKey { node, port: port.into(), output };
        let ports = [(key(n, "out", true), pos2(0.0, 100.0)), (key(abs, "in", false), pos2(5.0, 120.0))];
        let pairs = magnet::courting(&g, ports.iter().map(|(k, p)| (k, *p)), &abs);
        assert_eq!(pairs.len(), 1);
        assert!(pairs[0].close());
        assert!(magnet::wire(&mut g, &pairs));
        assert!(g.edges.iter().any(|e| e.from == n && e.to == abs));
        let im = by_ty(&g, "image");
        let far = [(key(im, "out", true), pos2(0.0, 0.0)), (key(abs, "in", false), pos2(0.0, 10.0))];
        assert!(magnet::courting(&g, far.iter().map(|(k, p)| (k, *p)), &abs).is_empty(), "an image does not fit a scalar input");
    }

    #[test]
    fn a_loose_node_dropped_on_a_wire_joins_it_and_a_dot_joins_back_up_when_removed() {
        let mut g = Demo::sample();
        let abs = g.add_node("abs", pos2(0.0, 0.0));
        let (cv, o) = (by_ty(&g, "curve"), by_ty(&g, "output"));
        let e = g.edges.iter().find(|e| e.to == o).cloned().unwrap();
        assert!(splice::splice(&mut g, &abs, &e));
        assert!(g.edges.iter().any(|e| e.from == cv && e.to == abs));
        assert!(g.edges.iter().any(|e| e.from == abs && e.to == o));
        let e = g.edges.iter().find(|e| e.to == o).cloned().unwrap();
        let dot = g.add_dot(pos2(0.0, 0.0)).unwrap();
        assert!(splice::splice(&mut g, &dot, &e));
        assert_eq!(g.wire_style(&dot, "out").label.as_deref(), Some("-1..1"), "a dot carries what comes in");
        g.remove(&dot);
        assert!(g.edges.iter().any(|e| e.from == abs && e.to == o), "its wire joined back up");
    }
}
