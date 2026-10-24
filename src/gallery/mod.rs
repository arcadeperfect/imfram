//! A thumbnail grid over the app's items: files, shots, photos, refs.
//!
//! The app implements [`Items`] (count, ids, a source key per item,
//! labels) and owns a [`Thumbs`] cache whose loader turns a source key
//! into pixels on worker threads. A [`Gallery`] keeps the tile size, the
//! selection and the keyboard cursor, and answers each frame with a
//! [`GalleryEvent`]. Only the visible tiles (and a screen's worth either
//! side) ask for thumbnails, nearest the middle first; tiles scrolled past
//! are never loaded.
//!
//! Hands:
//! - click selects, shift-click a range, cmd/ctrl-click toggles; drag on
//!   empty space box-selects (shift adds, cmd toggles); double-click or
//!   Enter activates; Space asks for a preview; right-click a context menu
//! - arrows walk the grid (shift extends), Home / End, Page Up / Down;
//!   A or cmd-A selects all, alt-A none, ctrl-I inverts (Blender's keys)
//! - ctrl/cmd + wheel or pinch, or + / -, resize the tiles; the tile under
//!   the pointer stays put while they glide to the new size
//! - over a sequence or clip, moving across the tile scrubs its frames
//! - drag a tile out: the selection travels as a [`Dragged`] payload
//!   (egui's drag-and-drop), for any other view to take
//!
//! [`List`] is the same items as rows (a thumbnail column that scrubs, the
//! name, the app's [`Details`] columns), with the same hands and the same
//! cache. [`files`] lists a folder with frame sequences grouped, and
//! [`browse::FileView`] puts both views over it with a path bar: the
//! pieces of a file browser like organisicator's.
//!
//! Colours come from `Visuals`.

mod hands;
mod layout;
pub mod list;
mod paint;
mod tile;
pub mod thumbs;
#[cfg(not(target_arch = "wasm32"))]
pub mod files;
#[cfg(not(target_arch = "wasm32"))]
pub mod browse;
#[cfg(test)]
mod tests;

use std::collections::{BTreeSet, HashMap};
use std::hash::Hash;
use std::ops::Range;

use egui::{pos2, vec2, Pos2, Rect, Sense, Ui};

pub use layout::{Layout, Metrics, Row};
pub use list::{Details, List};
pub use thumbs::{bucket, Request, Shown, Stats, Thumbs};

pub const MIN_SIZE: f32 = 48.0;
pub const MAX_SIZE: f32 = 640.0;
/// One + or - press (and how far a wheel notch of zoom goes, roughly).
pub const SIZE_STEP: f32 = 1.25;
const GLIDE: f32 = 0.12;

/// The app's items, read-only. `id` must follow the item across
/// re-sorts (the selection remembers ids); `src` is what the loader gets.
pub trait Items {
    type Src: Clone + Eq + Hash + Send + 'static;
    fn len(&self) -> usize;
    fn id(&self, i: usize) -> u64;
    fn src(&self, i: usize) -> Self::Src;
    fn label(&self, i: usize) -> String;
    /// Frames to scrub through (1: a still).
    fn frames(&self, _i: usize) -> u32 {
        1
    }
    /// The frame shown when not scrubbing, `0..frames`.
    fn poster(&self, _i: usize) -> u32 {
        0
    }
    /// What frame `f` (`0..frames`) is called while scrubbing ("1043").
    fn frame_name(&self, _i: usize, f: u32) -> String {
        f.to_string()
    }
    /// Over the image's corner ("EXR", "1001-1100").
    fn badge(&self, _i: usize) -> Option<String> {
        None
    }
    /// In the empty box while there is no picture ("EXR", "folder").
    fn kind(&self, _i: usize) -> Option<String> {
        None
    }
    /// Some: item i starts a section with this title.
    fn section(&self, _i: usize) -> Option<String> {
        None
    }
    /// False: never ask the loader (a folder); the box shows `kind`.
    fn has_picture(&self, _i: usize) -> bool {
        true
    }
    /// Drawn dimmed (missing, offline).
    fn dim(&self, _i: usize) -> bool {
        false
    }
}

