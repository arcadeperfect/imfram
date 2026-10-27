//! The composer headless: keys, rows drawn from their sources, a drop.

use super::*;
use crate::snapshot::discard;
use crate::widgets::group::{group, Start};
use egui::{pos2, Event, Modifiers, PointerButton, Pos2, RawInput, UiBuilder};

const W: f32 = 600.0;

fn frame(ctx: &egui::Context, events: Vec<Event>, modifiers: Modifiers, mut build: impl FnMut(&mut Ui)) {
    let mut events = events;
    events.insert(0, Event::ModifiersChanged(modifiers));
    let input = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(W, 400.0))), events, ..Default::default() };
    discard(ctx.run_ui(input, |ui| build(ui)));
}

#[test]
fn a_key_is_its_scopes_then_its_name_and_repeats_count() {
    let ctx = egui::Context::default();
    let mut c = Composer::default();
    for k in ["Sky/Colours/intensity", "Sky/Colours/intensity#2", "loose"] {
        c.push(Entry { key: k.into(), name: k.into(), label: None });
    }
    // Drawn into a slot, a control is somewhere to be seen; drawn for
    // nothing, it is clipped to nothing.
    let mut hits = Vec::new();
    frame(&ctx, vec![], Modifiers::NONE, |ui| {
        hits.clear();
        c.show(ui, |ui| {
            scope(ui, "Sky", |ui| {
                group(ui, "Gradient", "", Start::Folded, |ui| {
                    item(ui, "intensity", |ui| assert!(!ui.clip_rect().is_positive()));
                });
                group(ui, "Colours", "", Start::Open, |ui| {
                    for i in 0..2 {
                        item(ui, "intensity", |ui| {
                            if ui.clip_rect().is_positive() {
                                hits.push(i);
                            }
                        });
                    }
                });
            });
            item(ui, "loose", |ui| {
                if ui.clip_rect().is_positive() {
                    hits.push(9);
                }
            });
        });
    });
    assert_eq!(hits, vec![0, 1, 9], "each key drawn into its slot");
}

/// A composer over a page with a flag deep in a folded group; where the
/// flag's box was drawn, and the flag.
struct Rig {
    ctx: egui::Context,
    composer: Composer,
    flag: bool,
    gain: f32,
    drawn_at: Option<Rect>,
}

impl Rig {
    fn new(keys: &[&str]) -> Self {
        let mut composer = Composer::default();
        for k in keys {
            composer.push(Entry { key: (*k).into(), name: (*k).into(), label: None });
        }
        Self { ctx: egui::Context::default(), composer, flag: false, gain: 0.5, drawn_at: None }
    }

    fn page(ui: &mut Ui, flag: &mut bool, gain: &mut f32, drawn_at: &mut Option<Rect>) {
        scope(ui, "Page", |ui| {
            group(ui, "Deep", "", Start::Folded, |ui| {
                item(ui, "flag", |ui| {
                    let r = ui.checkbox(flag, "flag");
                    if harvesting(ui) {
                        *drawn_at = Some(r.rect);
                    }
                });
                crate::widgets::slider(ui, gain, (0.0, 1.0), "gain", false, "");
            });
        });
    }

    /// The composer on the right half, the page on the left.
    fn run(&mut self, events: Vec<Event>, modifiers: Modifiers) -> bool {
        let Rig { ctx, composer, flag, gain, drawn_at } = self;
        let mut changed = false;
        frame(ctx, events, modifiers, |ui| {
            *drawn_at = None;
            let full = ui.max_rect();
            let (left, right) = full.split_left_right_at_fraction(0.5);
            let mut l = ui.new_child(UiBuilder::new().max_rect(left).id_salt("left"));
            Self::page(&mut l, flag, gain, drawn_at);
            let mut r = ui.new_child(UiBuilder::new().max_rect(right).id_salt("right"));
            changed |= composer.show(&mut r, |ui| Self::page(ui, flag, gain, drawn_at));
        });
        changed
    }
}

fn press(at: Pos2, pressed: bool) -> Event {
    Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE }
}

