//! A ready-made `Scene`: a stack of items, each a placement and a piece of
//! content. Four kinds of content come with it, and an app adds its own by
//! implementing `Content` (a registry of kinds, not an enum to extend):
//!
//! | kind      | what it is                                                  |
//! |-----------|-------------------------------------------------------------|
//! | `Shape`   | a vector path: fill and/or stroke, concave is fine          |
//! | `Picture` | a texture the app uploaded (decode is the app's)            |
//! | `Clip`    | video: a `FrameSource` asked for the texture at the clock   |
//! | `Note`    | a card of text, double-click to edit (a Milanote board)     |
//!
//! The board owns a clock for clips. It never decodes or uploads: pixels
//! reach it as `TextureId`s, so an app keeps its own GPU path (wobble's
//! readers, a photo editor's tiles) and the board only places them.

use std::any::Any;
use std::sync::Arc;

use egui::{pos2, vec2, Color32, Pos2, Rect, Stroke, TextureId, Ui, Vec2};

use super::path::{self, Path};
use super::pen::Pen;
use super::scene::Scene;
use super::xform::Xform;

/// What an item shows. Local coordinates, laid out about the origin.
pub trait Content: Any {
    /// A word for the kind, for an inspector or a status line.
    fn kind(&self) -> &'static str;
    fn local_rect(&self) -> Rect;
    fn paint(&self, pen: &Pen);
    fn hit(&self, local: Pos2, tol: f32) -> bool {
        self.local_rect().expand(tol).contains(local)
    }
    fn clone_box(&self) -> Box<dyn Content>;
    fn editable(&self) -> bool {
        false
    }
    fn edit_ui(&mut self, _ui: &mut Ui, _screen: Rect) -> bool {
        false
    }
    fn animated(&self) -> bool {
        false
    }
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct ItemId(pub u64);

pub struct Item {
    pub id: ItemId,
    pub xf: Xform,
    pub content: Box<dyn Content>,
}

#[derive(Default)]
pub struct Board {
    pub items: Vec<Item>,
    next: u64,
    /// Seconds, for clips.
    pub time: f64,
    pub playing: bool,
}

impl Board {
    pub fn add(&mut self, xf: Xform, content: impl Content) -> ItemId {
        self.add_box(xf, Box::new(content))
    }

    pub fn add_box(&mut self, xf: Xform, content: Box<dyn Content>) -> ItemId {
        self.next += 1;
        let id = ItemId(self.next);
        self.items.push(Item { id, xf, content });
        id
    }

    pub fn get(&self, id: ItemId) -> Option<&Item> {
        self.items.iter().find(|i| i.id == id)
    }

    pub fn get_mut(&mut self, id: ItemId) -> Option<&mut Item> {
        self.items.iter_mut().find(|i| i.id == id)
    }

    /// The content, if it is a `T`.
    pub fn content<T: Content>(&self, id: ItemId) -> Option<&T> {
        self.get(id)?.content.as_any().downcast_ref()
    }

    /// Advance the clock by `dt` if playing. Call once a frame.
    pub fn tick(&mut self, dt: f64) {
        if self.playing {
            self.time += dt;
        }
    }

    fn item(&self, id: ItemId) -> &Item {
        self.get(id).expect("a live item id")
    }
}

impl Scene for Board {
    type Id = ItemId;

    fn ids(&self) -> Vec<ItemId> {
        self.items.iter().map(|i| i.id).collect()
    }

    fn local_rect(&self, id: ItemId) -> Rect {
        self.item(id).content.local_rect()
    }

    fn xform(&self, id: ItemId) -> Xform {
        self.item(id).xf
    }

    fn set_xform(&mut self, id: ItemId, xf: Xform) {
        if let Some(i) = self.get_mut(id) {
            i.xf = xf;
        }
    }

    fn paint(&self, id: ItemId, pen: &Pen) {
        self.item(id).content.paint(pen);
    }

    fn hit(&self, id: ItemId, local: Pos2, tol: f32) -> bool {
        self.item(id).content.hit(local, tol)
    }

    fn remove(&mut self, ids: &[ItemId]) {
        self.items.retain(|i| !ids.contains(&i.id));
    }

    fn duplicate(&mut self, ids: &[ItemId]) -> Vec<ItemId> {
        // In stacking order, so the copies stack as the originals did.
        let picks: Vec<(Xform, Box<dyn Content>)> = self.items.iter().filter(|i| ids.contains(&i.id)).map(|i| (i.xf, i.content.clone_box())).collect();
        picks.into_iter().map(|(xf, c)| self.add_box(xf, c)).collect()
    }

