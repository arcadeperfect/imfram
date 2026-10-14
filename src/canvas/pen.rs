//! What content draws with: a painter that takes LOCAL coordinates and
//! carries them through the item's placement and the camera, so a content
//! kind never sees the screen. Images and video frames go down as textured
//! meshes (they rotate with the item); outlines are flattened at a
//! tolerance that follows the on-screen size, so a curve is as smooth
//! zoomed in as out and costs nothing extra zoomed out.

use egui::epaint::{Mesh, PathShape, PathStroke, TextShape, Vertex};
use egui::{pos2, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke};

use super::camera::Camera;
use super::path::{self, Path};
use super::xform::Xform;

/// Text smaller than this on screen is drawn as a bar, not glyphs: it
/// cannot be read, and laying it out is the expensive part of a board.
pub const TEXT_MIN_PX: f32 = 3.0;
/// Curve flattening tolerance, in screen points.
const FLAT_TOL_PX: f32 = 0.25;

pub struct Pen<'a> {
    pub painter: &'a Painter,
    pub cam: &'a Camera,
    pub view: Rect,
    pub xf: Xform,
    /// The scene's clock, in seconds (video, anything animated).
    pub time: f64,
}

impl Pen<'_> {
    pub fn to_screen(&self, local: Pos2) -> Pos2 {
        self.cam.to_screen(self.view, self.xf.apply(local))
    }

    /// Screen points per local unit (the larger axis).
    pub fn px(&self) -> f32 {
        self.cam.zoom * self.xf.unit()
    }

    /// The four corners of a local rect on screen, clockwise from top-left.
    pub fn quad(&self, r: Rect) -> [Pos2; 4] {
        [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()].map(|p| self.to_screen(p))
    }

    /// A texture over a local rect. `uv` picks the part of the texture.
    pub fn image(&self, tex: egui::TextureId, r: Rect, uv: Rect, tint: Color32) {
        let q = self.quad(r);
        let uvs = [uv.left_top(), uv.right_top(), uv.right_bottom(), uv.left_bottom()];
        let mut m = Mesh::with_texture(tex);
        for (p, uv) in q.into_iter().zip(uvs) {
            m.vertices.push(Vertex { pos: p, uv, color: tint });
        }
        m.indices.extend([0, 1, 2, 0, 2, 3]);
        self.painter.add(Shape::mesh(m));
    }

    pub fn rect(&self, r: Rect, fill: Color32, stroke: Stroke) {
        let q = self.quad(r).to_vec();
        if fill.a() > 0 {
            self.fill_screen(&q, fill);
        }
        if stroke.width > 0.0 && stroke.color.a() > 0 {
            self.painter.add(PathShape::closed_line(q, stroke));
        }
    }

    /// A vector path: filled (concave is fine), stroked, or both. The
    /// stroke width is in LOCAL units, so it scales with the item.
    pub fn path(&self, p: &Path, fill: Color32, stroke: Stroke) {
        let pts: Vec<Pos2> = p.flatten(FLAT_TOL_PX / self.px()).into_iter().map(|q| self.to_screen(q)).collect();
        if p.closed && fill.a() > 0 {
            self.fill_screen(&pts, fill);
        }
        if stroke.width > 0.0 && stroke.color.a() > 0 {
            let w = (stroke.width * self.px()).max(0.5);
            let s = PathStroke::new(w, stroke.color);
            self.painter.add(if p.closed { PathShape::closed_line(pts, s) } else { PathShape::line(pts, s) });
        }
    }

    /// Fill a polygon already in screen space.
    pub fn fill_screen(&self, pts: &[Pos2], color: Color32) {
        let tris = path::triangulate(pts);
        if tris.is_empty() {
            return;
        }
        let mut m = Mesh::default();
        for &p in pts {
            m.colored_vertex(p, color);
        }
        m.indices = tris;
        self.painter.add(Shape::mesh(m));
    }

    /// Text at a local point, wrapped at `wrap` local units, `size` local
    /// units tall; turns with the item. Below `TEXT_MIN_PX` a bar per line
    /// stands in.
    pub fn text(&self, at: Pos2, text: &str, size: f32, wrap: f32, color: Color32) {
        let px = size * self.px();
        if px < TEXT_MIN_PX {
            // One bar per paragraph: enough to see there is writing.
            for (i, line) in text.lines().take(8).enumerate() {
                let w = (line.chars().count() as f32 * size * 0.5).min(wrap);
                let y = at.y + (i as f32 + 0.5) * size * 1.3;
                let r = Rect::from_min_max(pos2(at.x, y - size * 0.15), pos2(at.x + w, y + size * 0.15));
                self.rect(r, color.gamma_multiply(0.4), Stroke::NONE);
            }
            return;
        }
        let galley = self.painter.layout(text.to_owned(), FontId::proportional(px), color, wrap * self.px());
        self.painter.add(TextShape::new(self.to_screen(at), galley, color).with_angle(self.xf.rot));
    }
}
