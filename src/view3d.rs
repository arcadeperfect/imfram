//! A 3D view: an orbiting eye, Blender's hands on it, and the overlays a
//! 3D viewport draws over its picture (the floor grid, the axis gizmo).
//!
//! The eye is Blender's `RegionView3D`: a pivot, an orientation and a
//! distance, the eye `dist` behind the pivot along the view's +Z. The view
//! looks down its own −Z with +Y up and +X right, so `rot` is also the
//! orientation of a camera standing at the eye. World up is a choice
//! (`Up::Y` for a Y-up app, `Up::Z` for Blender's own world); the turntable
//! turns about it and the numpad views are named against it.
//!
//! Hands (manifesto: Blender's, so muscle memory carries):
//!
//! | input                         | does                                   |
//! |-------------------------------|----------------------------------------|
//! | MMB drag                      | orbit (turntable, the horizon stays)   |
//! | shift+MMB                     | pan                                    |
//! | ctrl+MMB                      | dolly                                  |
//! | alt+LMB / shift+alt / ctrl+alt| the same three ("emulate 3 button")   |
//! | wheel, pinch, ctrl+two fingers| dolly                                  |
//! | two fingers                   | orbit; shift+two fingers pan           |
//! | ctrl / shift + wheel          | pan sideways / vertically              |
//! | numpad 1 3 7 (ctrl: opposite) | front, right, top (back, left, bottom) |
//! | numpad 4 6 8 2                | orbit 15° left, right, up, down        |
//! | numpad 9                      | the opposite side                      |
//! | Home                          | frame everything (`Orbit::frame`)      |
//!
//! The keys are the host's to bind (egui reports numpad and top-row digits
//! as the same key); [`View`] and [`Step`] are what they do.
//!
//! The projection fits a 36mm sensor across the LARGER side of the picture
//! with no other zoom, so a camera with the same lens on a back 36mm wide
//! along the larger side renders exactly what [`Orbit::project`] predicts.

use egui::{Color32, Modifiers, PointerButton, Pos2, Rect, Stroke, Vec2};
use std::f32::consts::{FRAC_PI_2, PI};

pub type V3 = [f32; 3];
/// A quaternion, (w, x, y, z).
pub type Quat = [f32; 4];

/// Blender's turntable sensitivity (0.4° a pixel).
pub const TURNTABLE_RAD_PER_PX: f32 = 0.4 * PI / 180.0;
/// Blender's numpad orbit step.
pub const STEP_RAD: f32 = 15.0 * PI / 180.0;
/// Half the sensor the lens is measured against, in mm.
const HALF_SENSOR: f32 = 18.0;

/// Which world axis points up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Up {
    #[default]
    Y,
    Z,
}

impl Up {
    pub fn axis(self) -> V3 {
        match self {
            Up::Y => [0.0, 1.0, 0.0],
            Up::Z => [0.0, 0.0, 1.0],
        }
    }

    /// The turn that takes Blender's Z-up world into this one (identity
    /// for Z): every named view is built Z-up and carried over.
    fn from_z(self) -> Quat {
        match self {
            Up::Z => [1.0, 0.0, 0.0, 0.0],
            Up::Y => qaxis([1.0, 0.0, 0.0], -FRAC_PI_2),
        }
    }
}

/// Blender's numpad views.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Front,
    Back,
    Right,
    Left,
    Top,
    Bottom,
}

/// Blender's numpad orbit steps (4 6 8 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Left,
    Right,
    Up,
    Down,
}

/// The eye. See the module docs for the conventions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Orbit {
    pub pivot: V3,
    /// View → world.
    pub rot: Quat,
    pub dist: f32,
    /// Focal length, mm, on a 36mm back across the larger side.
    pub lens: f32,
    pub up: Up,
}

impl Orbit {
    /// Blender's default 3D view, looking at the origin from in front,
    /// above and to the right, ten units off.
    pub fn new(up: Up) -> Self {
        // Blender's startup `view_rotation`.
        let blender = qnorm([0.716, 0.439, 0.291, 0.459]);
        Orbit { pivot: [0.0; 3], rot: qnorm(qmul(up.from_z(), blender)), dist: 10.0, lens: 35.0, up }
    }

