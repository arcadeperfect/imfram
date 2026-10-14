//! wobble's pure keymap tests, on a toy action set in two scopes.

use super::board::{cap_font, cap_rect, keyboard_rows, Cap, ModKey};
use super::ui::row_matches;
use super::*;
use egui::{Color32, Event, InputState, Key, Modifiers};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Toy {
    Save,
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

impl Action for Toy {
    fn all() -> &'static [Self] {
        use Toy::*;
        &[Save, Undo, Redo, Frame, Delete, Grab, Tidy, Swap, Pin, SelectAll]
    }
    fn name(self) -> &'static str {
        match self {
            Toy::Save => "app.save",
            Toy::Undo => "app.undo",
            Toy::Redo => "app.redo",
            Toy::Frame => "graph.frame",
            Toy::Delete => "graph.delete",
            Toy::Grab => "graph.grab",
            Toy::Tidy => "graph.tidy",
            Toy::Swap => "graph.swap",
            Toy::Pin => "graph.pin",
            Toy::SelectAll => "graph.select_all",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Toy::Save => "save",
            Toy::Undo => "undo",
            Toy::Redo => "redo",
            Toy::Frame => "frame the view",
            Toy::Delete => "delete the selection",
            Toy::Grab => "grab",
            Toy::Tidy => "tidy",
            Toy::Swap => "swap inputs",
            Toy::Pin => "pin / unpin (shift = replace)",
            Toy::SelectAll => "select all",
        }
    }
    fn scope(self) -> &'static str {
        match self {
            Toy::Save | Toy::Undo | Toy::Redo => "app",
            _ => "graph",
        }
    }
    fn default(self) -> Binding {
        match self {
            Toy::Save => Binding::cmd(Key::S),
            Toy::Undo => Binding::cmd(Key::Z),
            Toy::Redo => Binding::cmd_shift(Key::Z),
            Toy::Frame => Binding::plain(Key::F),
            Toy::Delete => Binding::plain(Key::Delete),
            Toy::Grab => Binding::plain(Key::G),
            Toy::Tidy => Binding::plain(Key::T),
            Toy::Swap => Binding::plain(Key::S),
            Toy::Pin => Binding::plain(Key::Z),
            Toy::SelectAll => Binding::ctrl(Key::A),
        }
    }
    fn mode(self) -> MatchMode {
        if self == Toy::Pin { MatchMode::KeyOnly } else { MatchMode::Exact }
    }
    fn also(self) -> Option<Key> {
        (self == Toy::Delete).then_some(Key::Backspace)
    }
    fn also_mods(self) -> Option<Modifiers> {
        (self == Toy::SelectAll).then_some(Modifiers::COMMAND)
    }
}

fn press(key: Key, modifiers: Modifiers) -> Event {
    Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }
}

fn release(key: Key) -> Event {
    Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE }
}

#[test]
fn norm_folds_the_ctrl_mirror_into_command_and_drops_mac_cmd() {
    let m = norm(Modifiers { alt: false, ctrl: true, shift: true, mac_cmd: true, command: true });
    assert_eq!(m, Modifiers { alt: false, ctrl: false, shift: true, mac_cmd: false, command: true });
    // ctrl alone is the real mac ctrl key and stays.
    assert_eq!(norm(Modifiers::CTRL), Modifiers::CTRL);
    // What a mac ⌘K event carries, as the prefs will store it.
    assert_eq!(Binding::new(Key::K, Modifiers::MAC_CMD | Modifiers::COMMAND).normalized().mods, Modifiers::COMMAND);
}

#[test]
fn every_default_round_trips_through_the_prefs_form() {
    for a in Toy::all() {
        let s = format(a.default());
        let b = parse(&s).unwrap_or_else(|| panic!("{}: {s:?} did not parse", a.name()));
        assert_eq!(b, a.default(), "{}: {s:?} round-tripped wrong", a.name());
    }
}

