//! Playground tab for `imfram::toast` and `imfram::statusbar` (to be written).

use egui::Ui;

#[derive(Default)]
pub struct State {}

/// The tab: buttons that raise toasts, a status bar demo.
pub fn ui(ui: &mut Ui, _s: &mut State) {
    ui.weak("toast / statusbar: to be written");
}

/// Drawn over every window's frame after its dock (toasts, the status bar).
pub fn overlay(_ui: &mut Ui, _s: &mut State) {}