    /// Where the eye is.
    pub fn eye(&self) -> V3 {
        add(self.pivot, qrot(self.rot, [0.0, 0.0, self.dist]))
    }

    /// The view's right, up and back (+Z, towards the eye) axes in the world.
    pub fn axes(&self) -> (V3, V3, V3) {
        (qrot(self.rot, [1.0, 0.0, 0.0]), qrot(self.rot, [0.0, 1.0, 0.0]), qrot(self.rot, [0.0, 0.0, 1.0]))
    }

    /// World → view: a point as the eye sees it (it looks down −Z).
    pub fn to_view(&self, p: V3) -> V3 {
        qrot(qconj(self.rot), sub(p, self.eye()))
    }

    /// Turntable: a horizontal drag turns about the WORLD's up, a vertical
    /// one tilts about the view's own sideways axis, so the horizon never
    /// rolls. The hand grabs the scene: drag right and the eye travels left
    /// round the pivot; drag down and it rises.
    pub fn orbit(&mut self, dx_px: f32, dy_px: f32) {
        self.turn(-dx_px * TURNTABLE_RAD_PER_PX, -dy_px * TURNTABLE_RAD_PER_PX);
    }

    /// Yaw about the world up, then pitch about the view's X, in radians.
    pub fn turn(&mut self, yaw: f32, pitch: f32) {
        let q = qmul(qaxis(self.up.axis(), yaw), self.rot);
        self.rot = qnorm(qmul(q, qaxis([1.0, 0.0, 0.0], pitch)));
    }

    /// Move the pivot so the picture follows the hand: `units_per_px` is
    /// one screen pixel at the pivot's depth ([`Orbit::units_per_px`]).
    pub fn pan(&mut self, dx_px: f32, dy_px: f32, units_per_px: f32) {
        let (right, up, _) = self.axes();
        let (sx, sy) = (-dx_px * units_per_px, dy_px * units_per_px);
        for i in 0..3 {
            self.pivot[i] += right[i] * sx + up[i] * sy;
        }
    }

    /// Dolly: scale the distance (above 1 backs off).
    pub fn dolly(&mut self, factor: f32) {
        self.dist = (self.dist * factor).clamp(1e-3, 1e6);
    }

    /// Focal length in pixels for a picture `size` big.
    pub fn focal_px(&self, size: Vec2) -> f32 {
        size.x.max(size.y).max(1.0) * 0.5 * self.lens.max(1.0) / HALF_SENSOR
    }

    /// World units a screen pixel covers at the pivot, the picture `size`
    /// big on screen.
    pub fn units_per_px(&self, size: Vec2) -> f32 {
        self.dist / self.focal_px(size)
    }

    /// Where `p` lands in `rect`, or `None` behind the eye.
    pub fn project(&self, p: V3, rect: Rect) -> Option<Pos2> {
        self.project_view(self.to_view(p), rect)
    }

    fn project_view(&self, v: V3, rect: Rect) -> Option<Pos2> {
        if v[2] > -NEAR {
            return None;
        }
        let f = self.focal_px(rect.size());
        let c = rect.center();
        Some(Pos2::new(c.x + f * v[0] / -v[2], c.y - f * v[1] / -v[2]))
    }

    /// A numpad view: the eye moves round the pivot, distance kept.
    pub fn snap(&mut self, v: View) {
        let front = qaxis([1.0, 0.0, 0.0], FRAC_PI_2);
        let z = [0.0, 0.0, 1.0];
        let q = match v {
            View::Front => front,
            View::Back => qmul(qaxis(z, PI), front),
            View::Right => qmul(qaxis(z, FRAC_PI_2), front),
            View::Left => qmul(qaxis(z, -FRAC_PI_2), front),
            View::Top => [1.0, 0.0, 0.0, 0.0],
            View::Bottom => qaxis([1.0, 0.0, 0.0], PI),
        };
        self.rot = qnorm(qmul(self.up.from_z(), q));
    }

