//! Playground tab for `imfram::outliner`: a scene-ish tree of ~40 nodes in
//! a toy model that applies the Rename and Move events it is handed, next
//! to a log of every event, so the hands can be tried against real
//! consequences (drag a light under a camera, F2 a prop).

use std::collections::HashMap;

use egui::Ui;
use imfram::outliner::{Event, Outliner, TreeModel};

struct Node {
    name: String,
    icon: &'static str,
    kids: Vec<u32>,
}

struct Toy {
    nodes: HashMap<u32, Node>,
    roots: Vec<u32>,
}

impl TreeModel for Toy {
    type Id = u32;
    fn roots(&self) -> Vec<u32> {
        self.roots.clone()
    }
    fn children(&self, id: u32) -> Vec<u32> {
        self.nodes.get(&id).map_or(vec![], |n| n.kids.clone())
    }
    fn label(&self, id: u32) -> String {
        self.nodes.get(&id).map_or_else(|| "?".into(), |n| n.name.clone())
    }
    fn icon(&self, id: u32) -> Option<&'static str> {
        self.nodes.get(&id).map(|n| n.icon).filter(|i| !i.is_empty())
    }
    fn can_rename(&self, id: u32) -> bool {
        !self.roots.contains(&id)
    }
}

impl Toy {
    fn add(&mut self, parent: Option<u32>, name: &str, icon: &'static str) -> u32 {
        let id = self.nodes.len() as u32 + 1;
        self.nodes.insert(id, Node { name: name.into(), icon, kids: vec![] });
        match parent {
            Some(p) => self.nodes.get_mut(&p).unwrap().kids.push(id),
            None => self.roots.push(id),
        }
        id
    }

    fn list_mut(&mut self, parent: Option<u32>) -> &mut Vec<u32> {
        match parent {
            Some(p) => &mut self.nodes.get_mut(&p).unwrap().kids,
            None => &mut self.roots,
        }
    }

    /// The model is the only thing that changes itself; the outliner asked.
    fn apply(&mut self, ev: &Event<u32>) {
        match ev {
            Event::Rename(id, name) => {
                if let Some(n) = self.nodes.get_mut(id) {
                    n.name = name.clone();
                }
            }
            Event::Move { id, parent, index } => {
                let from: Vec<Option<u32>> = std::iter::once(None).chain(self.nodes.keys().copied().map(Some)).collect();
                for p in from {
                    let list = self.list_mut(p);
                    if let Some(k) = list.iter().position(|n| n == id) {
                        list.remove(k);
                        break;
                    }
                }
                let list = self.list_mut(*parent);
                list.insert((*index).min(list.len()), *id);
            }
            Event::Select(_) | Event::Activate(_) | Event::Check(..) | Event::Button(_) => {}
        }
    }
}

fn scene() -> Toy {
    let mut t = Toy { nodes: HashMap::new(), roots: vec![] };
    let scene = t.add(None, "Scene", "");
    let cams = t.add(Some(scene), "Cameras", "");
    for n in ["main_cam", "wide_cam", "insert_cam"] {
        t.add(Some(cams), n, "C");
    }
    let lights = t.add(Some(scene), "Lights", "");
    for n in ["key", "fill", "rim", "sun", "bounce"] {
        t.add(Some(lights), n, "L");
    }
    let geo = t.add(Some(scene), "Geometry", "");
    let chars = t.add(Some(geo), "Characters", "");
    for who in ["hero", "villain", "dog"] {
        let w = t.add(Some(chars), who, "G");
        for part in ["body", "head", "hair", "eyes"] {
            t.add(Some(w), &format!("{who}_{part}"), "");
        }
    }
    let props = t.add(Some(geo), "Props", "");
    for n in ["chair", "table", "lamp", "book", "mug", "rug", "door", "window", "plant", "clock"] {
        t.add(Some(props), n, "G");
    }
    let comp = t.add(None, "Comp", "");
    for n in ["Read1", "Read2", "Read3", "Merge", "Grade", "Defocus", "Write"] {
        t.add(Some(comp), n, "N");
    }
    t
}

pub struct State {
    model: Toy,
    outliner: Outliner<u32>,
    log: Vec<String>,
}

impl Default for State {
    fn default() -> Self {
        let model = scene();
        let mut outliner = Outliner::new();
        for id in model.roots() {
            outliner.expanded.insert(id);
            for k in model.children(id) {
                outliner.expanded.insert(k);
            }
        }
        Self { model, outliner, log: vec![] }
    }
}

const LOG_W: f32 = 220.0;
const LOG_MAX: usize = 40;

pub fn ui(ui: &mut Ui, s: &mut State) {
    let h = ui.available_height();
    let w = (ui.available_width() - LOG_W - ui.spacing().item_spacing.x).max(100.0);
    ui.horizontal_top(|ui| {
        let events = ui.allocate_ui(egui::vec2(w, h), |ui| s.outliner.show(ui, &s.model)).inner;
        for ev in &events {
            s.model.apply(ev);
            s.log.push(format!("{ev:?}"));
        }
        if s.log.len() > LOG_MAX {
            let cut = s.log.len() - LOG_MAX;
            s.log.drain(..cut);
        }
        ui.vertical(|ui| {
            ui.set_width(LOG_W);
            ui.weak("click · shift · cmd · double-click / F2 · drag between or onto · arrows");
            ui.separator();
            egui::ScrollArea::vertical().id_salt("outliner-log").stick_to_bottom(true).show(ui, |ui| {
                for line in &s.log {
                    ui.monospace(line);
                }
            });
        });
    });
}