#[test]
fn parse_is_forgiving_and_bounded() {
    assert_eq!(parse("cmd+shift+Z"), Some(Binding::cmd_shift(Key::Z)));
    assert_eq!(parse("Cmd + T"), Some(Binding::cmd(Key::T)));
    assert_eq!(parse("ctrl+K").map(|b| (b.mods.ctrl, b.mods.command)), Some((true, false)));
    assert_eq!(parse("cmd+"), None);
    assert_eq!(parse("notakey"), None);
    assert_eq!(parse(""), None);
}

#[test]
fn display_uses_the_platforms_glyphs() {
    let ctx = egui::Context::default();
    // (The symbols need the fonts, which exist after a first frame.)
    crate::snapshot::discard(ctx.run_ui(Default::default(), |_| {}));
    ctx.set_os(egui::os::OperatingSystem::Mac);
    // Symbols when the fonts have them, mac names otherwise; never "Ctrl".
    let mac = display(&ctx, Binding::cmd_shift(Key::Z));
    assert!(mac == "⌘⇧Z" || mac == "Shift+Cmd+Z", "{mac:?}");
    ctx.set_os(egui::os::OperatingSystem::Windows);
    assert_eq!(display(&ctx, Binding::cmd_shift(Key::Z)), "Ctrl+Shift+Z");
}

/// Exact matching reads the EVENT's modifiers: Z and cmd+Z are two
/// actions, a chord's letter does not leak into the plain binding, and a
/// key-only action takes every flavour.
#[test]
fn matching_reads_event_modifiers() {
    let map = Keymap::<Toy>::new();
    let mut i = InputState::default();
    i.events = vec![press(Key::S, Modifiers::COMMAND)];
    assert!(map.pressed(&i, Toy::Save));
    assert!(!map.pressed(&i, Toy::Swap), "cmd+S is not S");
    i.events = vec![press(Key::Z, Modifiers::SHIFT)];
    assert!(map.pressed(&i, Toy::Pin), "key-only takes any mods");
    assert!(!map.pressed(&i, Toy::Undo));
    i.events = vec![press(Key::Z, Modifiers::COMMAND | Modifiers::SHIFT)];
    assert!(map.pressed(&i, Toy::Redo));
    assert!(!map.pressed(&i, Toy::Undo), "the shift chord is redo, not undo");
    // The Delete row's fixed synonym.
    i.events = vec![press(Key::Backspace, Modifiers::NONE)];
    assert!(map.pressed(&i, Toy::Delete));
    // The fixed second chord: ctrl+A and cmd+A both select all.
    i.events = vec![press(Key::A, Modifiers::COMMAND)];
    assert!(map.pressed(&i, Toy::SelectAll));
    // Repeats are not presses: a held G must not flicker grab.
    i.events = vec![Event::Key { key: Key::G, physical_key: None, pressed: true, repeat: true, modifiers: Modifiers::NONE }];
    assert!(!map.pressed(&i, Toy::Grab));
}

#[test]
fn released_and_down_read_the_bare_key() {
    let map = Keymap::<Toy>::new();
    let mut i = InputState::default();
    i.events = vec![release(Key::Backspace)];
    assert!(map.released(&i, Toy::Delete), "the synonym releases too");
    assert!(!map.released(&i, Toy::Grab));
    i.events = vec![press(Key::G, Modifiers::NONE)];
    assert!(!map.down(&i, Toy::Grab), "an event alone is not a held key");
    i.keys_down.insert(Key::G);
    assert!(map.down(&i, Toy::Grab));
}

#[test]
fn consume_removes_the_event() {
    let map = Keymap::<Toy>::new();
    let mut i = InputState::default();
    i.events = vec![press(Key::S, Modifiers::COMMAND), press(Key::A, Modifiers::COMMAND)];
    assert!(map.consume(&mut i, Toy::Save));
    assert!(!map.consume(&mut i, Toy::Save), "gone");
    assert!(map.consume(&mut i, Toy::SelectAll), "the second chord consumes too");
    assert!(i.events.is_empty());
}

