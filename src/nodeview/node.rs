//! Drawing one node: its frame, title, the app's body, and its ports.
//! Inputs sit on the top edge, outputs on the bottom edge.

use std::collections::HashSet;

use egui::{pos2, vec2, Align2, FontId, Frame, Margin, PointerButton, Pos2, Rect, Sense, Stroke, Ui, UiBuilder};

use super::graph::{Body, Graph, Port};
use super::style::{PAD, PORT_HIT, PORT_LABEL_H, PORT_R, ROW_H};

/// One port of one node.
#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub struct PortKey<Id> {
    pub node: Id,
    pub port: String,
    pub output: bool,
}

/// What drawing a node produced.
pub struct NodeOut<Id> {
    pub rect: Rect,
    pub body: Body,
    /// Its position changed (a drag).
    pub moved: bool,
    /// Its delete button was clicked.
    pub delete: bool,
    /// It is being dragged by its body; and the frame that drag ends.
    pub dragging: bool,
    pub dropped: bool,
    /// Each port: key, centre (canvas space) and this frame's interaction.
    pub ports: Vec<(PortKey<Id>, Pos2, egui::Response)>,
}

/// How a node is drawn this frame, beyond the node itself.
pub struct Look<'a, Id> {
    pub wired: &'a HashSet<(Id, String)>,
    /// A wire is in hand: the input ports are drop targets.
    pub wire_in_hand: bool,
    /// E is held: nodes are being stretched, not moved.
    pub hold_still: bool,
}

/// Port labels are skipped when there is nothing to tell apart.
pub fn show_input_labels(ins: &[Port]) -> bool {
    !(ins.is_empty() || (ins.len() == 1 && ins[0].label.is_empty()))
}

pub fn show_output_labels(outs: &[Port]) -> bool {
    outs.len() > 1
}

/// Draw `id` and its body.
pub fn show<G: Graph>(ui: &mut Ui, g: &mut G, id: &G::Id, look: &Look<G::Id>) -> NodeOut<G::Id> {
    let mut out = NodeOut { rect: Rect::NOTHING, body: Body::default(), moved: false, delete: false, dragging: false, dropped: false, ports: Vec::new() };
    let origin = g.pos(id);
    let is_sink = g.is_sink(id);
    let visuals = ui.visuals().clone();
    let hot = visuals.selection.stroke.color;
    let frame = Frame::new()
        .fill(visuals.window_fill)
        .stroke(Stroke::new(1.0, if is_sink { hot } else { visuals.window_stroke.color }))
        .corner_radius(6.0)
        .inner_margin(Margin::same(PAD as i8));

    let ins = g.inputs(id);
    let outs = g.outputs(id);
    let width = g.width(id);
    let inner_w = width - 2.0 * PAD;
    // Ports are spread evenly along the top (inputs) and bottom (outputs) edges.
    let port_x = |i: usize, n: usize| origin.x + width * (i as f32 + 0.5) / n as f32;

    // The whole node is the drag handle: anywhere a control doesn't claim
    // the press moves it.
    let builder = UiBuilder::new().id_salt(("node", id)).max_rect(Rect::from_min_size(origin, vec2(width, 0.0))).sense(Sense::click_and_drag());
    let title = g.title(id);
    let about = g.about(id);
    let deletable = g.deletable(id);
    let rect = ui
        .scope_builder(builder, |ui| {
            // Labels must not swallow the drag (they are selectable by default).
            ui.style_mut().interaction.selectable_labels = false;
            let body = ui.response();
            if body.dragged_by(PointerButton::Primary) && !look.hold_still {
                g.set_pos(id, g.pos(id) + body.drag_delta());
                out.moved = true;
                out.dragging = true;
            }
            out.dropped = body.drag_stopped_by(PointerButton::Primary);
            frame
                .show(ui, |ui| {
                    ui.set_width(inner_w);
                    ui.spacing_mut().item_spacing.y = 3.0;
                    if show_input_labels(&ins) {
                        port_labels(ui, inner_w, ins.iter().enumerate().map(|(i, p)| (port_x(i, ins.len()), p.label.as_str())));
                    }
                    out.delete = header(ui, &body, inner_w, &title, &about, deletable);
                    out.body = g.body(ui, id, inner_w);
                    if show_output_labels(&outs) {
                        port_labels(ui, inner_w, outs.iter().enumerate().map(|(i, p)| (port_x(i, outs.len()), p.label.as_str())));
                    }
                })
                .response
                .rect
        })
        .inner;
    out.rect = rect;

    // Ports sit on the node's edges; added last so they win clicks there.
    let top = ins.iter().enumerate().map(|(i, p)| (p, pos2(port_x(i, ins.len()), rect.top()), false));
    let bottom = outs.iter().enumerate().map(|(i, p)| (p, pos2(port_x(i, outs.len()), rect.bottom()), true));
    for (p, center, output) in top.chain(bottom) {
        let key = PortKey { node: id.clone(), port: p.id.clone(), output };
        let pid = ui.id().with(("port", &key.node, &key.port, output));
        let resp = ui.interact(Rect::from_center_size(center, vec2(PORT_HIT, PORT_HIT)), pid, Sense::click_and_drag());
        let is_wired = !output && look.wired.contains(&(key.node.clone(), key.port.clone()));
        let lit = resp.hovered() || (look.wire_in_hand && !output && resp.contains_pointer());
        let color = g.port_color(id, &p.id, output, visuals.weak_text_color());
        let fill = if lit {
            hot
        } else if is_wired || output {
            color
        } else {
            visuals.extreme_bg_color
        };
        let width = if output { g.wire_style(id, &p.id).width } else { 2.0 };
        let r = PORT_R + (width - 2.0).max(0.0) * 0.6;
        ui.painter().circle(center, r, fill, Stroke::new(1.5, color));
        let resp = resp.on_hover_text(g.port_tip(id, p, output));
        out.ports.push((key, center, resp));
    }
    out
}

/// Title row with the delete button; `about` is the title's tooltip, shown
/// while `body` (the node's drag handle) is hovered on the title. (egui
/// puts the body over the title, so the title is never hovered itself.)
/// True when delete was clicked.
fn header(ui: &mut Ui, body: &egui::Response, width: f32, title: &str, about: &str, deletable: bool) -> bool {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(width - 26.0, ROW_H), Sense::hover());
        if !about.is_empty() {
            crate::widgets::tip::show(ui, body.id.with("about"), body.hover_pos().is_some_and(|p| r.contains(p)), about);
        }
        ui.painter().text(r.left_center(), Align2::LEFT_CENTER, title, FontId::proportional(13.0), ui.visuals().strong_text_color());
        deletable && ui.small_button("×").on_hover_text("delete node").clicked()
    })
    .inner
}

/// One row of small port names, each centred on its port's x.
fn port_labels<'a>(ui: &mut Ui, width: f32, labels: impl Iterator<Item = (f32, &'a str)>) {
    let (r, _) = ui.allocate_exact_size(vec2(width, PORT_LABEL_H), Sense::hover());
    for (x, label) in labels {
        ui.painter().text(pos2(x, r.center().y), Align2::CENTER_CENTER, label, FontId::proportional(10.5), ui.visuals().weak_text_color());
    }
}