/// The drag-and-drop payload: the dragged items' ids, in grid order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dragged(pub Vec<u64>);

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GalleryEvent {
    pub selection_changed: bool,
    /// Double-click or Enter (the item's id).
    pub activated: Option<u64>,
    /// Space on the cursor's item.
    pub preview: Option<u64>,
    /// A right-click: on an item (already selected by then) or on empty space.
    pub context: Option<(Option<u64>, Pos2)>,
    /// A drag of the selection started this frame.
    pub drag_started: bool,
    /// The list's sort changed: the source should sort itself.
    pub sort_changed: bool,
}

pub(crate) struct BoxSel {
    /// Content space.
    from: Pos2,
    base: BTreeSet<u64>,
    toggle: bool,
    add: bool,
}

pub struct Gallery {
    /// Tile width asked for, in points (rows fill the width, so a tile is
    /// up to one tile's share wider).
    pub size: f32,
    pub metrics: Metrics,
    /// Crop to fill the box, or letterbox the whole picture.
    pub fill: bool,
    pub scrub: bool,
    pub selection: BTreeSet<u64>,
    pub(crate) cursor: Option<usize>,
    pub(crate) anchor: Option<usize>,
    pub(crate) boxsel: Option<BoxSel>,
    pub(crate) scroll_to: Option<usize>,
    /// The frame each scrubbed tile last showed, held while the next loads.
    hold: HashMap<u64, u32>,
    /// While tiles glide to a new size: this item stays this far below the view's top.
    keep: Option<(usize, f32)>,
    layout: Option<Layout>,
    scroll_y: f32,
    visible: Range<usize>,
    /// The content's top left on screen last frame (tests find tiles by it).
    pub(crate) origin: Pos2,
}

impl Default for Gallery {
    fn default() -> Self {
        Self {
            size: 160.0,
            metrics: Metrics::default(),
            fill: false,
            scrub: true,
            selection: BTreeSet::new(),
            cursor: None,
            anchor: None,
            boxsel: None,
            scroll_to: None,
            hold: HashMap::new(),
            keep: None,
            layout: None,
            scroll_y: 0.0,
            visible: 0..0,
            origin: Pos2::ZERO,
        }
    }
}

impl Gallery {
    pub fn new() -> Self {
        Self::default()
    }

    /// The items laid out last frame.
    pub fn visible(&self) -> Range<usize> {
        self.visible.clone()
    }

    /// Last frame's layout (where every tile is).
    pub fn layout(&self) -> Option<&Layout> {
        self.layout.as_ref()
    }

    /// The keyboard cursor's item.
    pub fn cursor(&self) -> Option<usize> {
        self.cursor
    }

    /// Bring item i into view next frame.
    pub fn scroll_to(&mut self, i: usize) {
        self.scroll_to = Some(i);
    }

