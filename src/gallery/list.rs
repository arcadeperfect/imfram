//! The gallery's items as rows: a thumbnail that scrubs, the name with
//! its badge, then the app's own columns ([`Details`]). Same hands as the
//! grid (clicks, keys, drag out, cmd-wheel to resize), same [`Thumbs`]
//! cache, same [`GalleryEvent`]; a header click sorts like the table.
//!
//! Hands:
//! - click selects, shift-click a range, cmd/ctrl-click toggles;
//!   double-click or Enter activates; Space previews; right-click a menu
//! - move across a clip's thumbnail to scrub its frames
//! - arrows walk the rows (shift extends), Home / End, Page Up / Down;
//!   A or cmd-A all, alt-A none, ctrl-I inverts
//! - ctrl/cmd + wheel or pinch, or + / -, resize the rows (the thumbnails
//!   with them); the row under the pointer stays put
//! - click a header to sort (again: the other way); drag a boundary to
//!   size the column right of it (the name takes up the slack)
//! - drag a row out: the selection travels as a [`Dragged`] payload
//!
//! The list never reorders data: `sort_changed` says the source should.

use std::collections::{BTreeSet, HashMap};
use std::ops::Range;

use egui::text::{LayoutJob, TextFormat, TextWrapping};
use egui::{pos2, vec2, Align, Align2, Color32, CursorIcon, Id, Key, Modifiers, Pos2, Rect, Sense, Stroke, StrokeKind, Ui};

use super::{bucket, hands, paint, tile, Dragged, GalleryEvent, Items, Thumbs, SIZE_STEP};
use crate::table::Column;

pub const HEADER_H: f32 = 22.0;
pub const MIN_ROW: f32 = 18.0;
pub const MAX_ROW: f32 = 160.0;
/// A cell's content inset from its edges.
const PAD: f32 = 6.0;
/// The thumbnail's inset from the row's top and bottom.
const INSET: f32 = 2.0;
const GRIP_W: f32 = 8.0;
const COL_MIN_W: f32 = 32.0;
const GLIDE: f32 = 0.12;

/// The columns past the name, as text.
pub trait Details: Items {
    /// Item i's text in column `col` (1..; column 0 is the name).
    fn cell(&self, i: usize, col: usize) -> String;
}

pub struct List {
    /// Row height asked for, in points; the thumbnail fills it.
    pub row_h: f32,
    /// The thumbnail box's width over its height.
    pub thumb_aspect: f32,
    /// Column 0 is the name and stretches; the rest keep their widths.
    pub columns: Vec<Column>,
    /// `(column, ascending)`.
    pub sort: Option<(usize, bool)>,
    pub scrub: bool,
    /// Crop thumbnails to fill their box, or letterbox them.
    pub fill: bool,
    pub striped: bool,
    /// The item's badge after its name (off when a column already says it).
    pub name_badge: bool,
    pub selection: BTreeSet<u64>,
    pub(crate) cursor: Option<usize>,
    pub(crate) anchor: Option<usize>,
    pub(crate) scroll_to: Option<usize>,
    hold: HashMap<u64, u32>,
    keep: Option<(usize, f32)>,
    scroll_y: f32,
    visible: Range<usize>,
    /// The body's top left on screen last frame (tests find rows by it).
    pub(crate) origin: Pos2,
    shown_h: f32,
}

impl Default for List {
    fn default() -> Self {
        Self::new(vec![Column::new("Name", 200.0)])
    }
}

/// Where each column sits across a row: the thumbnail, then `columns`.
struct Cols {
    thumb: f32,
    edges: Vec<(f32, f32)>,
}

impl List {
    pub fn new(columns: Vec<Column>) -> Self {
        Self {
            row_h: 36.0,
            thumb_aspect: 16.0 / 9.0,
            columns,
            sort: None,
            scrub: true,
            fill: true,
            striped: true,
            name_badge: true,
            selection: BTreeSet::new(),
            cursor: None,
            anchor: None,
            scroll_to: None,
            hold: HashMap::new(),
            keep: None,
            scroll_y: 0.0,
            visible: 0..0,
            origin: Pos2::ZERO,
            shown_h: 36.0,
        }
    }