    /// The named view this eye is looking along, if it is one.
    pub fn named(&self) -> Option<View> {
        [View::Front, View::Back, View::Right, View::Left, View::Top, View::Bottom].into_iter().find(|v| {
            let mut o = *self;
            o.snap(*v);
            qdot(o.rot, self.rot).abs() > 0.9999
        })
    }

    /// A numpad orbit step.
    pub fn step(&mut self, s: Step) {
        match s {
            Step::Left => self.turn(-STEP_RAD, 0.0),
            Step::Right => self.turn(STEP_RAD, 0.0),
            Step::Up => self.turn(0.0, -STEP_RAD),
            Step::Down => self.turn(0.0, STEP_RAD),
        }
    }

    /// Numpad 9: the same view from the other side of the pivot.
    pub fn opposite(&mut self) {
        self.rot = qnorm(qmul(self.rot, qaxis([0.0, 1.0, 0.0], PI)));
    }

    /// Frame a sphere: the pivot on its centre, backed off until it fits
    /// the narrower side of a picture `size` big. The view's direction is
    /// kept.
    pub fn frame(&mut self, centre: V3, radius: f32, size: Vec2) {
        let f = self.focal_px(size);
        let half = size.x.min(size.y).max(1.0) * 0.5;
        let half_angle = (half / f).atan();
        self.pivot = centre;
        self.dist = (radius.max(1e-3) / half_angle.sin() * 1.05).clamp(1e-3, 1e6);
    }

    /// One frame of Blender's hands over the view: `resp` is the view's
    /// response (it must sense drags), `rect` the picture on screen. True
    /// when the eye moved. A drag's role is fixed at its press: letting go
    /// of shift mid-drag does not turn a pan into an orbit.
    pub fn navigate(&mut self, ui: &egui::Ui, resp: &egui::Response, rect: Rect) -> bool {
        let id = resp.id.with("view3d-hands");
        let before = *self;
        for b in [PointerButton::Middle, PointerButton::Primary] {
            if resp.drag_started_by(b) {
                let m = ui.input(|i| i.modifiers);
                ui.data_mut(|d| d.insert_temp(id, drag_nav(b, m).map(|n| (b, n))));
            }
        }
        let held = ui.data(|d| d.get_temp::<Option<(PointerButton, Nav)>>(id)).flatten();
        if let Some((b, n)) = held {
            if resp.dragged_by(b) {
                let d = resp.drag_delta();
                self.apply(n, d, rect.size());
            }
            if resp.drag_stopped_by(b) || !ui.input(|i| i.pointer.button_down(b)) {
                ui.data_mut(|d| d.remove::<Option<(PointerButton, Nav)>>(id));
            }
        }
        if resp.hovered() {
            let (eye, zoom) = ui.input(|i| (classify_eye3d(&i.events), crate::nav::classify_wheel(&i.events).zoom));
            if eye.orbit != Vec2::ZERO {
                self.orbit(eye.orbit.x, eye.orbit.y);
            }
            if eye.pan != Vec2::ZERO {
                let u = self.units_per_px(rect.size());
                self.pan(eye.pan.x, eye.pan.y, u);
            }
            if (zoom - 1.0).abs() > 1e-5 {
                self.dolly(1.0 / zoom);
            }
        }
        *self != before
    }

    /// One drag step of a hand.
    pub fn apply(&mut self, n: Nav, d: Vec2, size: Vec2) {
        match n {
            Nav::Orbit => self.orbit(d.x, d.y),
            Nav::Pan => {
                let u = self.units_per_px(size);
                self.pan(d.x, d.y, u);
            }
            // Blender's dolly: drag up comes closer.
            Nav::Dolly => self.dolly((d.y * 0.005).exp()),
        }
    }
}

/// The near clip the overlays use, world units in front of the eye.
const NEAR: f32 = 1e-3;

/// What a drag over a 3D view does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nav {
    Orbit,
    Pan,
    Dolly,
}

