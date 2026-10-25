//! The outliner's rows: the model flattened to what is on screen, one row
//! painted (indent, disclosure triangle, icon, label), the inline rename
//! field, and the drop slot a drag is over. Nothing here touches state;
//! `mod.rs` decides, this draws.

use std::collections::HashSet;

use egui::text::{CCursor, CCursorRange};
use egui::{Align2, Color32, FontId, Id, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, Ui, UiBuilder, Vec2};

use super::TreeModel;

/// Height of one row, in points.
pub const ROW_H: f32 = 20.0;
/// Horizontal step per depth.
pub const INDENT: f32 = 14.0;
/// The disclosure triangle's column.
pub const TRI_W: f32 = 14.0;
/// An icon's (or a swatch's) column.
pub const ICON_W: f32 = 18.0;
/// A checkbox's column.
pub const CHECK_W: f32 = 18.0;
/// A checkbox's side, and a swatch's.
const BOX: f32 = 12.0;
/// Space after the triangle's column.
pub const GAP: f32 = 4.0;
/// The drop line's thickness.
const SLOT_W: f32 = 2.0;
/// The top and bottom bands of a row that mean "between", not "onto".
const SLOT_BAND: f32 = 0.25;

/// A node as it sits on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row<Id> {
    pub id: Id,
    pub parent: Option<Id>,
    /// Index among its siblings.
    pub index: usize,
    pub depth: usize,
    pub has_children: bool,
    /// Nothing on show has children: a flat list, with no column kept for
    /// the triangles.
    pub flat: bool,
}

/// Every node under an expanded ancestor, in display order.
pub fn flatten<M: TreeModel>(m: &M, expanded: &HashSet<M::Id>) -> Vec<Row<M::Id>> {
    let mut out = Vec::new();
    let mut stack: Vec<(M::Id, Option<M::Id>, usize, usize)> =
        m.roots().into_iter().enumerate().rev().map(|(i, id)| (id, None, i, 0)).collect();
    while let Some((id, parent, index, depth)) = stack.pop() {
        let kids = m.children(id);
        out.push(Row { id, parent, index, depth, has_children: !kids.is_empty(), flat: false });
        if expanded.contains(&id) {
            stack.extend(kids.into_iter().enumerate().rev().map(|(i, k)| (k, Some(id), i, depth + 1)));
        }
    }
    let flat = out.iter().all(|r| r.depth == 0 && !r.has_children);
    for r in &mut out {
        r.flat = flat;
    }
    out
}

/// A node and everything below it, expanded or not.
pub fn subtree<M: TreeModel>(m: &M, id: M::Id) -> HashSet<M::Id> {
    let mut set = HashSet::new();
    let mut stack = vec![id];
    while let Some(n) = stack.pop() {
        if set.insert(n) {
            stack.extend(m.children(n));
        }
    }
    set
}

/// Where a node sits now: its parent and index among siblings.
pub fn locate<M: TreeModel>(m: &M, id: M::Id) -> Option<(Option<M::Id>, usize)> {
    let mut stack: Vec<(M::Id, Option<M::Id>, usize)> = m.roots().into_iter().enumerate().map(|(i, r)| (r, None, i)).collect();
    while let Some((n, parent, index)) = stack.pop() {
        if n == id {
            return Some((parent, index));
        }
        stack.extend(m.children(n).into_iter().enumerate().map(|(i, k)| (k, Some(n), i)));
    }
    None
}

/// The theme, read once per frame from `Visuals`.
pub struct Paint {
    pub font: FontId,
    pub text: Color32,
    pub text_selected: Color32,
    pub weak: Color32,
    pub hover: Color32,
    pub selected: Color32,
    pub accent: Color32,
    pub check_bg: Color32,
}

impl Paint {
    pub fn from(ui: &Ui) -> Self {
        let v = ui.visuals();
        Self {
            font: egui::TextStyle::Body.resolve(ui.style()),
            text: v.text_color(),
            text_selected: v.selection.stroke.color,
            weak: v.weak_text_color(),
            hover: v.widgets.hovered.weak_bg_fill,
            selected: v.selection.bg_fill,
            accent: v.selection.stroke.color,
            check_bg: v.widgets.inactive.bg_fill,
        }
    }
}