#[test]
fn a_composed_control_is_the_real_one_even_from_a_folded_group() {
    let mut rig = Rig::new(&["Page/Deep/flag"]);
    rig.run(vec![], Modifiers::NONE);
    rig.run(vec![], Modifiers::NONE);
    let at = rig.drawn_at.expect("the flag was drawn into its row").center();
    assert!(at.x > W / 2.0, "in the composer, on the right: {at:?}");
    rig.run(vec![Event::PointerMoved(at)], Modifiers::NONE);
    rig.run(vec![press(at, true)], Modifiers::NONE);
    rig.run(vec![press(at, false)], Modifiers::NONE);
    assert!(rig.flag, "a click on the composed box set the page's own flag");
}

#[test]
fn a_control_not_reached_keeps_its_row() {
    let mut rig = Rig::new(&["Page/Gone/thing", "Page/Deep/flag"]);
    rig.run(vec![], Modifiers::NONE);
    assert_eq!(rig.composer.entries.len(), 2);
    assert!(rig.drawn_at.is_some());
}

#[test]
fn alt_drag_from_the_page_drops_a_control_in() {
    let mut rig = Rig::new(&[]);
    let ctx = rig.ctx.clone();
    // Where a label on the page, on the left, is drawn.
    let mut found = None;
    frame(&ctx, vec![], Modifiers::NONE, |ui| {
        let mut l = ui.new_child(UiBuilder::new().max_rect(ui.max_rect().split_left_right_at_fraction(0.5).0).id_salt("probe"));
        scope(&mut l, "Page", |ui| {
            item(ui, "probe", |ui| found = Some(ui.label("probe").rect));
        });
    });
    let from = found.unwrap().center();
    let to = pos2(W * 0.75, 60.0);
    let alt = Modifiers::ALT;
    let probe = |rig: &mut Rig, events: Vec<Event>| {
        let Rig { ctx, composer, .. } = rig;
        let mut changed = false;
        frame(ctx, events, alt, |ui| {
            let (left, right) = ui.max_rect().split_left_right_at_fraction(0.5);
            let mut l = ui.new_child(UiBuilder::new().max_rect(left).id_salt("probe"));
            scope(&mut l, "Page", |ui| {
                item(ui, "probe", |ui| {
                    ui.label("probe");
                });
            });
            let mut r = ui.new_child(UiBuilder::new().max_rect(right).id_salt("right"));
            changed |= composer.show(&mut r, |ui| {
                scope(ui, "Page", |ui| {
                    item(ui, "probe", |ui| {
                        ui.label("probe");
                    });
                });
            });
        });
        changed
    };
    probe(&mut rig, vec![Event::PointerMoved(from)]);
    probe(&mut rig, vec![press(from, true)]);
    for i in 1..=6 {
        let t = i as f32 / 6.0;
        probe(&mut rig, vec![Event::PointerMoved(from + (to - from) * t)]);
    }
    let changed = probe(&mut rig, vec![press(to, false)]);
    assert!(changed);
    assert_eq!(rig.composer.entries, vec![Entry { key: "Page/probe".into(), name: "probe".into(), label: None }]);
}

#[test]
fn insert_and_reorder_keep_one_of_each() {
    let e = |k: &str| Entry { key: k.into(), name: k.into(), label: None };
    let mut c = Composer::default();
    for k in ["a", "b", "c"] {
        assert!(c.push(e(k)));
    }
    assert!(!c.push(e("a")));
    c.insert(0, e("c"));
    assert_eq!(c.entries.iter().map(|e| e.key.as_str()).collect::<Vec<_>>(), ["c", "a", "b"]);
    c.reorder(0, 3);
    assert_eq!(c.entries.iter().map(|e| e.key.as_str()).collect::<Vec<_>>(), ["a", "b", "c"]);
    c.reorder(2, 0);
    assert_eq!(c.entries.iter().map(|e| e.key.as_str()).collect::<Vec<_>>(), ["c", "a", "b"]);
    assert!(c.remove("a") && !c.remove("a"));
}

#[test]
fn a_composer_round_trips_through_json() {
    let mut c = Composer::default();
    c.push(Entry { key: "Sky/Gradient/gradient follow".into(), name: "gradient follow".into(), label: None });
    let s = serde_json::to_string(&c).unwrap();
    assert_eq!(serde_json::from_str::<Composer>(&s).unwrap(), c);
}

