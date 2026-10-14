//! The trait an app's document implements to sit on a canvas: a stack of
//! placed items. The canvas owns the hands (camera, selection, G/R/S, the
//! box); the scene owns what the items ARE. `Board` is a ready-made one.

use egui::{Pos2, Rect, Ui};

use super::pen::Pen;
use super::xform::Xform;

pub trait Scene {
    type Id: Copy + Eq + std::hash::Hash + std::fmt::Debug;

    /// Every item, back to front (paint order; hits are tested front first).
    fn ids(&self) -> Vec<Self::Id>;

    /// The item's extent in its own coordinates. Selection boxes, culling
    /// and the default hit test use it.
    fn local_rect(&self, id: Self::Id) -> Rect;

    fn xform(&self, id: Self::Id) -> Xform;
    fn set_xform(&mut self, id: Self::Id, xf: Xform);

    /// Draw the item. Only called when its box is on screen.
    fn paint(&self, id: Self::Id, pen: &Pen);

    /// Is `local` on the item, `tol` local units of slack? Defaults to its
    /// rect; a vector shape answers with its outline.
    fn hit(&self, id: Self::Id, local: Pos2, tol: f32) -> bool {
        self.local_rect(id).expand(tol).contains(local)
    }

    /// Delete (X / Delete).
    fn remove(&mut self, ids: &[Self::Id]);

    /// Copies of the items, above everything, in the same places (shift+D,
    /// which then grabs them). Return none if the scene cannot copy.
    fn duplicate(&mut self, _ids: &[Self::Id]) -> Vec<Self::Id> {
        vec![]
    }

    /// Move items to the front (`true`) or the back of the stack.
    fn restack(&mut self, _ids: &[Self::Id], _front: bool) {}

    /// Can a double-click edit this item in place (a note's text)?
    fn editable(&self, _id: Self::Id) -> bool {
        false
    }

    /// Draw the in-place editor over `screen` (the item's box on screen).
    /// Return true when the item changed.
    fn edit_ui(&mut self, _id: Self::Id, _ui: &mut Ui, _screen: Rect) -> bool {
        false
    }

    /// Something on the canvas moves by itself (a playing clip): repaint.
    fn animating(&self) -> bool {
        false
    }

    /// The clock `Pen::time` reports.
    fn time(&self) -> f64 {
        0.0
    }
}

/// The world-space corners of an item's rect, clockwise from top-left.
pub fn corners<S: Scene>(s: &S, id: S::Id) -> [Pos2; 4] {
    let r = s.local_rect(id);
    let xf = s.xform(id);
    [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()].map(|p| xf.apply(p))
}

/// The world-space bounding box of an item.
pub fn bounds<S: Scene>(s: &S, id: S::Id) -> Rect {
    let c = corners(s, id);
    let mut r = Rect::from_min_max(c[0], c[0]);
    c[1..].iter().for_each(|p| r.extend_with(*p));
    r
}