/// Classify a drag over a 3D view: MMB, or alt+LMB (Blender's "emulate 3
/// button mouse"); shift pans, ctrl/cmd dollies. `None` is not navigation.
pub fn drag_nav(button: PointerButton, m: Modifiers) -> Option<Nav> {
    let ctrl = m.ctrl || m.command;
    let three = button == PointerButton::Middle || (button == PointerButton::Primary && m.alt);
    three.then_some(if ctrl {
        Nav::Dolly
    } else if m.shift {
        Nav::Pan
    } else {
        Nav::Orbit
    })
}

/// Two fingers over a 3D view, as Blender reads them: a swipe orbits,
/// shift+swipe pans, ctrl/shift on a wheel pan by axis. Zoom (pinch,
/// ctrl+swipe, a wheel notch) is `nav::classify_wheel`'s.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Eye3d {
    pub orbit: Vec2,
    pub pan: Vec2,
}

/// Classify raw wheel events for a 3D view. Pure, like `nav::classify_wheel`.
pub fn classify_eye3d(events: &[egui::Event]) -> Eye3d {
    let mut out = Eye3d::default();
    for e in events {
        if let egui::Event::MouseWheel { unit, delta, modifiers, .. } = e {
            let ctrl = modifiers.ctrl || modifiers.command;
            if matches!(unit, egui::MouseWheelUnit::Point) {
                if ctrl {
                    // ctrl + two fingers zooms.
                } else if modifiers.shift {
                    out.pan += *delta;
                } else {
                    out.orbit += *delta;
                }
            } else if ctrl {
                out.pan.x += delta.y * crate::nav::LINE_PX;
            } else if modifiers.shift {
                out.pan.y += delta.y * crate::nav::LINE_PX;
            }
        }
    }
    out
}

/// Colours of the overlays. The defaults are Blender's.
#[derive(Clone, Copy, Debug)]
pub struct Style {
    pub grid: Color32,
    pub x: Color32,
    pub y: Color32,
    pub z: Color32,
}

impl Default for Style {
    fn default() -> Self {
        Style {
            grid: Color32::from_gray(110),
            x: Color32::from_rgb(232, 62, 82),
            y: Color32::from_rgb(130, 200, 30),
            z: Color32::from_rgb(46, 132, 232),
        }
    }
}

/// A floor grid on the ground plane through the origin, its step a power
/// of ten that suits the distance, fading out from under the pivot; the
/// two ground axes drawn in their colours. Draw it over a picture that
/// has no floor of its own: it has no depth test.
pub fn paint_grid(p: &egui::Painter, rect: Rect, o: &Orbit, style: &Style) {
    let up = o.up.axis();
    // The ground's two axes: X, and Z (Y-up) or Y (Z-up).
    let (a, b, b_col) = match o.up {
        Up::Y => (0usize, 2usize, style.z),
        Up::Z => (0usize, 1usize, style.y),
    };
    // Where the view's centre meets the ground (or the pivot's foot).
    let foot = {
        let mut f = o.pivot;
        f[if o.up == Up::Y { 1 } else { 2 }] = 0.0;
        f
    };
    let h = dot(sub(o.eye(), foot), up).abs();
    let reach = (o.dist + h).max(1e-3);
    let step = 10f32.powf((reach / 10.0).log10().floor());
    let n = 40i32;
    let extent = step * n as f32;
    let ca = (foot[a] / step).round() as i32;
    let cb = (foot[b] / step).round() as i32;
    let line = |s: V3, e: V3, col: Color32, w: f32| {
        if let Some((s, e)) = clip_segment(o, s, e, rect) {
            p.line_segment([s, e], Stroke::new(w, col));
        }
    };
    for k in -n..=n {
        for (along, across, ci) in [(a, b, ca), (b, a, cb)] {
            let i = ci + k;
            let off = i as f32 * step;
            let fade = 1.0 - (k.abs() as f32 / n as f32);
            let major = i.rem_euclid(10) == 0;
            let axis = i == 0;
            let mut s = [0.0; 3];
            let mut e = [0.0; 3];
            s[along] = off;
            e[along] = off;
            let c = if along == a { cb } else { ca } as f32 * step;
            s[across] = c - extent;
            e[across] = c + extent;
            if axis {
                // A line ALONG `across` at 0 on `along` is the `across` axis.
                let col = if across == a { style.x } else { b_col };
                line(s, e, col.gamma_multiply(0.85), 1.5);
            } else {
                let g = if major { 0.55 } else { 0.25 } * fade * fade;
                line(s, e, style.grid.gamma_multiply(g), 1.0);
            }
        }
    }
}