#[test]
fn overrides_are_diffs_and_round_trip() {
    let map = Keymap::<Toy>::new();
    assert!(map.overrides().is_empty());
    assert_eq!(map.binding(Toy::Tidy), Binding::plain(Key::T));

    map.rebind(Toy::Tidy, Binding::plain(Key::Y));
    assert_eq!(map.binding(Toy::Tidy), Binding::plain(Key::Y));
    assert_eq!(map.overrides().len(), 1);
    map.rebind(Toy::Tidy, Binding::plain(Key::T));
    assert!(map.overrides().is_empty(), "a default is not an override");

    // A rebind is stored normalized.
    map.rebind(Toy::Tidy, Binding::new(Key::T, Modifiers::MAC_CMD | Modifiers::COMMAND));
    assert_eq!(map.binding(Toy::Tidy), Binding::cmd(Key::T));
    map.reset(Toy::Tidy);
    assert!(!map.is_overridden(Toy::Tidy));

    // install: prefs strings in, same map out; junk skipped.
    let mut m = BTreeMap::new();
    m.insert("graph.tidy".to_string(), "cmd+T".to_string());
    m.insert("no.such.action".to_string(), "Q".to_string());
    m.insert("graph.grab".to_string(), "not a key".to_string());
    m.insert("app.save".to_string(), "cmd+S".to_string());
    map.install(&m);
    assert_eq!(map.binding(Toy::Tidy), Binding::cmd(Key::T));
    assert_eq!(map.binding(Toy::Grab), Binding::plain(Key::G), "junk skipped");
    let out = map.overrides();
    assert_eq!(out.len(), 1, "a stored default is no override: {out:?}");
    assert_eq!(out.get("graph.tidy").map(String::as_str), Some("cmd+T"));
    map.install(&BTreeMap::new());
    assert!(map.overrides().is_empty());
}

#[test]
fn conflicts_are_within_a_scope_and_key_only_is_loose() {
    let map = Keymap::<Toy>::new();
    // App S (save, cmd) and graph S (swap, plain) differ in chord; app Z
    // (undo) and graph Z (pin) coexist by design: scopes differ.
    assert!(map.conflicts().is_empty(), "defaults conflict: {:?}", map.conflicts());
    map.rebind(Toy::Tidy, Binding::plain(Key::S));
    let c = map.conflicts();
    assert!(c.contains(&(Toy::Tidy, Toy::Swap)) || c.contains(&(Toy::Swap, Toy::Tidy)), "T moved onto S: {c:?}");
    assert_eq!(map.conflict_of(Toy::Tidy), Some(Toy::Swap));
    map.reset_all();
    // A key-only action collides on the bare key: cmd+Z onto graph Z.
    map.rebind(Toy::Grab, Binding::cmd(Key::Z));
    assert_eq!(map.conflict_of(Toy::Grab), Some(Toy::Pin));
    // The fixed second chord counts: a graph action on cmd+A hits select-all.
    map.reset_all();
    map.rebind(Toy::Grab, Binding::cmd(Key::A));
    assert_eq!(map.conflict_of(Toy::Grab), Some(Toy::SelectAll));
    map.reset_all();
    // ...but not once select-all is rebound: the synonym does not follow.
    map.rebind(Toy::SelectAll, Binding::ctrl(Key::B));
    map.rebind(Toy::Grab, Binding::cmd(Key::A));
    assert_eq!(map.conflict_of(Toy::Grab), None);
}

#[test]
fn the_global_map_is_one_per_action_type() {
    let g = global::<Toy>();
    assert!(std::ptr::eq(g, global::<Toy>()));
    let mut i = InputState::default();
    i.events = vec![press(Key::F, Modifiers::NONE)];
    assert!(pressed(&i, Toy::Frame));
    assert_eq!(binding(Toy::Frame), Binding::plain(Key::F));
}

