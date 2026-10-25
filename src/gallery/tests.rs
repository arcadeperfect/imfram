use std::collections::BTreeSet;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use egui::{pos2, vec2, Color32, ColorImage, Context, Event, Modifiers, Pos2};

use super::*;
use crate::outliner::drive::Drive;

// ---- a toy source ----------------------------------------------------------

struct Toy {
    n: usize,
    frames: u32,
    sections: Vec<usize>,
}

impl Items for Toy {
    type Src = u32;
    fn len(&self) -> usize {
        self.n
    }
    fn id(&self, i: usize) -> u64 {
        i as u64 * 10 + 1
    }
    fn src(&self, i: usize) -> u32 {
        i as u32
    }
    fn label(&self, i: usize) -> String {
        format!("item {i}")
    }
    fn frames(&self, _i: usize) -> u32 {
        self.frames
    }
    fn section(&self, i: usize) -> Option<String> {
        self.sections.contains(&i).then(|| format!("section at {i}"))
    }
}

fn toy(n: usize) -> Toy {
    Toy { n, frames: 1, sections: vec![] }
}

fn pixels() -> ColorImage {
    ColorImage::new([4, 3], vec![Color32::RED; 12])
}

/// A cache whose loader notes every request.
fn noting(threads: usize) -> (Thumbs<u32>, Arc<Mutex<Vec<Request<u32>>>>) {
    let seen = Arc::new(Mutex::new(vec![]));
    let s = seen.clone();
    let t = Thumbs::new(threads, move |r: &Request<u32>| {
        s.lock().unwrap().push(r.clone());
        Ok(pixels())
    });
    (t, seen)
}