/// To look at: a page on the left, a composer holding three of its
/// controls on the right (`target/composer.png`).
#[test]
#[ignore]
fn shot() {
    let mut c = Composer::default();
    for (k, n) in [("Page/Light/sun", "sun"), ("Page/Sky/sky brightness", "sky brightness"), ("Page/Light/Shadows", "Shadows"), ("Page/Old/gone", "gone")] {
        c.push(Entry { key: k.into(), name: n.into(), label: None });
    }
    let (mut sun, mut sky, mut soft, mut shadows) = (0.8, 1.2, 0.3, true);
    let page = |ui: &mut Ui, sun: &mut f32, sky: &mut f32, soft: &mut f32, shadows: &mut bool| {
        scope(ui, "Page", |ui| {
            group(ui, "Light", "", Start::Open, |ui| {
                crate::widgets::slider(ui, sun, (0.0, 2.0), "sun", false, "");
                crate::widgets::group::switched(ui, shadows, "Shadows", "", Start::Open, |ui| {
                    crate::widgets::slider(ui, soft, (0.0, 1.0), "softness", false, "");
                });
            });
            group(ui, "Sky", "", Start::Folded, |ui| {
                crate::widgets::slider(ui, sky, (0.0, 4.0), "sky brightness", false, "");
            });
        });
    };
    crate::uishot::render("composer", 760.0, 220.0, 2.0, 4, |ui, _| {
        let (left, right) = ui.max_rect().shrink(8.0).split_left_right_at_fraction(0.5);
        let mut l = ui.new_child(UiBuilder::new().max_rect(left.shrink2(vec2(6.0, 0.0))).id_salt("left"));
        page(&mut l, &mut sun, &mut sky, &mut soft, &mut shadows);
        let mut r = ui.new_child(UiBuilder::new().max_rect(right.shrink2(vec2(6.0, 0.0))).id_salt("right"));
        c.show(&mut r, |ui| page(ui, &mut sun, &mut sky, &mut soft, &mut shadows));
    });
}

#[test]
fn a_hidden_run_of_a_page_leaves_its_props_rects_alone() {
    use crate::props::{rects, Sheet};
    let ctx = egui::Context::default();
    let mut c = Composer::default();
    c.push(Entry { key: "Page/gain".into(), name: "gain".into(), label: None });
    let mut v = 0.5;
    let mut page = None;
    for _ in 0..2 {
        frame(&ctx, vec![], Modifiers::NONE, |ui| {
            let (l, r) = ui.max_rect().split_left_right_at_fraction(0.5);
            ui.scope_builder(UiBuilder::new().max_rect(l), |ui| {
                scope(ui, "Page", |ui| {
                    Sheet::new(ui).slider("gain", &mut v, (0.0, 1.0), false);
                });
            });
            page = rects(ui.ctx(), egui::Id::new("gain")).map(|r| r.row);
            ui.scope_builder(UiBuilder::new().max_rect(r), |ui| {
                c.show(ui, |ui| {
                    scope(ui, "Page", |ui| {
                        Sheet::new(ui).slider("gain", &mut v, (0.0, 1.0), false);
                    });
                });
            });
        });
    }
    let after = rects(&ctx, egui::Id::new("gain")).map(|r| r.row);
    assert!(page.is_some());
    assert_eq!(page, after, "the composer's run moved the page row's rect");
}

#[test]
fn a_renamed_row_hands_its_control_the_new_label() {
    let ctx = egui::Context::default();
    let mut c = Composer::default();
    c.push(Entry { key: "Page/gain".into(), name: "gain".into(), label: Some("boost".into()) });
    let mut seen = Vec::new();
    for _ in 0..2 {
        frame(&ctx, vec![], Modifiers::NONE, |ui| {
            seen.clear();
            item_with(ui, "outside", |_, at| seen.push(("page", at.label.to_owned())));
            c.show(ui, |ui| {
                scope(ui, "Page", |ui| {
                    item_with(ui, "gain", |ui, at| {
                        if ui.clip_rect().is_positive() {
                            seen.push(("slot", at.label.to_owned()));
                        }
                    });
                });
            });
        });
    }
    assert_eq!(seen, vec![("page", "outside".to_owned()), ("slot", "boost".to_owned())]);
}

