//! Tests named as sentences. The ones that move a live table use their own
//! table (`app`) or are `#[ignore]`d: the core table's LIVE is shared.

use super::*;
use std::collections::BTreeMap;

/// An app's table, as wobble will declare one: the core groups plus its own.
mod app {
    crate::slots! {
        core;
        "node graph" {
            /// The canvas behind the nodes.
            graph_bg: "background" = shade(Slot::bg, gray(0x14));
            wire: "wires" = rgb(0xb8, 0x8a, 0x40);
            wire_held: "wire held" = like(Slot::highlight);
            scrim: "scrim" [alpha] = gray(0);
        }
        "status" {
            cut: "cut" = shade(Slot::error, rgb(0xe0, 0x50, 0x50));
        }
    }
}

#[test]
fn links_point_backwards() {
    for t in [&TABLE, &app::TABLE] {
        for (i, d) in t.defs.iter().enumerate() {
            if let Some(p) = d.base.parent() {
                assert!(p < i, "{} links forward to {}", d.id, t.defs[p].id);
            }
        }
    }
}

#[test]
fn ids_and_labels_are_unique() {
    for t in [&TABLE, &app::TABLE] {
        let mut ids = std::collections::HashSet::new();
        let mut labels = std::collections::HashSet::new();
        for d in t.defs {
            assert!(ids.insert(d.id), "duplicate id {}", d.id);
            assert!(labels.insert((d.group, d.label)), "duplicate label {} in {}", d.label, d.group);
        }
    }
}

#[test]
fn nothing_overridden_is_every_literal() {
    let c = TABLE.resolve(&[]);
    for (i, s) in ALL.iter().enumerate() {
        assert_eq!(c[i], TABLE.default_of(i), "{}", DEFS[i].id);
        assert_eq!(get(*s), TABLE.default_of(i), "LIVE starts at the defaults: {}", DEFS[i].id);
    }
}

#[test]
fn links_follow_their_slot() {
    let mut over = vec![None; N];
    over[Slot::bg as usize] = Some(Color32::from_rgb(40, 50, 90));
    over[Slot::button as usize] = Some(Color32::from_gray(250));
    let c = TABLE.resolve(&over);
    // shade: own default + the parent's move, per channel.
    assert_eq!(c[Slot::field as usize], Color32::from_rgb(23, 33, 73));
    // clamped at the top, not wrapped.
    assert_eq!(c[Slot::button_hover as usize], Color32::from_gray(255));
    // like: exactly the parent.
    assert_eq!(c[Slot::curve_line as usize], c[Slot::handle as usize]);
    // an override of the child wins over the link.
    over[Slot::field as usize] = Some(Color32::RED);
    assert_eq!(TABLE.resolve(&over)[Slot::field as usize], Color32::RED);
}

#[test]
fn the_alpha_flag_reads_the_literal() {
    assert!(!DEFS[Slot::bg as usize].alpha);
    assert!(DEFS[Slot::hover_fill as usize].alpha, "translucent default");
    assert!(DEFS[Slot::hud_bg as usize].alpha, "[alpha] on an opaque one");
    assert!(!DEFS[Slot::curve_line as usize].alpha, "like() answers opaque");
}

#[test]
fn hex_round_trips_with_and_without_alpha() {
    assert_eq!(hex(Color32::from_rgb(0xff, 0x88, 0x00)), "#ff8800");
    assert_eq!(hex(Color32::from_rgba_unmultiplied(0x10, 0x20, 0x30, 0x40)), "#10203040");
    assert_eq!(parse_hex("ff8800"), Some(Color32::from_rgb(0xff, 0x88, 0x00)));
    assert_eq!(parse_hex(" #10203040 "), Some(Color32::from_rgba_unmultiplied(0x10, 0x20, 0x30, 0x40)));
    assert_eq!(parse_hex("chartreuse"), None);
}

#[test]
fn prefs_round_trip() {
    let mut map = BTreeMap::new();
    map.insert("accent".to_string(), "#ff8800".to_string());
    map.insert("field".to_string(), "#10203040".to_string());
    map.insert("not_a_slot".to_string(), "#ffffff".to_string());
    map.insert("text".to_string(), "chartreuse".to_string());
    // What the slot has anyway is not stored.
    map.insert("border".to_string(), "#3c3c3c".to_string());
    let over = TABLE.parse_overrides(&map);
    assert_eq!(over[Slot::accent as usize], Some(Color32::from_rgb(0xff, 0x88, 0x00)));
    assert_eq!(over[Slot::text as usize], None);
    assert_eq!(over[Slot::border as usize], None);
    let back = TABLE.format_overrides(&over);
    assert_eq!(back.len(), 2);
    assert_eq!(back["accent"], "#ff8800");
    assert_eq!(back["field"], "#10203040");
    assert_eq!(TABLE.parse_overrides(&back), over);
}