pub struct Look {
    pub selected: bool,
    pub hovered: bool,
    pub dragged: bool,
}

pub fn tri_rect<Id>(rect: Rect, row: &Row<Id>) -> Rect {
    let x = rect.left() + row.depth as f32 * INDENT;
    let w = if row.flat { 0.0 } else { TRI_W };
    Rect::from_min_size(Pos2::new(x, rect.top()), Vec2::new(w, rect.height()))
}

/// What sits left of a row's label, from the model.
pub struct Lead<'a> {
    pub check: Option<super::Check>,
    pub swatch: Option<Color32>,
    pub icon: Option<&'a str>,
    pub dimmed: bool,
}

impl Lead<'_> {
    fn width(&self) -> f32 {
        let check = if self.check.is_some() { CHECK_W } else { 0.0 };
        let icon = if self.swatch.is_some() || self.icon.is_some() { ICON_W } else { 0.0 };
        check + icon
    }
}

/// The checkbox's column, right of the triangle's.
pub fn check_rect<Id>(rect: Rect, row: &Row<Id>) -> Rect {
    let x = tri_rect(rect, row).right() + GAP;
    Rect::from_min_size(Pos2::new(x, rect.top()), Vec2::new(CHECK_W, rect.height()))
}

pub fn label_rect<Id>(rect: Rect, row: &Row<Id>, lead: &Lead) -> Rect {
    let x = tri_rect(rect, row).right() + GAP + lead.width();
    Rect::from_min_max(Pos2::new(x, rect.top()), rect.max)
}

/// Background, triangle, icon; the label unless `skip_label` (a rename
/// field sits there instead).
#[allow(clippy::too_many_arguments)]
pub fn paint_row<Id>(p: &Painter, rect: Rect, row: &Row<Id>, label: &str, lead: &Lead, expanded: bool, look: &Look, pt: &Paint, skip_label: bool) {
    if look.selected {
        p.rect_filled(rect, 0.0, pt.selected);
    } else if look.hovered {
        p.rect_filled(rect, 0.0, pt.hover);
    }
    let fg = if look.dragged { pt.weak } else if look.selected { pt.text_selected } else { pt.text };
    if row.has_children {
        let t = tri_rect(rect, row);
        let c = t.center();
        let r = 3.5;
        let pts = if expanded {
            vec![c + Vec2::new(-r, -r * 0.6), c + Vec2::new(r, -r * 0.6), c + Vec2::new(0.0, r * 0.8)]
        } else {
            vec![c + Vec2::new(-r * 0.6, -r), c + Vec2::new(r * 0.8, 0.0), c + Vec2::new(-r * 0.6, r)]
        };
        p.add(Shape::convex_polygon(pts, if look.selected { fg } else { pt.weak }, Stroke::NONE));
    }
    let mut x = tri_rect(rect, row).right() + GAP;
    if let Some(c) = lead.check {
        paint_check(p, Rect::from_center_size(check_rect(rect, row).center(), Vec2::splat(BOX)), c, pt);
        x += CHECK_W;
    }
    if let Some(sw) = lead.swatch {
        let r = Rect::from_center_size(Pos2::new(x + ICON_W * 0.5, rect.center().y), Vec2::splat(BOX));
        p.rect_filled(r, 2.0, sw);
        x += ICON_W;
    } else if let Some(ic) = lead.icon {
        p.text(Pos2::new(x + ICON_W * 0.5, rect.center().y), Align2::CENTER_CENTER, ic, pt.font.clone(), fg);
        x += ICON_W;
    }
    let fg = if lead.dimmed && !look.selected { pt.weak } else { fg };
    if !skip_label {
        let clip = Rect::from_min_max(Pos2::new(x, rect.top()), rect.max);
        p.with_clip_rect(clip.intersect(p.clip_rect())).text(Pos2::new(x, rect.center().y), Align2::LEFT_CENTER, label, pt.font.clone(), fg);
    }
}

