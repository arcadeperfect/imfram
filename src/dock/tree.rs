//! The layout of one window's panels: a tree of splits down to leaves,
//! each leaf a row of tabs with one active. Pure: no egui here. The tree
//! answers where everything goes (`layout`), takes tabs out and puts
//! them in (`remove`, `insert`), and makes the tree a drag would leave
//! behind (`proposed`), which the frame animates toward before the drop.

use egui::{pos2, Rect};
use serde::{Deserialize, Serialize};

/// A leaf's identity, kept across edits so its rect can animate.
pub type LeafId = u32;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Dir {
    /// `a` left of `b`.
    Row,
    /// `a` above `b`.
    Col,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

impl Side {
    fn dir(self) -> Dir {
        match self {
            Side::Left | Side::Right => Dir::Row,
            Side::Top | Side::Bottom => Dir::Col,
        }
    }
    /// The new node comes first (left or above).
    fn first(self) -> bool {
        matches!(self, Side::Left | Side::Top)
    }
}

/// Where in a leaf a drop lands.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Zone {
    /// As a tab beside the leaf's.
    Center,
    /// A new leaf split off this side.
    Side(Side),
}

/// A drop: into a leaf, or beside the whole window's content.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Drop {
    Leaf(LeafId, Zone),
    Root(Side),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Node<T> {
    Leaf { id: LeafId, tabs: Vec<T>, active: usize },
    Split { dir: Dir, frac: f32, a: Box<Node<T>>, b: Box<Node<T>> },
}

/// A split's place: the a/b turns from the root.
pub type Path = Vec<bool>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tree<T> {
    root: Option<Node<T>>,
    next: LeafId,
}

/// Where everything is, for a rect.
pub struct Layout {
    pub leaves: Vec<(LeafId, Rect)>,
    /// Each split: its path, the band between its children, its whole
    /// rect, and which way it splits.
    pub seps: Vec<(Path, Rect, Rect, Dir)>,
}

const MIN_FRAC: f32 = 0.08;

impl<T: Copy + PartialEq> Tree<T> {
    /// One leaf of `tabs` (none: an empty tree).
    pub fn new(tabs: Vec<T>) -> Self {
        let mut t = Self { root: None, next: 1 };
        if !tabs.is_empty() {
            t.root = Some(t.leaf(tabs));
        }
        t
    }

    fn leaf(&mut self, tabs: Vec<T>) -> Node<T> {
        let id = self.next;
        self.next += 1;
        Node::Leaf { id, tabs, active: 0 }
    }

    /// The id the next new leaf will have (a proposal's ghost).
    pub fn next_id(&self) -> LeafId {
        self.next
    }

    pub fn is_empty(&self) -> bool {
        self.root.is_none()
    }

    /// A new leaf of `tabs` beside everything, on `side`; `keep` is the
    /// share the existing content keeps.
    pub fn split(&mut self, side: Side, keep: f32, tabs: Vec<T>) -> &mut Self {
        let new = self.leaf(tabs);
        self.root = Some(match self.root.take() {
            None => new,
            Some(old) => join(side, keep, old, new),
        });
        self
    }

    /// Every tab, leaf by leaf.
    pub fn tabs(&self) -> Vec<T> {
        let mut out = Vec::new();
        for (_, tabs, _) in self.leaves() {
            out.extend_from_slice(tabs);
        }
        out
    }

    pub fn leaves(&self) -> Vec<(LeafId, &[T], usize)> {
        let mut out = Vec::new();
        if let Some(r) = &self.root {
            collect(r, &mut out);
        }
        out
    }

    pub fn find(&self, tab: T) -> Option<LeafId> {
        self.leaves().iter().find(|(_, tabs, _)| tabs.contains(&tab)).map(|(id, ..)| *id)
    }

    pub fn has_leaf(&self, id: LeafId) -> bool {
        self.leaves().iter().any(|(l, ..)| *l == id)
    }

    /// The active tab of leaf `id`.
    pub fn active(&self, id: LeafId) -> Option<T> {
        self.leaves().iter().find(|(l, ..)| *l == id).and_then(|(_, tabs, a)| tabs.get(*a).copied())
    }

    pub fn set_active(&mut self, tab: T) {
        if let Some(r) = &mut self.root {
            r.walk_leaves(&mut |tabs, active| {
                if let Some(i) = tabs.iter().position(|t| *t == tab) {
                    *active = i;
                }
            });
        }
    }

    /// Take `tab` out. An emptied leaf goes, its split folding into the
    /// sibling. False if it was not here.
    pub fn remove(&mut self, tab: T) -> bool {
        let Some(root) = self.root.take() else { return false };
        let (node, found) = remove_from(root, tab);
        self.root = node;
        found
    }