    /// The rows laid out last frame.
    pub fn visible(&self) -> Range<usize> {
        self.visible.clone()
    }

    pub fn cursor(&self) -> Option<usize> {
        self.cursor
    }

    /// Bring item i into view next frame.
    pub fn scroll_to(&mut self, i: usize) {
        self.scroll_to = Some(i);
    }

    /// Row i's rect on screen as of last frame.
    pub fn row_rect(&self, i: usize, width: f32) -> Rect {
        Rect::from_min_size(self.origin + vec2(0.0, i as f32 * self.shown_h), vec2(width, self.shown_h))
    }

    fn cols(&self, width: f32, row_h: f32) -> Cols {
        let thumb = ((row_h - 2.0 * INSET) * self.thumb_aspect + 2.0 * PAD).round();
        let fixed: f32 = self.columns.iter().skip(1).map(|c| c.width).sum();
        let name = (width - thumb - fixed).max(self.columns.first().map_or(COL_MIN_W, |c| c.width.min(COL_MIN_W * 3.0)));
        let mut x = thumb;
        let mut edges = vec![];
        for (k, c) in self.columns.iter().enumerate() {
            let w = if k == 0 { name } else { c.width };
            edges.push((x, x + w));
            x += w;
        }
        Cols { thumb, edges }
    }

