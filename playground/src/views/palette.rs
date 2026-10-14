//! Playground tab for `imfram::palette`: a button or ⌘K / Ctrl+K (with
//! the pointer over the tab) opens a palette over ~25 toy commands; the
//! last pick is shown.

use egui::{Key, Modifiers, Ui};
use imfram::palette::{Command, Palette};

pub struct Toy {
    id: u32,
    title: &'static str,
    group: &'static str,
    keys: &'static str,
}

impl Command for Toy {
    type Id = u32;
    fn id(&self) -> u32 {
        self.id
    }
    fn title(&self) -> &str {
        self.title
    }
    fn group(&self) -> &str {
        self.group
    }
    fn keys(&self) -> &str {
        self.keys
    }
}

pub struct State {
    palette: Palette,
    commands: Vec<Toy>,
    last: Option<u32>,
}

impl Default for State {
    fn default() -> Self {
        let toys = [
            ("File", "New", "⌘N"),
            ("File", "Open…", "⌘O"),
            ("File", "Open Recent", ""),
            ("File", "Save", "⌘S"),
            ("File", "Save As…", "⇧⌘S"),
            ("File", "Revert", ""),
            ("File", "Export Image", ""),
            ("Edit", "Undo", "⌘Z"),
            ("Edit", "Redo", "⇧⌘Z"),
            ("Edit", "Cut", "⌘X"),
            ("Edit", "Copy", "⌘C"),
            ("Edit", "Paste", "⌘V"),
            ("Edit", "Duplicate", "⇧D"),
            ("Edit", "Delete", "X"),
            ("Edit", "Preferences…", "⌘,"),
            ("View", "Frame All", "Home"),
            ("View", "Frame Selected", "."),
            ("View", "Toggle Grid", ""),
            ("View", "Toggle Overlays", "⇧⌥Z"),
            ("View", "Zoom In", "⌘="),
            ("View", "Zoom Out", "⌘-"),
            ("Node", "Add Node…", "⇧A"),
            ("Node", "Mute", "M"),
            ("Node", "Hide Unused Sockets", "⌃H"),
            ("Node", "Group Selected", "⌘G"),
            ("Window", "Toggle Fullscreen", "⌃⌘F"),
        ];
        let commands = toys.iter().enumerate().map(|(i, (g, t, k))| Toy { id: i as u32, title: t, group: g, keys: k }).collect();
        Self { palette: Palette::default(), commands, last: None }
    }
}

pub fn ui(ui: &mut Ui, s: &mut State) {
    let shortcut = ui.ui_contains_pointer() && ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::K));
    ui.horizontal(|ui| {
        if ui.button("Open palette").clicked() || shortcut {
            s.palette.toggle();
        }
        ui.weak("⌘K / Ctrl+K with the pointer over this tab");
    });
    ui.add_space(8.0);
    match s.last.and_then(|id| s.commands.iter().find(|c| c.id == id)) {
        Some(c) => ui.label(format!("Last picked: {} › {}", c.group, c.title)),
        None => ui.weak("Nothing picked yet"),
    };
    ui.add_space(8.0);
    ui.weak(format!("{} commands. Type to fuzzy-match; ↑ ↓ to move, Enter to pick, Esc to close. Recent picks float to the top.", s.commands.len()));
    if let Some(id) = s.palette.show(ui.ctx(), &s.commands) {
        s.last = Some(id);
    }
}
