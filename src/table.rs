//! A table: columns the app declares ([`Column`]), rows it serves through
//! [`Rows`] (a count and a cell painter, so any source fits: a Vec, a
//! query, a file listing). A [`Table`] keeps the sort, the selection (by
//! the source's row ids, so it survives a re-sort) and the column widths,
//! and answers each frame with a [`TableEvent`]. It never reorders data:
//! `sort_changed` says the source should sort itself.
//!
//! Hands: click a header to sort (again: the other way), drag a boundary to
//! resize; click a row, shift-click a range, cmd/ctrl-click toggles, a
//! double-click activates. Rows are virtual and striped; columns past the
//! width scroll sideways with the header following. Colours come from
//! `Visuals`.

use std::collections::BTreeSet;
use std::ops::Range;

use egui::{Align, Align2, Color32, CursorIcon, Direction, FontId, Id, Layout, Modifiers, Pos2, Rect, Sense, Stroke, Ui, UiBuilder, Vec2};

pub const ROW_H: f32 = 20.0;
pub const HEADER_H: f32 = 22.0;
/// Inset of a cell's content from the column's edges.
pub const CELL_PAD: f32 = 6.0;
/// The grab zone straddling a column boundary.
pub const GRIP_W: f32 = 8.0;
pub const COL_MIN_W: f32 = 24.0;
const ARROW_W: f32 = 12.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Column {
    pub title: String,
    pub width: f32,
    pub resizable: bool,
    pub sortable: bool,
    pub align: Align,
}

impl Column {
    pub fn new(title: impl Into<String>, width: f32) -> Self {
        Self { title: title.into(), width, resizable: true, sortable: true, align: Align::LEFT }
    }
    pub fn align(mut self, a: Align) -> Self {
        self.align = a;
        self
    }
    pub fn fixed(mut self) -> Self {
        self.resizable = false;
        self
    }
    pub fn unsortable(mut self) -> Self {
        self.sortable = false;
        self
    }
}

/// The app's rows. `cell` draws into a `Ui` sized to the cell; `id` is
/// what the selection remembers, so it should follow the row when the
/// source sorts.
pub trait Rows {
    fn len(&self) -> usize;
    fn cell(&self, ui: &mut Ui, row: usize, col: usize);
    fn id(&self, row: usize) -> u64 {
        row as u64
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TableEvent {
    /// `Table::sort` changed: the source should sort itself.
    pub sort_changed: bool,
    pub selection_changed: bool,
    /// A row was double-clicked (its id).
    pub activated: Option<u64>,
}

pub struct Table {
    pub columns: Vec<Column>,
    /// `(column, ascending)`.
    pub sort: Option<(usize, bool)>,
    pub selection: BTreeSet<u64>,
    pub striped: bool,
    /// Where a shift-range starts (a row index).
    anchor: Option<usize>,
    /// The body's sideways scroll, which the header follows.
    scroll_x: f32,
    visible: Range<usize>,
}

impl Table {
    pub fn new(columns: Vec<Column>) -> Self {
        Self { columns, sort: None, selection: BTreeSet::new(), striped: true, anchor: None, scroll_x: 0.0, visible: 0..0 }
    }

    /// The columns' width end to end.
    pub fn content_width(&self) -> f32 {
        self.columns.iter().map(|c| c.width).sum()
    }

    /// The rows laid out last frame (the virtual window).
    pub fn visible(&self) -> Range<usize> {
        self.visible.clone()
    }

    pub fn scroll_x(&self) -> f32 {
        self.scroll_x
    }

    pub fn show(&mut self, ui: &mut Ui, rows: &dyn Rows) -> TableEvent {
        let mut ev = TableEvent::default();
        let id = ui.make_persistent_id("imfram.table");
        let pt = Paint::from(ui);
        let total = self.content_width();
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            self.header(ui, id, &pt, &mut ev);
            let out = egui::ScrollArea::both().id_salt(id).auto_shrink([false, false]).show_rows(ui, ROW_H, rows.len(), |ui, range| {
                ui.set_min_width(total);
                self.visible = range.clone();
                let top0 = ui.max_rect().top() - range.start as f32 * ROW_H;
                let left = ui.max_rect().left();
                let w = total.max(ui.max_rect().width());
                for i in range {
                    let rect = Rect::from_min_size(Pos2::new(left, top0 + i as f32 * ROW_H), Vec2::new(w, ROW_H));
                    self.row(ui, rows, i, rect, id, &pt, &mut ev);
                }
            });
            self.scroll_x = out.state.offset.x;
        });
        ev
    }