/// Frames until the workers have nothing left.
fn settle(d: &mut Drive, build: &mut dyn FnMut(&mut egui::Ui) -> Stats) {
    let t = Instant::now();
    loop {
        let s = d.frame(vec![], build);
        if s.queued == 0 && s.running == 0 && s.waiting_upload == 0 && t.elapsed() > Duration::from_millis(30) {
            return;
        }
        assert!(t.elapsed() < Duration::from_secs(5), "the workers never finished: {s:?}");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn centre(g: &Gallery, i: usize) -> Pos2 {
    g.layout().unwrap().tile_rect(i).center() + g.origin.to_vec2()
}

// ---- layout ---------------------------------------------------------------

#[test]
fn rows_reach_both_edges_and_no_tile_crosses_one() {
    let m = Metrics::default();
    for w in [200.0, 333.0, 512.0, 1000.0] {
        let l = Layout::new(w, 100.0, m, 40, &|_| false);
        let last = l.tile_rect(l.cols - 1);
        assert!((last.right() - (w - m.gap)).abs() < 0.01, "width {w}: the row ends at {}", last.right());
        assert!(l.tile.x >= 100.0 && l.tile.x < 100.0 * 2.0 + m.gap, "tiles only grow, by under a tile");
    }
}

#[test]
fn a_section_starts_a_new_row_under_its_header() {
    let l = Layout::new(500.0, 100.0, Metrics::default(), 10, &|i| i == 0 || i == 3);
    assert_eq!(l.rows[0], Row::Header { first: 0 });
    assert_eq!(l.rows[1], Row::Tiles { start: 0, end: 3 });
    assert_eq!(l.rows[2], Row::Header { first: 3 });
    assert!(l.tile_rect(3).top() > l.tile_rect(2).bottom() + Metrics::default().header_h);
}

#[test]
fn the_item_under_a_point_is_the_one_whose_cell_holds_it_and_gaps_are_nobodys() {
    let l = Layout::new(500.0, 100.0, Metrics::default(), 20, &|_| false);
    for i in 0..20 {
        assert_eq!(l.at(l.cell_rect(i).center()), Some(i));
    }
    let gap = pos2(l.tile_rect(0).right() + 2.0, l.tile_rect(0).center().y);
    assert_eq!(l.at(gap), None);
}

#[test]
fn up_and_down_keep_the_column_and_land_in_a_short_rows_last_tile() {
    let l = Layout::new(500.0, 100.0, Metrics::default(), 10, &|_| false);
    assert_eq!(l.cols, 4);
    assert_eq!(l.vertical(1, 1), Some(5));
    assert_eq!(l.vertical(7, 1), Some(9), "row three holds 8 and 9 only");
    assert_eq!(l.vertical(1, -1), None);
}

#[test]
fn only_the_rows_in_view_are_listed() {
    let l = Layout::new(500.0, 100.0, Metrics::default(), 400, &|_| false);
    let r = l.items_in(1000.0, 1300.0);
    assert!(r.start > 0 && r.len() <= 4 * 3, "{r:?}");
    assert!(r.clone().all(|i| l.cell_rect(i).bottom() >= 1000.0 && l.cell_rect(i).top() <= 1300.0));
}

// ---- the cache ------------------------------------------------------------

#[test]
fn only_the_tiles_in_view_and_a_screen_beyond_are_loaded() {
    let items = toy(2000);
    let (mut thumbs, seen) = noting(2);
    let mut g = Gallery::new();
    g.size = 80.0;
    let mut d = Drive::new(400.0, 300.0);
    settle(&mut d, &mut |ui| {
        g.show(ui, &items, &mut thumbs);
        thumbs.stats()
    });
    let asked: BTreeSet<u32> = seen.lock().unwrap().iter().map(|r| r.src).collect();
    let vis = g.visible();
    assert!(vis.clone().all(|i| asked.contains(&(i as u32))), "every visible tile was loaded");
    assert!(asked.len() < 60, "{} of 2000 were loaded", asked.len());
    assert!(thumbs.stats().textures >= vis.len());
}

#[test]
fn a_tile_scrolled_past_before_a_worker_reached_it_is_never_loaded() {
    let (gate_tx, gate_rx) = mpsc::channel::<()>();
    let gate = Mutex::new(gate_rx);
    let seen = Arc::new(Mutex::new(vec![]));
    let s = seen.clone();
    let mut t: Thumbs<u32> = Thumbs::new(1, move |r: &Request<u32>| {
        s.lock().unwrap().push(r.src);
        let _ = gate.lock().unwrap().recv_timeout(Duration::from_secs(2));
        Ok(pixels())
    });
    let ctx = Context::default();
    for src in 0..10 {
        let _ = t.want(&src, 0, 64, src as f32);
    }
    t.flush(&ctx);
    // The worker takes the most urgent (9) and waits at the gate.
    let start = Instant::now();
    while seen.lock().unwrap().is_empty() && start.elapsed() < Duration::from_secs(2) {
        std::thread::sleep(Duration::from_millis(1));
    }
    let _ = t.want(&100, 0, 64, 1.0);
    t.flush(&ctx);
    gate_tx.send(()).unwrap();
    gate_tx.send(()).unwrap();
    let start = Instant::now();
    while seen.lock().unwrap().len() < 2 && start.elapsed() < Duration::from_secs(2) {
        std::thread::sleep(Duration::from_millis(1));
    }
    std::thread::sleep(Duration::from_millis(20));
    assert_eq!(*seen.lock().unwrap(), vec![9, 100]);
}

#[test]
fn finished_thumbnails_upload_a_few_per_frame() {
    let (mut t, _) = noting(0);
    t.upload_budget = 2;
    let ctx = Context::default();
    // Inline: one load per flush, so queue six then let them land.
    for _ in 0..6 {
        for src in 0..6 {
            let _ = t.want(&src, 0, 64, 1.0);
        }
        t.flush(&ctx);
    }
    let s = t.stats();
    assert!(s.textures <= 6 && s.loaded >= 2, "{s:?}");
    t.upload_budget = 2;
    let before = t.stats().textures;
    t.flush(&ctx);
    assert!(t.stats().textures - before <= 2);
}

#[test]
fn past_the_budget_the_oldest_go_but_never_one_drawn_this_frame() {
    let (mut t, _) = noting(0);
    t.upload_budget = 100;
    t.texel_budget = 12 * 3; // three 4x3 thumbnails
    let ctx = Context::default();
    for src in 0..6 {
        let _ = t.want(&src, 0, 64, 1.0);
        t.flush(&ctx);
        t.flush(&ctx);
    }
    // Everything drawn in one frame stays, budget or not.
    for src in 0..6 {
        let _ = t.want(&src, 0, 64, 1.0);
    }
    t.flush(&ctx);
    let kept = t.stats().textures;
    for _ in 0..3 {
        t.flush(&ctx);
    }
    assert!(kept >= 3);
    assert!(t.stats().texels <= 12 * 3, "{:?}", t.stats());
}

#[test]
fn a_smaller_bucket_stands_in_while_a_bigger_one_loads() {
    let (mut t, _) = noting(0);
    let ctx = Context::default();
    let _ = t.want(&7, 0, 64, 1.0);
    t.flush(&ctx);
    t.flush(&ctx);
    assert!(t.want(&7, 0, 64, 1.0).is_some());
    assert!(t.want(&7, 0, 512, 1.0).is_some(), "the 64 is drawn while the 512 loads");
}

#[test]
fn a_failed_load_says_why_and_is_not_asked_for_again() {
    let calls = Arc::new(Mutex::new(0));
    let c = calls.clone();
    let mut t: Thumbs<u32> = Thumbs::new(0, move |_| {
        *c.lock().unwrap() += 1;
        Err("not an image".into())
    });
    let ctx = Context::default();
    for _ in 0..5 {
        let _ = t.want(&1, 0, 64, 1.0);
        t.flush(&ctx);
    }
    assert_eq!(*calls.lock().unwrap(), 1);
    assert_eq!(t.failed(&1, 0), Some("not an image"));
}

#[test]
fn buckets_are_powers_of_two_within_bounds() {
    assert_eq!(bucket(10.0), thumbs::MIN_PX);
    assert_eq!(bucket(130.0), 256);
    assert_eq!(bucket(5000.0), thumbs::MAX_PX);
}

// ---- hands ----------------------------------------------------------------

fn rig(n: usize) -> (Toy, Thumbs<u32>, Gallery, Drive) {
    let (t, _) = noting(0);
    let mut g = Gallery::new();
    g.size = 80.0;
    (toy(n), t, g, Drive::new(400.0, 300.0))
}

#[test]
fn click_shift_click_and_cmd_click_select_like_the_table() {
    let (items, mut t, mut g, mut d) = rig(30);
    let mut b = |ui: &mut egui::Ui| g.show(ui, &items, &mut t);
    d.idle(3, &mut b);
    drop(b);
    let (c1, c3, c4) = (centre(&g, 1), centre(&g, 3), centre(&g, 4));
    let mut b = |ui: &mut egui::Ui| g.show(ui, &items, &mut t);
    d.click(c1, Modifiers::NONE, &mut b);
    d.click(c4, Modifiers::SHIFT, &mut b);
    d.click(c3, Modifiers::COMMAND, &mut b);
    drop(b);
    assert_eq!(g.selection, BTreeSet::from([11, 21, 41]));
}

#[test]
fn arrow_down_moves_a_row_and_shift_extends() {
    let (items, mut t, mut g, mut d) = rig(30);
    let mut b = |ui: &mut egui::Ui| g.show(ui, &items, &mut t);
    d.idle(3, &mut b);
    drop(b);
    let c0 = centre(&g, 0);
    let cols = g.layout().unwrap().cols;
    let mut b = |ui: &mut egui::Ui| g.show(ui, &items, &mut t);
    d.click(c0, Modifiers::NONE, &mut b);
    let key = |k, m| Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: m };
    d.frame(vec![key(egui::Key::ArrowDown, Modifiers::NONE)], &mut b);
    d.frame(vec![Event::ModifiersChanged(Modifiers::SHIFT), key(egui::Key::ArrowRight, Modifiers::SHIFT)], &mut b);
    drop(b);
    assert_eq!(g.cursor(), Some(cols + 1));
    assert_eq!(g.selection, BTreeSet::from([items.id(cols), items.id(cols + 1)]));
}

#[test]
fn a_box_drawn_on_empty_space_selects_what_it_touches() {
    let (items, mut t, mut g, mut d) = rig(30);
    let mut b = |ui: &mut egui::Ui| g.show(ui, &items, &mut t);
    d.idle(3, &mut b);
    drop(b);
    let l = g.layout().unwrap().clone();
    let o = g.origin.to_vec2();
    // From the gap left of tile 0's row to tile 1's middle on the next row.
    let from = pos2(2.0, l.tile_rect(0).top() + 4.0) + o;
    let to = l.tile_rect(l.cols + 1).center() + o;
    let mut b = |ui: &mut egui::Ui| g.show(ui, &items, &mut t);
    d.drag(from, to, &mut b);
    drop(b);
    let want: BTreeSet<u64> = [0, 1, l.cols, l.cols + 1].iter().map(|&i| items.id(i)).collect();
    assert_eq!(g.selection, want);
}

#[test]
fn moving_across_a_clip_asks_for_frames_along_it() {
    let (mut items, _, mut g, mut d) = rig(4);
    items.frames = 100;
    let (mut t, seen) = noting(0);
    let mut b = |ui: &mut egui::Ui| g.show(ui, &items, &mut t);
    d.idle(3, &mut b);
    drop(b);
    let r = g.layout().unwrap().tile_rect(0).translate(g.origin.to_vec2());
    let mut b = |ui: &mut egui::Ui| g.show(ui, &items, &mut t);
    for x in [0.1, 0.5, 0.9] {
        d.frame(vec![Event::PointerMoved(pos2(r.left() + r.width() * x, r.center().y))], &mut b);
        d.idle(2, &mut b);
    }
    drop(b);
    let frames: BTreeSet<u32> = seen.lock().unwrap().iter().filter(|q| q.src == 0).map(|q| q.frame).collect();
    for f in [10, 50, 90] {
        assert!(frames.contains(&f), "frame {f} was asked for: {frames:?}");
    }
}

#[test]
fn zooming_resizes_the_tiles_within_bounds() {
    let (items, mut t, mut g, mut d) = rig(30);
    let mut b = |ui: &mut egui::Ui| g.show(ui, &items, &mut t);
    d.frame(vec![Event::PointerMoved(pos2(100.0, 100.0))], &mut b);
    d.frame(vec![Event::Zoom(2.0)], &mut b);
    drop(b);
    assert!((g.size - 160.0).abs() < 0.01);
    let mut b = |ui: &mut egui::Ui| g.show(ui, &items, &mut t);
    d.frame(vec![Event::Zoom(100.0)], &mut b);
    drop(b);
    assert_eq!(g.size, MAX_SIZE);
}

#[test]
fn dragging_a_tile_carries_the_selection_in_grid_order() {
    let (items, mut t, mut g, mut d) = rig(30);
    let mut b = |ui: &mut egui::Ui| g.show(ui, &items, &mut t);
    d.idle(3, &mut b);
    drop(b);
    let (c2, c5) = (centre(&g, 2), centre(&g, 5));
    g.selection = BTreeSet::from([items.id(5), items.id(2)]);
    let mut got = None;
    let mut b = |ui: &mut egui::Ui| {
        let ev = g.show(ui, &items, &mut t);
        if ev.drag_started {
            got = egui::DragAndDrop::payload::<Dragged>(ui.ctx()).map(|p| (*p).clone());
        }
    };
    let press = |pos, pressed| Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE };
    d.frame(vec![Event::PointerMoved(c5)], &mut b);
    d.frame(vec![press(c5, true)], &mut b);
    d.frame(vec![Event::PointerMoved(c5 + vec2(20.0, 0.0))], &mut b);
    d.frame(vec![Event::PointerMoved(c2)], &mut b);
    drop(b);
    assert_eq!(got, Some(Dragged(vec![items.id(2), items.id(5)])));
}

