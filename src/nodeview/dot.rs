//! Drawing a corner dot: a small disc in its wire's colour. Both its ports
//! sit at its centre, so its wires meet there. Drag it to move it, G over
//! it to pull a new wire out of it, right-click it to take it out (its
//! wire joins back up).

use egui::{vec2, Color32, PointerButton, Rect, Sense, Stroke, Ui};

use super::graph::{Body, Graph};
use super::node::{NodeOut, PortKey};
use super::style::{DOT_HIT, DOT_R};

/// Draw dot `id` in `color` (what its wire carries). `wire_in_hand`: a
/// wire could land on it.
pub fn show<G: Graph>(ui: &mut Ui, g: &mut G, id: &G::Id, color: Color32, wire_in_hand: bool, hold_still: bool, selected: bool) -> NodeOut<G::Id> {
    let uid = ui.id().with(("dot", id));
    let centre = g.pos(id) + vec2(DOT_R, DOT_R);
    let resp = ui.interact(Rect::from_center_size(centre, vec2(DOT_HIT, DOT_HIT) * 2.0), uid, Sense::click_and_drag());
    let mut moved = false;
    let mut drag_delta = egui::Vec2::ZERO;
    if resp.dragged_by(PointerButton::Primary) && !hold_still {
        drag_delta = resp.drag_delta();
        g.set_pos(id, g.pos(id) + drag_delta);
        moved = true;
    }
    let centre = g.pos(id) + vec2(DOT_R, DOT_R);
    let lit = resp.hovered() || (wire_in_hand && resp.contains_pointer());
    let hot = ui.visuals().selection.stroke.color;
    let ring = if selected { Stroke::new(2.0, hot) } else { Stroke::new(1.5, ui.visuals().extreme_bg_color) };
    ui.painter().circle(centre, DOT_R, if lit { hot } else { color }, ring);
    let port = |port: &str, output| PortKey { node: id.clone(), port: port.to_string(), output };
    // (The dot itself answers clicks and drags: its ports take none.)
    let quiet = |p: &str| ui.interact(Rect::NOTHING, uid.with(p), Sense::hover());
    NodeOut {
        rect: Rect::from_center_size(centre, vec2(DOT_R, DOT_R) * 2.0),
        body: Body::default(),
        moved,
        delete: resp.secondary_clicked(),
        clicked: resp.clicked(),
        dragging: moved,
        drag_delta,
        dropped: resp.drag_stopped_by(PointerButton::Primary),
        ports: vec![(port("in", false), centre, quiet("in")), (port("out", true), centre, quiet("out"))],
    }
}
