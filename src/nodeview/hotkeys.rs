//! The node canvas' hot keys and mouse chords, as one list, the box that
//! shows them, and the caps for the key strip along the canvas' top.

use egui::{Align2, Rect, Ui};

use crate::widgets::keystrip::Cap;

/// One row: the keys as shown, and what they do.
pub struct Hotkey {
    pub keys: &'static str,
    pub does: &'static str,
}

const fn row(keys: &'static str, does: &'static str) -> Hotkey {
    Hotkey { keys, does }
}

pub const HOTKEYS: &[Hotkey] = &[
    row("Tab / right-click", "add a node: browse the menus, or type to search"),
    row("drag output to input", "wire them (an input holds one wire)"),
    row("drag a wired input", "take its wire off (right-click: just remove it)"),
    row("G", "pick up a wire at the pointer; click a port to land it, G / Esc to drop it"),
    row("ctrl drag", "cut the wires the stroke crosses"),
    row("shift", "wire the ports courting (drag a node near another: a dashed line)"),
    row("shift-click a wire", "put a corner dot on it; drag the dot to bend the wire"),
    row("right-click a dot", "take it out, joining its wire back up"),
    row("drop a node on a wire", "put it in the middle of that wire"),
    row("hold E, move right / left", "widen / narrow the node under the pointer"),
    row("T", "tidy the graph"),
    row("hover a title or a port", "what it does, what it carries"),
    row("wheel / pinch", "zoom"),
    row("two-finger scroll", "pan"),
    row("middle / alt left-drag", "pan"),
    row("alt right-drag, ctrl middle-drag", "zoom"),
];

/// Inset from the canvas' corner.
const INSET: f32 = 8.0;

/// The list, in a box in the top right of `canvas` (window coordinates),
/// scrolling when the canvas is short. `extra`: the app's own rows.
pub fn show(ui: &Ui, canvas: Rect, extra: &[Hotkey]) {
    let room = (canvas.height() - 2.0 * INSET).max(80.0);
    egui::Area::new(egui::Id::new("graph-hotkeys")).order(egui::Order::Foreground).pivot(Align2::RIGHT_TOP).fixed_pos(canvas.right_top() + egui::vec2(-INSET, INSET)).show(ui.ctx(), |ui| {
        ui.set_max_height(room);
        egui::Frame::popup(ui.style()).show(ui, |ui| {
            ui.strong("Node graph");
            egui::ScrollArea::vertical().max_height(room - 3.0 * ui.spacing().interact_size.y).show(ui, |ui| {
                egui::Grid::new("graph-hotkeys-grid").num_columns(2).striped(true).show(ui, |ui| {
                    for key in HOTKEYS.iter().chain(extra) {
                        ui.label(egui::RichText::new(key.keys).monospace().small());
                        ui.label(egui::RichText::new(key.does).small());
                        ui.end_row();
                    }
                });
            });
        });
    });
}

/// The canvas' keys for the key strip, lit as held now.
pub fn caps(ui: &Ui) -> Vec<Cap> {
    let (shift, ctrl, e) = ui.input(|i| (i.modifiers.shift, i.modifiers.ctrl, i.key_down(egui::Key::E)));
    let cap = |cap, does, held| Cap { cap, does, held };
    vec![
        cap("Tab", "add a node", false),
        cap("G", "grab a wire", false),
        cap("T", "tidy", false),
        cap("E", "hold and move: widen / narrow", e),
        cap("shift", "wire what courts", shift),
        cap("shift-click", "corner dot on a wire", false),
        cap("ctrl-drag", "cut wires", ctrl),
    ]
}