    /// Put `tab` at `drop`. If the target is gone, into the first leaf.
    pub fn insert(&mut self, tab: T, drop: Drop) {
        match drop {
            Drop::Root(side) => {
                self.split(side, 0.7, vec![tab]);
                return;
            }
            Drop::Leaf(id, zone) if self.has_leaf(id) => {
                let new = match zone {
                    Zone::Center => None,
                    Zone::Side(_) => Some(self.leaf(vec![tab])),
                };
                if let Some(root) = self.root.take() {
                    self.root = Some(insert_at(root, id, tab, zone, new));
                }
            }
            Drop::Leaf(..) => self.push_first(tab),
        }
    }

    /// Into the first leaf (a new root leaf if there is none), active.
    pub fn push_first(&mut self, tab: T) {
        match &mut self.root {
            None => self.root = Some(self.leaf(vec![tab])),
            Some(r) => {
                let mut done = false;
                r.walk_leaves(&mut |tabs, active| {
                    if !done {
                        tabs.push(tab);
                        *active = tabs.len() - 1;
                        done = true;
                    }
                });
            }
        }
    }

    /// The tree a drop of `tab` at `drop` would leave. None when the drop
    /// changes nothing (its own leaf's centre) or its target would be
    /// gone (the tab was alone there).
    pub fn proposed(&self, tab: T, drop: Drop) -> Option<Tree<T>> {
        let here = self.find(tab);
        if let (Some(here), Drop::Leaf(id, Zone::Center)) = (here, drop) {
            if here == id {
                return None;
            }
        }
        let mut t = self.clone();
        t.remove(tab);
        if let Drop::Leaf(id, _) = drop {
            if !t.has_leaf(id) {
                return None;
            }
            // A side split of a leaf the tab alone was beside is the same
            // layout: nothing to show.
        }
        t.insert(tab, drop);
        Some(t)
    }

    pub fn set_frac(&mut self, path: &[bool], frac: f32) {
        let Some(mut n) = self.root.as_mut() else { return };
        for &turn in path {
            match n {
                Node::Split { a, b, .. } => n = if turn { b } else { a },
                Node::Leaf { .. } => return,
            }
        }
        if let Node::Split { frac: f, .. } = n {
            *f = frac.clamp(MIN_FRAC, 1.0 - MIN_FRAC);
        }
    }

    /// Where each leaf and separator goes in `rect`; `gap` the band
    /// between a split's children.
    pub fn layout(&self, rect: Rect, gap: f32) -> Layout {
        self.layout_fixed(rect, gap, &|_| None)
    }

    /// `layout`, where `fixed` gives a leaf (by its tabs) a height of its
    /// own: in a split above or below it, that leaf gets exactly that
    /// height and the split no divider (its share is not the user's).
    pub fn layout_fixed(&self, rect: Rect, gap: f32, fixed: &dyn Fn(&[T]) -> Option<f32>) -> Layout {
        let mut out = Layout { leaves: Vec::new(), seps: Vec::new() };
        if let Some(r) = &self.root {
            place(r, rect, gap, fixed, &mut Vec::new(), &mut out);
        }
        out
    }
}

fn join<T>(side: Side, keep: f32, old: Node<T>, new: Node<T>) -> Node<T> {
    let keep = keep.clamp(MIN_FRAC, 1.0 - MIN_FRAC);
    if side.first() {
        Node::Split { dir: side.dir(), frac: 1.0 - keep, a: Box::new(new), b: Box::new(old) }
    } else {
        Node::Split { dir: side.dir(), frac: keep, a: Box::new(old), b: Box::new(new) }
    }
}

fn collect<'a, T>(n: &'a Node<T>, out: &mut Vec<(LeafId, &'a [T], usize)>) {
    match n {
        Node::Leaf { id, tabs, active } => out.push((*id, tabs, *active)),
        Node::Split { a, b, .. } => {
            collect(a, out);
            collect(b, out);
        }
    }
}

impl<T> Node<T> {
    fn walk_leaves(&mut self, f: &mut impl FnMut(&mut Vec<T>, &mut usize)) {
        match self {
            Node::Leaf { tabs, active, .. } => f(tabs, active),
            Node::Split { a, b, .. } => {
                a.walk_leaves(f);
                b.walk_leaves(f);
            }
        }
    }
}

