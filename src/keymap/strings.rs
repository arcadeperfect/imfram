//! Bindings as strings: the prefs form (platform-neutral, round-trips) and
//! the display form (the platform's glyphs).

use super::{norm, Binding};
use egui::{Key, Modifiers};

/// Prefs form: `cmd+shift+S`, `[`, `Space`, `cmd+Up`. `Key::name()` spells
/// the key; [`parse`] reads it back.
pub fn format(b: Binding) -> String {
    let mut s = String::new();
    if b.mods.command {
        s.push_str("cmd+");
    }
    if b.mods.ctrl {
        s.push_str("ctrl+");
    }
    if b.mods.shift {
        s.push_str("shift+");
    }
    if b.mods.alt {
        s.push_str("alt+");
    }
    s.push_str(b.key.name());
    s
}

/// The prefs form back to a binding; forgiving about case and spaces,
/// `None` for a missing or unknown key.
pub fn parse(s: &str) -> Option<Binding> {
    let mut mods = Modifiers::NONE;
    let mut key = None;
    for part in s.split('+') {
        let part = part.trim();
        match part.to_ascii_lowercase().as_str() {
            "cmd" | "command" | "super" => mods.command = true,
            "ctrl" | "control" => mods.ctrl = true,
            "shift" => mods.shift = true,
            "alt" | "option" => mods.alt = true,
            _ => key = Key::from_name(part),
        }
    }
    key.map(|key| Binding { key, mods: norm(mods) })
}

/// Display form for panels and help: "⌘⇧Z" on a Mac, "Ctrl+Shift+Z"
/// elsewhere. The egui context knows the platform.
pub fn display(ctx: &egui::Context, b: Binding) -> String {
    ctx.format_shortcut(&egui::KeyboardShortcut { modifiers: b.mods, logical_key: b.key })
}