// ---- to look at -----------------------------------------------------------

/// `cargo test gallery_to_look_at -- --ignored` → target/gallery.png
#[test]
#[ignore]
fn gallery_to_look_at() {
    struct Shots;
    impl Items for Shots {
        type Src = u32;
        fn len(&self) -> usize {
            80
        }
        fn id(&self, i: usize) -> u64 {
            i as u64
        }
        fn src(&self, i: usize) -> u32 {
            i as u32
        }
        fn label(&self, i: usize) -> String {
            format!("sh{:03}_comp_v{:02}.####.exr", i * 10, i % 9 + 1)
        }
        fn frames(&self, i: usize) -> u32 {
            if i % 3 == 0 { 48 } else { 1 }
        }
        fn badge(&self, i: usize) -> Option<String> {
            (i % 3 == 0).then(|| "1001-1048".into())
        }
        fn kind(&self, _i: usize) -> Option<String> {
            Some("EXR".into())
        }
        fn section(&self, i: usize) -> Option<String> {
            match i {
                0 => Some("sq010".into()),
                7 => Some("sq020".into()),
                _ => None,
            }
        }
        fn dim(&self, i: usize) -> bool {
            i == 5
        }
    }
    let mut thumbs: Thumbs<u32> = Thumbs::new(4, |r: &Request<u32>| {
        if r.src == 9 {
            return Err("unreadable".into());
        }
        let a = [16.0 / 9.0, 4.0 / 3.0, 2.39, 1.0][r.src as usize % 4];
        let w = r.max_px as usize;
        let h = (w as f32 / a) as usize;
        let hue = (r.src as f32 * 0.618).fract();
        let px = (0..w * h)
            .map(|k| {
                let (x, y) = ((k % w) as f32 / w as f32, (k / w) as f32 / h as f32);
                egui::ecolor::Hsva::new((hue + 0.1 * x).fract(), 0.6, 0.35 + 0.5 * (1.0 - y), 1.0).into()
            })
            .collect();
        Ok(ColorImage::new([w, h], px))
    });
    let mut g = Gallery::new();
    g.size = 150.0;
    g.metrics.aspect = 16.0 / 9.0;
    g.selection = BTreeSet::from([2, 3, 4]);
    crate::uishot::render("gallery", 900.0, 560.0, 2.0, 80, |ui, frame| {
        if frame > 70 {
            // Hover tile 0 a third of the way along: the scrub bar shows.
            if let Some(l) = g.layout() {
                let r = l.tile_rect(0).translate(g.origin.to_vec2());
                ui.ctx().input_mut(|i| i.pointer = Default::default());
                let p = pos2(r.left() + r.width() * 0.33, r.center().y);
                ui.ctx().input_mut(|i| i.events.push(Event::PointerMoved(p)));
            }
        }
        g.show(ui, &Shots, &mut thumbs);
        std::thread::sleep(Duration::from_millis(15));
    });
}

