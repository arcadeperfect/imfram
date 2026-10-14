//! Playground tab for `imfram::props`: a sheet with every row kind, defaults
//! so the reset dots show, a key dot that cycles its state on click, and the
//! last event the sheet reported.

use egui::Ui;
use imfram::props::{Defaults, KeyState, RowEvent, Sheet, Start};

pub struct State {
    gain: f32,
    size: i32,
    solo: bool,
    name: String,
    tint: [f32; 4],
    mult: [f32; 3],
    gang: bool,
    filter: usize,
    offset: [f32; 2],
    custom: f32,
    key: KeyState,
    defaults: Defaults,
    last: String,
}

const FILTERS: [&str; 4] = ["box", "triangle", "gaussian", "lanczos"];

impl Default for State {
    fn default() -> Self {
        let defaults = Defaults::new()
            .set("gain", 1.0f32)
            .set("size", 3)
            .set("solo", false)
            .set("name", "untitled")
            .set("tint", [1.0f32, 0.5, 0.2, 1.0])
            .set("mult", [1.0f32, 1.0, 1.0])
            .set("filter", 2usize)
            .set("offset", [0.0f32, 0.0]);
        Self {
            gain: 1.6,
            size: 5,
            solo: true,
            name: "shot_010".into(),
            tint: [0.2, 0.6, 0.9, 0.8],
            mult: [1.2, 1.0, 0.8],
            gang: true,
            filter: 0,
            offset: [12.0, -4.0],
            custom: 0.5,
            key: KeyState::None,
            defaults,
            last: "nothing yet".into(),
        }
    }
}

fn next(k: KeyState) -> KeyState {
    match k {
        KeyState::None => KeyState::Keyed,
        KeyState::Keyed => KeyState::Animated,
        KeyState::Animated => KeyState::Driven,
        KeyState::Driven => KeyState::None,
    }
}

pub fn ui(ui: &mut Ui, s: &mut State) {
    let State { gain, size, solo, name, tint, mult, gang, filter, offset, custom, key, defaults, last } = s;
    let mut sheet = Sheet::new(ui).label_width(90.0).defaults(defaults).key_column(true);
    sheet.section("Numbers", Start::Open, |s| {
        s.slider("gain", gain, (0.0, 2.0), false).tip("a float on a rail; Z X C / V B N change the drag rate").key(*key);
        s.int("size", size, (1, 16)).tip("an integer");
        s.vec2("offset", offset).tip("two number fields");
        s.custom("custom", |ui| ui.add(egui::Slider::new(custom, 0.0..=1.0)).changed()).tip("anything in the control column");
    });
    sheet.section("Words", Start::Open, |s| {
        s.toggle("solo", solo).tip("a bool");
        s.text("name", name).tip("a line of text");
        s.choice("filter", filter, &FILTERS).tip("one of a list");
    });
    sheet.section("Colour", Start::Open, |s| {
        s.colour("tint", tint).tip("an rgba swatch; click to pick");
        s.rgb("mult", mult, gang).tip("an rgb triple; G gang, S D F colour axes");
    });
    for (id, ev) in sheet.finish() {
        if ev == RowEvent::ToggleKey {
            *key = next(*key);
        }
        *last = format!("{ev:?} on {id:?}");
    }
    ui.separator();
    ui.weak(format!("last event: {last}"));
}