#[test]
fn default_visuals_are_egui_dark() {
    let mut v = egui::Visuals::dark();
    TABLE.write_visuals_from(&mut v, &TABLE.resolve(&[]));
    let mut want = egui::Visuals::dark();
    want.warn_fg_color = TABLE.default_of(Slot::warn as usize);
    want.error_fg_color = TABLE.default_of(Slot::error as usize);
    assert_eq!(v, want);
}

#[test]
fn an_app_table_is_core_plus_its_own_and_has_the_same_api() {
    use app::Slot as A;
    // The core slots come first, under the same ids and defaults.
    assert_eq!(app::N, N + 5);
    for i in 0..N {
        assert_eq!(app::DEFS[i].id, DEFS[i].id);
        assert_eq!(app::TABLE.default_of(i), TABLE.default_of(i));
    }
    assert!(app::DEFS[A::scrim as usize].alpha);
    // A second "status" run joins the first group in the window's eyes.
    assert_eq!(app::DEFS[A::cut as usize].group, "status");
    assert_eq!(app::graph_bg(), Color32::from_gray(0x14));
    assert_eq!(app::wire_held(), app::highlight());

    // set / overrides / install / reset on the live table.
    app::set(A::bg, Color32::from_rgb(40, 50, 90));
    assert!(app::is_overridden(A::bg));
    assert_eq!(app::graph_bg(), Color32::from_rgb(33, 43, 83), "shade follows the live parent");
    assert_eq!(app::field(), Color32::from_rgb(23, 33, 73));
    let saved = app::overrides();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved["bg"], "#28325a");
    // Setting a child to what it inherits is no override.
    app::set(A::field, Color32::from_rgb(23, 33, 73));
    assert!(!app::is_overridden(A::field));
    app::reset_all();
    assert!(!app::any_overridden());
    assert_eq!(app::graph_bg(), Color32::from_gray(0x14));
    app::install(&saved);
    assert_eq!(app::bg(), Color32::from_rgb(40, 50, 90));
    app::reset(A::bg);
    assert_eq!(app::bg(), Color32::from_gray(27));
}

#[test]
fn sync_writes_visuals_once_per_generation() {
    let ctx = egui::Context::default();
    let gen = egui::Id::new("imfram-theme-generation");
    app::sync(&ctx);
    let g1 = ctx.data(|d| d.get_temp::<u32>(gen)).expect("generation noted on the context");
    app::sync(&ctx);
    assert_eq!(ctx.data(|d| d.get_temp::<u32>(gen)), Some(g1), "nothing moved, nothing written");
    app::set(app::Slot::accent, Color32::from_rgb(1, 2, 3));
    app::sync(&ctx);
    assert_ne!(ctx.data(|d| d.get_temp::<u32>(gen)), Some(g1));
    let got = ctx.style_of(egui::Theme::Dark).visuals.selection.bg_fill;
    assert_eq!(got, Color32::from_rgb(1, 2, 3));
    app::reset(app::Slot::accent);
}

/// `cargo test --lib theme_window_render -- --ignored` renders the colours
/// window twice, text and all — the default theme, then a recoloured one —
/// to `target/theme-window.png` and `target/theme-window-custom.png`. It
/// moves the core LIVE table, so it is ignored and runs alone.
#[test]
#[ignore]
#[cfg(not(target_arch = "wasm32"))]
fn theme_window_render() {
    crate::uishot::set_style(sync);
    render_window("theme-window");
    let custom: BTreeMap<String, String> = [
        ("bg", "#1c2230"),
        ("border", "#3a4660"),
        ("text", "#a8b4cc"),
        ("button", "#34405a"),
        ("accent", "#6d3fd0"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    install(&custom);
    render_window("theme-window-custom");
    install(&BTreeMap::new());
}

#[cfg(not(target_arch = "wasm32"))]
fn render_window(name: &str) {
    let mut theme_ui = ThemeUi::default();
    theme_ui.open = true;
    crate::uishot::render(name, 420.0, 640.0, 2.0, 4, |ui, _| {
        egui::Frame::window(ui.style()).show(ui, |ui| theme_ui.contents(ui, &TABLE));
    });
}
