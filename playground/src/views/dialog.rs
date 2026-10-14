//! Playground tab for `imfram::dialog`: buttons raising a Confirm, a
//! Progress and a Prompt, and a settings frame with three toy pages.

use egui::Ui;
use imfram::dialog::{Confirm, Progress, ProgressEvent, Prompt, SettingsFrame};

const PAGES: [&str; 3] = ["General", "Colour management", "Key map"];
/// Progress per frame of the toy job.
const JOB_STEP: f32 = 0.004;

pub struct State {
    confirm: bool,
    progress: Option<Progress>,
    prompt: Option<Prompt>,
    outcome: String,
    settings: SettingsFrame,
    // The toy pages' values.
    autosave: bool,
    scale: f32,
    working_space: usize,
    swap_cmd_ctrl: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            confirm: false,
            progress: None,
            prompt: None,
            outcome: String::new(),
            settings: SettingsFrame::default(),
            autosave: true,
            scale: 1.0,
            working_space: 0,
            swap_cmd_ctrl: false,
        }
    }
}

pub fn ui(ui: &mut Ui, s: &mut State) {
    ui.horizontal(|ui| {
        if ui.button("Confirm…").clicked() {
            s.confirm = true;
        }
        if ui.button("Progress…").clicked() {
            s.progress = Some(Progress { fraction: 0.0, label: "Baking the shot…".into(), cancellable: true });
        }
        if ui.button("Prompt…").clicked() {
            s.prompt = Some(Prompt::new("Rename shot").text("shot_010").hint("a name"));
        }
        if s.outcome.is_empty() {
            ui.weak("Enter answers, Esc backs out");
        } else {
            ui.label(&s.outcome);
        }
    });
    ui.add_space(8.0);

    if s.confirm {
        if let Some(yes) = Confirm::new("Delete 3 nodes?", "They go for good; undo brings them back.").yes("delete").no("keep").show(ui.ctx()) {
            s.outcome = if yes { "deleted".into() } else { "kept".into() };
            s.confirm = false;
        }
    }
    if let Some(p) = &mut s.progress {
        p.fraction += JOB_STEP;
        ui.ctx().request_repaint();
        let ev = p.show(ui.ctx());
        if ev == ProgressEvent::Cancel {
            s.outcome = "job cancelled".into();
            s.progress = None;
        } else if p.fraction >= 1.0 {
            s.outcome = "job done".into();
            s.progress = None;
        }
    }
    if let Some(p) = &mut s.prompt {
        if let Some(answer) = p.show(ui.ctx()) {
            s.outcome = match answer {
                Some(name) => format!("renamed to {name}"),
                None => "rename cancelled".into(),
            };
            s.prompt = None;
        }
    }

    ui.separator();
    ui.strong("Settings frame");
    let State { settings, autosave, scale, working_space, swap_cmd_ctrl, .. } = s;
    settings.show(ui, &PAGES, |ui, page| match page {
        0 => {
            ui.checkbox(autosave, "Autosave every two minutes");
            ui.add(egui::Slider::new(scale, 0.5..=2.0).text("UI scale"));
        }
        1 => {
            ui.label("Working space");
            for (i, name) in ["ACEScg", "Linear sRGB", "Linear Rec.2020"].iter().enumerate() {
                ui.radio_value(working_space, i, *name);
            }
        }
        _ => {
            ui.checkbox(swap_cmd_ctrl, "Swap ⌘ and Ctrl");
            ui.weak("The real key map editor lives on the Keymap tab.");
        }
    });
}
