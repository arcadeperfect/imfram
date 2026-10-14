//! The faces egui draws with, as a setting: a size for the body and the
//! code, and a font file of the user's for the proportional face (the
//! built-in faces stay as fallbacks, so every glyph still has a home).
//! Apply once when it changes, not every frame: loading a file is slow.

use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
pub struct Fonts {
    /// Body text, points. egui's default is 14.
    pub size: f32,
    /// Monospace text, points. egui's default is 12.
    pub mono: f32,
    /// A .ttf / .otf for the proportional face; none keeps egui's.
    pub file: Option<PathBuf>,
}

impl Default for Fonts {
    fn default() -> Self {
        Self { size: 14.0, mono: 12.0, file: None }
    }
}

impl Fonts {
    /// Install into `ctx`. Returns the file's error, if it would not load
    /// (the rest is applied anyway).
    pub fn apply(&self, ctx: &egui::Context) -> Result<(), String> {
        let mut defs = egui::FontDefinitions::default();
        let mut err = Ok(());
        if let Some(path) = &self.file {
            match std::fs::read(path) {
                Ok(bytes) => {
                    defs.font_data.insert("user".into(), Arc::new(egui::FontData::from_owned(bytes)));
                    defs.families.entry(egui::FontFamily::Proportional).or_default().insert(0, "user".into());
                }
                Err(e) => err = Err(format!("{}: {e}", path.display())),
            }
        }
        ctx.set_fonts(defs);
        let k = self.size / 14.0;
        ctx.all_styles_mut(|style| {
            use egui::{FontFamily, FontId, TextStyle};
            style.text_styles = [
                (TextStyle::Small, FontId::new(10.0 * k, FontFamily::Proportional)),
                (TextStyle::Body, FontId::new(self.size, FontFamily::Proportional)),
                (TextStyle::Button, FontId::new(self.size, FontFamily::Proportional)),
                (TextStyle::Heading, FontId::new(18.0 * k, FontFamily::Proportional)),
                (TextStyle::Monospace, FontId::new(self.mono, FontFamily::Monospace)),
            ]
            .into();
        });
        err
    }
}
