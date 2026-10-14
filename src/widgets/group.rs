//! A group of controls: a header that folds its controls away, with a
//! switch on it when the group has one. (The parameters window's tabs are
//! made of these, and so is the editor's objects panel.)

use egui::collapsing_header::CollapsingState;
use egui::Ui;

/// Room kept beside a slider's rail for its value box and the longest
/// label ("smoothing over time").
const BESIDE_SLIDER: f32 = 190.0;

/// Size sliders to the room there is here. (Call again wherever the room
/// changes: a group's controls are indented.)
pub fn fit_sliders(ui: &mut Ui) {
    ui.spacing_mut().slider_width = (ui.available_width() - BESIDE_SLIDER).max(120.0);
}

/// Whether a group starts unfolded.
#[derive(Clone, Copy, PartialEq)]
pub enum Start {
    Open,
    Folded,
}

fn folding(ui: &mut Ui, name: &str, start: Start, header: impl FnOnce(&mut Ui), body: impl FnOnce(&mut Ui)) {
    let id = ui.make_persistent_id(("group", name));
    // (`GRAAV_UNFOLD=1` starts every group unfolded, to capture what is in them.)
    let open = start == Start::Open || std::env::var_os("GRAAV_UNFOLD").is_some();
    CollapsingState::load_with_default_open(ui.ctx(), id, open).show_header(ui, header).body(|ui| {
        fit_sliders(ui);
        body(ui);
    });
}

/// A named group. `hint`: its hover text ("" for none).
pub fn group(ui: &mut Ui, name: &str, hint: &str, start: Start, body: impl FnOnce(&mut Ui)) {
    let header = |ui: &mut Ui| {
        let label = ui.strong(name);
        if !hint.is_empty() {
            label.on_hover_text(hint);
        }
    };
    folding(ui, name, start, header, body);
}

/// A group with a switch on its header: off, its controls are greyed.
pub fn switched(ui: &mut Ui, on: &mut bool, name: &str, hint: &str, start: Start, body: impl FnOnce(&mut Ui)) {
    let mut now = *on;
    let header = |ui: &mut Ui| {
        let switch = ui.checkbox(&mut now, egui::RichText::new(name).strong());
        if !hint.is_empty() {
            switch.on_hover_text(hint);
        }
    };
    let was = *on;
    folding(ui, name, start, header, |ui| ui.add_enabled_ui(was, body).inner);
    *on = now;
}
