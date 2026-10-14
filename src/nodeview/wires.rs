//! Wire geometry: the curve a wire follows, and whether a stroke cuts it.
//! Wires leave an output downward and enter an input from above.

use egui::epaint::CubicBezierShape;
use egui::{vec2, Color32, Pos2, Rect, Shape, Stroke};

/// The four bezier control points of a wire from output `a` to input `b`.
pub fn points(a: Pos2, b: Pos2) -> [Pos2; 4] {
    let k = ((b.y - a.y).abs() * 0.5).clamp(30.0, 140.0);
    [a, a + vec2(0.0, k), b - vec2(0.0, k), b]
}

pub fn shape(a: Pos2, b: Pos2, color: Color32, width: f32) -> Shape {
    CubicBezierShape::from_points_stroke(points(a, b), false, Color32::TRANSPARENT, Stroke::new(width, color)).into()
}

/// Where the wire from `a` to `b` is halfway along: its label goes here.
pub fn midpoint(a: Pos2, b: Pos2) -> Pos2 {
    let [p0, p1, p2, p3] = points(a, b);
    ((p0.to_vec2() + p3.to_vec2()) * 0.125 + (p1.to_vec2() + p2.to_vec2()) * 0.375).to_pos2()
}

const STEPS: usize = 24;

/// The wire from `a` to `b` as a run of short straight stretches.
fn polyline(a: Pos2, b: Pos2) -> [Pos2; STEPS + 1] {
    let [p0, p1, p2, p3] = points(a, b);
    std::array::from_fn(|i| {
        let t = i as f32 / STEPS as f32;
        let u = 1.0 - t;
        (p0.to_vec2() * (u * u * u) + p1.to_vec2() * (3.0 * u * u * t) + p2.to_vec2() * (3.0 * u * t * t) + p3.to_vec2() * (t * t * t)).to_pos2()
    })
}

/// Does the wire from `a` to `b` cross the stroke `c -> d`?
pub fn crosses(a: Pos2, b: Pos2, c: Pos2, d: Pos2) -> bool {
    polyline(a, b).windows(2).any(|w| segments_cross(w[0], w[1], c, d))
}

/// If the wire from `a` to `b` passes through `rect`: how near it comes to
/// the rect's centre.
pub fn through(a: Pos2, b: Pos2, rect: Rect) -> Option<f32> {
    let line = polyline(a, b);
    let corners = [rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom()];
    let enters = |p: Pos2, q: Pos2| rect.contains(p) || rect.contains(q) || (0..4).any(|i| segments_cross(p, q, corners[i], corners[(i + 1) % 4]));
    if !line.windows(2).any(|w| enters(w[0], w[1])) {
        return None;
    }
    Some(distance(a, b, rect.center()))
}

/// How near the wire from `a` to `b` comes to `c`.
pub fn distance(a: Pos2, b: Pos2, c: Pos2) -> f32 {
    let to_stretch = |p: Pos2, q: Pos2| {
        let d = q - p;
        let t = if d.length_sq() > 0.0 { ((c - p).dot(d) / d.length_sq()).clamp(0.0, 1.0) } else { 0.0 };
        c.distance(p + d * t)
    };
    polyline(a, b).windows(2).map(|w| to_stretch(w[0], w[1])).fold(f32::INFINITY, f32::min)
}

fn segments_cross(a: Pos2, b: Pos2, c: Pos2, d: Pos2) -> bool {
    let side = |p: Pos2, q: Pos2, r: Pos2| (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
    let (d1, d2) = (side(c, d, a), side(c, d, b));
    let (d3, d4) = (side(a, b, c), side(a, b, d));
    // Touching counts (a stroke sample can land exactly on the wire);
    // collinear segments don't.
    let collinear = d1 == 0.0 && d2 == 0.0;
    d1 * d2 <= 0.0 && d3 * d4 <= 0.0 && !collinear
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;

    const A: Pos2 = pos2(0.0, 0.0);
    const B: Pos2 = pos2(40.0, 200.0);

    #[test]
    fn a_stroke_through_the_middle_cuts_and_one_beside_does_not() {
        let mid = pos2(20.0, 100.0);
        assert!(crosses(A, B, mid - vec2(30.0, 0.0), mid + vec2(30.0, 0.0)));
        assert!(crosses(A, B, mid - vec2(30.0, 0.0), mid), "ending on the wire counts");
        assert!(!crosses(A, B, pos2(200.0, 0.0), pos2(200.0, 200.0)));
    }

    #[test]
    fn a_wire_through_a_box_reports_how_central_it_runs() {
        let mid = pos2(20.0, 100.0);
        assert!(through(A, B, Rect::from_center_size(mid, vec2(60.0, 40.0))).unwrap() < 1.0);
        let d = through(A, B, Rect::from_center_size(mid + vec2(25.0, 0.0), vec2(60.0, 40.0))).unwrap();
        assert!(d > 15.0 && d < 30.0, "off-centre by about 25, got {d}");
        assert_eq!(through(A, B, Rect::from_center_size(mid + vec2(200.0, 0.0), vec2(60.0, 40.0))), None);
        assert!(through(A, B, Rect::from_center_size(mid, vec2(2.0, 2.0))).is_some(), "a tiny box still counts");
    }

    #[test]
    fn wires_leave_downward_and_a_point_on_one_is_no_distance_from_it() {
        let [p0, p1, p2, p3] = points(A, B);
        assert!(p1.y > p0.y && p1.x == p0.x);
        assert!(p2.y < p3.y && p2.x == p3.x);
        assert!(distance(A, B, midpoint(A, B)) < 0.5);
        assert!(distance(A, B, pos2(-100.0, 100.0)) > 90.0);
    }
}