#[test]
fn every_default_key_is_on_the_keyboard() {
    let caps: Vec<Key> = keyboard_rows().iter().flatten().filter_map(|c| c.key).collect();
    for a in Toy::all() {
        assert!(
            caps.contains(&a.default().key) || a.also().is_some_and(|k| caps.contains(&k)),
            "{}: {} has no keycap",
            a.name(),
            a.default().key.name()
        );
    }
}

/// The rows stay a rectangle — 15 units each — or the columns shear the
/// first time someone edits the layout. (Row 0 is esc alone.)
#[test]
fn keyboard_rows_line_up() {
    for (n, row) in keyboard_rows().iter().enumerate().skip(1) {
        let w: f32 = row.iter().map(|c| c.w).sum();
        assert!((w - 15.0).abs() < 1e-3, "row {n} is {w} units wide");
    }
}

/// No missing-glyph boxes in the keys window: every non-ASCII character
/// in a string literal of the window's files exists in the proportional
/// family, and every keycap label draws in the face `cap_font` uses.
#[test]
fn every_drawn_character_has_a_glyph() {
    let ctx = egui::Context::default();
    crate::snapshot::discard(ctx.run_ui(Default::default(), |_| {}));
    let has = |font: &egui::FontId, c: char| ctx.fonts_mut(|f| f.has_glyph(font, c));
    let prop = egui::FontId::proportional(12.0);
    let mut missing = Vec::new();
    for src in [include_str!("ui.rs"), include_str!("caps.rs"), include_str!("board.rs")] {
        let (mut in_str, mut in_comment, mut escaped) = (false, false, false);
        let mut prev = '\0';
        for c in src.chars() {
            if in_comment {
                in_comment = c != '\n';
            } else if in_str {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == '\u{22}' {
                    in_str = false;
                } else if !c.is_ascii() && !has(&prop, c) {
                    missing.push(c);
                }
            } else if c == '/' && prev == '/' {
                in_comment = true;
            } else if c == '\u{22}' {
                in_str = true;
            }
            prev = c;
        }
    }
    missing.dedup();
    assert!(missing.is_empty(), "no glyph in the proportional font for {missing:?}");
    for cap in keyboard_rows().iter().flatten() {
        let font = cap_font(cap.label);
        for c in cap.label.chars() {
            assert!(has(&font, c), "keycap {:?}: no glyph for {c:?} in {font:?}", cap.label);
        }
    }
}

#[test]
fn search_matches_name_scope_and_key() {
    let map = Keymap::<Toy>::new();
    assert!(row_matches(&map, Toy::Save, "save"));
    assert!(row_matches(&map, Toy::Save, "cmd+s"));
    assert!(row_matches(&map, Toy::Frame, "graph"));
    assert!(row_matches(&map, Toy::Frame, ""));
    assert!(!row_matches(&map, Toy::Frame, "zzz"));
    let keys = KeysUi::<Toy>::new().with_fixed(vec![Fixed {
        how: "hold S",
        label: "rgb slider: slide saturation",
        key: Some(Key::S),
        mods: Modifiers::NONE,
    }]);
    assert!(keys.pick_matches(&map, ui::Pick::Fixed(0), "saturation"));
    assert!(keys.pick_matches(&map, ui::Pick::Fixed(0), "hold s"));
    assert!(keys.pick_matches(&map, ui::Pick::Fixed(0), "fixed"));
    assert!(!keys.pick_matches(&map, ui::Pick::Fixed(0), "tidy"));
}