    fn restack(&mut self, ids: &[ItemId], front: bool) {
        let (mut moved, rest): (Vec<Item>, Vec<Item>) = std::mem::take(&mut self.items).into_iter().partition(|i| ids.contains(&i.id));
        self.items = if front {
            let mut v = rest;
            v.append(&mut moved);
            v
        } else {
            moved.extend(rest);
            moved
        };
    }

    fn editable(&self, id: ItemId) -> bool {
        self.item(id).content.editable()
    }

    fn edit_ui(&mut self, id: ItemId, ui: &mut Ui, screen: Rect) -> bool {
        self.get_mut(id).is_some_and(|i| i.content.edit_ui(ui, screen))
    }

    fn animating(&self) -> bool {
        self.playing && self.items.iter().any(|i| i.content.animated())
    }

    fn time(&self) -> f64 {
        self.time
    }
}

macro_rules! any_impls {
    () => {
        fn as_any(&self) -> &dyn Any {
            self
        }
        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
        fn clone_box(&self) -> Box<dyn Content> {
            Box::new(self.clone())
        }
    };
}

/// A vector path. `stroke.width` is in local units: it scales with the item.
#[derive(Clone, Debug)]
pub struct Shape {
    pub path: Path,
    pub fill: Color32,
    pub stroke: Stroke,
}

impl Content for Shape {
    fn kind(&self) -> &'static str {
        "shape"
    }
    fn local_rect(&self) -> Rect {
        self.path.bounds().expand(self.stroke.width * 0.5)
    }
    fn paint(&self, pen: &Pen) {
        pen.path(&self.path, self.fill, self.stroke);
    }
    fn hit(&self, local: Pos2, tol: f32) -> bool {
        let pts = self.path.flatten(tol.max(1e-3) * 0.5);
        (self.path.closed && self.fill.a() > 0 && path::contains(&pts, local)) || path::distance(&pts, self.path.closed, local) <= tol + self.stroke.width * 0.5
    }
    any_impls!();
}

/// A picture the app uploaded, centred on the origin at one local unit
/// per texel.
#[derive(Clone, Debug)]
pub struct Picture {
    pub tex: TextureId,
    pub size: Vec2,
    pub tint: Color32,
}

impl Picture {
    pub fn new(tex: TextureId, size: [usize; 2]) -> Self {
        Self { tex, size: vec2(size[0] as f32, size[1] as f32), tint: Color32::WHITE }
    }
}

const FULL_UV: Rect = Rect { min: pos2(0.0, 0.0), max: pos2(1.0, 1.0) };

impl Content for Picture {
    fn kind(&self) -> &'static str {
        "picture"
    }
    fn local_rect(&self) -> Rect {
        Rect::from_center_size(Pos2::ZERO, self.size)
    }
    fn paint(&self, pen: &Pen) {
        pen.image(self.tex, self.local_rect(), FULL_UV, self.tint);
    }
    any_impls!();
}

/// Where a clip's pictures come from. The app decodes and uploads (a
/// movie reader, an image sequence, a live feed); the board asks for the
/// texture at a time and draws whatever comes back.
pub trait FrameSource {
    /// Pixel size of a frame.
    fn size(&self) -> [usize; 2];
    /// Seconds. A clip loops over it.
    fn duration(&self) -> f64;
    /// The frame to show at `t` (already wrapped into `0..duration`), or
    /// `None` while it is not decoded yet (the last one stays up).
    fn frame(&self, t: f64) -> Option<TextureId>;
}

/// Video: a `FrameSource` on the board's clock, `offset` seconds in.
#[derive(Clone)]
pub struct Clip {
    pub source: Arc<dyn FrameSource>,
    pub offset: f64,
    /// The last frame shown, held while the next decodes.
    held: std::cell::Cell<Option<TextureId>>,
}

impl Clip {
    pub fn new(source: Arc<dyn FrameSource>) -> Self {
        Self { source, offset: 0.0, held: Default::default() }
    }

    /// The clip's own time at board time `t`.
    pub fn local_time(&self, t: f64) -> f64 {
        let d = self.source.duration();
        if d <= 0.0 { 0.0 } else { (t + self.offset).rem_euclid(d) }
    }
}