/// A world segment as screen points, clipped to the near plane; `None`
/// when all of it is behind the eye.
fn clip_segment(o: &Orbit, s: V3, e: V3, rect: Rect) -> Option<(Pos2, Pos2)> {
    let (mut a, mut b) = (o.to_view(s), o.to_view(e));
    let z = -NEAR * 10.0;
    if a[2] > z && b[2] > z {
        return None;
    }
    let cut = |p: V3, q: V3| {
        let t = (z - p[2]) / (q[2] - p[2]);
        [p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t, z]
    };
    if a[2] > z {
        a = cut(b, a);
    } else if b[2] > z {
        b = cut(a, b);
    }
    Some((o.project_view(a, rect)?, o.project_view(b, rect)?))
}

/// Blender's navigation gizmo, in a disc centred at `at`: the six axis
/// ends as they face the eye, positive ones labelled. Click an end to look
/// from it; drag on the disc to orbit. True when the eye changed.
pub fn gizmo(ui: &mut egui::Ui, at: Pos2, radius: f32, o: &mut Orbit, style: &Style) -> bool {
    let rect = Rect::from_center_size(at, Vec2::splat(radius * 2.0));
    let resp = ui.interact(rect, ui.id().with("view3d-gizmo"), egui::Sense::click_and_drag());
    let p = ui.painter_at(rect.expand(2.0));
    let hover = resp.hovered() || resp.dragged();
    if hover {
        p.circle_filled(at, radius, Color32::from_white_alpha(28));
    }
    let arm = radius * 0.72;
    let dot_r = radius * 0.2;
    let inv = qconj(o.rot);
    let mut ends: Vec<(f32, Pos2, Color32, Option<&str>, View)> = Vec::new();
    let names = |i: usize| match (i, o.up) {
        (0, _) => ("X", View::Right, View::Left),
        (1, Up::Y) => ("Y", View::Top, View::Bottom),
        (1, Up::Z) => ("Y", View::Back, View::Front),
        (2, Up::Y) => ("Z", View::Front, View::Back),
        (_, _) => ("Z", View::Top, View::Bottom),
    };
    for i in 0..3 {
        let mut v = [0.0; 3];
        v[i] = 1.0;
        let s = qrot(inv, v);
        let col = [style.x, style.y, style.z][i];
        let (label, pos_view, neg_view) = names(i);
        ends.push((s[2], at + Vec2::new(s[0], -s[1]) * arm, col, Some(label), pos_view));
        ends.push((-s[2], at - Vec2::new(s[0], -s[1]) * arm, col.gamma_multiply(0.45), None, neg_view));
    }
    // Back to front, so the nearer ends paint over the farther.
    ends.sort_by(|a, b| a.0.total_cmp(&b.0));
    let pointer = ui.input(|i| i.pointer.hover_pos());
    let mut hit: Option<View> = None;
    for (_, pos, col, label, view) in &ends {
        if label.is_some() {
            p.line_segment([at, *pos], Stroke::new(2.0, *col));
        }
        let over = pointer.is_some_and(|q| q.distance(*pos) <= dot_r);
        if over {
            hit = Some(*view);
        }
        p.circle_filled(*pos, dot_r, *col);
        if over {
            p.circle_stroke(*pos, dot_r, Stroke::new(1.5, Color32::WHITE));
        }
        if let Some(l) = label {
            p.text(*pos, egui::Align2::CENTER_CENTER, *l, egui::FontId::proportional(dot_r * 1.3), Color32::BLACK);
        }
    }
    let before = *o;
    if resp.dragged_by(PointerButton::Primary) {
        o.orbit(resp.drag_delta().x, resp.drag_delta().y);
    }
    if resp.clicked() {
        if let Some(v) = hit {
            // A second click on the end already faced looks from behind.
            let mut t = *o;
            t.snap(v);
            if qdot(t.rot, o.rot).abs() > 0.9999 {
                o.opposite();
            } else {
                *o = t;
            }
        }
    }
    *o != before
}

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn qmul(a: Quat, b: Quat) -> Quat {
    let [aw, ax, ay, az] = a;
    let [bw, bx, by, bz] = b;
    [
        aw * bw - ax * bx - ay * by - az * bz,
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
    ]
}

