//! The properties sheet: an inspector panel's rows on one two-column grid.
//!
//! A [`Sheet`] is built per frame over a `Ui`. Every row is a fixed label
//! column (right-aligned, weak) and a control that takes the rest, so each
//! control in the panel starts at the same x whatever its label says
//! (wobble's `LABEL_W`, graav's Globals page). At the far left a reset dot
//! shows while the value is away from its default (click: back to it); at
//! the far right an optional key dot says whether the value is keyed,
//! animated or driven (click: [`RowEvent::ToggleKey`]), where Blender puts
//! its keyframe decorator. Rows fold into sections through
//! `widgets::group`. Number fields are `numfield::fixed` boxes at the house
//! drag rates (`widgets::rated`); float sliders are `widgets::slider`, so
//! the rate keys mean the same here as everywhere.
//!
//! What was done comes back from [`Sheet::finish`] as `(RowId, RowEvent)`
//! pairs. A row is named by its label's hash, or by a caller id (`.id(..)`
//! on the row), so an app maps events to its own channels. A row's default
//! comes from `.default(v)` on the row or from a [`Defaults`] table handed
//! to the sheet, keyed the same way.
//!
//! A row draws when its builder is dropped, at the end of the statement
//! that made it: `s.slider("gain", &mut g, (0.0, 2.0), false).tip("..")`.
//!
//! Colours come from `ui.visuals()`: the key dot borrows the theme's
//! selection, hyperlink and warn colours for keyed, animated and driven.

use std::collections::HashMap;
use std::hash::Hash;

use egui::{vec2, Align2, Id, Layout, Rect, Sense, Ui};

pub use crate::widgets::group::Start;
use crate::widgets::{self, group, tip};
use crate::rgbslider;

mod control;
mod dots;
#[cfg(test)]
mod tests;

use control::Control;
use dots::{disc, key_tip, paint_key};

/// Default width of the label column: room for "smoothing over time".
pub const LABEL_W: f32 = 110.0;
/// The reset dot's slot, at the far left of every row.
pub const DOT_W: f32 = 14.0;
/// The key dot's slot, at the far right of a row that has one.
pub const KEY_W: f32 = 16.0;
/// Width of a number field: the house slider's, so a row of fields ends
/// where a slider's value box does.
pub const NUM_W: f32 = widgets::slider::NUM_W;
/// Height of a one-line row (egui's interact height).
pub const ROW_H: f32 = 18.0;
/// The least a control is given, however narrow the panel.
const MIN_CONTROL: f32 = 60.0;
/// Rail kept for a slider beside its number field.
const MIN_RAIL: f32 = 40.0;
const DOT_R: f32 = 3.0;
const KEY_R: f32 = 3.5;

/// How a row is named in events and defaults: its label's hash unless the
/// caller gave it an id.
pub type RowId = Id;

/// What a key dot says about a value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyState {
    /// Not animated: a hollow dot.
    None,
    /// A key sits at the current time: a filled diamond.
    Keyed,
    /// Animated, with the keys elsewhere: a hollow diamond.
    Animated,
    /// Driven by an expression or a link: a filled circle.
    Driven,
}

/// What the user did to a row this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowEvent {
    Changed,
    /// The reset dot was clicked; the value is already back at its default.
    Reset,
    /// The key dot was clicked; the app decides what that means.
    ToggleKey,
}

/// A default a row can be reset to. A default of the wrong kind for its
/// row is ignored.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    F32(f32),
    I32(i32),
    Bool(bool),
    Text(String),
    Colour([f32; 4]),
    Rgb([f32; 3]),
    Choice(usize),
    Vec2([f32; 2]),
}

macro_rules! value_from {
    ($($t:ty => $v:ident),* $(,)?) => {$(
        impl From<$t> for Value {
            fn from(x: $t) -> Self { Value::$v(x.into()) }
        }
    )*};
}
value_from!(f32 => F32, i32 => I32, bool => Bool, String => Text, &str => Text, [f32; 4] => Colour, [f32; 3] => Rgb, usize => Choice, [f32; 2] => Vec2);

/// Defaults for a whole sheet, by row: the label (or the caller id) each
/// row goes by.
#[derive(Clone, Debug, Default)]
pub struct Defaults {
    by_row: HashMap<RowId, Value>,
}

impl Defaults {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(mut self, row: impl Hash + std::fmt::Debug, v: impl Into<Value>) -> Self {
        self.by_row.insert(Id::new(row), v.into());
        self
    }

    pub fn get(&self, row: RowId) -> Option<&Value> {
        self.by_row.get(&row)
    }
}

/// Where a row's parts were last drawn, in window coordinates, for UI
/// suites to aim at (the `slider::rects` pattern).
#[derive(Clone, Copy, Debug)]
pub struct RowRects {
    pub row: Rect,
    pub dot: Rect,
    pub label: Rect,
    pub control: Rect,
    pub key: Option<Rect>,
}

fn rects_id(row: RowId) -> Id {
    Id::new("props-rects").with(row)
}