/// Arm a capture, press a chord: the binding moves, the map says a
/// capture is live until the key comes back up, and Escape cancels.
#[test]
fn a_capture_takes_the_next_chord_and_holds_until_release() {
    let ctx = egui::Context::default();
    let map = Keymap::<Toy>::new();
    let mut keys = KeysUi::<Toy>::new();
    keys.open = true;
    let frame = |keys: &mut KeysUi<Toy>, events: Vec<Event>| {
        let mut raw = egui::RawInput { events, ..Default::default() };
        raw.events.insert(0, Event::ModifiersChanged(Modifiers::NONE));
        let mut changed = false;
        crate::snapshot::discard(ctx.run_ui(raw, |ui| changed = keys.window(ui.ctx(), &map)));
        changed
    };
    assert!(!frame(&mut keys, vec![]));
    keys.begin_capture(Toy::Tidy);
    assert_eq!(keys.capturing(), Some(Toy::Tidy));
    assert!(frame(&mut keys, vec![press(Key::Y, Modifiers::COMMAND)]), "a binding changed");
    assert_eq!(map.binding(Toy::Tidy), Binding::cmd(Key::Y));
    assert!(map.capturing(), "held until Y is released");
    assert!(!frame(&mut keys, vec![release(Key::Y)]));
    assert!(!map.capturing());
    // Escape cancels without a change.
    keys.begin_capture(Toy::Grab);
    assert!(!frame(&mut keys, vec![press(Key::Escape, Modifiers::NONE), release(Key::Escape)]));
    assert_eq!(map.binding(Toy::Grab), Binding::plain(Key::G));
    assert!(!map.capturing());
    // Key-only actions take the bare key whatever was held.
    keys.begin_capture(Toy::Pin);
    assert!(frame(&mut keys, vec![press(Key::P, Modifiers::SHIFT), release(Key::P)]));
    assert_eq!(map.binding(Toy::Pin), Binding::plain(Key::P));
}

/// The board lights what the list selects — read off the pixels of the
/// window's body, rasterized headlessly. Selecting undo (cmd+Z) lights the
/// Z cap AND both cmd caps in the app scope's blue; a key it does not use
/// stays neutral.
#[test]
fn selecting_a_command_lights_its_chord() {
    let rows = keyboard_rows();
    let caps = |want: &dyn Fn(&Cap) -> bool| -> Vec<egui::Rect> {
        let mut out = Vec::new();
        for (ri, row) in rows.iter().enumerate() {
            for (ci, c) in row.iter().enumerate() {
                if want(c) {
                    out.push(cap_rect(&rows, ri, ci));
                }
            }
        }
        out
    };
    let map = Keymap::<Toy>::new();
    let mut keys = KeysUi::<Toy>::new()
        .with_scope_colour(|s| if s == "app" { Color32::from_rgb(40, 90, 255) } else { Color32::from_rgb(60, 200, 90) });
    keys.select(Toy::Undo);
    let shot = crate::snapshot::shoot(1360.0, 600.0, |ui| {
        keys.show(ui, &map);
    });
    shot.save("keys-window");
    assert!(keys.board.is_positive(), "the board was never laid out");
    let origin = keys.board.min.to_vec2();
    // Below centre and left of it, in proportion to the cap so it stays
    // clear of the stroke and the dots at any KB_SCALE.
    let sample = |r: egui::Rect| {
        let r = r.translate(origin);
        shot.at(r.min.x + r.width() * 0.25, r.center().y + r.height() * 0.2)
    };
    let blue = |px: [u8; 3]| px[2] as i32 - px[0] as i32 > 25;
    let neutral = |px: [u8; 3]| (px[2] as i32 - px[0] as i32).abs() <= 3;

    let z = caps(&|c| c.key == Some(Key::Z));
    assert_eq!(z.len(), 1);
    assert!(blue(sample(z[0])), "Z cap not lit: {:?}", sample(z[0]));
    let cmd = caps(&|c| c.modifier == Some(ModKey::Cmd));
    assert_eq!(cmd.len(), 2, "both cmd caps");
    for r in cmd {
        assert!(blue(sample(r)), "cmd cap not lit: {:?}", sample(r));
    }
    for r in caps(&|c| c.key == Some(Key::J) || c.modifier == Some(ModKey::Shift)) {
        assert!(neutral(sample(r)), "a cap undo does not use is lit: {:?}", sample(r));
    }
}