fn remove_from<T: Copy + PartialEq>(n: Node<T>, tab: T) -> (Option<Node<T>>, bool) {
    match n {
        Node::Leaf { id, mut tabs, active } => {
            let Some(i) = tabs.iter().position(|t| *t == tab) else {
                return (Some(Node::Leaf { id, tabs, active }), false);
            };
            tabs.remove(i);
            if tabs.is_empty() {
                return (None, true);
            }
            let active = if active > i || active >= tabs.len() { active.saturating_sub(1).min(tabs.len() - 1) } else { active };
            (Some(Node::Leaf { id, tabs, active }), true)
        }
        Node::Split { dir, frac, a, b } => {
            let (a, found) = remove_from(*a, tab);
            if found {
                return match a {
                    Some(a) => (Some(Node::Split { dir, frac, a: Box::new(a), b }), true),
                    None => (Some(*b), true),
                };
            }
            let a = a.expect("untouched");
            let (b, found) = remove_from(*b, tab);
            match b {
                Some(b) => (Some(Node::Split { dir, frac, a: Box::new(a), b: Box::new(b) }), found),
                None => (Some(a), true),
            }
        }
    }
}

fn insert_at<T: Copy>(n: Node<T>, id: LeafId, tab: T, zone: Zone, new: Option<Node<T>>) -> Node<T> {
    match n {
        Node::Leaf { id: here, mut tabs, active } if here == id => match (zone, new) {
            (Zone::Side(side), Some(new)) => join(side, 0.5, Node::Leaf { id: here, tabs, active }, new),
            _ => {
                tabs.push(tab);
                let active = tabs.len() - 1;
                Node::Leaf { id: here, tabs, active }
            }
        },
        leaf @ Node::Leaf { .. } => leaf,
        Node::Split { dir, frac, a, b } => {
            let a = insert_at(*a, id, tab, zone, new.clone_if_unused());
            Node::Split { dir, frac, a: Box::new(a), b }.then_b(id, tab, zone, new)
        }
    }
}

// (A new leaf is used once: whichever branch holds the target takes it.
// Cloning a one-tab leaf is cheap, so the simple route is to offer it to
// both branches; only the one with the target consumes it.)
trait CloneIfUnused: Sized {
    fn clone_if_unused(&self) -> Self;
}
impl<T: Copy> CloneIfUnused for Option<Node<T>> {
    fn clone_if_unused(&self) -> Self {
        self.as_ref().map(|n| match n {
            Node::Leaf { id, tabs, active } => Node::Leaf { id: *id, tabs: tabs.clone(), active: *active },
            Node::Split { .. } => unreachable!("a new leaf is a leaf"),
        })
    }
}

impl<T: Copy> Node<T> {
    fn then_b(self, id: LeafId, tab: T, zone: Zone, new: Option<Node<T>>) -> Node<T> {
        match self {
            Node::Split { dir, frac, a, b } => {
                let b = insert_at(*b, id, tab, zone, new);
                Node::Split { dir, frac, a, b: Box::new(b) }
            }
            leaf => leaf,
        }
    }
}

fn place<T>(n: &Node<T>, rect: Rect, gap: f32, fixed: &dyn Fn(&[T]) -> Option<f32>, path: &mut Path, out: &mut Layout) {
    match n {
        Node::Leaf { id, .. } => out.leaves.push((*id, rect)),
        Node::Split { dir, frac, a, b } => {
            let height = |n: &Node<T>| match n {
                Node::Leaf { tabs, .. } => fixed(tabs),
                Node::Split { .. } => None,
            };
            let room = (rect.height() - gap).max(1.0);
            let pinned = match dir {
                Dir::Col => match (height(a), height(b)) {
                    (Some(h), _) => Some((h / room).clamp(0.0, 1.0)),
                    (None, Some(h)) => Some(1.0 - (h / room).clamp(0.0, 1.0)),
                    _ => None,
                },
                Dir::Row => None,
            };
            let (ra, sep, rb) = cut(rect, *dir, pinned.unwrap_or(*frac), gap);
            if pinned.is_none() {
                out.seps.push((path.clone(), sep, rect, *dir));
            }
            path.push(false);
            place(a, ra, gap, fixed, path, out);
            path.pop();
            path.push(true);
            place(b, rb, gap, fixed, path, out);
            path.pop();
        }
    }
}

