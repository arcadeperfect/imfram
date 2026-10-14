//! The tilt menubar: a file menu (quit), a view menu (the windows and
//! tabs), and a prefs menu with a slider and a choice, so every item kind
//! is on show. Built fresh each frame from the playground's state.

use imfram::dock::Tab as _;
use imfram::tiltmenu::{Event, Item, Menu};

use crate::tabs::{Playground, Tab};

pub fn menus(p: &Playground) -> Vec<Menu> {
    let mut view = vec![Item::check("strip", "key strip on the canvas", p.strip)];
    for &t in Tab::all() {
        view.push(Item::action(tab_id(t), format!("toggle {}", t.title())));
    }
    vec![
        Menu { id: "file", short: "file".into(), full: "file".into(), items: vec![Item::action("quit", "quit")] },
        Menu { id: "view", short: "view".into(), full: "view".into(), items: view },
        Menu {
            id: "prefs",
            short: "prefs".into(),
            full: "preferences".into(),
            items: vec![
                Item::slider("zoom", "ui zoom", p.zoom, 0.6, 2.0).step(0.05),
                Item::choice("theme", "theme", vec!["dark".into(), "light".into()], Some(p.theme)),
                Item::sub(
                    "font",
                    vec![
                        Item::slider("font-size", "body size", p.fonts.size, 9.0, 24.0).step(0.5).unit("pt"),
                        Item::slider("font-mono", "code size", p.fonts.mono, 8.0, 22.0).step(0.5).unit("pt"),
                        Item::action("font-file", "font file…"),
                        Item::action("font-reset", "egui's own faces"),
                    ],
                ),
                Item::sub("more", vec![Item::check("hidden", "show hidden files", p.hidden), Item::action("reset", "reset everything")]),
            ],
        },
    ]
}

fn tab_id(t: Tab) -> &'static str {
    match t {
        Tab::Widgets => "tab-widgets",
        Tab::Colour => "tab-colour",
        Tab::Text => "tab-text",
        Tab::Files => "tab-files",
        Tab::Canvas => "tab-canvas",
        Tab::Nodes => "tab-nodes",
        Tab::Keys => "tab-keys",
        Tab::Nothing => "tab-nothing",
        Tab::Props => "tab-props",
        Tab::Outliner => "tab-outliner",
        Tab::Table => "tab-table",
        Tab::Timeline => "tab-timeline",
        Tab::Image => "tab-image",
        Tab::Palette => "tab-palette",
        Tab::Dialogs => "tab-dialogs",
        Tab::Toasts => "tab-toasts",
        Tab::Theme => "tab-theme",
        Tab::Keymap => "tab-keymap",
    }
}

pub fn apply(p: &mut Playground, events: Vec<Event>) {
    for e in events {
        match e {
            Event::Action("quit") => p.quit = true,
            Event::Action("reset") => *p = Playground::default(),
            Event::Toggle("strip", on) => p.strip = on,
            Event::Toggle("hidden", on) => {
                p.hidden = on;
                p.files.show_hidden = on;
                p.files.refresh();
            }
            Event::Slide("zoom", v) => p.zoom = v,
            Event::Slide("font-size", v) => {
                p.fonts.size = v;
                p.fonts_dirty = true;
            }
            Event::Slide("font-mono", v) => {
                p.fonts.mono = v;
                p.fonts_dirty = true;
            }
            Event::Action("font-file") => {
                p.font_pick = true;
                p.files.exts = vec!["ttf".into(), "otf".into()];
                p.toggles.push(Tab::Files);
            }
            Event::Action("font-reset") => {
                p.fonts.file = None;
                p.fonts_dirty = true;
            }
            Event::Action(id) => {
                if let Some(&t) = Tab::all().iter().find(|t| tab_id(**t) == id) {
                    p.toggles.push(t);
                }
            }
            Event::Pick("theme", i) => p.theme = i,
            _ => {}
        }
    }
}
