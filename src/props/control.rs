//! The control column: one enum over every row kind, with how each is
//! drawn, whether it is off its default, and how it goes back.

use egui::{Id, Ui};

use super::{Value, MIN_RAIL, NUM_W, ROW_H};
use crate::{colorpicker, numfield, rgbslider, textedit, widgets};

/// The held drag rate as the colour picker wants it (None for 1:1).
fn held_rate(ui: &Ui) -> Option<f32> {
    let r = widgets::drag_rate(ui);
    (r != 1.0).then_some(r)
}

fn no_claim(_: &egui::Context) {}

pub(super) enum Control<'v> {
    Slider { v: &'v mut f32, range: (f32, f32), log: bool },
    Int { v: &'v mut i32, range: (i32, i32) },
    Toggle(&'v mut bool),
    Text(&'v mut String),
    Colour(&'v mut [f32; 4]),
    Rgb { v: &'v mut [f32; 3], gang: &'v mut bool },
    Choice { v: &'v mut usize, names: &'v [&'v str] },
    Vec2(&'v mut [f32; 2]),
    Custom(Box<dyn FnOnce(&mut Ui) -> bool + 'v>),
}

impl Control<'_> {
    pub(super) fn height(&self) -> f32 {
        match self {
            Control::Rgb { .. } => rgbslider::NATURAL_H,
            _ => ROW_H,
        }
    }

    pub(super) fn differs(&self, d: &Value) -> bool {
        match (self, d) {
            (Control::Slider { v, .. }, Value::F32(d)) => **v != *d,
            (Control::Int { v, .. }, Value::I32(d)) => **v != *d,
            (Control::Toggle(v), Value::Bool(d)) => **v != *d,
            (Control::Text(v), Value::Text(d)) => **v != *d,
            (Control::Colour(v), Value::Colour(d)) => **v != *d,
            (Control::Rgb { v, .. }, Value::Rgb(d)) => **v != *d,
            (Control::Choice { v, .. }, Value::Choice(d)) => **v != *d,
            (Control::Vec2(v), Value::Vec2(d)) => **v != *d,
            _ => false,
        }
    }

    pub(super) fn reset(&mut self, d: &Value) {
        match (self, d) {
            (Control::Slider { v, .. }, Value::F32(d)) => **v = *d,
            (Control::Int { v, .. }, Value::I32(d)) => **v = *d,
            (Control::Toggle(v), Value::Bool(d)) => **v = *d,
            (Control::Text(v), Value::Text(d)) => v.clone_from(d),
            (Control::Colour(v), Value::Colour(d)) => **v = *d,
            (Control::Rgb { v, .. }, Value::Rgb(d)) => **v = *d,
            (Control::Choice { v, .. }, Value::Choice(d)) => **v = *d,
            (Control::Vec2(v), Value::Vec2(d)) => **v = *d,
            _ => {}
        }
    }

    /// Draw the control in `ui` (its whole width). True when the value
    /// changed.
    pub(super) fn show(&mut self, ui: &mut Ui, id: Id, rgb_spec: rgbslider::RgbSpec) -> bool {
        let gap = ui.spacing().item_spacing.x;
        match self {
            Control::Slider { v, range, log } => {
                // The slider's own value box follows its rail, a gap between.
                ui.spacing_mut().slider_width = (ui.available_width() - NUM_W - gap).max(MIN_RAIL);
                widgets::slider(ui, v, *range, "", *log, "")
            }
            Control::Int { v, range: (lo, hi) } => {
                let before = **v;
                ui.spacing_mut().slider_width = (ui.available_width() - NUM_W - gap).max(MIN_RAIL);
                ui.add(egui::Slider::new(*v, *lo..=*hi).integer().show_value(false));
                let speed = widgets::rated(ui, 0.1);
                numfield::fixed(ui, NUM_W, ROW_H, egui::DragValue::new(*v).speed(speed).range(*lo..=*hi));
                **v != before
            }
            Control::Toggle(v) => ui.checkbox(v, "").changed(),
            Control::Text(v) => textedit::line(ui, v, "").changed(),
            Control::Colour(v) => {
                let before = **v;
                colorpicker::button(ui, id, v, colorpicker::PickSpec::rgba().with_input(held_rate, no_claim));
                **v != before
            }
            Control::Rgb { v, gang } => rgbslider::show(ui, id, v, gang, rgb_spec, None, false).changed,
            Control::Choice { v, names } => {
                let before = **v;
                let shown = names.get(**v).copied().unwrap_or("");
                egui::ComboBox::from_id_salt(id).width(ui.available_width()).selected_text(shown).show_ui(ui, |ui| {
                    for (i, n) in names.iter().enumerate() {
                        ui.selectable_value(*v, i, *n);
                    }
                });
                **v != before
            }
            Control::Vec2(v) => {
                let before = **v;
                let w = ((ui.available_width() - gap) / 2.0).max(NUM_W);
                let speed = widgets::rated(ui, 0.01);
                for x in v.iter_mut() {
                    numfield::fixed(ui, w, ROW_H, egui::DragValue::new(x).speed(speed).max_decimals(3));
                }
                **v != before
            }
            Control::Custom(add) => {
                // Taken once; a second show (there is none) would be a no-op.
                let add = std::mem::replace(add, Box::new(|_| false));
                add(ui)
            }
        }
    }
}