/// A checkbox: a box, ticked when on; faint when it cannot be clicked.
fn paint_check(p: &Painter, r: Rect, c: super::Check, pt: &Paint) {
    let alpha = if c.enabled { 1.0 } else { 0.4 };
    p.rect_filled(r, 2.0, pt.check_bg.gamma_multiply(alpha));
    if c.on {
        let tick = [r.left_center() + Vec2::new(2.5, 0.0), r.center_bottom() + Vec2::new(-1.0, -3.0), r.right_top() + Vec2::new(-2.5, 3.0)];
        p.add(Shape::line(tick.to_vec(), Stroke::new(1.6, pt.text.gamma_multiply(alpha))));
    }
}

pub enum RenameOut {
    Editing,
    Commit,
    Cancel,
}

/// The inline rename field over a label. `fresh` is its first frame: it
/// takes focus and selects everything.
pub fn rename_field(ui: &mut Ui, rect: Rect, id: Id, text: &mut String, fresh: bool) -> RenameOut {
    let out = ui
        .scope_builder(UiBuilder::new().max_rect(rect), |ui| {
            egui::TextEdit::singleline(text)
                .id(id)
                .frame(egui::Frame::NONE)
                .margin(egui::Margin::ZERO)
                .vertical_align(egui::Align::Center)
                .desired_width(rect.width())
                .show(ui)
        })
        .inner;
    let resp = out.response.response;
    if fresh {
        resp.request_focus();
        let mut st = out.state;
        let n = text.chars().count();
        st.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(0), CCursor::new(n))));
        st.store(ui.ctx(), resp.id);
        return RenameOut::Editing;
    }
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        RenameOut::Cancel
    } else if resp.lost_focus() {
        RenameOut::Commit
    } else {
        RenameOut::Editing
    }
}

/// Where a drag would land, as a position among the rows on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Before(usize),
    After(usize),
    Onto(usize),
}

/// `rects` are the rows laid out this frame, `(row index, rect)`, in order.
pub fn slot_at(p: Pos2, rects: &[(usize, Rect)]) -> Option<Slot> {
    let (first, last) = (rects.first()?, rects.last()?);
    if p.y < first.1.top() {
        return Some(Slot::Before(first.0));
    }
    if p.y >= last.1.bottom() {
        return Some(Slot::After(last.0));
    }
    let (i, r) = rects.iter().find(|(_, r)| r.y_range().contains(p.y))?;
    let f = (p.y - r.top()) / r.height();
    Some(if f < SLOT_BAND {
        Slot::Before(*i)
    } else if f > 1.0 - SLOT_BAND {
        Slot::After(*i)
    } else {
        Slot::Onto(*i)
    })
}

/// A line in the gap for Before / After (indented to the depth it lands
/// at), an outline for Onto.
pub fn paint_slot<Id>(p: &Painter, slot: Slot, rects: &[(usize, Rect)], rows: &[Row<Id>], into_expanded: bool, colour: Color32) {
    let rect_of = |i: usize| rects.iter().find(|(k, _)| *k == i).map(|(_, r)| *r);
    let stroke = Stroke::new(SLOT_W, colour);
    match slot {
        Slot::Before(i) => {
            if let Some(r) = rect_of(i) {
                let x = tri_rect(r, &rows[i]).left();
                p.hline(x..=r.right(), r.top(), stroke);
            }
        }
        Slot::After(i) => {
            if let Some(r) = rect_of(i) {
                let depth = rows[i].depth + usize::from(into_expanded);
                p.hline(r.left() + depth as f32 * INDENT..=r.right(), r.bottom(), stroke);
            }
        }
        Slot::Onto(i) => {
            if let Some(r) = rect_of(i) {
                p.rect_stroke(r.shrink(1.0), 2.0, stroke, StrokeKind::Inside);
            }
        }
    }
}