    /// Titles, sort arrows, boundaries to drag; shifted by the body's scroll.
    fn header(&mut self, ui: &mut Ui, id: Id, pt: &Paint, ev: &mut TableEvent) {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), HEADER_H), Sense::hover());
        let p = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
        p.rect_filled(rect, 0.0, pt.header_bg);
        let mut x = rect.left() - self.scroll_x;
        for c in 0..self.columns.len() {
            let col = &self.columns[c];
            let cell = Rect::from_min_size(Pos2::new(x, rect.top()), Vec2::new(col.width, HEADER_H));
            x += col.width;
            let sorted = self.sort.filter(|s| s.0 == c);
            let (anchor, tx) = match col.align {
                Align::Max => (Align2::RIGHT_CENTER, cell.right() - CELL_PAD - if sorted.is_some() { ARROW_W } else { 0.0 }),
                Align::Center => (Align2::CENTER_CENTER, cell.center().x),
                Align::Min => (Align2::LEFT_CENTER, cell.left() + CELL_PAD),
            };
            p.text(Pos2::new(tx, cell.center().y), anchor, &col.title, pt.font.clone(), pt.header_text);
            if let Some((_, up)) = sorted {
                p.text(Pos2::new(cell.right() - CELL_PAD, cell.center().y), Align2::RIGHT_CENTER, if up { "▲" } else { "▼" }, pt.small.clone(), pt.header_text);
            }
            p.vline(cell.right(), rect.y_range(), pt.sep);
            if col.sortable {
                let r = ui.interact(cell.intersect(rect), id.with(("h", c)), Sense::click());
                if r.clicked() {
                    self.sort = Some(match sorted {
                        Some((_, up)) => (c, !up),
                        None => (c, true),
                    });
                    ev.sort_changed = true;
                }
            }
            if col.resizable {
                let grip = Rect::from_center_size(Pos2::new(cell.right(), cell.center().y), Vec2::new(GRIP_W, HEADER_H));
                let g = ui.interact(grip.intersect(rect), id.with(("g", c)), Sense::drag());
                if g.hovered() || g.dragged() {
                    ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
                }
                if g.dragged() {
                    let col = &mut self.columns[c];
                    col.width = (col.width + g.drag_delta().x).max(COL_MIN_W);
                }
            }
        }
        p.hline(rect.x_range(), rect.bottom(), pt.sep);
    }

    #[allow(clippy::too_many_arguments)]
    fn row(&mut self, ui: &mut Ui, rows: &dyn Rows, i: usize, rect: Rect, id: Id, pt: &Paint, ev: &mut TableEvent) {
        let rid = rows.id(i);
        let resp = ui.interact(rect, id.with(("r", i)), Sense::click());
        let selected = self.selection.contains(&rid);
        let p = ui.painter();
        if selected {
            p.rect_filled(rect, 0.0, pt.selected);
        } else if resp.hovered() {
            p.rect_filled(rect, 0.0, pt.hover);
        } else if self.striped && i % 2 == 1 {
            p.rect_filled(rect, 0.0, pt.stripe);
        }
        let mut x = rect.left();
        for (c, col) in self.columns.iter().enumerate() {
            let cell = Rect::from_min_size(Pos2::new(x + CELL_PAD, rect.top()), Vec2::new((col.width - 2.0 * CELL_PAD).max(0.0), ROW_H));
            x += col.width;
            if !ui.is_rect_visible(cell) {
                continue;
            }
            let layout = match col.align {
                Align::Max => Layout::right_to_left(Align::Center),
                Align::Center => Layout::centered_and_justified(Direction::LeftToRight),
                Align::Min => Layout::left_to_right(Align::Center),
            };
            ui.scope_builder(UiBuilder::new().max_rect(cell).layout(layout), |ui| {
                ui.set_clip_rect(cell.intersect(ui.clip_rect()));
                if selected {
                    ui.visuals_mut().override_text_color = Some(pt.text_selected);
                }
                rows.cell(ui, i, c);
            });
        }
        if resp.double_clicked() || resp.triple_clicked() {
            ev.activated = Some(rid);
        } else if resp.clicked() {
            let mods = ui.input(|inp| inp.modifiers);
            self.click(rows, i, mods, ev);
        }
    }

    fn click(&mut self, rows: &dyn Rows, i: usize, mods: Modifiers, ev: &mut TableEvent) {
        let rid = rows.id(i);
        let before = self.selection.clone();
        if mods.shift && self.anchor.is_some() {
            let a = self.anchor.unwrap().min(rows.len().saturating_sub(1));
            self.selection = (a.min(i)..=a.max(i)).map(|r| rows.id(r)).collect();
        } else if mods.command {
            if !self.selection.remove(&rid) {
                self.selection.insert(rid);
            }
            self.anchor = Some(i);
        } else {
            self.selection = BTreeSet::from([rid]);
            self.anchor = Some(i);
        }
        ev.selection_changed |= self.selection != before;
    }
}