#[test]
fn an_alt_drag_lands_in_a_composer_in_another_window() {
    // Two windows side by side on the screen: the page at x 0, the
    // composer's at x 1000 (physical pixels, scale 2).
    let (page, other) = (egui::Context::default(), egui::Context::default());
    set_screen_origin(&page, [0.0, 0.0], 2.0);
    set_screen_origin(&other, [1000.0, 0.0], 2.0);
    let mut c = Composer::default();
    let mut found = None;
    let draw_page = |events: Vec<Event>, found: &mut Option<Rect>| {
        frame(&page, events, Modifiers::ALT, |ui| {
            scope(ui, "Page", |ui| {
                item(ui, "probe", |ui| *found = Some(ui.label("probe").rect));
            });
        });
    };
    let mut draw_other = |c: &mut Composer| {
        let mut changed = false;
        frame(&other, vec![], Modifiers::NONE, |ui| {
            changed |= c.show(ui, |ui| {
                scope(ui, "Page", |ui| {
                    item(ui, "probe", |ui| {
                        ui.label("probe");
                    });
                });
            });
        });
        changed
    };
    draw_page(vec![], &mut found);
    draw_other(&mut c);
    let from = found.unwrap().center();
    // 1000 px right of the page's origin is the other window's left edge:
    // 500 points at scale 2. Aim 60 points into it.
    let to = pos2(560.0, 30.0);
    draw_page(vec![Event::PointerMoved(from)], &mut found);
    draw_page(vec![press(from, true)], &mut found);
    for i in 1..=6 {
        draw_page(vec![Event::PointerMoved(from + (to - from) * (i as f32 / 6.0))], &mut found);
        draw_other(&mut c);
    }
    draw_page(vec![press(to, false)], &mut found);
    assert!(carrying(), "let go over another window: still on offer");
    assert!(draw_other(&mut c), "the other window's composer takes it");
    assert_eq!(c.entries, vec![Entry { key: "Page/probe".into(), name: "probe".into(), label: None }]);
    assert!(!carrying(), "taken once");
}

#[test]
fn a_control_in_a_scroll_area_is_reached() {
    let ctx = egui::Context::default();
    let mut c = Composer::default();
    c.push(Entry { key: "Sim/water".into(), name: "water".into(), label: None });
    let mut hit = false;
    for _ in 0..3 {
        frame(&ctx, vec![], Modifiers::NONE, |ui| {
            hit = false;
            c.show(ui, |ui| {
                scope(ui, "Sim", |ui| {
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                        ui.add_space(400.0);
                        item(ui, "water", |ui| {
                            if ui.clip_rect().is_positive() {
                                hit = true;
                            }
                        });
                    });
                });
            });
        });
    }
    assert!(hit, "drawn into its slot from inside a scroll area");
}

#[test]
fn a_second_copy_of_a_panel_keys_its_controls_as_the_first() {
    let ctx = egui::Context::default();
    let mut keys = Vec::new();
    frame(&ctx, vec![], Modifiers::NONE, |ui| {
        for _ in 0..2 {
            scope(ui, "Sim", |ui| {
                item_with(ui, "water", |_, at| keys.push(at.key.to_owned()));
                item_with(ui, "water", |_, at| keys.push(at.key.to_owned()));
            });
        }
    });
    assert_eq!(keys, ["Sim/water", "Sim/water#2", "Sim/water", "Sim/water#2"]);
}

#[test]
fn a_divider_is_a_row_of_its_own_and_takes_no_control() {
    let ctx = egui::Context::default();
    let mut c = Composer::default();
    let key = c.push_divider("sky");
    c.push(Entry { key: "loose".into(), name: "loose".into(), label: None });
    assert!(c.entries[0].is_divider() && c.entries[0].title() == "sky");
    let mut hits = 0;
    frame(&ctx, vec![], Modifiers::NONE, |ui| {
        hits = 0;
        c.show(ui, |ui| {
            item(ui, "loose", |ui| {
                if ui.clip_rect().is_positive() {
                    hits += 1;
                }
            });
        });
    });
    assert_eq!(hits, 1, "the control still lands in its row");
    let grips = grip_rects(&ctx);
    assert_eq!(grips.len(), 2);
    assert_eq!(grips[0].0, key);
    assert!(grips[0].1.bottom() <= grips[1].1.top() + 0.5, "the divider sits above");
    c.reorder(0, 2);
    assert_eq!(c.entries[1].key, key, "moved like any row");
    assert_eq!(c.push_divider("b"), format!("{DIVIDER}2"));
}
