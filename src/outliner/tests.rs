use super::drive::Drive;
use super::*;
use egui::{Modifiers, Pos2};

struct Toy {
    names: Vec<&'static str>,
    kids: Vec<Vec<usize>>,
    roots: Vec<usize>,
}

impl TreeModel for Toy {
    type Id = usize;
    fn roots(&self) -> Vec<usize> {
        self.roots.clone()
    }
    fn children(&self, id: usize) -> Vec<usize> {
        self.kids[id].clone()
    }
    fn label(&self, id: usize) -> String {
        self.names[id].to_string()
    }
}

/// a [b, c [d, e]], f: fully unfolded the rows are a b c d e f.
fn toy() -> Toy {
    Toy { names: vec!["a", "b", "c", "d", "e", "f"], kids: vec![vec![1, 2], vec![], vec![3, 4], vec![], vec![], vec![]], roots: vec![0, 5] }
}

fn open() -> (Toy, Outliner<usize>) {
    let t = toy();
    let mut o = Outliner::new();
    o.expand_all(&t);
    (t, o)
}

/// The middle of row `i`, clear of the triangles.
fn row(i: usize) -> Pos2 {
    Pos2::new(100.0, i as f32 * ROW_H + ROW_H * 0.5)
}

fn flat(rs: Vec<Vec<Event<usize>>>) -> Vec<Event<usize>> {
    rs.into_iter().flatten().collect()
}

/// The frame closure, made afresh so the test can look at `o` between hands.
macro_rules! show {
    ($o:ident, $t:ident) => {
        &mut |ui: &mut egui::Ui| $o.show(ui, &$t)
    };
}

#[test]
fn only_roots_show_until_a_node_is_unfolded() {
    let t = toy();
    let mut o = Outliner::<usize>::new();
    let ids = |o: &Outliner<usize>| row::flatten(&t, &o.expanded).iter().map(|r| r.id).collect::<Vec<_>>();
    assert_eq!(ids(&o), vec![0, 5]);
    o.expanded.insert(0);
    assert_eq!(ids(&o), vec![0, 1, 2, 5]);
    o.expanded.insert(2);
    assert_eq!(ids(&o), vec![0, 1, 2, 3, 4, 5]);
    let rows = row::flatten(&t, &o.expanded);
    assert_eq!((rows[3].parent, rows[3].index, rows[3].depth), (Some(2), 0, 2));
}

#[test]
fn a_click_selects_shift_click_a_range_and_cmd_click_toggles() {
    let (t, mut o) = open();
    let mut d = Drive::new(300.0, 200.0);
    d.idle(1, show!(o, t));
    assert_eq!(flat(d.click(row(1), Modifiers::NONE, show!(o, t))), vec![Event::Select(vec![1])]);
    assert_eq!(flat(d.click(row(3), Modifiers::SHIFT, show!(o, t))), vec![Event::Select(vec![1, 2, 3])]);
    assert_eq!(flat(d.click(row(1), Modifiers::COMMAND, show!(o, t))), vec![Event::Select(vec![2, 3])]);
    assert_eq!(flat(d.click(Pos2::new(100.0, 190.0), Modifiers::NONE, show!(o, t))), vec![Event::Select(vec![])], "empty space clears");
}

#[test]
fn the_triangle_folds_a_node_without_selecting_it() {
    let (t, mut o) = open();
    let mut d = Drive::new(300.0, 200.0);
    d.idle(1, show!(o, t));
    assert!(flat(d.click(Pos2::new(TRI_W * 0.5, ROW_H * 0.5), Modifiers::NONE, show!(o, t))).is_empty());
    assert!(!o.expanded.contains(&0));
    assert!(o.selection.is_empty());
}

#[test]
fn arrow_keys_walk_the_rows_right_unfolds_left_folds() {
    let (t, mut o) = open();
    let mut d = Drive::new(300.0, 200.0);
    d.idle(1, show!(o, t));
    d.click(row(0), Modifiers::NONE, show!(o, t));
    assert_eq!(flat(d.key(egui::Key::ArrowDown, Modifiers::NONE, show!(o, t))), vec![Event::Select(vec![1])]);
    assert_eq!(flat(d.key(egui::Key::ArrowDown, Modifiers::SHIFT, show!(o, t))), vec![Event::Select(vec![1, 2])]);
    assert!(flat(d.key(egui::Key::ArrowLeft, Modifiers::NONE, show!(o, t))).is_empty());
    assert!(!o.expanded.contains(&2), "a branch's left folds it");
    assert_eq!(flat(d.key(egui::Key::ArrowLeft, Modifiers::NONE, show!(o, t))), vec![Event::Select(vec![0])], "a folded row's left climbs");
    d.key(egui::Key::ArrowLeft, Modifiers::NONE, show!(o, t));
    assert!(!o.expanded.contains(&0));
    d.key(egui::Key::ArrowRight, Modifiers::NONE, show!(o, t));
    assert!(o.expanded.contains(&0), "right unfolds");
    assert_eq!(flat(d.key(egui::Key::ArrowRight, Modifiers::NONE, show!(o, t))), vec![Event::Select(vec![1])], "right again descends");
    assert_eq!(flat(d.key(egui::Key::Enter, Modifiers::NONE, show!(o, t))), vec![Event::Activate(1)]);
}