// ---- the list --------------------------------------------------------------

impl Details for Toy {
    fn cell(&self, i: usize, col: usize) -> String {
        format!("{i}:{col}")
    }
}

fn list_rig(n: usize) -> (Toy, Thumbs<u32>, List, Drive) {
    let cols = vec![crate::table::Column::new("Name", 120.0), crate::table::Column::new("Size", 60.0)];
    (toy(n), noting(0).0, List::new(cols), Drive::new(400.0, 300.0))
}

/// Row i's name cell (past the thumbnail), on screen.
fn row_at(l: &List, i: usize) -> Pos2 {
    l.row_rect(i, 400.0).left_center() + vec2(200.0, 0.0)
}

#[test]
fn list_rows_select_like_the_grid() {
    let (items, mut thumbs, mut l, mut d) = list_rig(20);
    d.idle(2, &mut |ui| l.show(ui, &items, &mut thumbs));
    d.click(row_at(&l, 1), Modifiers::NONE, &mut |ui| l.show(ui, &items, &mut thumbs));
    assert_eq!(l.selection, BTreeSet::from([11]));
    d.click(row_at(&l, 3), Modifiers::SHIFT, &mut |ui| l.show(ui, &items, &mut thumbs));
    assert_eq!(l.selection, BTreeSet::from([11, 21, 31]));
    d.click(row_at(&l, 2), Modifiers::COMMAND, &mut |ui| l.show(ui, &items, &mut thumbs));
    assert_eq!(l.selection, BTreeSet::from([11, 31]));
    d.key(egui::Key::ArrowDown, Modifiers::NONE, &mut |ui| l.show(ui, &items, &mut thumbs));
    assert_eq!(l.selection, BTreeSet::from([31]), "down from the cursor (row 2) picks row 3");
}