    pub fn show<I: Items>(&mut self, ui: &mut Ui, items: &I, thumbs: &mut Thumbs<I::Src>) -> GalleryEvent {
        let mut ev = GalleryEvent::default();
        let id = ui.make_persistent_id("imfram.gallery");
        let n = items.len();
        self.cursor = self.cursor.filter(|&c| c < n);
        self.anchor = self.anchor.filter(|&a| a < n);
        let full = ui.available_rect_before_wrap();
        ui.interact(full, id, Sense::focusable_noninteractive());
        let focused = ui.memory(|m| m.has_focus(id));
        self.resize(ui, full, focused);
        let shown = ui.ctx().animate_value_with_time(id.with("size"), self.size, GLIDE);
        let width = full.width() - ui.spacing().scroll.allocated_width();
        let layout = Layout::new(width, shown, self.metrics, n, &|i| items.section(i).is_some());
        let mut area = egui::ScrollArea::vertical().id_salt(id).auto_shrink([false, false]);
        if let Some((i, below)) = self.keep {
            if i < n {
                area = area.vertical_scroll_offset((layout.tile_rect(i).top() - below).max(0.0));
            }
            if (shown - self.size).abs() < 0.01 {
                self.keep = None;
            }
        }
        let look = paint::Look::from(ui);
        let out = area.show_viewport(ui, |ui, vp| {
            let origin = ui.max_rect().min.to_vec2();
            self.origin = ui.max_rect().min;
            let content = Rect::from_min_size(ui.max_rect().min, vec2(width, layout.height().max(vp.height())));
            ui.set_min_size(content.size());
            self.background(ui, id, content, origin, vp, &layout, items, &mut ev);
            let p = ui.painter().clone();
            for r in layout.rows_in(vp.top(), vp.bottom()) {
                if let Row::Header { first } = layout.rows[r] {
                    let title = items.section(first).unwrap_or_default();
                    paint::header(&p, layout.row_rect(r).translate(origin), &title, self.metrics.gap, &look);
                }
            }
            let vis = layout.items_in(vp.top(), vp.bottom());
            let ppp = ui.ctx().pixels_per_point();
            let px = bucket(layout.tile.x.max(layout.tile.y) * ppp * if self.fill { 1.5 } else { 1.0 });
            let mid = vp.center().y;
            for i in vis.clone() {
                let near = 1.0 - ((layout.tile_rect(i).center().y - mid).abs() / vp.height().max(1.0)).min(1.0);
                self.tile(ui, id, i, items, thumbs, &layout, origin, px, 2.0 + near, &look, &mut ev);
            }
            // A screen's worth below and half above, for scrolling into.
            let ahead = layout.items_in(vp.bottom(), vp.bottom() + vp.height());
            let behind = layout.items_in(vp.top() - vp.height() * 0.5, vp.top());
            for i in ahead.chain(behind).filter(|&i| !vis.contains(&i) && items.has_picture(i)) {
                let _ = thumbs.want(&items.src(i), items.poster(i), px, 1.0);
            }
            if focused {
                self.keys(ui, &layout, items, vp.height(), &mut ev);
            }
            if let Some(i) = self.scroll_to.take().filter(|&i| i < n) {
                ui.scroll_to_rect(layout.cell_rect(i).translate(origin), None);
            }
            self.visible = vis;
        });
        self.scroll_y = out.state.offset.y;
        tile::drag_badge(ui, id, &look);
        thumbs.flush(ui.ctx());
        self.layout = Some(layout);
        ev
    }

    /// Zoom gestures and + / -; picks the item to hold still while tiles glide.
    fn resize(&mut self, ui: &mut Ui, full: Rect, focused: bool) {
        let hovered = ui.rect_contains_pointer(full);
        let mut k = if hovered { ui.input(|i| i.zoom_delta()) } else { 1.0 };
        if focused {
            ui.input_mut(|i| {
                if i.consume_key(egui::Modifiers::NONE, egui::Key::Plus) || i.consume_key(egui::Modifiers::NONE, egui::Key::Equals) {
                    k *= SIZE_STEP;
                }
                if i.consume_key(egui::Modifiers::NONE, egui::Key::Minus) {
                    k /= SIZE_STEP;
                }
            });
        }
        if (k - 1.0).abs() < 1e-4 {
            return;
        }
        self.size = (self.size * k).clamp(MIN_SIZE, MAX_SIZE);
        if self.keep.is_none() {
            if let Some(l) = &self.layout {
                let under = ui.ctx().pointer_hover_pos().map(|p| pos2(p.x - full.left(), p.y - full.top() + self.scroll_y));
                let pick = under.and_then(|p| l.at(p)).or(self.cursor).or(Some(self.visible.start)).filter(|&i| i < l.len());
                if let Some(i) = pick {
                    self.keep = Some((i, l.tile_rect(i).top() - self.scroll_y));
                }
            }
        }
    }
}