#[test]
fn f2_renames_in_place_enter_commits_esc_cancels_and_the_model_is_untouched() {
    let (t, mut o) = open();
    let mut d = Drive::new(300.0, 200.0);
    d.idle(1, show!(o, t));
    d.click(row(1), Modifiers::NONE, show!(o, t));
    d.key(egui::Key::F2, Modifiers::NONE, show!(o, t));
    d.idle(1, show!(o, t));
    d.text("zz", show!(o, t));
    assert_eq!(flat(d.key(egui::Key::Enter, Modifiers::NONE, show!(o, t))), vec![Event::Rename(1, "zz".into())]);
    assert_eq!(t.label(1), "b");
    d.key(egui::Key::F2, Modifiers::NONE, show!(o, t));
    d.idle(1, show!(o, t));
    d.text("q", show!(o, t));
    assert!(flat(d.key(egui::Key::Escape, Modifiers::NONE, show!(o, t))).is_empty());
    assert!(flat(d.idle(2, show!(o, t))).is_empty());
}

#[test]
fn a_drag_onto_a_row_reparents_and_between_rows_gives_the_slot() {
    let (t, mut o) = open();
    let mut d = Drive::new(300.0, 200.0);
    d.idle(1, show!(o, t));
    let got = flat(d.drag(row(5), row(1), show!(o, t)));
    assert!(got.contains(&Event::Move { id: 5, parent: Some(1), index: 0 }), "onto b: {got:?}");
    // d, from under c, to the line above b: a's first child.
    let got = flat(d.drag(row(3), Pos2::new(100.0, ROW_H + 2.0), show!(o, t)));
    assert!(got.contains(&Event::Move { id: 3, parent: Some(0), index: 0 }), "before b: {got:?}");
    // d to the line below its sibling e: index counts as if d were gone.
    let got = flat(d.drag(row(3), Pos2::new(100.0, 5.0 * ROW_H - 2.0), show!(o, t)));
    assert!(got.contains(&Event::Move { id: 3, parent: Some(2), index: 1 }), "after e: {got:?}");
    // Below an unfolded branch means into it, first.
    let got = flat(d.drag(row(5), Pos2::new(100.0, 3.0 * ROW_H - 2.0), show!(o, t)));
    assert!(got.contains(&Event::Move { id: 5, parent: Some(2), index: 0 }), "after open c: {got:?}");
    assert!(!o.is_dragging());
}

#[test]
fn a_node_cannot_be_dropped_into_its_own_subtree_and_a_no_op_drop_is_silent() {
    let (t, mut o) = open();
    let mut d = Drive::new(300.0, 200.0);
    d.idle(1, show!(o, t));
    let got = flat(d.drag(row(2), row(3), show!(o, t)));
    assert!(!got.iter().any(|e| matches!(e, Event::Move { .. })), "{got:?}");
    let got = flat(d.drag(row(1), Pos2::new(100.0, 2.0 * ROW_H - 2.0), show!(o, t)));
    assert!(!got.iter().any(|e| matches!(e, Event::Move { .. })), "b after b is where it is: {got:?}");
}

#[test]
fn a_selected_row_is_painted_in_the_selection_colour() {
    let (t, mut o) = open();
    o.selection = vec![2];
    let shot = crate::snapshot::shoot(200.0, 140.0, |ui| {
        o.show(ui, &t);
    });
    let want = egui::Visuals::dark().selection.bg_fill;
    let got = shot.at(150.0, 2.5 * ROW_H);
    assert!(got.iter().zip(want.to_array()).all(|(a, b)| (*a as i32 - b as i32).abs() < 8), "{got:?} vs {want:?}");
    let plain = shot.at(150.0, 3.5 * ROW_H);
    assert_ne!(plain, got);
}

#[test]
fn only_the_visible_slice_of_a_long_tree_is_laid_out() {
    let n = 10_000;
    let t = Toy { names: vec!["n"; n], kids: vec![vec![]; n], roots: (0..n).collect() };
    let mut o = Outliner::<usize>::new();
    let shot = crate::snapshot::shoot(200.0, 100.0, |ui| {
        o.show(ui, &t);
    });
    let v = o.visible();
    assert!(v.start == 0 && v.len() <= 8, "{v:?}");
    let _ = shot;
}