impl Content for Clip {
    fn kind(&self) -> &'static str {
        "clip"
    }
    fn local_rect(&self) -> Rect {
        let [w, h] = self.source.size();
        Rect::from_center_size(Pos2::ZERO, vec2(w as f32, h as f32))
    }
    fn paint(&self, pen: &Pen) {
        let r = self.local_rect();
        let tex = self.source.frame(self.local_time(pen.time)).or(self.held.get());
        match tex {
            Some(t) => {
                self.held.set(Some(t));
                pen.image(t, r, FULL_UV, Color32::WHITE);
            }
            None => pen.rect(r, Color32::from_gray(20), Stroke::NONE),
        }
        // A progress line along the bottom, so a clip reads as one.
        let d = self.source.duration().max(1e-9);
        let f = (self.local_time(pen.time) / d) as f32;
        let h = (2.0 / pen.px()).max(r.height() * 0.006);
        let bar = Rect::from_min_max(pos2(r.left(), r.bottom() - h), pos2(r.left() + r.width() * f, r.bottom()));
        pen.rect(bar, Color32::from_white_alpha(200), Stroke::NONE);
    }
    fn animated(&self) -> bool {
        true
    }
    any_impls!();
}

/// A card of text: the board app's note.
#[derive(Clone, Debug)]
pub struct Note {
    pub text: String,
    pub size: Vec2,
    pub fill: Color32,
    pub ink: Color32,
    /// Text height in local units.
    pub font: f32,
}

impl Note {
    pub fn new(text: impl Into<String>, size: Vec2, fill: Color32) -> Self {
        Self { text: text.into(), size, fill, ink: Color32::from_gray(25), font: 16.0 }
    }
}

const NOTE_PAD: f32 = 12.0;

impl Content for Note {
    fn kind(&self) -> &'static str {
        "note"
    }
    fn local_rect(&self) -> Rect {
        Rect::from_center_size(Pos2::ZERO, self.size)
    }
    fn paint(&self, pen: &Pen) {
        let r = self.local_rect();
        // A soft shadow, then the card.
        pen.rect(r.translate(vec2(3.0, 4.0)), Color32::from_black_alpha(50), Stroke::NONE);
        pen.rect(r, self.fill, Stroke::NONE);
        pen.text(r.min + vec2(NOTE_PAD, NOTE_PAD), &self.text, self.font, r.width() - NOTE_PAD * 2.0, self.ink);
    }
    fn editable(&self) -> bool {
        true
    }
    fn edit_ui(&mut self, ui: &mut Ui, screen: Rect) -> bool {
        let edit = egui::TextEdit::multiline(&mut self.text).frame(egui::Frame::NONE).text_color(self.ink).background_color(self.fill);
        let r = ui.put(screen.shrink(4.0), edit);
        if !r.has_focus() && !r.lost_focus() {
            r.request_focus();
        }
        r.changed()
    }
    any_impls!();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note() -> Note {
        Note::new("hi", vec2(100.0, 60.0), Color32::YELLOW)
    }

    #[test]
    fn duplicates_stack_on_top_and_restack_moves_them() {
        let mut b = Board::default();
        let a = b.add(Xform::default(), note());
        let c = b.add(Xform::at(pos2(10.0, 0.0)), note());
        let d = b.duplicate(&[a]);
        assert_eq!(b.ids(), vec![a, c, d[0]]);
        b.restack(&[a], true);
        assert_eq!(b.ids(), vec![c, d[0], a]);
        b.restack(&[a, d[0]], false);
        assert_eq!(b.ids(), vec![d[0], a, c], "back, keeping their own order");
        b.remove(&[c]);
        assert_eq!(b.ids().len(), 2);
    }

    #[test]
    fn a_shape_is_hit_on_its_ink_not_its_box() {
        let ring = Shape { path: Path::polygon(&[pos2(0.0, 0.0), pos2(10.0, 0.0), pos2(10.0, 10.0), pos2(0.0, 10.0)]), fill: Color32::TRANSPARENT, stroke: Stroke::new(1.0, Color32::WHITE) };
        assert!(ring.hit(pos2(0.2, 5.0), 0.1), "on the stroke");
        assert!(!ring.hit(pos2(5.0, 5.0), 0.1), "an unfilled middle is not the shape");
        let solid = Shape { fill: Color32::RED, ..ring };
        assert!(solid.hit(pos2(5.0, 5.0), 0.1));
    }

    struct Frames;
    impl FrameSource for Frames {
        fn size(&self) -> [usize; 2] {
            [16, 9]
        }
        fn duration(&self) -> f64 {
            2.0
        }
        fn frame(&self, _t: f64) -> Option<TextureId> {
            None
        }
    }

    #[test]
    fn a_clip_loops_on_the_board_clock() {
        let mut c = Clip::new(Arc::new(Frames));
        c.offset = 0.5;
        assert!((c.local_time(3.0) - 1.5).abs() < 1e-9);
        assert!((c.local_time(-1.0) - 1.5).abs() < 1e-9, "before zero wraps too");
        assert_eq!(c.local_rect().size(), vec2(16.0, 9.0));
    }
}