/// The turn of `angle` radians about the unit `axis`.
pub fn qaxis(axis: V3, angle: f32) -> Quat {
    let (s, c) = (angle * 0.5).sin_cos();
    [c, axis[0] * s, axis[1] * s, axis[2] * s]
}

pub fn qnorm(q: Quat) -> Quat {
    let n = qdot(q, q).sqrt().max(1e-9);
    [q[0] / n, q[1] / n, q[2] / n, q[3] / n]
}

fn qdot(a: Quat, b: Quat) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3]
}

pub fn qconj(q: Quat) -> Quat {
    [q[0], -q[1], -q[2], -q[3]]
}

/// Rotate `v` by `q`.
pub fn qrot(q: Quat, v: V3) -> V3 {
    let [w, x, y, z] = q;
    let cross = |a: V3, b: V3| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    let u = [x, y, z];
    let uv = cross(u, v);
    let uuv = cross(u, uv);
    [v[0] + 2.0 * (w * uv[0] + uuv[0]), v[1] + 2.0 * (w * uv[1] + uuv[1]), v[2] + 2.0 * (w * uv[2] + uuv[2])]
}

/// The rotation matrix of `q`, rows; `m[r][c]`, so column c is where the
/// local axis c points.
pub fn qmat(q: Quat) -> [[f32; 3]; 3] {
    let x = qrot(q, [1.0, 0.0, 0.0]);
    let y = qrot(q, [0.0, 1.0, 0.0]);
    let z = qrot(q, [0.0, 0.0, 1.0]);
    [[x[0], y[0], z[0]], [x[1], y[1], z[1]], [x[2], y[2], z[2]]]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: V3, b: V3) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-3)
    }

    #[test]
    fn named_views_put_the_eye_where_blender_does_in_either_world() {
        for up in [Up::Y, Up::Z] {
            let mut o = Orbit { dist: 4.0, ..Orbit::new(up) };
            let u = up.axis();
            let toward = match up {
                Up::Y => [0.0, 0.0, 4.0],  // front looks down -Z
                Up::Z => [0.0, -4.0, 0.0], // front looks along +Y
            };
            o.snap(View::Front);
            assert!(close(o.eye(), toward), "{up:?} front {:?}", o.eye());
            assert!(close(o.axes().1, u), "{up:?} front keeps up up");
            o.snap(View::Back);
            assert!(close(o.eye(), [-toward[0], -toward[1], -toward[2]]), "{up:?} back");
            o.snap(View::Right);
            assert!(close(o.eye(), [4.0, 0.0, 0.0]), "{up:?} right {:?}", o.eye());
            o.snap(View::Left);
            assert!(close(o.eye(), [-4.0, 0.0, 0.0]), "{up:?} left");
            o.snap(View::Top);
            assert!(close(o.eye(), [u[0] * 4.0, u[1] * 4.0, u[2] * 4.0]), "{up:?} top {:?}", o.eye());
            assert_eq!(o.named(), Some(View::Top));
            o.snap(View::Bottom);
            assert!(close(o.eye(), [u[0] * -4.0, u[1] * -4.0, u[2] * -4.0]), "{up:?} bottom");
            o.snap(View::Front);
            o.opposite();
            assert_eq!(o.named(), Some(View::Back), "{up:?}: 9 from front is back");
        }
    }

    #[test]
    fn the_turntable_turns_about_world_up_and_never_rolls() {
        let mut o = Orbit { dist: 4.0, ..Orbit::new(Up::Y) };
        o.snap(View::Front);
        o.orbit(FRAC_PI_2 / TURNTABLE_RAD_PER_PX, 0.0);
        let e = o.eye();
        assert!(e[1].abs() < 1e-3 && e[0] < -3.9, "drag right: the eye goes left, same height {e:?}");
        assert!(close(o.axes().1, [0.0, 1.0, 0.0]), "no roll");
        o.orbit(0.0, 0.25 * PI / TURNTABLE_RAD_PER_PX);
        assert!(o.eye()[1] > 2.5, "drag down: the eye rises {:?}", o.eye());
        assert!(o.axes().0[1].abs() < 1e-4, "the sideways axis stays level");
    }

    #[test]
    fn a_pan_follows_the_hand_at_the_pivot() {
        let mut o = Orbit { dist: 5.0, ..Orbit::new(Up::Y) };
        o.snap(View::Front);
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 400.0));
        let before = o.project([0.0; 3], rect).unwrap();
        assert!((before - rect.center()).length() < 1e-3, "the pivot is centred");
        // Drag the scene 30px right and 10 down: the origin goes with it.
        o.pan(30.0, 10.0, o.units_per_px(rect.size()));
        let after = o.project([0.0; 3], rect).unwrap();
        assert!((after - before - Vec2::new(30.0, 10.0)).length() < 1e-2, "{before:?} -> {after:?}");
    }

    #[test]
    fn the_projection_is_a_36mm_back_across_the_larger_side() {
        // 36mm lens on a 36mm back: 45 degrees each side horizontally.
        let o = Orbit { pivot: [0.0; 3], rot: [1.0, 0.0, 0.0, 0.0], dist: 1.0, lens: 36.0, up: Up::Y };
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 200.0));
        let p = o.project([0.5, 0.0, 0.0], rect).unwrap();
        assert!((p.x - 400.0).abs() < 1e-2, "{p:?}");
        assert!(o.project([0.0, 0.0, 2.0], rect).is_none(), "behind the eye");
    }

    #[test]
    fn framing_fits_the_sphere_and_keeps_the_direction() {
        let mut o = Orbit::new(Up::Y);
        let dir = o.axes().2;
        let size = Vec2::new(800.0, 400.0);
        o.frame([1.0, 2.0, 3.0], 2.0, size);
        assert!(close(o.pivot, [1.0, 2.0, 3.0]) && close(o.axes().2, dir));
        let rect = Rect::from_min_size(Pos2::ZERO, size);
        // The sphere's top, seen square on, is inside the short side.
        let (_, up, _) = o.axes();
        let top = o.project(add(o.pivot, [up[0] * 2.0, up[1] * 2.0, up[2] * 2.0]), rect).unwrap();
        assert!(top.y > 0.0 && top.y < 30.0, "{top:?}");
    }

    #[test]
    fn drags_and_fingers_map_like_blender() {
        let m = |shift, ctrl, alt| Modifiers { shift, ctrl, alt, ..Default::default() };
        use PointerButton::*;
        assert_eq!(drag_nav(Middle, m(false, false, false)), Some(Nav::Orbit));
        assert_eq!(drag_nav(Middle, m(true, false, false)), Some(Nav::Pan));
        assert_eq!(drag_nav(Middle, m(false, true, false)), Some(Nav::Dolly));
        assert_eq!(drag_nav(Primary, m(false, false, true)), Some(Nav::Orbit));
        assert_eq!(drag_nav(Primary, m(true, false, true)), Some(Nav::Pan));
        assert_eq!(drag_nav(Primary, m(false, true, true)), Some(Nav::Dolly));
        assert_eq!(drag_nav(Primary, m(false, false, false)), None);
        let wheel = |unit, d: [f32; 2], modifiers| egui::Event::MouseWheel {
            unit,
            delta: Vec2::new(d[0], d[1]),
            modifiers,
            phase: egui::TouchPhase::Move,
        };
        let swipe = classify_eye3d(&[wheel(egui::MouseWheelUnit::Point, [30.0, -12.0], m(false, false, false))]);
        assert_eq!(swipe.orbit, Vec2::new(30.0, -12.0));
        let pan = classify_eye3d(&[wheel(egui::MouseWheelUnit::Point, [30.0, -12.0], m(true, false, false))]);
        assert_eq!(pan.pan, Vec2::new(30.0, -12.0));
    }
}