pub fn rects(ctx: &egui::Context, row: RowId) -> Option<RowRects> {
    ctx.data(|d| d.get_temp(rects_id(row)))
}

fn remember(ui: &Ui, row: RowId, r: RowRects) {
    let to = ui.ctx().layer_transform_to_global(ui.layer_id()).unwrap_or_default();
    let r = RowRects { row: to * r.row, dot: to * r.dot, label: to * r.label, control: to * r.control, key: r.key.map(|k| to * k) };
    ui.data_mut(|d| d.insert_temp(rects_id(row), r));
}

/// The rows of one panel, built per frame.
pub struct Sheet<'a> {
    ui: &'a mut Ui,
    label_w: f32,
    key_column: bool,
    rgb_spec: rgbslider::RgbSpec,
    defaults: Option<&'a Defaults>,
    events: Vec<(RowId, RowEvent)>,
}

impl<'a> Sheet<'a> {
    pub fn new(ui: &'a mut Ui) -> Self {
        // Rec.709 luma, a gain's range: what an rgb row means unless told.
        let rgb_spec = rgbslider::RgbSpec::scale(0.0, 2.0, [0.2126, 0.7152, 0.0722]);
        Self { ui, label_w: LABEL_W, key_column: false, rgb_spec, defaults: None, events: Vec::new() }
    }

    /// Width of the label column.
    pub fn label_width(mut self, w: f32) -> Self {
        self.label_w = w;
        self
    }

    /// Keep a key-dot slot on every row, so rows with and without a key
    /// line up. Off, only rows given `.key(..)` have the slot.
    pub fn key_column(mut self, on: bool) -> Self {
        self.key_column = on;
        self
    }

    /// What the rgb rows' triples mean (range, luma weights, display).
    pub fn rgb_spec(mut self, spec: rgbslider::RgbSpec) -> Self {
        self.rgb_spec = spec;
        self
    }

    /// Defaults for rows that do not give their own.
    pub fn defaults(mut self, d: &'a Defaults) -> Self {
        self.defaults = Some(d);
        self
    }

    /// The ui between rows, for anything that is not a row.
    pub fn ui(&mut self) -> &mut Ui {
        self.ui
    }

    /// A foldable group of rows, aligned to the same columns.
    pub fn section(&mut self, name: &str, start: Start, body: impl FnOnce(&mut Sheet<'_>)) {
        let (label_w, key_column, rgb_spec, defaults) = (self.label_w, self.key_column, self.rgb_spec, self.defaults);
        let mut events = Vec::new();
        group::group(self.ui, name, "", start, |ui| {
            let mut inner = Sheet { ui, label_w, key_column, rgb_spec, defaults, events: Vec::new() };
            body(&mut inner);
            events = inner.events;
        });
        self.events.extend(events);
    }

    /// What was done this frame, row by row.
    pub fn finish(self) -> Vec<(RowId, RowEvent)> {
        self.events
    }

    fn row<'r>(&'r mut self, label: &'r str, control: Control<'r>) -> Row<'r, 'a> {
        Row { sheet: self, label, control: Some(control), tip: "", default: None, key: None, id: None }
    }

