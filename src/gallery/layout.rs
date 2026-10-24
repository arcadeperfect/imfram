//! Where every tile sits: pure arithmetic over a width, a tile size and
//! where sections start. No egui state, so it is tested on its own.
//!
//! Columns are as many as fit at the asked size; the leftover width is
//! shared out so a row always reaches both edges (tiles grow, never
//! shrink, by less than one tile's worth). A section starts a new row
//! under a full-width header.

use std::ops::Range;

use egui::{pos2, vec2, Pos2, Rect, Vec2};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    /// Between tiles and around the edge.
    pub gap: f32,
    /// Under each box, for the label (0: no labels).
    pub label_h: f32,
    pub header_h: f32,
    /// The box's width over its height (1: square, 16/9: plates).
    pub aspect: f32,
}

impl Default for Metrics {
    fn default() -> Self {
        Self { gap: 8.0, label_h: 18.0, header_h: 26.0, aspect: 1.0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    /// A section's title, before item `first`.
    Header { first: usize },
    Tiles { start: usize, end: usize },
}

#[derive(Clone, Debug)]
pub struct Layout {
    pub cols: usize,
    /// The image box (labels go under it).
    pub tile: Vec2,
    pub metrics: Metrics,
    pub rows: Vec<Row>,
    /// Each row's top, then the content's bottom.
    tops: Vec<f32>,
    /// Each item's row.
    row_of: Vec<u32>,
    width: f32,
}

impl Layout {
    /// `n` items across `width` at `size` points wide; `section(i)` says
    /// item i begins a section.
    pub fn new(width: f32, size: f32, m: Metrics, n: usize, section: &dyn Fn(usize) -> bool) -> Self {
        let width = width.max(size + 2.0 * m.gap);
        let cols = (((width - m.gap) / (size + m.gap)).floor() as usize).max(1);
        let w = (width - m.gap) / cols as f32 - m.gap;
        let tile = vec2(w, w / m.aspect.max(0.05));
        let pitch = tile.y + m.label_h + m.gap;
        let mut rows = vec![];
        let mut tops = vec![];
        let mut row_of = Vec::with_capacity(n);
        let mut y = m.gap;
        let mut i = 0;
        while i < n {
            if section(i) {
                rows.push(Row::Header { first: i });
                tops.push(y);
                y += m.header_h;
            }
            let mut end = i + 1;
            while end < n && end - i < cols && !section(end) {
                end += 1;
            }
            rows.push(Row::Tiles { start: i, end });
            tops.push(y);
            row_of.extend(std::iter::repeat(rows.len() as u32 - 1).take(end - i));
            y += pitch;
            i = end;
        }
        tops.push(y);
        Self { cols, tile, metrics: m, rows, tops, row_of, width }
    }

    pub fn height(&self) -> f32 {
        *self.tops.last().unwrap_or(&0.0)
    }

    pub fn len(&self) -> usize {
        self.row_of.len()
    }

    pub fn row_rect(&self, r: usize) -> Rect {
        Rect::from_min_max(pos2(0.0, self.tops[r]), pos2(self.width, self.tops[r + 1]))
    }

    /// Item i's image box, in content space (origin at the content's top left).
    pub fn tile_rect(&self, i: usize) -> Rect {
        let r = self.row_of[i] as usize;
        let Row::Tiles { start, .. } = self.rows[r] else { unreachable!("items sit in tile rows") };
        let c = (i - start) as f32;
        let m = self.metrics;
        Rect::from_min_size(pos2(m.gap + c * (self.tile.x + m.gap), self.tops[r]), self.tile)
    }

    /// The box and the label under it: what a click lands on.
    pub fn cell_rect(&self, i: usize) -> Rect {
        let t = self.tile_rect(i);
        Rect::from_min_size(t.min, t.size() + vec2(0.0, self.metrics.label_h))
    }

    /// The item under a content-space point (gaps belong to nobody).
    pub fn at(&self, p: Pos2) -> Option<usize> {
        let r = self.tops.partition_point(|&t| t <= p.y).checked_sub(1)?;
        let Row::Tiles { start, end } = *self.rows.get(r)? else { return None };
        let m = self.metrics;
        let c = ((p.x - m.gap) / (self.tile.x + m.gap)).floor();
        if c < 0.0 {
            return None;
        }
        let i = start + c as usize;
        (i < end && self.cell_rect(i).contains(p)).then_some(i)
    }

    /// Rows touching content-space `y0..y1`.
    pub fn rows_in(&self, y0: f32, y1: f32) -> Range<usize> {
        let a = self.tops.partition_point(|&t| t <= y0).saturating_sub(1);
        let b = self.tops.partition_point(|&t| t < y1).min(self.rows.len());
        a.min(b)..b
    }

    /// Items in the rows touching `y0..y1`, in order.
    pub fn items_in(&self, y0: f32, y1: f32) -> Range<usize> {
        let mut out: Option<Range<usize>> = None;
        for r in self.rows_in(y0, y1) {
            if let Row::Tiles { start, end } = self.rows[r] {
                out = Some(out.map_or(start..end, |o| o.start..end));
            }
        }
        out.unwrap_or(0..0)
    }

    /// Items whose cells meet `rect` (a box select).
    pub fn items_meeting(&self, rect: Rect) -> Vec<usize> {
        self.items_in(rect.top(), rect.bottom()).filter(|&i| self.cell_rect(i).intersects(rect)).collect()
    }

    /// The item a row up (`-1`) or down (`+1`) from i: same column, or the
    /// nearest in a shorter row. None at the top or bottom.
    pub fn vertical(&self, i: usize, dir: i32) -> Option<usize> {
        let r = self.row_of[i] as usize;
        let Row::Tiles { start, .. } = self.rows[r] else { return None };
        let col = i - start;
        let mut r2 = r as i64;
        loop {
            r2 += dir as i64;
            let row = self.rows.get(usize::try_from(r2).ok()?)?;
            if let Row::Tiles { start, end } = *row {
                return Some((start + col).min(end - 1));
            }
        }
    }

    /// How many tile rows fit in `h` points (a page for PageUp / PageDown).
    pub fn rows_per(&self, h: f32) -> usize {
        ((h / (self.tile.y + self.metrics.label_h + self.metrics.gap)).floor() as usize).max(1)
    }
}
