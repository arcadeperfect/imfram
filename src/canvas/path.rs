//! Vector paths: lines and cubic béziers, flattened to a polyline at a
//! tolerance that follows the zoom, filled by ear clipping (egui's own fill
//! is a fan, which is wrong for anything concave) and hit-tested exactly.
//! Pure, so a vector editor can build on it without a window.

use egui::{Pos2, Rect, Vec2};

#[derive(Clone, Copy, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Seg {
    Line(Pos2),
    /// Two control points, then the end point.
    Cubic(Pos2, Pos2, Pos2),
}

impl Seg {
    pub fn end(&self) -> Pos2 {
        match *self {
            Seg::Line(p) | Seg::Cubic(_, _, p) => p,
        }
    }
}

/// One contour. A closed path's last segment runs back to `start`
/// implicitly (a straight close), as in SVG's `Z`.
#[derive(Clone, PartialEq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Path {
    pub start: Pos2,
    pub segs: Vec<Seg>,
    pub closed: bool,
}

impl Path {
    pub fn new(start: Pos2) -> Self {
        Self { start, segs: vec![], closed: false }
    }

    pub fn line_to(mut self, p: Pos2) -> Self {
        self.segs.push(Seg::Line(p));
        self
    }

    pub fn cubic_to(mut self, c1: Pos2, c2: Pos2, p: Pos2) -> Self {
        self.segs.push(Seg::Cubic(c1, c2, p));
        self
    }

    pub fn close(mut self) -> Self {
        self.closed = true;
        self
    }

    pub fn polygon(points: &[Pos2]) -> Self {
        let mut p = Path::new(points.first().copied().unwrap_or(Pos2::ZERO));
        p.segs = points.iter().skip(1).map(|&q| Seg::Line(q)).collect();
        p.closed = true;
        p
    }

    /// The control hull's bounds: contains the curve, cheap, and what
    /// selection boxes and culling want.
    pub fn bounds(&self) -> Rect {
        let mut r = Rect::from_min_max(self.start, self.start);
        for s in &self.segs {
            match *s {
                Seg::Line(p) => r.extend_with(p),
                Seg::Cubic(a, b, p) => {
                    r.extend_with(a);
                    r.extend_with(b);
                    r.extend_with(p);
                }
            }
        }
        r
    }

    /// The outline as points, each curve split until its chords stay
    /// within `tol` of it. The start is not repeated at the end of a
    /// closed path.
    pub fn flatten(&self, tol: f32) -> Vec<Pos2> {
        let tol = tol.max(1e-4);
        let mut out = vec![self.start];
        let mut at = self.start;
        for s in &self.segs {
            match *s {
                Seg::Line(p) => out.push(p),
                Seg::Cubic(c1, c2, p) => {
                    // The second difference bounds how far a chord strays:
                    // n segments keep the error under tol.
                    let dd = (at.to_vec2() - c1.to_vec2() * 2.0 + c2.to_vec2()).length().max((c1.to_vec2() - c2.to_vec2() * 2.0 + p.to_vec2()).length());
                    let n = ((0.75 * dd / tol).sqrt().ceil() as usize).clamp(1, 256);
                    for i in 1..=n {
                        out.push(cubic(at, c1, c2, p, i as f32 / n as f32));
                    }
                }
            }
            at = s.end();
        }
        if self.closed && out.len() > 1 && (out[0] - out[out.len() - 1]).length() < 1e-6 {
            out.pop();
        }
        out
    }
}

pub fn cubic(a: Pos2, b: Pos2, c: Pos2, d: Pos2, t: f32) -> Pos2 {
    let u = 1.0 - t;
    let v = a.to_vec2() * (u * u * u) + b.to_vec2() * (3.0 * u * u * t) + c.to_vec2() * (3.0 * u * t * t) + d.to_vec2() * (t * t * t);
    v.to_pos2()
}

/// Signed area, positive for clockwise on screen (y down).
pub fn area(poly: &[Pos2]) -> f32 {
    let n = poly.len();
    (0..n).map(|i| {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        a.x * b.y - b.x * a.y
    }).sum::<f32>() * 0.5
}

fn cross(o: Pos2, a: Pos2, b: Pos2) -> f32 {
    (a - o).x * (b - o).y - (a - o).y * (b - o).x
}

