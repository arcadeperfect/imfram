//! Sizes for the node canvas. Colours come from the app's `WireStyle`s
//! and the context's `Visuals` (hot = selection stroke, cut = error).

pub const NODE_W: f32 = 200.0;
/// How narrow and how wide E can stretch a node.
pub const NODE_W_MIN: f32 = 150.0;
pub const NODE_W_MAX: f32 = 1200.0;
/// A corner dot's radius, and how far from it a drag still takes it.
pub const DOT_R: f32 = 6.0;
pub const DOT_HIT: f32 = 9.0;
/// How near (canvas units) a free input and an output must come to court:
/// a dashed line between them, which shift turns into a wire. They are
/// wired when nearer than `MAGNET_CLOSE` of that.
pub const MAGNET_REACH: f32 = 110.0;
pub const MAGNET_CLOSE: f32 = 0.65;
pub const PAD: f32 = 8.0;
pub const ROW_H: f32 = 18.0;
pub const PORT_LABEL_H: f32 = 13.0;
pub const PORT_R: f32 = 5.0;
/// How close (canvas units) the pointer must be to a port to hit it.
pub const PORT_HIT: f32 = 16.0;
pub const ZOOM_MIN: f32 = 0.1;
pub const ZOOM_MAX: f32 = 2.0;
/// A wire's label.
pub const LABEL_FONT: f32 = 11.0;
