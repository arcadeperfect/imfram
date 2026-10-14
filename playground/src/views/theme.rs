//! Playground tab for `imfram::theme`: the colours window on the crate's
//! core table, a strip of every interface slot as it is painted now, and
//! the overrides as prefs would store them.

use egui::Ui;
use imfram::theme::{self, ThemeUi, DEFS, TABLE};

#[derive(Default)]
pub struct State {
    window: ThemeUi,
    /// What the last `window()` asked us to save: the prefs mirror.
    saved: Option<String>,
}

const SWATCH: f32 = 28.0;

pub fn ui(ui: &mut Ui, s: &mut State) {
    // egui's own widgets follow the table; an app calls this before every frame.
    theme::sync(ui.ctx());

    ui.horizontal(|ui| {
        if ui.button("colours…").clicked() {
            s.window.open = true;
        }
        if ui
            .add_enabled(theme::any_overridden(), egui::Button::new("reset all"))
            .clicked()
        {
            theme::reset_all();
        }
        ui.weak(format!("{} slots in the core table", theme::N));
    });
    if s.window.window(ui.ctx(), &TABLE, false) {
        s.saved = Some(prefs_text());
    }

    ui.add_space(8.0);
    ui.label("interface");
    swatch_strip(ui, "interface");
    ui.add_space(4.0);
    ui.label("status · canvases");
    ui.horizontal_wrapped(|ui| {
        swatches(ui, "status");
        ui.separator();
        swatches(ui, "canvases");
    });

    ui.add_space(8.0);
    ui.label("overrides, as prefs store them");
    let text = prefs_text();
    egui::Frame::new()
        .fill(theme::field())
        .inner_margin(6.0)
        .corner_radius(3.0)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.monospace(if text.is_empty() { "{}" } else { &text });
        });
    if let Some(saved) = &s.saved {
        ui.weak(format!("the window last asked to save {} override(s)", saved.lines().count().saturating_sub(2)));
    }
}

fn swatch_strip(ui: &mut Ui, group: &str) {
    ui.horizontal_wrapped(|ui| swatches(ui, group));
}

fn swatches(ui: &mut Ui, group: &str) {
    ui.spacing_mut().item_spacing.x = 2.0;
    for (i, d) in DEFS.iter().enumerate().filter(|(_, d)| d.group == group) {
        let c = TABLE.get(i);
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(SWATCH, SWATCH), egui::Sense::hover());
        let p = ui.painter();
        if !c.is_opaque() {
            imfram::colorpicker::paint_checker(p, rect, 6.0);
        }
        p.rect_filled(rect, 2.0, c);
        let rim = if theme::is_overridden(theme::ALL[i]) { theme::highlight() } else { theme::border() };
        p.rect_stroke(rect, 2.0, egui::Stroke::new(1.0, rim), egui::StrokeKind::Inside);
        resp.on_hover_text(format!("{}  {}", d.label, theme::hex(c)));
    }
}

/// The overrides as the prefs file would hold them: one `"id": "#hex"` a line.
fn prefs_text() -> String {
    let o = theme::overrides();
    if o.is_empty() {
        return String::new();
    }
    let mut s = String::from("{\n");
    for (k, v) in &o {
        s.push_str(&format!("  {k:?}: {v:?},\n"));
    }
    s.push('}');
    s
}