/// Triangles covering a simple polygon (no self-crossings), as indices
/// into it. Ear clipping: O(n²), fine at the sizes an outline flattens to.
/// A self-crossing outline still gets triangles (the leftovers are fanned)
/// rather than a hole in the picture.
pub fn triangulate(poly: &[Pos2]) -> Vec<u32> {
    let n = poly.len();
    if n < 3 {
        return vec![];
    }
    let cw = area(poly) >= 0.0;
    let convex = |a: Pos2, b: Pos2, c: Pos2| if cw { cross(a, b, c) > 0.0 } else { cross(a, b, c) < 0.0 };
    let mut idx: Vec<usize> = (0..n).collect();
    let mut tris = Vec::with_capacity((n - 2) * 3);
    let mut guard = 0;
    while idx.len() > 3 && guard < n * n {
        guard += 1;
        let m = idx.len();
        let mut clipped = false;
        for i in 0..m {
            let (ia, ib, ic) = (idx[(i + m - 1) % m], idx[i], idx[(i + 1) % m]);
            let (a, b, c) = (poly[ia], poly[ib], poly[ic]);
            if !convex(a, b, c) {
                continue;
            }
            let inside = idx.iter().any(|&j| j != ia && j != ib && j != ic && in_triangle(poly[j], a, b, c));
            if inside {
                continue;
            }
            tris.extend([ia as u32, ib as u32, ic as u32]);
            idx.remove(i);
            clipped = true;
            break;
        }
        if !clipped {
            break;
        }
    }
    // Three left (the normal end), or a knot ear clipping could not undo.
    for i in 1..idx.len().saturating_sub(1) {
        tris.extend([idx[0] as u32, idx[i] as u32, idx[i + 1] as u32]);
    }
    tris
}

fn in_triangle(p: Pos2, a: Pos2, b: Pos2, c: Pos2) -> bool {
    let (d1, d2, d3) = (cross(a, b, p), cross(b, c, p), cross(c, a, p));
    let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(neg && pos)
}

/// Even-odd point in polygon.
pub fn contains(poly: &[Pos2], p: Pos2) -> bool {
    let mut inside = false;
    let n = poly.len();
    let mut j = n.wrapping_sub(1);
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Distance from `p` to the polyline (closed: back to the start too).
pub fn distance(poly: &[Pos2], closed: bool, p: Pos2) -> f32 {
    let n = poly.len();
    if n == 0 {
        return f32::INFINITY;
    }
    if n == 1 {
        return (poly[0] - p).length();
    }
    let segs = if closed { n } else { n - 1 };
    (0..segs).map(|i| seg_distance(poly[i], poly[(i + 1) % n], p)).fold(f32::INFINITY, f32::min)
}

fn seg_distance(a: Pos2, b: Pos2, p: Pos2) -> f32 {
    let ab: Vec2 = b - a;
    let t = if ab.length_sq() < 1e-12 { 0.0 } else { ((p - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0) };
    (a + ab * t - p).length()
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;

    /// An L: concave, which a fan fill gets wrong.
    fn ell() -> Vec<Pos2> {
        vec![pos2(0.0, 0.0), pos2(2.0, 0.0), pos2(2.0, 1.0), pos2(1.0, 1.0), pos2(1.0, 2.0), pos2(0.0, 2.0)]
    }

    fn tri_area(poly: &[Pos2], tris: &[u32]) -> f32 {
        tris.chunks(3).map(|t| area(&[poly[t[0] as usize], poly[t[1] as usize], poly[t[2] as usize]]).abs()).sum()
    }

    #[test]
    fn a_concave_outline_triangulates_to_exactly_its_area() {
        for poly in [ell(), ell().into_iter().rev().collect()] {
            let t = triangulate(&poly);
            assert_eq!(t.len(), (poly.len() - 2) * 3);
            assert!((tri_area(&poly, &t) - 3.0).abs() < 1e-5, "covered {}", tri_area(&poly, &t));
        }
    }

    #[test]
    fn hits_land_inside_the_notch_and_not_in_it() {
        let l = ell();
        assert!(contains(&l, pos2(0.5, 1.5)));
        assert!(!contains(&l, pos2(1.5, 1.5)), "the notch is outside");
        assert!((distance(&l, true, pos2(1.5, 1.5)) - 0.5).abs() < 1e-6);
        assert!((distance(&l, true, pos2(-1.0, 1.0)) - 1.0).abs() < 1e-6);
        assert!((distance(&l, false, pos2(-1.0, 1.0)) - 2f32.sqrt()).abs() < 1e-6, "open: no closing edge");
    }

    #[test]
    fn a_curve_flattens_within_its_tolerance() {
        // A quarter circle, near enough, as one cubic.
        let k = 0.5523;
        let p = Path::new(pos2(1.0, 0.0)).cubic_to(pos2(1.0, k), pos2(k, 1.0), pos2(0.0, 1.0));
        let coarse = p.flatten(0.1).len();
        let fine = p.flatten(0.0001).len();
        assert!(fine > coarse * 4, "finer tolerance, more points ({coarse} -> {fine})");
        for q in p.flatten(0.001) {
            assert!((q.to_vec2().length() - 1.0).abs() < 0.002);
        }
        assert!(p.bounds().contains(pos2(0.7, 0.7)));
    }
}
