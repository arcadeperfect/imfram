//! Playground tab for `imfram::keymap`: a toy action set in two scopes,
//! the keys window, and the last action that fired while the tab was
//! hovered.

use egui::{Key, Modifiers, Ui};
use imfram::keymap::{Action, Binding, Fixed, Keymap, KeysUi, MatchMode};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Act {
    Save,
    Open,
    Undo,
    Redo,
    Frame,
    Delete,
    Grab,
    Tidy,
    Swap,
    Pin,
    SelectAll,
}

impl Action for Act {
    fn all() -> &'static [Self] {
        use Act::*;
        &[Save, Open, Undo, Redo, Frame, Delete, Grab, Tidy, Swap, Pin, SelectAll]
    }
    fn name(self) -> &'static str {
        match self {
            Act::Save => "app.save",
            Act::Open => "app.open",
            Act::Undo => "app.undo",
            Act::Redo => "app.redo",
            Act::Frame => "canvas.frame",
            Act::Delete => "canvas.delete",
            Act::Grab => "canvas.grab",
            Act::Tidy => "canvas.tidy",
            Act::Swap => "canvas.swap",
            Act::Pin => "canvas.pin",
            Act::SelectAll => "canvas.select_all",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Act::Save => "save",
            Act::Open => "open…",
            Act::Undo => "undo",
            Act::Redo => "redo",
            Act::Frame => "frame the view",
            Act::Delete => "delete the selection",
            Act::Grab => "grab the selection",
            Act::Tidy => "tidy the layout",
            Act::Swap => "swap inputs",
            Act::Pin => "pin / unpin (shift = replace)",
            Act::SelectAll => "select all",
        }
    }
    fn scope(self) -> &'static str {
        match self {
            Act::Save | Act::Open | Act::Undo | Act::Redo => "app",
            _ => "canvas",
        }
    }
    fn default(self) -> Binding {
        match self {
            Act::Save => Binding::cmd(Key::S),
            Act::Open => Binding::cmd(Key::O),
            Act::Undo => Binding::cmd(Key::Z),
            Act::Redo => Binding::cmd_shift(Key::Z),
            Act::Frame => Binding::plain(Key::F),
            Act::Delete => Binding::plain(Key::Delete),
            Act::Grab => Binding::plain(Key::G),
            Act::Tidy => Binding::plain(Key::T),
            Act::Swap => Binding::plain(Key::S),
            Act::Pin => Binding::plain(Key::Z),
            Act::SelectAll => Binding::ctrl(Key::A),
        }
    }
    fn mode(self) -> MatchMode {
        if self == Act::Pin { MatchMode::KeyOnly } else { MatchMode::Exact }
    }
    fn also(self) -> Option<Key> {
        (self == Act::Delete).then_some(Key::Backspace)
    }
    fn also_mods(self) -> Option<Modifiers> {
        (self == Act::SelectAll).then_some(Modifiers::COMMAND)
    }
}

pub struct State {
    map: Keymap<Act>,
    keys: KeysUi<Act>,
    last: Option<(Act, Modifiers)>,
    fired: u32,
}

impl Default for State {
    fn default() -> Self {
        let fixed = vec![
            Fixed { how: "alt-drag", label: "pan, every canvas (or middle-drag)", key: None, mods: Modifiers::ALT },
            Fixed { how: "hold Z", label: "drag rate ×0.01 (finest)", key: Some(Key::Z), mods: Modifiers::NONE },
            Fixed { how: "hold X", label: "drag rate ×0.1", key: Some(Key::X), mods: Modifiers::NONE },
        ];
        Self { map: Keymap::new(), keys: KeysUi::new().with_fixed(fixed), last: None, fired: 0 }
    }
}

pub fn ui(ui: &mut Ui, s: &mut State) {
    // The tab under the pointer owns the keys (and stands down during a
    // capture, or teaching "G" would also grab).
    if ui.ui_contains_pointer() && !s.map.capturing() {
        let hit = ui.input(|i| Act::all().iter().copied().find(|a| s.map.pressed(i, *a)).map(|a| (a, i.modifiers)));
        if let Some(hit) = hit {
            s.last = Some(hit);
            s.fired += 1;
        }
    }
    ui.horizontal(|ui| {
        ui.toggle_value(&mut s.keys.open, "keys…").on_hover_text("the rebind window");
        if ui.small_button("reset all").clicked() {
            s.map.reset_all();
        }
        ui.weak(format!("{} override(s)", s.map.overrides().len()));
    });
    ui.add_space(6.0);
    match s.last {
        Some((a, m)) if a.mode() == MatchMode::KeyOnly => {
            ui.strong(format!("last: {} ({}) with shift={} alt={}  — #{}", a.label(), a.scope(), m.shift, m.alt, s.fired));
        }
        Some((a, _)) => {
            ui.strong(format!("last: {} ({})  — #{}", a.label(), a.scope(), s.fired));
        }
        None => {
            ui.weak("hover this tab and press a bound key; the last action to fire shows here");
        }
    }
    ui.add_space(8.0);
    egui::Grid::new("keymap-demo").striped(true).num_columns(3).show(ui, |ui| {
        for scope in Keymap::<Act>::scopes() {
            for a in Act::all().iter().filter(|a| a.scope() == scope) {
                ui.weak(scope);
                ui.label(a.label());
                let mut t = egui::RichText::new(s.map.label(ui.ctx(), *a)).strong();
                if s.map.conflict_of(*a).is_some() {
                    t = t.color(ui.visuals().warn_fg_color);
                }
                if ui.button(t).on_hover_text("rebind: press the new chord").clicked() {
                    s.keys.open = true;
                    s.keys.begin_capture(*a);
                }
                ui.end_row();
            }
        }
    });
    if s.keys.window(ui.ctx(), &s.map) {
        // An app would mirror `s.map.overrides()` into its prefs here.
        log::info!("keymap overrides: {:?}", s.map.overrides());
    }
}
