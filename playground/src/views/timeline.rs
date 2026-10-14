//! Playground tab for `imfram::timeline`: a 1..=240 strip with eight keys
//! that move when dragged, playing from the start.

use egui::Ui;
use imfram::timeline::{Event, Timeline};

pub struct State {
    tl: Timeline,
    keys: Vec<i64>,
    last: String,
}

impl Default for State {
    fn default() -> Self {
        let mut tl = Timeline::new(1, 240);
        tl.playing = true;
        Self { tl, keys: vec![1, 24, 48, 90, 120, 160, 200, 240], last: String::new() }
    }
}

pub fn ui(ui: &mut Ui, s: &mut State) {
    ui.weak("LMB scrub · drag a diamond (shift: all) · MMB / alt+LMB pan · wheel zoom · Home fit · Space · I / O");
    for e in s.tl.show(ui, &s.keys) {
        match e {
            Event::KeyMoved { from, to } => {
                if let Some(k) = s.keys.iter_mut().find(|k| **k == from) {
                    *k = to;
                }
            }
            Event::Scrub(_) if s.tl.playing => continue,
            _ => {}
        }
        s.last = format!("{e:?}");
    }
    s.keys.sort_unstable();
    ui.horizontal(|ui| {
        ui.weak(format!("frame {}  keys {:?}", s.tl.playhead, s.keys));
        if !s.last.is_empty() {
            ui.weak(format!("last: {}", s.last));
        }
    });
}