/// The theme, read once per frame from `Visuals`.
struct Paint {
    font: FontId,
    small: FontId,
    header_bg: Color32,
    header_text: Color32,
    text_selected: Color32,
    sep: Stroke,
    stripe: Color32,
    hover: Color32,
    selected: Color32,
}

impl Paint {
    fn from(ui: &Ui) -> Self {
        let v = ui.visuals();
        Self {
            font: egui::TextStyle::Body.resolve(ui.style()),
            small: egui::TextStyle::Small.resolve(ui.style()),
            header_bg: v.widgets.inactive.weak_bg_fill,
            header_text: v.strong_text_color(),
            text_selected: v.selection.stroke.color,
            sep: v.widgets.noninteractive.bg_stroke,
            stripe: v.faint_bg_color,
            hover: v.widgets.hovered.weak_bg_fill,
            selected: v.selection.bg_fill,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outliner::drive::Drive;
    use std::cell::Cell;

    struct Toy {
        n: usize,
        cells: Cell<usize>,
    }

    impl Rows for Toy {
        fn len(&self) -> usize {
            self.n
        }
        fn cell(&self, ui: &mut Ui, row: usize, col: usize) {
            self.cells.set(self.cells.get() + 1);
            ui.label(format!("{row}.{col}"));
        }
        fn id(&self, row: usize) -> u64 {
            row as u64 * 10
        }
    }

    fn toy(n: usize) -> (Toy, Table) {
        (Toy { n, cells: Cell::new(0) }, Table::new(vec![Column::new("name", 100.0), Column::new("size", 60.0).align(Align::Max), Column::new("kind", 80.0)]))
    }

    fn head(c: usize) -> Pos2 {
        Pos2::new([50.0, 130.0, 200.0][c], HEADER_H * 0.5)
    }

    fn row(i: usize) -> Pos2 {
        Pos2::new(50.0, HEADER_H + (i as f32 + 0.5) * ROW_H)
    }

    macro_rules! show {
        ($t:ident, $r:ident) => {
            &mut |ui: &mut Ui| $t.show(ui, &$r)
        };
    }

    #[test]
    fn clicking_a_header_sorts_up_then_down_and_another_column_starts_afresh() {
        let (r, mut t) = toy(5);
        let mut d = Drive::new(300.0, 200.0);
        d.idle(1, show!(t, r));
        let ev = d.click(head(0), Modifiers::NONE, show!(t, r));
        assert!(ev.iter().any(|e| e.sort_changed));
        assert_eq!(t.sort, Some((0, true)));
        d.click(head(0), Modifiers::NONE, show!(t, r));
        assert_eq!(t.sort, Some((0, false)));
        d.click(head(1), Modifiers::NONE, show!(t, r));
        assert_eq!(t.sort, Some((1, true)));
    }

    #[test]
    fn dragging_a_boundary_resizes_that_column_only() {
        let (r, mut t) = toy(5);
        let mut d = Drive::new(300.0, 200.0);
        d.idle(1, show!(t, r));
        d.drag(Pos2::new(100.0, HEADER_H * 0.5), Pos2::new(140.0, HEADER_H * 0.5), show!(t, r));
        assert!((t.columns[0].width - 140.0).abs() < 0.5, "{}", t.columns[0].width);
        assert_eq!(t.columns[1].width, 60.0);
        assert_eq!(t.sort, None, "a drag is not a click");
    }

    #[test]
    fn clicks_select_by_id_shift_a_range_cmd_toggles_and_a_double_click_activates() {
        let (r, mut t) = toy(5);
        let mut d = Drive::new(300.0, 200.0);
        d.idle(1, show!(t, r));
        let ev = d.click(row(1), Modifiers::NONE, show!(t, r));
        assert!(ev.iter().any(|e| e.selection_changed));
        assert_eq!(t.selection, BTreeSet::from([10]));
        d.click(row(3), Modifiers::SHIFT, show!(t, r));
        assert_eq!(t.selection, BTreeSet::from([10, 20, 30]));
        d.click(row(2), Modifiers::COMMAND, show!(t, r));
        assert_eq!(t.selection, BTreeSet::from([10, 30]));
        d.idle(40, show!(t, r)); // or the third click in 0.6s counts as a triple
        let ev = d.double_click(row(0), show!(t, r));
        assert!(ev.iter().any(|e| e.activated == Some(0)), "{ev:?}");
    }

    #[test]
    fn only_the_visible_rows_are_asked_for_cells() {
        let (r, mut t) = toy(10_000);
        crate::snapshot::shoot(300.0, 120.0, |ui| {
            t.show(ui, &r);
        });
        assert!(t.visible().len() <= 8, "{:?}", t.visible());
        assert!(r.cells.get() <= 8 * 3, "{}", r.cells.get());
    }

    #[test]
    fn selected_rows_wear_the_selection_colour_and_stripes_alternate() {
        let (r, mut t) = toy(6);
        t.selection.insert(10);
        let shot = crate::snapshot::shoot(300.0, 200.0, |ui| {
            t.show(ui, &r);
        });
        let v = egui::Visuals::dark();
        let near = |a: [u8; 3], b: Color32| a.iter().zip(b.to_array()).all(|(x, y)| (*x as i32 - y as i32).abs() < 8);
        assert!(near(shot.at(row(1).x, row(1).y), v.selection.bg_fill), "{:?}", shot.at(row(1).x, row(1).y));
        // The stripe is additive (dark theme: a few luminance steps over whatever is behind).
        let (even, odd) = (shot.at(row(2).x, row(2).y), shot.at(row(3).x, row(3).y));
        assert!(odd[0] > even[0] && odd != shot.at(row(1).x, row(1).y), "even {even:?} odd {odd:?}");
    }

    #[test]
    fn columns_wider_than_the_view_scroll_sideways_and_the_header_follows() {
        let (r, mut t) = toy(5);
        let mut d = Drive::new(120.0, 200.0);
        d.idle(1, show!(t, r));
        assert_eq!(t.content_width(), 240.0);
        let wheel = egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: Vec2::new(-50.0, 0.0), phase: egui::TouchPhase::Move, modifiers: Modifiers::NONE };
        d.frame(vec![egui::Event::PointerMoved(row(1))], show!(t, r));
        d.frame(vec![wheel], show!(t, r));
        d.idle(3, show!(t, r));
        assert!(t.scroll_x() > 10.0, "{}", t.scroll_x());
    }
}