    /// A float on a rail with a number field; `log` for a ratio-like range.
    pub fn slider<'r>(&'r mut self, label: &'r str, v: &'r mut f32, range: (f32, f32), log: bool) -> Row<'r, 'a> {
        self.row(label, Control::Slider { v, range, log })
    }

    /// An integer on a rail with a number field.
    pub fn int<'r>(&'r mut self, label: &'r str, v: &'r mut i32, range: (i32, i32)) -> Row<'r, 'a> {
        self.row(label, Control::Int { v, range })
    }

    pub fn toggle<'r>(&'r mut self, label: &'r str, v: &'r mut bool) -> Row<'r, 'a> {
        self.row(label, Control::Toggle(v))
    }

    pub fn text<'r>(&'r mut self, label: &'r str, v: &'r mut String) -> Row<'r, 'a> {
        self.row(label, Control::Text(v))
    }

    /// An rgba colour behind a swatch (`colorpicker::button`).
    pub fn colour<'r>(&'r mut self, label: &'r str, v: &'r mut [f32; 4]) -> Row<'r, 'a> {
        self.row(label, Control::Colour(v))
    }

    /// An rgb triple on three lanes (`rgbslider::show`), with its gang.
    pub fn rgb<'r>(&'r mut self, label: &'r str, v: &'r mut [f32; 3], gang: &'r mut bool) -> Row<'r, 'a> {
        self.row(label, Control::Rgb { v, gang })
    }

    /// One of `names`, as a dropdown.
    pub fn choice<'r>(&'r mut self, label: &'r str, v: &'r mut usize, names: &'r [&'r str]) -> Row<'r, 'a> {
        self.row(label, Control::Choice { v, names })
    }

    /// Two number fields.
    pub fn vec2<'r>(&'r mut self, label: &'r str, v: &'r mut [f32; 2]) -> Row<'r, 'a> {
        self.row(label, Control::Vec2(v))
    }

    /// Anything else in the control column; `add` says whether it changed
    /// the value.
    pub fn custom<'r>(&'r mut self, label: &'r str, add: impl FnOnce(&mut Ui) -> bool + 'r) -> Row<'r, 'a> {
        self.row(label, Control::Custom(Box::new(add)))
    }

    fn draw(&mut self, label: &str, mut control: Control<'_>, tip_text: &str, default: Option<Value>, key: Option<KeyState>, id: RowId) {
        let default = default.or_else(|| self.defaults.and_then(|d| d.get(id).cloned()));
        let height = control.height();
        let show_key = self.key_column || key.is_some();
        let (label_w, rgb_spec) = (self.label_w, self.rgb_spec);
        let mut events = Vec::new();
        // Every row can go in a composer (`crate::composer`), by its label.
        crate::composer::item(&mut *self.ui, label, |ui| {
            let wid = ui.id().with(("props", id));
            let row = ui.horizontal(|ui| {
                let gap = ui.spacing().item_spacing.x;
                // The key slot is pinned to the row's right edge rather than
                // flowed after the control, so a control that runs a little
                // over its width (the house slider's trailing label) cannot
                // push it out of line.
                let edge = ui.available_rect_before_wrap();
                let key_rect = Rect::from_min_size(egui::pos2(edge.right() - KEY_W, edge.top()), vec2(KEY_W, height));
                // Reset dot: there while the value is away from its default.
                let (dot_rect, dot) = ui.allocate_exact_size(vec2(DOT_W, height), Sense::click());
                if let Some(d) = default.as_ref().filter(|d| control.differs(d)) {
                    let c = if dot.hovered() { ui.visuals().widgets.hovered.fg_stroke.color } else { ui.visuals().widgets.inactive.fg_stroke.color };
                    disc(ui.painter(), dot_rect.center(), DOT_R, c);
                    if dot.clicked() {
                        control.reset(d);
                        events.push(RowEvent::Reset);
                    }
                    dot.on_hover_text("back to the default");
                }
                // Label: right-aligned, weak, clipped to its column.
                let (label_rect, _) = ui.allocate_exact_size(vec2(label_w, height), Sense::hover());
                let font = egui::TextStyle::Body.resolve(ui.style());
                ui.painter().with_clip_rect(label_rect).text(label_rect.right_center(), Align2::RIGHT_CENTER, label, font, ui.visuals().weak_text_color());
                // Control: the rest, less the key slot.
                let key_w = if show_key { KEY_W + gap } else { 0.0 };
                let cw = (ui.available_width() - key_w).max(MIN_CONTROL);
                let c = ui.allocate_ui_with_layout(vec2(cw, height), Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.set_min_size(vec2(cw, height));
                    control.show(ui, wid, rgb_spec)
                });
                if c.inner {
                    events.push(RowEvent::Changed);
                }
                let key_rect = show_key.then(|| {
                    let resp = ui.interact(key_rect, wid.with("key"), Sense::click());
                    ui.allocate_rect(key_rect, Sense::hover());
                    paint_key(ui, key_rect, key.unwrap_or(KeyState::None), resp.hovered());
                    if resp.clicked() {
                        events.push(RowEvent::ToggleKey);
                    }
                    key_tip(&resp, key.unwrap_or(KeyState::None));
                    key_rect
                });
                (dot_rect, label_rect, c.response.rect, key_rect)
            });
            tip::tip_row(ui, &row.response, tip_text);
            let (dot, label, control, key) = row.inner;
            remember(ui, id, RowRects { row: row.response.rect, dot, label, control, key });
        });
        self.events.extend(events.into_iter().map(|e| (id, e)));
    }
}

/// One row being described; drawn when dropped.
pub struct Row<'r, 'a> {
    sheet: &'r mut Sheet<'a>,
    label: &'r str,
    control: Option<Control<'r>>,
    tip: &'r str,
    default: Option<Value>,
    key: Option<KeyState>,
    id: Option<RowId>,
}

impl<'r> Row<'r, '_> {
    /// Hover text for the whole row.
    pub fn tip(mut self, text: &'r str) -> Self {
        self.tip = text;
        self
    }

    /// The value the reset dot goes back to.
    pub fn default(mut self, v: impl Into<Value>) -> Self {
        self.default = Some(v.into());
        self
    }

    /// Show a key dot in this state.
    pub fn key(mut self, k: KeyState) -> Self {
        self.key = Some(k);
        self
    }

    /// Name the row by this instead of its label.
    pub fn id(mut self, id: impl Hash + std::fmt::Debug) -> Self {
        self.id = Some(Id::new(id));
        self
    }
}

impl Drop for Row<'_, '_> {
    fn drop(&mut self) {
        if let Some(control) = self.control.take() {
            let id = self.id.unwrap_or_else(|| Id::new(self.label));
            let default = self.default.take();
            self.sheet.draw(self.label, control, self.tip, default, self.key, id);
        }
    }
}
