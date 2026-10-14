//! egui follows the table: the interface slots, found by id, written into
//! a context's dark `Visuals`.

use super::engine::{shift, Table};
use egui::Color32;
use std::sync::atomic::Ordering;

impl Table {
    /// egui's dark-mode colours, driven by the interface slots (found by id,
    /// so any table that carries them — the core group or a superset —
    /// drives egui; a slot the table lacks leaves egui's own colour). Every
    /// shade egui keeps that has no slot of its own moves with the slot it
    /// belongs to, so the defaults reproduce `Visuals::dark()` exactly (bar
    /// the status colours, which are the app's own).
    pub fn write_visuals_from(&self, v: &mut egui::Visuals, c: &[Color32]) {
        let at = |id: &str| self.index(id).map(|i| c[i]);
        let follow = |own: Color32, id: &str| match self.index(id) {
            Some(i) => shift(own, self.default_of(i), c[i]),
            None => own,
        };
        let w = &mut v.widgets;

        if let Some(bg) = at("bg") {
            v.panel_fill = bg;
            w.noninteractive.bg_fill = bg;
            w.noninteractive.weak_bg_fill = bg;
            w.open.bg_fill = bg;
            v.code_bg_color = follow(Color32::from_gray(64), "bg");
        }
        if let Some(c) = at("window") {
            v.window_fill = c;
        }
        if let Some(c) = at("field") {
            v.extreme_bg_color = c;
        }
        if let Some(c) = at("shadow") {
            v.window_shadow.color = c;
            v.popup_shadow.color = c;
        }
        if let Some(c) = at("border") {
            v.window_stroke.color = c;
            w.noninteractive.bg_stroke.color = c;
            w.open.bg_stroke.color = c;
            w.hovered.bg_stroke.color = follow(Color32::from_gray(150), "border");
        }
        if let Some(c) = at("text") {
            w.noninteractive.fg_stroke.color = c;
            w.inactive.fg_stroke.color = follow(Color32::from_gray(180), "text");
            w.hovered.fg_stroke.color = follow(Color32::from_gray(240), "text");
            w.open.fg_stroke.color = follow(Color32::from_gray(210), "text");
            w.active.fg_stroke.color = follow(Color32::WHITE, "text");
            w.active.bg_stroke.color = follow(Color32::WHITE, "text");
        }
        if let Some(c) = at("button") {
            w.inactive.bg_fill = c;
            w.inactive.weak_bg_fill = c;
            w.open.weak_bg_fill = follow(Color32::from_gray(45), "button");
        }
        if let Some(c) = at("button_hover") {
            w.hovered.bg_fill = c;
            w.hovered.weak_bg_fill = c;
        }
        if let Some(c) = at("button_active") {
            w.active.bg_fill = c;
            w.active.weak_bg_fill = c;
        }
        if let Some(c) = at("accent") {
            v.selection.bg_fill = c;
        }
        if let Some(c) = at("accent_text") {
            v.selection.stroke.color = c;
            v.text_cursor.stroke.color = c;
        }
        if let Some(c) = at("link") {
            v.hyperlink_color = c;
        }
        if let Some(c) = at("warn") {
            v.warn_fg_color = c;
        }
        if let Some(c) = at("error") {
            v.error_fg_color = c;
        }
    }

    /// [`Table::write_visuals_from`] with the live colours.
    pub fn write_visuals(&self, v: &mut egui::Visuals) {
        self.write_visuals_from(v, &self.live());
    }

    /// Bring a context's visuals up to date with the theme. Cheap when
    /// nothing moved (one atomic load and a memory read), so call it before
    /// every `run_ui` — the main window and each detached pane own a context.
    pub fn sync(&self, ctx: &egui::Context) {
        let gen = self.generation.load(Ordering::Relaxed);
        let id = egui::Id::new("imfram-theme-generation");
        if ctx.data(|d| d.get_temp::<u32>(id)) == Some(gen) {
            return;
        }
        ctx.data_mut(|d| d.insert_temp(id, gen));
        let c = self.live();
        ctx.style_mut_of(egui::Theme::Dark, |s| self.write_visuals_from(&mut s.visuals, &c));
    }
}