    pub fn show<I: Details>(&mut self, ui: &mut Ui, items: &I, thumbs: &mut Thumbs<I::Src>) -> GalleryEvent {
        let mut ev = GalleryEvent::default();
        let id = ui.make_persistent_id("imfram.list");
        let n = items.len();
        self.cursor = self.cursor.filter(|&c| c < n);
        self.anchor = self.anchor.filter(|&a| a < n);
        let full = ui.available_rect_before_wrap();
        ui.interact(full, id, Sense::focusable_noninteractive());
        let focused = ui.memory(|m| m.has_focus(id));
        self.resize(ui, full, focused);
        let row_h = ui.ctx().animate_value_with_time(id.with("row_h"), self.row_h, GLIDE);
        self.shown_h = row_h;
        let look = paint::Look::from(ui);
        let width = full.width() - ui.spacing().scroll.allocated_width();
        let cols = self.cols(width, row_h);
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            self.header(ui, id, &cols, &look, &mut ev);
            let mut area = egui::ScrollArea::vertical().id_salt(id).auto_shrink([false, false]);
            if let Some((i, below)) = self.keep {
                if i < n {
                    area = area.vertical_scroll_offset((i as f32 * row_h - below).max(0.0));
                }
                if (row_h - self.row_h).abs() < 0.01 {
                    self.keep = None;
                }
            }
            let out = area.show_viewport(ui, |ui, vp| {
                let origin = ui.max_rect().min;
                self.origin = origin;
                let content = Rect::from_min_size(origin, vec2(width, (n as f32 * row_h).max(vp.height())));
                ui.set_min_size(content.size());
                self.background(ui, id, content, &mut ev);
                let first = ((vp.top() / row_h).floor().max(0.0) as usize).min(n);
                let last = ((vp.bottom() / row_h).ceil().max(0.0) as usize).min(n);
                let ppp = ui.ctx().pixels_per_point();
                let tw = cols.thumb - 2.0 * PAD;
                let px = bucket(tw.max(row_h) * ppp * if self.fill { 1.5 } else { 1.0 });
                for i in first..last {
                    let near = 1.0 - (((i as f32 + 0.5) * row_h - vp.center().y).abs() / vp.height().max(1.0)).min(1.0);
                    self.row(ui, id, i, items, thumbs, &cols, width, row_h, px, 2.0 + near, focused, &look, &mut ev);
                }
                // A screen's worth below and half above, for scrolling into.
                let page = (vp.height() / row_h).ceil() as usize;
                let ahead = last..(last + page).min(n);
                let behind = first.saturating_sub(page / 2)..first;
                for i in ahead.chain(behind).filter(|&i| items.has_picture(i)) {
                    let _ = thumbs.want(&items.src(i), items.poster(i), px, 1.0);
                }
                if focused {
                    self.keys(ui, items, page.max(1), &mut ev);
                }
                if let Some(i) = self.scroll_to.take().filter(|&i| i < n) {
                    let r = Rect::from_min_size(origin + vec2(0.0, i as f32 * row_h), vec2(width, row_h));
                    ui.scroll_to_rect(r, None);
                }
                self.visible = first..last;
            });
            self.scroll_y = out.state.offset.y;
        });
        tile::drag_badge(ui, id, &look);
        thumbs.flush(ui.ctx());
        ev
    }

    /// Titles and sort arrows; boundaries to drag.
    fn header(&mut self, ui: &mut Ui, id: Id, cols: &Cols, look: &paint::Look, ev: &mut GalleryEvent) {
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), HEADER_H), Sense::hover());
        let v = ui.visuals();
        let (bg, text, sep) = (v.widgets.inactive.weak_bg_fill, v.strong_text_color(), v.widgets.noninteractive.bg_stroke);
        let p = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
        p.rect_filled(rect, 0.0, bg);
        p.vline(rect.left() + cols.thumb, rect.y_range(), sep);
        for (k, &(x0, x1)) in cols.edges.iter().enumerate() {
            let cell = Rect::from_min_max(pos2(rect.left() + x0, rect.top()), pos2(rect.left() + x1, rect.bottom()));
            let col = &self.columns[k];
            let sorted = self.sort.filter(|s| s.0 == k);
            let arrow = if sorted.is_some() { 12.0 } else { 0.0 };
            let (anchor, tx) = match col.align {
                Align::Max => (Align2::RIGHT_CENTER, cell.right() - PAD - arrow),
                Align::Center => (Align2::CENTER_CENTER, cell.center().x),
                Align::Min => (Align2::LEFT_CENTER, cell.left() + PAD),
            };
            p.with_clip_rect(cell.intersect(p.clip_rect())).text(pos2(tx, cell.center().y), anchor, &col.title, look.font.clone(), text);
            if let Some((_, up)) = sorted {
                p.text(pos2(cell.right() - PAD, cell.center().y), Align2::RIGHT_CENTER, if up { "⏶" } else { "⏷" }, look.small.clone(), text);
            }
            p.vline(cell.right(), rect.y_range(), sep);
            if col.sortable && ui.interact(cell, id.with(("h", k)), Sense::click()).clicked() {
                self.sort = Some(match sorted {
                    Some((_, up)) => (k, !up),
                    None => (k, true),
                });
                ev.sort_changed = true;
            }
        }
        // A boundary sizes the column on its right; the name takes the slack.
        for k in 1..cols.edges.len() {
            if !self.columns[k].resizable {
                continue;
            }
            let x = rect.left() + cols.edges[k].0;
            let grip = Rect::from_center_size(pos2(x, rect.center().y), vec2(GRIP_W, HEADER_H));
            let g = ui.interact(grip, id.with(("g", k)), Sense::drag());
            if g.hovered() || g.dragged() {
                ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
            }
            if g.dragged() {
                let c = &mut self.columns[k];
                c.width = (c.width - g.drag_delta().x).max(COL_MIN_W);
            }
        }
        p.hline(rect.x_range(), rect.bottom(), sep);
    }

    #[allow(clippy::too_many_arguments)]
    fn row<I: Details>(&mut self, ui: &mut Ui, id: Id, i: usize, items: &I, thumbs: &mut Thumbs<I::Src>, cols: &Cols, width: f32, row_h: f32, px: u32, prio: f32, focused: bool, look: &paint::Look, ev: &mut GalleryEvent) {
        let iid = items.id(i);
        let rect = Rect::from_min_size(self.origin + vec2(0.0, i as f32 * row_h), vec2(width, row_h));
        let resp = ui.interact(rect, id.with(("row", iid)), Sense::click_and_drag());
        let selected = self.selection.contains(&iid);
        let thumb_cell = Rect::from_min_size(rect.min, vec2(cols.thumb, row_h));
        let area = thumb_cell.shrink2(vec2(PAD, INSET));
        let scrub_at = resp.hover_pos().filter(|p| self.scrub && thumb_cell.contains(*p) && !resp.dragged());
        let (shown, scrub) = tile::picture(thumbs, &mut self.hold, items, i, area, scrub_at, px, prio);
        if ui.is_rect_visible(rect) {
            let p = ui.painter();
            let v = ui.visuals();
            if selected {
                p.rect_filled(rect, 0.0, look.sel_fill);
            } else if resp.hovered() {
                p.rect_filled(rect, 0.0, v.widgets.hovered.weak_bg_fill);
            } else if self.striped && i % 2 == 1 {
                p.rect_filled(rect, 0.0, v.faint_bg_color);
            }
            let src = items.src(i);
            let kind = items.kind(i);
            let t = paint::Tile {
                label: "",
                kind: if area.height() >= 24.0 { kind.as_deref() } else { None },
                badge: None,
                selected,
                hovered: resp.hovered(),
                cursor: false,
                dim: items.dim(i),
                failed: thumbs.failed(&src, items.poster(i)).is_some(),
                fill: self.fill,
                scrub,
            };
            paint::thumb(p, area, shown.as_ref(), &t, look);
            let ink = if selected {
                look.sel_text
            } else if items.dim(i) {
                look.weak
            } else {
                look.text
            };
            let weak = if selected { look.sel_text.gamma_multiply(0.75) } else { look.weak };
            for (k, &(x0, x1)) in cols.edges.iter().enumerate() {
                let cell = Rect::from_min_max(pos2(rect.left() + x0 + PAD, rect.top()), pos2(rect.left() + x1 - PAD, rect.bottom()));
                if cell.width() <= 2.0 {
                    continue;
                }
                let p = p.with_clip_rect(cell.intersect(p.clip_rect()));
                if k == 0 {
                    // The name, then its badge (a frame range) right-aligned in weak ink.
                    let mut room = cell.width();
                    if let Some(b) = items.badge(i).filter(|_| self.name_badge) {
                        let g = p.layout_no_wrap(b, look.small.clone(), weak);
                        if g.size().x + 40.0 < room {
                            room -= g.size().x + PAD;
                            p.galley(pos2(cell.right() - g.size().x, cell.center().y - g.size().y / 2.0), g, weak);
                        }
                    }
                    text(&p, cell, Align::Min, &items.label(i), room, look.font.clone(), ink);
                } else {
                    text(&p, cell, self.columns[k].align, &items.cell(i, k), cell.width(), look.small.clone(), weak);
                }
            }
            if focused && self.cursor == Some(i) && !selected {
                p.rect_stroke(rect.shrink(0.5), 0.0, Stroke::new(1.0, look.text.gamma_multiply(0.5)), StrokeKind::Inside);
            }
        }
        if resp.clicked() || resp.double_clicked() || resp.secondary_clicked() || resp.drag_started() {
            ui.memory_mut(|m| m.request_focus(id));
        }
        let mods = ui.input(|inp| inp.modifiers);
        if resp.double_clicked() {
            ev.activated = Some(iid);
        } else if resp.clicked() {
            self.click(items, i, mods, ev);
        }
        if resp.secondary_clicked() {
            if !selected {
                self.click(items, i, Modifiers::NONE, ev);
            }
            ev.context = Some((Some(iid), resp.interact_pointer_pos().unwrap_or(rect.center())));
        }
        if resp.drag_started() {
            if !self.selection.contains(&iid) {
                self.click(items, i, Modifiers::NONE, ev);
            }
            let ids: Vec<u64> = (0..items.len()).map(|j| items.id(j)).filter(|j| self.selection.contains(j)).collect();
            resp.dnd_set_drag_payload(Dragged(ids));
            ev.drag_started = true;
        }
    }

    fn click<I: Items>(&mut self, items: &I, i: usize, mods: Modifiers, ev: &mut GalleryEvent) {
        ev.selection_changed |= hands::pick(&mut self.selection, &mut self.anchor, &mut self.cursor, items, i, mods);
    }

    /// Below the last row: a click clears (and takes focus), right-click a menu.
    fn background(&mut self, ui: &mut Ui, id: Id, content: Rect, ev: &mut GalleryEvent) {
        let r = ui.interact(content, id.with("bg"), Sense::click());
        if r.clicked() || r.secondary_clicked() {
            ui.memory_mut(|m| m.request_focus(id));
        }
        let mods = ui.input(|i| i.modifiers);
        if r.clicked() && !mods.shift && !mods.command && !self.selection.is_empty() {
            self.selection.clear();
            ev.selection_changed = true;
        }
        if r.secondary_clicked() {
            ev.context = Some((None, r.interact_pointer_pos().unwrap_or(content.center())));
        }
    }

    fn keys<I: Items>(&mut self, ui: &mut Ui, items: &I, page: usize, ev: &mut GalleryEvent) {
        let n = items.len();
        if n == 0 || hands::select_keys(ui, &mut self.selection, self.cursor, items, ev) {
            return;
        }
        let mods = ui.input(|i| i.modifiers);
        let press = |ui: &mut Ui, k: Key| ui.input_mut(|i| i.consume_key(mods, k));
        let cur = self.cursor.map(|c| c as i64);
        let mut go = None;
        for (k, step) in [(Key::ArrowUp, -1i64), (Key::ArrowDown, 1), (Key::PageUp, -(page as i64)), (Key::PageDown, page as i64)] {
            if press(ui, k) {
                go = Some(cur.map_or(0, |c| (c + step).clamp(0, n as i64 - 1)) as usize);
            }
        }
        if press(ui, Key::Home) {
            go = Some(0);
        }
        if press(ui, Key::End) {
            go = Some(n - 1);
        }
        if let Some(i) = go {
            if mods.command && !mods.shift {
                self.cursor = Some(i);
            } else {
                self.click(items, i, Modifiers { shift: mods.shift, ..Modifiers::NONE }, ev);
            }
            self.scroll_to = Some(i);
        }
    }

    /// Zoom gestures and + / -; the row under the pointer holds still.
    fn resize(&mut self, ui: &mut Ui, full: Rect, focused: bool) {
        let hovered = ui.rect_contains_pointer(full);
        let mut k = if hovered { ui.input(|i| i.zoom_delta()) } else { 1.0 };
        if focused {
            ui.input_mut(|i| {
                if i.consume_key(Modifiers::NONE, Key::Plus) || i.consume_key(Modifiers::NONE, Key::Equals) {
                    k *= SIZE_STEP;
                }
                if i.consume_key(Modifiers::NONE, Key::Minus) {
                    k /= SIZE_STEP;
                }
            });
        }
        if (k - 1.0).abs() < 1e-4 {
            return;
        }
        self.row_h = (self.row_h * k).clamp(MIN_ROW, MAX_ROW);
        if self.keep.is_none() && self.shown_h > 0.0 {
            let y = ui.ctx().pointer_hover_pos().map(|p| p.y - full.top() - HEADER_H + self.scroll_y);
            let i = y.map(|y| (y / self.shown_h).max(0.0) as usize).or(self.cursor).unwrap_or(self.visible.start);
            self.keep = Some((i, i as f32 * self.shown_h - self.scroll_y));
        }
    }
}

/// One line of text in `cell`, cut with an ellipsis at `room` wide.
fn text(p: &egui::Painter, cell: Rect, align: Align, s: &str, room: f32, font: egui::FontId, color: Color32) {
    if s.is_empty() {
        return;
    }
    let mut job = LayoutJob::single_section(s.to_owned(), TextFormat { font_id: font, color, ..Default::default() });
    job.wrap = TextWrapping::truncate_at_width(room.max(8.0));
    let g = p.layout_job(job);
    let x = match align {
        Align::Max => cell.right() - g.size().x,
        Align::Center => cell.center().x - g.size().x / 2.0,
        Align::Min => cell.left(),
    };
    p.galley(pos2(x, cell.center().y - g.size().y / 2.0), g, color);
}
