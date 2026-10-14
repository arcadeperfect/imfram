//! An item's placement: translate, rotate, scale — a similarity per axis,
//! never a shear, so every item keeps a rect for a shape and a rotation
//! handle that means something. Pure.

use egui::{vec2, Pos2, Vec2};

/// Local -> world: `pos + R(rot) * (local * scale)`. Local content is laid
/// out about its own origin (a centred rect rotates about its centre).
#[derive(Clone, Copy, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
pub struct Xform {
    pub pos: Pos2,
    /// Radians, clockwise on screen (y is down).
    pub rot: f32,
    pub scale: Vec2,
}

impl Default for Xform {
    fn default() -> Self {
        Self { pos: Pos2::ZERO, rot: 0.0, scale: Vec2::splat(1.0) }
    }
}

pub fn rotate(v: Vec2, a: f32) -> Vec2 {
    let (s, c) = a.sin_cos();
    vec2(c * v.x - s * v.y, s * v.x + c * v.y)
}

impl Xform {
    pub fn at(pos: Pos2) -> Self {
        Self { pos, ..Default::default() }
    }

    pub fn apply(&self, local: Pos2) -> Pos2 {
        self.pos + rotate(local.to_vec2() * self.scale, self.rot)
    }

    /// World -> local. A zero scale maps everything to the origin rather
    /// than to infinity.
    pub fn inverse(&self, world: Pos2) -> Pos2 {
        let v = rotate(world - self.pos, -self.rot);
        let safe = |s: f32| if s.abs() < 1e-12 { 0.0 } else { 1.0 / s };
        Pos2::new(v.x * safe(self.scale.x), v.y * safe(self.scale.y))
    }

    /// How many world units one local unit spans (the larger axis): what a
    /// screen-space tolerance divides by.
    pub fn unit(&self) -> f32 {
        self.scale.x.abs().max(self.scale.y.abs()).max(1e-6)
    }

    /// This placement after a group move: rotate by `rot` and scale by
    /// `scale` (along the WORLD axes for the position, the item's own axes
    /// for its size) about `pivot`, then translate by `shift`. What G, R, S
    /// and the selection box's handles all reduce to.
    pub fn moved(&self, pivot: Pos2, shift: Vec2, rot: f32, scale: Vec2) -> Self {
        let rel = rotate((self.pos - pivot) * scale, rot);
        Self { pos: pivot + rel + shift, rot: self.rot + rot, scale: self.scale * scale }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;

    #[test]
    fn inverse_undoes_apply() {
        let x = Xform { pos: pos2(10.0, -4.0), rot: 0.7, scale: vec2(2.0, 0.5) };
        let p = pos2(3.0, 9.0);
        assert!((x.inverse(x.apply(p)) - p).length() < 1e-4);
    }

    #[test]
    fn rotation_is_clockwise_on_screen() {
        let x = Xform { rot: std::f32::consts::FRAC_PI_2, ..Default::default() };
        // +x turns to +y, which is DOWN on screen: clockwise.
        assert!((x.apply(pos2(1.0, 0.0)) - pos2(0.0, 1.0)).length() < 1e-6);
    }

    #[test]
    fn a_group_move_carries_positions_about_the_pivot() {
        let x = Xform::at(pos2(2.0, 0.0));
        let m = x.moved(pos2(1.0, 0.0), Vec2::ZERO, std::f32::consts::PI, Vec2::splat(2.0));
        assert!((m.pos - pos2(-1.0, 0.0)).length() < 1e-5, "{:?}", m.pos);
        assert_eq!(m.scale, Vec2::splat(2.0));
        assert!((m.rot - std::f32::consts::PI).abs() < 1e-6);
    }
}