/// `rect` cut at `frac` along `dir`, a `gap` band between.
pub fn cut(rect: Rect, dir: Dir, frac: f32, gap: f32) -> (Rect, Rect, Rect) {
    match dir {
        Dir::Row => {
            let x = rect.min.x + (rect.width() - gap) * frac;
            (
                Rect::from_min_max(rect.min, pos2(x, rect.max.y)),
                Rect::from_min_max(pos2(x, rect.min.y), pos2(x + gap, rect.max.y)),
                Rect::from_min_max(pos2(x + gap, rect.min.y), rect.max),
            )
        }
        Dir::Col => {
            let y = rect.min.y + (rect.height() - gap) * frac;
            (
                Rect::from_min_max(rect.min, pos2(rect.max.x, y)),
                Rect::from_min_max(pos2(rect.min.x, y), pos2(rect.max.x, y + gap)),
                Rect::from_min_max(pos2(rect.min.x, y + gap), rect.max),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> Tree<&'static str> {
        let mut t = Tree::new(vec!["a", "b"]);
        t.split(Side::Right, 0.6, vec!["c"]);
        t
    }

    #[test]
    fn a_split_lays_its_leaves_side_by_side_with_a_gap() {
        let l = tree().layout(Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(100.0, 50.0)), 4.0);
        assert_eq!(l.leaves.len(), 2);
        assert_eq!(l.seps.len(), 1);
        let (_, left) = l.leaves[0];
        let (_, right) = l.leaves[1];
        assert!((left.width() - 57.6).abs() < 0.01, "0.6 of the 96 left after the gap");
        assert_eq!(right.min.x, left.max.x + 4.0);
    }

    #[test]
    fn a_strip_keeps_its_height_whatever_the_window_and_has_no_divider() {
        let mut t: Tree<&str> = Tree::new(vec!["top"]);
        t.split(Side::Bottom, 0.5, vec!["strip"]);
        t.split(Side::Bottom, 0.7, vec!["bottom"]);
        let fixed = |tabs: &[&str]| (tabs == ["strip"]).then_some(30.0);
        for h in [200.0, 900.0] {
            let l = t.layout_fixed(Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(100.0, h)), 4.0, &fixed);
            let strip = l.leaves.iter().find(|(id, _)| Some(*id) == t.find("strip")).unwrap().1;
            assert!((strip.height() - 30.0).abs() < 0.01, "{h}: {}", strip.height());
            assert_eq!(l.seps.len(), 1, "only the outer split can be dragged");
        }
    }

    #[test]
    fn removing_the_last_tab_of_a_leaf_folds_its_split_away() {
        let mut t = tree();
        assert!(t.remove("c"));
        assert_eq!(t.leaves().len(), 1);
        assert_eq!(t.tabs(), ["a", "b"]);
        assert!(!t.remove("zzz"));
        assert!(t.remove("a") && t.remove("b"));
        assert!(t.is_empty());
    }

    #[test]
    fn a_proposal_moves_the_tab_without_touching_the_tree_it_came_from() {
        let t = tree();
        let c = t.find("c").unwrap();
        let p = t.proposed("a", Drop::Leaf(c, Zone::Side(Side::Bottom))).unwrap();
        assert_eq!(p.leaves().len(), 3);
        assert_eq!(p.find("a"), Some(t.next_id()), "the ghost leaf has the id the tree would give it");
        assert_eq!(t.tabs(), ["a", "b", "c"], "the original is as it was");
        let a = t.find("a").unwrap();
        assert!(t.proposed("a", Drop::Leaf(a, Zone::Center)).is_none(), "its own centre changes nothing");
        assert!(t.proposed("c", Drop::Leaf(c, Zone::Side(Side::Left))).is_none(), "its target would be gone");
    }

    #[test]
    fn a_drop_then_commits_the_same_shape_as_its_proposal() {
        let mut t = tree();
        let c = t.find("c").unwrap();
        let p = t.proposed("b", Drop::Leaf(c, Zone::Center)).unwrap();
        t.remove("b");
        t.insert("b", Drop::Leaf(c, Zone::Center));
        assert_eq!(t.tabs(), p.tabs());
        assert_eq!(t.active(c), Some("b"), "the moved tab is the one shown");
    }

    #[test]
    fn a_root_drop_wraps_everything() {
        let mut t = tree();
        t.insert("z", Drop::Root(Side::Top));
        let l = t.layout(Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(100.0, 100.0)), 0.0);
        let (id, top) = l.leaves[0];
        assert_eq!(t.active(id), Some("z"));
        assert_eq!(top.min.y, 0.0);
        assert!(top.height() < 40.0);
    }
}

#[cfg(test)]
mod serde_tests {
    use super::*;

    #[test]
    fn a_tree_survives_a_round_trip_through_json() {
        let mut t = Tree::new(vec!["a", "b"]);
        t.split(Side::Right, 0.6, vec!["c"]);
        let s = serde_json::to_string(&t).unwrap();
        let back: Tree<&str> = serde_json::from_str(&s).unwrap();
        assert_eq!(back.tabs(), t.tabs());
        assert_eq!(back.next_id(), t.next_id());
    }
}
