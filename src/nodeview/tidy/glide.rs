//! The canvas' motion after a tidy: the document already has the new
//! places; each moved node is drawn back where it was, then, after a
//! short wait (top rows first), eases over to its place, its leftover
//! offset falling away exponentially. (wobble's `tidy_cascade`.)

use std::collections::HashMap;
use std::hash::Hash;

use egui::{Pos2, Vec2};

/// Each node's wait after the one above it, and the longest wait.
const STAGGER: f32 = 0.012;
const LONGEST_WAIT: f32 = 0.25;
/// How fast the offset falls away (a second's share, as a rate).
const RATE: f32 = 12.0;
/// Offsets shorter than this are done with.
const DONE: f32 = 0.5;

/// Nodes on their way: the offset each is drawn at, and how long it waits
/// before it moves.
pub struct Glide<Id> {
    moving: HashMap<Id, (Vec2, f32)>,
}

impl<Id> Default for Glide<Id> {
    fn default() -> Self {
        Self { moving: HashMap::new() }
    }
}

impl<Id: Clone + Eq + Hash + Ord> Glide<Id> {
    /// The nodes moved from `before` (id, old top-left) to where the
    /// document has them now (id, new top-left), top rows first.
    pub fn start(&mut self, before: &[(Id, Pos2)], after: &[(Id, Pos2)]) {
        let was: HashMap<&Id, Pos2> = before.iter().map(|(id, p)| (id, *p)).collect();
        let mut moved: Vec<(&Id, Vec2, f32)> = after
            .iter()
            .filter_map(|(id, p)| {
                let old = was.get(id)?;
                let d = *old - *p;
                (d.length() > DONE).then_some((id, d, p.y))
            })
            .collect();
        moved.sort_by(|a, b| a.2.total_cmp(&b.2).then(a.0.cmp(b.0)));
        for (k, (id, d, _)) in moved.into_iter().enumerate() {
            self.moving.insert(id.clone(), (d, (k as f32 * STAGGER).min(LONGEST_WAIT)));
        }
    }

    /// Where node `id` is drawn, against where it is.
    pub fn offset(&self, id: &Id) -> Vec2 {
        self.moving.get(id).map_or(Vec2::ZERO, |m| m.0)
    }

    /// A frame `dt` seconds long passed. True while anything still moves.
    pub fn step(&mut self, dt: f32) -> bool {
        for (offset, wait) in self.moving.values_mut() {
            if *wait > 0.0 {
                *wait -= dt;
            } else {
                *offset *= (-dt * RATE).exp();
            }
        }
        self.moving.retain(|_, (offset, _)| offset.length() > DONE);
        !self.moving.is_empty()
    }

    /// Stop: everything drawn where it is (a node dragged, an undo).
    pub fn stop(&mut self) {
        self.moving.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;

    #[test]
    fn a_moved_node_starts_where_it_was_and_eases_to_its_place_top_rows_first() {
        let mut g: Glide<&str> = Glide::default();
        g.start(&[("a", pos2(100.0, 0.0))], &[("a", pos2(0.0, 0.0))]);
        assert_eq!(g.offset(&"a"), Vec2::new(100.0, 0.0), "drawn where it was");
        let mut last = 100.0;
        while g.step(1.0 / 60.0) {
            let now = g.offset(&"a").x;
            assert!(now <= last, "it only ever closes in");
            last = now;
        }
        assert_eq!(g.offset(&"a"), Vec2::ZERO);
        let mut g: Glide<&str> = Glide::default();
        g.start(&[("low", pos2(50.0, 0.0)), ("high", pos2(50.0, 0.0))], &[("low", pos2(0.0, 500.0)), ("high", pos2(0.0, 0.0))]);
        g.step(0.006);
        assert!(g.offset(&"high").length() < g.offset(&"low").length(), "the higher one is under way first");
    }
}
