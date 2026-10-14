//! Playground tab for `imfram::toast` and `imfram::statusbar`: buttons that
//! raise each kind of toast, a sticky one, a fake job that advances a
//! progress toast and the status bar's strip, a flash on the bar, and the
//! drained log. `overlay` runs after every window's dock and draws the
//! toasts and the bar.

use egui::Ui;
use imfram::statusbar::StatusBar;
use imfram::toast::{Kind, Toast, Toasts};

/// How far the fake job gets per frame.
const JOB_STEP: f32 = 0.004;
/// Log lines kept in the tab.
const LOG_KEEP: usize = 12;

pub struct State {
    pub toasts: Toasts,
    pub bar: StatusBar,
    /// The fake job's fraction while it runs.
    job: Option<f32>,
    /// Toasts raised so far, to number them.
    n: u32,
    frames: u64,
    log: Vec<(Kind, String)>,
}

impl Default for State {
    fn default() -> Self {
        let mut bar = StatusBar::new();
        bar.set("mode", "playground");
        bar.set_right("frame", "0");
        Self { toasts: Toasts::new(), bar, job: None, n: 0, frames: 0, log: Vec::new() }
    }
}

/// The tab: buttons that raise toasts, a status bar demo.
pub fn ui(ui: &mut Ui, s: &mut State) {
    ui.heading("toasts");
    ui.horizontal_wrapped(|ui| {
        for (kind, name) in [(Kind::Info, "info"), (Kind::Success, "success"), (Kind::Warn, "warn"), (Kind::Error, "error")] {
            if ui.button(name).clicked() {
                s.n += 1;
                s.toasts.push(kind, format!("{name} #{}: something happened", s.n));
            }
        }
        if ui.button("sticky").clicked() {
            s.n += 1;
            s.toasts.add(Toast::sticky(Kind::Warn, format!("sticky #{}: click me to dismiss", s.n)));
        }
        if ui.button("long one").clicked() {
            s.n += 1;
            s.toasts.push(
                Kind::Info,
                format!("#{}: a longer message that wraps onto a second line, to see the frame grow with its text", s.n),
            );
        }
    });
    ui.horizontal(|ui| {
        let running = s.job.is_some();
        if ui.add_enabled(!running, egui::Button::new("start fake job")).clicked() {
            s.job = Some(0.0);
        }
        if ui.add_enabled(running, egui::Button::new("cancel")).clicked() {
            s.job = None;
            s.bar.progress = None;
            s.toasts.progress("job", 1.0, "job cancelled");
        }
        if let Some(f) = s.job {
            ui.weak(format!("{:.0}%", f * 100.0));
        }
    });
    if ui.button("dismiss all").clicked() {
        s.toasts.dismiss_all();
    }
    ui.weak("hover a toast to pause its clock; click one to dismiss it");

    ui.add_space(8.0);
    ui.heading("status bar");
    ui.horizontal_wrapped(|ui| {
        if ui.button("flash 'saved'").clicked() {
            s.bar.flash("saved", 2.0);
        }
        if ui.button("set selection").clicked() {
            s.n += 1;
            s.bar.set("sel", format!("{} selected", s.n));
        }
        if ui.button("stale selection").clicked() {
            s.bar.stale("sel");
        }
        if ui.button("clear selection").clicked() {
            s.bar.remove("sel");
        }
    });

    ui.add_space(8.0);
    ui.heading("log");
    if s.log.is_empty() {
        ui.weak("nothing yet: raise a toast");
    }
    for (kind, text) in s.log.iter().rev() {
        ui.horizontal(|ui| {
            let c = kind.color(ui.visuals());
            ui.colored_label(c, format!("{kind:?}").to_lowercase());
            ui.label(text);
        });
    }
}

/// Drawn over every window's frame after its dock (toasts, the status bar).
pub fn overlay(ui: &mut Ui, s: &mut State) {
    s.frames += 1;
    s.bar.set_right("frame", format!("{:>6}", s.frames));
    if let Some(f) = &mut s.job {
        *f += JOB_STEP;
        let f = *f;
        if f >= 1.0 {
            s.job = None;
            s.bar.progress = None;
            s.toasts.progress("job", 1.0, "fake job done");
        } else {
            s.bar.progress = Some((f, "fake job".to_string()));
            s.toasts.progress("job", f, format!("fake job running: step {}", (f * 250.0) as u32));
            ui.ctx().request_repaint();
        }
    }
    for e in s.toasts.drain_log() {
        if s.log.len() >= LOG_KEEP {
            s.log.remove(0);
        }
        s.log.push(e);
    }
    s.bar.show(ui);
    s.toasts.show(ui.ctx());
}