#[test]
fn a_list_header_click_sorts_and_a_boundary_drag_sizes_the_column_right_of_it() {
    let (items, mut thumbs, mut l, mut d) = list_rig(5);
    d.idle(2, &mut |ui| l.show(ui, &items, &mut thumbs));
    let head_y = l.origin.y - list::HEADER_H / 2.0;
    let evs = d.click(pos2(370.0, head_y), Modifiers::NONE, &mut |ui| l.show(ui, &items, &mut thumbs));
    assert!(evs.iter().any(|e| e.sort_changed));
    assert_eq!(l.sort, Some((1, true)));
    // The Size column's left edge sits 60 in from the body's right.
    let width = 400.0 - egui::Style::default().spacing.scroll.allocated_width();
    let edge = width - 60.0;
    d.drag(pos2(edge, head_y), pos2(edge - 30.0, head_y), &mut |ui| l.show(ui, &items, &mut thumbs));
    assert!((l.columns[1].width - 90.0).abs() < 2.0, "{}", l.columns[1].width);
}

#[test]
fn moving_across_a_list_thumbnail_scrubs_the_clip() {
    let mut items = toy(4);
    items.frames = 20;
    let (mut thumbs, seen) = noting(0);
    let mut l = List::default();
    let mut d = Drive::new(400.0, 300.0);
    d.idle(2, &mut |ui| l.show(ui, &items, &mut thumbs));
    seen.lock().unwrap().clear();
    let r = l.row_rect(1, 400.0);
    for k in 0..8 {
        let at = pos2(r.left() + 8.0 + k as f32 * 6.0, r.center().y);
        d.frame(vec![Event::PointerMoved(at)], &mut |ui| l.show(ui, &items, &mut thumbs));
    }
    let frames: BTreeSet<u32> = seen.lock().unwrap().iter().filter(|q| q.src == 1).map(|q| q.frame).collect();
    assert!(frames.len() >= 4, "asked along the clip: {frames:?}");
    // Off the thumbnail, over the name, it does not scrub.
    seen.lock().unwrap().clear();
    d.frame(vec![Event::PointerMoved(row_at(&l, 2))], &mut |ui| l.show(ui, &items, &mut thumbs));
    d.idle(2, &mut |ui| l.show(ui, &items, &mut thumbs));
    assert!(seen.lock().unwrap().iter().all(|q| q.src != 2 || q.frame == items.poster(2)));
}

#[test]
fn only_rows_in_view_and_a_screen_beyond_are_loaded_in_the_list() {
    let (items, _, mut l, mut d) = list_rig(5000);
    let (mut thumbs, seen) = noting(1);
    settle(&mut d, &mut |ui| {
        l.show(ui, &items, &mut thumbs);
        thumbs.stats()
    });
    let asked: BTreeSet<u32> = seen.lock().unwrap().iter().map(|r| r.src).collect();
    assert!(l.visible().all(|i| asked.contains(&(i as u32))));
    assert!(asked.len() < 3 * l.visible().len() + 4, "{} asked for {} visible", asked.len(), l.visible().len());
}
