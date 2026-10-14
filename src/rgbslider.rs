//! General-purpose RGB slider: three tinted lanes, a gang box, and colour
//! axes you reach by holding a key mid-drag.
//!
//! One widget for every rgb triple in the app — a Mult's multiplier, a
//! Grade's gain, a tint. It is deliberately egui-only (no `engine`, no
//! `crate::`) so it can be lifted out whole; `widgets::rgb_row` is the thin
//! glue that gives it a label, a keyframe dot and a HUD drag-out.
//!
//! ## The gesture
//!
//! Ganged (the default) a grab moves all three channels together and
//! RELATIVELY — the offsets between them survive, same contract as the
//! Grade HUD's gang box. Ungang and each lane moves alone.
//!
//! | held | axis | moves |
//! |---|---|---|
//! | — | `rgb` / lane | intensity (the only axis that changes brightness) |
//! | `G` | inverts gang | ganged grab -> that lane alone, and back |
//! | `S` | `sat` | chroma about the working-space luma |
//! | `D` | `temp` | orange <-> cyan (Nuke's T) |
//! | `F` | `tint` | green <-> magenta (Nuke's M) |
//!
//! Keys are read LIVE, not latched at press: release `D` mid-drag and the
//! same gesture goes back to moving intensity. Precedence when several are
//! down is S, then D, then F.
//!
//! **S/D/F are exactly luma-preserving.** Each moves the triple along its
//! axis and then restores the weighted luma it started with — additively
//! for offset-type params, multiplicatively for scale-type ones (which also
//! keeps them positive). The weights are the WORKING SPACE's, so this is
//! AP1 luma in an ACEScg comp, not a hardcoded Rec.709 one.
//!
//! Owned by instance J (see .agents/board.md).

use egui::{vec2, Color32, Id, Key, Rect, Sense, Stroke, Ui};

/// Per-channel lane tints, matching the Grade HUD's.
pub const LANE_TINT: [Color32; 3] = [
    Color32::from_rgb(0xc9, 0x5c, 0x55),
    Color32::from_rgb(0x63, 0xb3, 0x62),
    Color32::from_rgb(0x5f, 0x83, 0xd4),
];

/// Lane row height: tall enough to seat a standard drag field.
const LANE_H: f32 = 20.0;
/// A strip above the lanes, empty until an axis is armed. Reserved rather
/// than drawn over the neighbours: a modifier you cannot see is a modifier
/// you forget, and a tag painted outside the widget gets clipped by the
/// tile's scroll area exactly when the row is at the top of it.
const TAG_H: f32 = 12.0;
/// Fixed chrome (never scales with zoom — read, not aimed).
const GANG_W: f32 = 15.0;
const LETTER_W: f32 = 10.0;
const NUM_W: f32 = 54.0;
const NUM_H: f32 = 18.0;
const KNOB_R: f32 = 4.5;
const GAP: f32 = 4.0;
/// The colour chip at the right end, spanning all three lanes.
pub const SWATCH_W: f32 = 22.0;
/// Everything on the row that is not track: gang box, lane letter, the
/// knob's overhang at each end, the number field and the chip.
pub const CHROME_W: f32 =
    GANG_W + GAP + LETTER_W + GAP + KNOB_R + SWATCH_W + GAP + NUM_W + GAP + KNOB_R;
/// Below this a slider is not a slider. `widgets::tiles_for` gives any node
/// carrying an rgb row two tiles for exactly this reason.
pub const MIN_TRACK: f32 = 90.0;

/// The widget's NATURAL height: its tag row plus three lanes. Public so a
/// layout can reserve exactly the room one needs instead of guessing at it —
/// a cell four pixels short clips the bottom lane's value box, which reads
/// as a broken widget rather than as a layout that was off by four.
pub const NATURAL_H: f32 = TAG_H + LANE_H * 3.0;
/// Offset-type chip gain: mid grey at 0, saturating around ±0.25 — the
/// range a lift or an offset actually lives in.
const SWATCH_OFFSET_GAIN: f32 = 2.0;
/// Tone pivot for the chip: `y / (y + PIVOT)` puts a neutral 1.0 on 0.22
/// linear, which is mid grey once the 2.2 encode is applied — and nothing
/// above it ever reaches flat white.
const SWATCH_PIVOT: f32 = 3.545;

/// Log-space floor for scale-type params: `ln(0)` is not a colour.
const EPS: f32 = 1e-4;
/// How hard the preview swatch exaggerates. The band is a picture of WHICH
/// WAY the axis goes, not a colour readout — at true strength a temperature
/// push reads as two barely different greys, which tells the hand nothing.
/// A lift of 0.05 is a real cast and has to look like one.
const PREVIEW_GAIN: f32 = 4.0;
const PREVIEW_SAT: f32 = 2.0;

/// How far a full-track drag pushes each colour axis. Intensity is 1:1 with
/// the track (the knob follows the pointer, which is the whole point of a
/// slider); the colour axes are relative, so they get their own rates.
const SAT_SPAN: f32 = 1.5;
/// Offset-type chroma: fraction of the track's value range.
const CHROMA_SPAN_OFFSET: f32 = 0.5;
/// Scale-type chroma, in log units — a full track is a ~2.1x/0.47x split.
const CHROMA_SPAN_SCALE: f32 = 0.75;

/// Colour-axis directions. `+d` is always the right-hand end of the drag:
/// warmer, and more magenta — the Lightroom convention, which is Nuke's T
/// and M with the sign that matches the way a hand expects to push.
const WARM_DIR: [f32; 3] = [1.0, 0.0, -1.0];
const TINT_DIR: [f32; 3] = [0.5, -1.0, 0.5];

/// What the triple MEANS, which decides how the colour axes move it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RgbMode {
    /// Additive around 0 — a lift, an offset. Chroma moves are linear.
    Offset,
    /// Multiplicative around 1 — a gain, a multiplier. Chroma moves happen
    /// in log space, so they are ratios and can never cross zero.
    Scale,
}

/// Everything the widget needs that isn't the value itself.
#[derive(Clone, Copy, Debug)]
pub struct RgbSpec {
    pub lo: f32,
    pub hi: f32,
    pub mode: RgbMode,
    /// Working-space luma weights (`Primaries::luma_weights()`), the frame
    /// the colour axes preserve brightness in.
    pub luma: [f32; 3],
    /// Units per pixel for the numeric fields.
    pub speed: f32,
    /// Working primaries -> Rec.709, for the colour-op preview only
    /// (`color::primaries_matrix`). Identity draws the swatch as if the comp
    /// were already Rec.709, which reads oversaturated in an ACEScg one.
    pub display: [[f32; 3]; 3],
}

const IDENTITY3: [[f32; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

impl RgbSpec {
    pub fn scale(lo: f32, hi: f32, luma: [f32; 3]) -> Self {
        Self { lo, hi, mode: RgbMode::Scale, luma, speed: 0.01, display: IDENTITY3 }
    }
    pub fn offset(lo: f32, hi: f32, luma: [f32; 3]) -> Self {
        Self { lo, hi, mode: RgbMode::Offset, luma, speed: 0.005, display: IDENTITY3 }
    }
    /// Working primaries -> Rec.709 for the preview swatch.
    pub fn with_display(mut self, m: [[f32; 3]; 3]) -> Self {
        self.display = m;
        self
    }
}

/// Which way a drag pushes the triple.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    /// All three together, relative — the ganged default.
    Rgb,
    /// One channel alone.
    Lane(usize),
    /// Chroma about the luma pivot.
    Sat,
    /// Orange <-> cyan.
    Warm,
    /// Green <-> magenta.
    Tint,
}

impl Axis {
    /// Short tag painted on the widget while the axis is live.
    pub fn tag(self) -> &'static str {
        match self {
            Axis::Rgb => "rgb",
            Axis::Lane(0) => "r",
            Axis::Lane(1) => "g",
            Axis::Lane(2) => "b",
            Axis::Lane(_) => "?",
            Axis::Sat => "sat",
            Axis::Warm => "temp",
            Axis::Tint => "tint",
        }
    }

    /// Sat/temp/tint: the axes that move colour without moving brightness.
    pub fn is_colour(self) -> bool {
        matches!(self, Axis::Sat | Axis::Warm | Axis::Tint)
    }
}

/// Live modifier state, read once per frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub gang_invert: bool,
    pub sat: bool,
    pub warm: bool,
    pub tint: bool,
}

impl Mods {
    pub fn read(ui: &Ui) -> Self {
        // A text field owns the keyboard while it is being typed into —
        // "d" belongs to the number, not to the temperature axis.
        if ui.ctx().egui_wants_keyboard_input() {
            return Self::default();
        }
        ui.input(|i| Self {
            gang_invert: i.key_down(Key::G),
            sat: i.key_down(Key::S),
            warm: i.key_down(Key::D),
            tint: i.key_down(Key::F),
        })
    }

    /// The axis a grab on `lane` would move. Colour axes win over the
    /// gang state: holding S on an unganged widget still slides saturation.
    pub fn axis(self, lane: usize, gang: bool) -> Axis {
        if self.sat {
            Axis::Sat
        } else if self.warm {
            Axis::Warm
        } else if self.tint {
            Axis::Tint
        } else if gang != self.gang_invert {
            Axis::Rgb
        } else {
            Axis::Lane(lane)
        }
    }
}

/// Drag-rate keys: three finer, three coarser, walking the bottom row out
/// from the middle — z/x/c slow a drag down (z finest), v/b/n speed it up
/// (n coarsest). Held, not toggled, so the rate can be stepped mid-drag.
/// Mirrored in log space around 1 (c·v = 1, x·b = 1); n stops at 30
/// because a whole-rail throw in a few pixels was tried and rejected.
/// THE table: every parameter row in the app reads it through here
/// (`widgets::sens_factor` wraps [`rate`]), so a retune lands everywhere
/// at once.
pub const RATE_KEYS: [(Key, f32); 6] = [
    (Key::Z, 0.01),
    (Key::X, 0.1),
    (Key::C, 1.0 / 3.0),
    (Key::V, 3.0),
    (Key::B, 10.0),
    (Key::N, 30.0),
];

/// The held rate factor, if any. Same typing guard as [`Mods::read`]: an
/// "x" typed into a number field must not arm a rate.
///
/// BARE keys only. `cmd+Z` is undo and `cmd+shift+Z` redo — read as a bare
/// key_down, an undo fired mid-drag would also arm the finest rate. Shift
/// is left alone: it is not a chord modifier here, and shift-hover opens a
/// row's range fields.
pub fn rate(ui: &Ui) -> Option<f32> {
    if ui.ctx().egui_wants_keyboard_input() {
        return None;
    }
    ui.input(|i| {
        let m = i.modifiers;
        if m.command || m.mac_cmd || m.ctrl || m.alt {
            return None;
        }
        RATE_KEYS.iter().find(|(k, _)| i.key_down(*k)).map(|&(_, f)| f)
    })
}

/// Weighted mean — the luma of the triple in the working space.
pub fn wmean(w: [f32; 3], v: [f32; 3]) -> f32 {
    w[0] * v[0] + w[1] * v[1] + w[2] * v[2]
}

/// Move `v` along `axis` by `d` and clamp into the spec's range.
///
/// `d` is in value units for `Rgb`/`Lane` (so the knob tracks the pointer)
/// and in axis units for the colour axes.
pub fn nudge(v: [f32; 3], axis: Axis, d: f32, spec: RgbSpec) -> [f32; 3] {
    let mut out = match axis {
        Axis::Rgb => [v[0] + d, v[1] + d, v[2] + d],
        Axis::Lane(i) => {
            let mut o = v;
            if let Some(c) = o.get_mut(i) {
                *c += d;
            }
            o
        }
        _ => chroma(v, axis, d, spec),
    };
    for c in &mut out {
        *c = c.clamp(spec.lo, spec.hi);
    }
    out
}

/// The colour axes: move, then put the luma back exactly.
fn chroma(v: [f32; 3], axis: Axis, d: f32, spec: RgbSpec) -> [f32; 3] {
    let l0 = wmean(spec.luma, v);
    match spec.mode {
        RgbMode::Offset => {
            let mut x = slide(v, axis, d, spec.luma);
            // Additive restore: the right correction for a triple that
            // lives around zero.
            let dl = l0 - wmean(spec.luma, x);
            for c in &mut x {
                *c += dl;
            }
            x
        }
        // A channel at or below ZERO has no ratios to work with (a Grade's
        // gain range reaches -4), so the gesture falls back to the additive
        // axis, which still does something sane. Strictly non-positive: a
        // merely tiny channel is fine in log space, and treating it as
        // broken would let a long push drive it to zero — the one thing the
        // log path exists to prevent.
        RgbMode::Scale if v.iter().any(|x| *x <= 0.0) => {
            let mut x = slide(v, axis, d, spec.luma);
            let dl = l0 - wmean(spec.luma, x);
            for c in &mut x {
                *c += dl;
            }
            x
        }
        RgbMode::Scale => {
            // Ratios, not offsets: slide in log space so a temperature push
            // on a gain of 4 feels like one on a gain of 0.25, and so no
            // channel can be pushed through zero.
            let lv = v.map(|c| c.max(EPS).ln());
            let x = slide(lv, axis, d, spec.luma).map(f32::exp);
            // Multiplicative restore — a positive scalar, so the triple
            // stays positive.
            let l1 = wmean(spec.luma, x);
            let k = if l1 > EPS && l0 > 0.0 { l0 / l1 } else { 1.0 };
            x.map(|c| c * k)
        }
    }
}

/// The axis move itself, in whichever space the caller is working in.
fn slide(x: [f32; 3], axis: Axis, d: f32, w: [f32; 3]) -> [f32; 3] {
    match axis {
        Axis::Sat => {
            // Chroma is what is left after the luma pivot; scale it.
            // A neutral triple has none, so saturation cannot move it —
            // reach for D or F first, then S shapes what they made.
            let m = wmean(w, x);
            let k = (1.0 + d).max(0.0);
            [m + (x[0] - m) * k, m + (x[1] - m) * k, m + (x[2] - m) * k]
        }
        Axis::Warm => [
            x[0] + d * WARM_DIR[0],
            x[1] + d * WARM_DIR[1],
            x[2] + d * WARM_DIR[2],
        ],
        Axis::Tint => [
            x[0] + d * TINT_DIR[0],
            x[1] + d * TINT_DIR[1],
            x[2] + d * TINT_DIR[2],
        ],
        // Intensity never goes through here: it must track the pointer.
        _ => x,
    }
}

/// What the triple does to MID GREY, as a screen colour.
///
/// Only the chroma is shown: a scale-type triple is divided by its own luma,
/// an offset-type one is read relative to its luma. That is what makes the
/// band a picture of the AXIS rather than of the exposure — the colour axes
/// hold luma fixed, so brightness is exactly the thing that does not change.
///
/// The encode is a plain 2.2 gamma after the working -> Rec.709 matrix. This
/// is a param swatch, not comp output: it deliberately does not go through
/// the viewer's display transform (which lives at the blit, in the engine).
pub fn swatch(v: [f32; 3], spec: RgbSpec) -> Color32 {
    let l = wmean(spec.luma, v);
    let c = match spec.mode {
        RgbMode::Scale => {
            let l = l.max(EPS);
            let ch = |x: f32| 0.5 * (1.0 + PREVIEW_SAT * (x / l - 1.0));
            [ch(v[0]), ch(v[1]), ch(v[2])]
        }
        // Offsets live around zero, where a ratio means nothing: show the
        // chroma as a departure from mid grey, gained up to be visible.
        RgbMode::Offset => [
            0.5 + (v[0] - l) * PREVIEW_GAIN,
            0.5 + (v[1] - l) * PREVIEW_GAIN,
            0.5 + (v[2] - l) * PREVIEW_GAIN,
        ],
    };
    let m = spec.display;
    let d = [
        m[0][0] * c[0] + m[0][1] * c[1] + m[0][2] * c[2],
        m[1][0] * c[0] + m[1][1] * c[1] + m[1][2] * c[2],
        m[2][0] * c[0] + m[2][1] * c[1] + m[2][2] * c[2],
    ];
    let enc = |x: f32| (x.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0).round() as u8;
    Color32::from_rgb(enc(d[0]), enc(d[1]), enc(d[2]))
}

/// The triple itself, as a colour — the chip beside the lanes.
///
/// Unlike [`swatch`], brightness is NOT divided out: a ganged drag has to
/// move the chip, or the control looks broken while the numbers climb. A
/// scale-type triple tones its LUMA and lets the channels keep their ratios
/// to it — toning per channel instead compresses them together, and a gain
/// of 4 with a warm cast comes out white, which is the one thing the chip
/// exists to show. An offset-type triple rides either side of mid grey. The
/// working -> Rec.709 matrix is applied while the values are still linear,
/// the only order that is a colour transform at all.
pub fn value_swatch(v: [f32; 3], spec: RgbSpec) -> Color32 {
    let m = spec.display;
    let l = [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ];
    let enc = |x: f32| (x.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0).round() as u8;
    let c = match spec.mode {
        RgbMode::Scale => {
            let y = wmean(spec.luma, v).max(0.0);
            let k = if y > EPS { (y / (y + SWATCH_PIVOT)) / y } else { 0.0 };
            l.map(|x| x * k)
        }
        // A lift of 0.05 is a real cast, and at 1:1 the chip would not show
        // it at all: offsets are read against mid grey with a gain, so the
        // useful part of the range is the part you can see.
        RgbMode::Offset => l.map(|x| 0.5 + x * SWATCH_OFFSET_GAIN),
    };
    Color32::from_rgb(enc(c[0]), enc(c[1]), enc(c[2]))
}

/// Paint the colour-op preview ON THE LANE LINE: the track itself becomes
/// what the armed axis does to mid grey across a full drag, the current
/// value at its centre. It replaces the lane's flat tint rather than laying
/// a band over the row — these sit on top of the picture in the viewer, and
/// a filled block hides both the knobs and the shot.
///
/// The gradient is RELATIVE: its centre is "no change", which is not where
/// the knobs are, so the centre gets a tick.
pub fn paint_axis_line(
    painter: &egui::Painter,
    a: egui::Pos2,
    b: egui::Pos2,
    thickness: f32,
    v: [f32; 3],
    axis: Axis,
    track_w: f32,
    spec: RgbSpec,
) {
    if !axis.is_colour() || (b.x - a.x) < 4.0 {
        return;
    }
    // A full drag each way: the ends of the line are what the ends of the
    // gesture actually give you.
    let d_max = per_px(axis, track_w, spec) * track_w;
    let th = thickness.max(1.0) * 0.5;
    let mut mesh = egui::epaint::Mesh::default();
    const N: usize = 48;
    for i in 0..=N {
        let t = i as f32 / N as f32;
        let x = a.x + t * (b.x - a.x);
        let c = swatch(nudge(v, axis, (t - 0.5) * 2.0 * d_max, spec), spec);
        for y in [a.y - th, a.y + th] {
            mesh.vertices.push(egui::epaint::Vertex {
                pos: egui::pos2(x, y),
                uv: egui::epaint::WHITE_UV,
                color: c,
            });
        }
        if i > 0 {
            let k = (i as u32) * 2;
            mesh.indices.extend_from_slice(&[k - 2, k - 1, k, k - 1, k + 1, k]);
        }
    }
    painter.add(egui::Shape::mesh(mesh));
    // Where the value is NOW: everything left of it undoes, right goes further.
    let cx = (a.x + b.x) * 0.5;
    painter.line_segment(
        [egui::pos2(cx, a.y - th - 2.0), egui::pos2(cx, a.y + th + 2.0)],
        Stroke::new(1.0, Color32::from_black_alpha(120)),
    );
}

/// Drag pixels -> axis units, for callers driving their own handles (the
/// viewer HUDs paint themselves and only borrow the gesture). Pure — a
/// LIVE gesture wants [`drag_units_rated`], which folds the z/x/c/v rate
/// keys in; this stays the raw mapping for expectation maths and tests.
pub fn drag_units(axis: Axis, px: f32, track_w: f32, spec: RgbSpec) -> f32 {
    px * per_px(axis, track_w, spec)
}

/// [`drag_units`] with the held rate key applied — what every live drag
/// call site wants.
pub fn drag_units_rated(ui: &Ui, axis: Axis, px: f32, track_w: f32, spec: RgbSpec) -> f32 {
    drag_units(axis, px, track_w, spec) * rate(ui).unwrap_or(1.0)
}

/// Drag pixels -> axis units.
fn per_px(axis: Axis, track_w: f32, spec: RgbSpec) -> f32 {
    let t = track_w.max(1.0);
    match axis {
        Axis::Sat => SAT_SPAN / t,
        Axis::Warm | Axis::Tint => match spec.mode {
            RgbMode::Offset => (spec.hi - spec.lo) * CHROMA_SPAN_OFFSET / t,
            RgbMode::Scale => CHROMA_SPAN_SCALE / t,
        },
        // 1:1 with the track, so the knob stays under the finger.
        _ => (spec.hi - spec.lo) / t,
    }
}

/// Where the widget last drew itself: the track ends and the centre line
/// of each lane, in screen points. `--test-rgb` aims its synthetic drags
/// with this rather than at hard-coded coordinates, which panel chrome
/// would shift out from under it.
#[derive(Clone, Copy, Debug)]
pub struct Geom {
    pub track_x0: f32,
    pub track_x1: f32,
    pub lane_y: [f32; 3],
    pub knob_x: [f32; 3],
}

pub fn last_geom(ctx: &egui::Context, id_src: impl std::hash::Hash + std::fmt::Debug) -> Option<Geom> {
    ctx.data(|d| d.get_temp::<Geom>(Id::new("rgbslider").with(id_src).with("geom")))
}

/// Is an rgb slider being dragged right now?
///
/// Single-key hotkeys (D frames the properties bar, F frames the viewer)
/// would fire under the colour axes, so the app's hotkey block asks this
/// first. Gated on the pointer as well as the flag, so a widget that
/// vanishes mid-drag can never wedge the keyboard.
pub fn grabbing(ctx: &egui::Context) -> bool {
    let (now, down) = ctx.input(|i| (i.time, i.pointer.any_down()));
    let claimed = ctx.data(|d| d.get_temp::<f64>(Id::new("rgbslider-grab")));
    // A TIMESTAMP, not a flag: several sliders and HUDs write this key every
    // frame, and a plain bool would let whichever ran last clear a claim the
    // one before it had just made. The pointer check makes it self-healing
    // if a widget vanishes mid-drag.
    claimed.is_some_and(|c| now - c < 0.25) && down
}

/// Claim the keyboard for a drag this module did not draw — the viewer HUDs
/// arm the same axes and need the same protection from the app's hotkeys.
pub fn claim_keyboard(ctx: &egui::Context, on: bool) {
    set_grabbing(ctx, on);
}

fn set_grabbing(ctx: &egui::Context, on: bool) {
    if !on {
        return; // claims expire on their own; clearing would race
    }
    let now = ctx.input(|i| i.time);
    ctx.data_mut(|d| d.insert_temp(Id::new("rgbslider-grab"), now));
}

/// What the caller needs back.
pub struct RgbResponse {
    pub changed: bool,
    /// The axis the current (or next) grab moves — for tooltips and tags.
    pub axis: Axis,
    pub dragging: bool,
    /// A ctrl-drag started on the widget: the caller spawns a HUD slider
    /// and no edit is applied.
    pub ctrl_drag: bool,
}

/// Draw the lanes and handle the gesture.
///
/// `driven` shows a live patch-cable value read-only instead of `v`.
/// `suppress` reverts edits for the whole of a HUD drag-out gesture.
pub fn show(
    ui: &mut Ui,
    id_src: impl std::hash::Hash + std::fmt::Debug,
    v: &mut [f32; 3],
    gang: &mut bool,
    spec: RgbSpec,
    driven: Option<[f32; 3]>,
    suppress: bool,
) -> RgbResponse {
    let id = Id::new("rgbslider").with(id_src);
    let before = *v;
    let mods = Mods::read(ui);
    let ganged = *gang != mods.gang_invert;
    let mut out =
        RgbResponse { changed: false, axis: Axis::Rgb, dragging: false, ctrl_drag: false };

    // Never collapse quietly: below this the row overflows its container,
    // which is visible, instead of degrading into a stub of a slider that
    // still looks like one.
    let w = ui.available_width().max(CHROME_W + MIN_TRACK);
    let (rect, _) = ui.allocate_exact_size(vec2(w, TAG_H + LANE_H * 3.0), Sense::hover());
    // The track is inset by a knob radius at both ends, so a knob parked at
    // lo or hi (and the gang box that hugs it) never sits on the lane letter
    // or on the number.
    let track_x0 = rect.min.x + GANG_W + GAP + LETTER_W + GAP + KNOB_R;
    let track_x1 = rect.max.x - SWATCH_W - GAP - NUM_W - GAP - KNOB_R;
    let track_w = (track_x1 - track_x0).max(8.0);
    let lanes_top = rect.min.y + TAG_H;
    let lane_y = |lane: usize| lanes_top + (lane as f32 + 0.5) * LANE_H;
    let shown = driven.unwrap_or(*v);
    let pos_of = |val: f32| {
        track_x0 + ((val - spec.lo) / (spec.hi - spec.lo)).clamp(0.0, 1.0) * track_w
    };
    let live = driven.is_none();

    // Gang toggle, mirroring the Grade HUD's G box: filled when ganged.
    let grect = Rect::from_center_size(
        egui::pos2(rect.min.x + GANG_W * 0.5, lane_y(1)),
        vec2(GANG_W, 15.0),
    );
    let gr = ui.interact(grect, id.with("gang"), Sense::click());
    {
        let p = ui.painter();
        let c = ui.visuals().widgets.inactive.fg_stroke.color;
        let c = if gr.hovered() { ui.visuals().widgets.hovered.fg_stroke.color } else { c };
        if ganged {
            p.rect_filled(grect, 3.0, c);
        } else {
            p.rect_stroke(grect, 3.0, Stroke::new(1.2, c), egui::StrokeKind::Outside);
        }
        p.text(
            grect.center(),
            egui::Align2::CENTER_CENTER,
            "G",
            egui::FontId::proportional(9.0),
            if ganged { ui.visuals().extreme_bg_color } else { c },
        );
    }
    if gr.clicked() {
        *gang = !*gang;
    }
    gr.on_hover_text(
        "gang r/g/b (relative)\nhold G to invert while dragging\n\
         S saturation · D orange↔cyan · F green↔magenta",
    );

    // Colour-op preview: with S/D/F armed the lane block becomes a picture
    // of the axis — what the triple does to mid grey across a full drag,
    // now in the middle. Only while the hand is here; an always-on band
    // would make four of these in a panel look like a paint program.
    // What a grab would move right now. The lane only matters when the gang
    // is off and no colour axis is armed, so the pointer's own row is the
    // honest default — falling back to lane 0 (or to `rgb`) made the tag
    // read "rgb" while S was held and the band was already showing sat.
    let hover_lane = ui
        .ctx()
        .pointer_latest_pos()
        .map(|p| (((p.y - lanes_top) / LANE_H) as i32).clamp(0, 2) as usize)
        .unwrap_or(0);
    let armed = mods.axis(hover_lane, *gang);
    out.axis = armed;
    // "The hand is here" = hovering, or dragging THIS widget (a drag can
    // wander off the rect, and another slider's drag is not ours).
    let mine_dragged = ui.ctx().is_being_dragged(id.with("gangbox"))
        || (0..3).any(|l| ui.ctx().is_being_dragged(id.with(("lane", l))));
    let show_axis =
        armed.is_colour() && live && (ui.rect_contains_pointer(rect) || mine_dragged);

    // Lanes.
    let mut knob_x = [track_x0; 3];
    for lane in 0..3 {
        let y = lane_y(lane);
        let tint = LANE_TINT[lane];
        let axis = mods.axis(lane, *gang);
        let p = ui.painter();
        p.text(
            egui::pos2(rect.min.x + GANG_W + GAP + LETTER_W * 0.5, y),
            egui::Align2::CENTER_CENTER,
            ["r", "g", "b"][lane],
            egui::FontId::proportional(9.0),
            tint,
        );
        if show_axis {
            paint_axis_line(
                p,
                egui::pos2(track_x0, y),
                egui::pos2(track_x1, y),
                4.0,
                *v,
                armed,
                track_w,
                spec,
            );
        } else {
            p.line_segment(
                [egui::pos2(track_x0, y), egui::pos2(track_x1, y)],
                Stroke::new(1.2, tint.gamma_multiply(0.55)),
            );
        }
        let hx = pos_of(shown[lane]);
        knob_x[lane] = hx;

        // The whole lane is grabbable, not just the knob — the gang box
        // (drawn last) still wins inside its own rect.
        let lr = ui.interact(
            Rect::from_min_max(
                egui::pos2(track_x0 - KNOB_R, y - LANE_H * 0.5),
                egui::pos2(track_x1 + KNOB_R, y + LANE_H * 0.5),
            ),
            id.with(("lane", lane)),
            if live { Sense::drag() } else { Sense::hover() },
        );
        ui.painter().circle_filled(
            egui::pos2(hx, y),
            KNOB_R,
            if lr.hovered() { Color32::WHITE } else { tint },
        );
        if lr.dragged() {
            out.dragging = true;
            out.axis = axis;
            drag(ui, v, &lr, axis, track_w, spec, suppress, &mut out);
        }
        if lr.hovered() && !out.dragging {
            out.axis = axis;
        }


        // Numeric entry, one per lane, always the standard field size.
        let nrect = Rect::from_min_size(
            egui::pos2(track_x1 + GAP, y - NUM_H * 0.5),
            vec2(NUM_W, NUM_H),
        );
        if let Some(dv) = driven {
            let mut val = dv[lane];
            ui.put(nrect, egui::DragValue::new(&mut val).max_decimals(3))
                .on_hover_text("driven by patch cable (live value)");
        } else {
            // The numbers honour the GANG, exactly like the rails above:
            // ganged, dragging (or typing into) one lane's field moves all
            // three RELATIVELY — the triple keeps its shape — and holding
            // G peels off the lane under the hand. Bound to a SCRATCH and
            // applied through `nudge` so the gang is the same arithmetic
            // the rail gang uses; binding `v[lane]` direct is what made
            // the fields the one part of the instrument that could not
            // move together.
            //
            // The colour axes deliberately stay on the rails: S/D/F are
            // aimed gestures whose units are not this lane's number, and
            // a field that answered them would print a figure its own
            // drag did not produce.
            let mut val = v[lane];
            let was = val;
            let r = ui.put(
                nrect,
                egui::DragValue::new(&mut val)
                    .speed(spec.speed as f64 * rate(ui).unwrap_or(1.0) as f64)
                    .max_decimals(3),
            );
            if val != was {
                let axis = if ganged { Axis::Rgb } else { Axis::Lane(lane) };
                *v = nudge(*v, axis, val - was, spec);
            }
            if r.dragged() {
                out.dragging = true;
                out.axis = if ganged { Axis::Rgb } else { Axis::Lane(lane) };
            }
        }
    }

    // Ganged: a box hugging the knobs wherever they sit. Grabbing it moves
    // all three, and it is registered last so it takes the press.
    if ganged && live {
        let (kmin, kmax) =
            knob_x.iter().fold((f32::MAX, f32::MIN), |(a, b), &x| (a.min(x), b.max(x)));
        let brect = Rect::from_min_max(
            egui::pos2(kmin - KNOB_R - 3.0, lanes_top + 1.0),
            egui::pos2(kmax + KNOB_R + 3.0, rect.max.y - 1.0),
        );
        let br = ui.interact(brect, id.with("gangbox"), Sense::drag());
        let c = if br.hovered() || out.dragging {
            ui.visuals().widgets.hovered.fg_stroke.color
        } else {
            ui.visuals().widgets.inactive.fg_stroke.color.gamma_multiply(0.8)
        };
        ui.painter().rect_stroke(
            brect,
            3.0,
            Stroke::new(1.2, c),
            egui::StrokeKind::Outside,
        );
        if br.dragged() {
            // Which lane the hand is on still matters: hold G in the middle
            // of a ganged drag and it should peel off the lane you grabbed,
            // not always red.
            let lane = br
                .interact_pointer_pos()
                .map(|p| (((p.y - lanes_top) / LANE_H) as i32).clamp(0, 2) as usize)
                .unwrap_or(0);
            let axis = mods.axis(lane, *gang);
            out.dragging = true;
            out.axis = axis;
            drag(ui, v, &br, axis, track_w, spec, suppress, &mut out);
        }
    }

    // The tag strip: what a grab would move, shown whenever the widget is
    // under the hand or being dragged.
    if out.dragging || ui.rect_contains_pointer(rect) {
        ui.painter().text(
            egui::pos2(track_x0 - KNOB_R, rect.min.y + TAG_H * 0.5),
            egui::Align2::LEFT_CENTER,
            out.axis.tag(),
            egui::FontId::proportional(9.0),
            if out.axis.is_colour() {
                ui.visuals().widgets.hovered.fg_stroke.color
            } else {
                ui.visuals().weak_text_color()
            },
        );
    }

    // The chip: what the triple IS, right of the numbers, spanning the
    // lanes. Ganged drags move its brightness, the colour axes move its hue
    // — so every gesture on the widget has somewhere to show up.
    {
        let sw = Rect::from_min_max(
            egui::pos2(rect.max.x - SWATCH_W, lanes_top + 2.0),
            egui::pos2(rect.max.x, rect.max.y - 2.0),
        );
        let p = ui.painter();
        p.rect_filled(sw, 3.0, value_swatch(shown, spec));
        p.rect_stroke(
            sw,
            3.0,
            Stroke::new(1.0, ui.visuals().widgets.inactive.bg_stroke.color),
            egui::StrokeKind::Inside,
        );
        ui.interact(sw, id.with("swatch"), Sense::hover()).on_hover_text(format!(
            "{:.3}  {:.3}  {:.3}\nbrightness is toned to fit the chip",
            shown[0], shown[1], shown[2]
        ));
    }

    ui.ctx().data_mut(|d| {
        d.insert_temp(
            id.with("geom"),
            Geom {
                track_x0,
                track_x1,
                lane_y: [lane_y(0), lane_y(1), lane_y(2)],
                knob_x,
            },
        )
    });
    set_grabbing(ui.ctx(), out.dragging);
    if suppress {
        *v = before;
    }
    out.changed = *v != before;
    out
}

/// One frame of a drag on either handle.
#[allow(clippy::too_many_arguments)]
fn drag(
    ui: &Ui,
    v: &mut [f32; 3],
    r: &egui::Response,
    axis: Axis,
    track_w: f32,
    spec: RgbSpec,
    suppress: bool,
    out: &mut RgbResponse,
) {
    if ui.input(|i| i.modifiers.ctrl || i.modifiers.command) {
        // Ctrl-drag is the HUD drag-out gesture, not an edit.
        out.ctrl_drag = r.drag_started();
        return;
    }
    if suppress || r.drag_started() {
        return;
    }
    let d = drag_units_rated(ui, axis, r.drag_delta().x, track_w, spec);
    if d != 0.0 {
        *v = nudge(*v, axis, d, spec);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const W709: [f32; 3] = [0.2126, 0.7152, 0.0722];
    const AP1: [f32; 3] = [0.2722, 0.6741, 0.0537];

    fn scale() -> RgbSpec {
        RgbSpec::scale(0.0, 16.0, AP1)
    }
    fn offset() -> RgbSpec {
        RgbSpec::offset(-1.0, 1.0, W709)
    }

    /// The z/x/c/v table is THE table (rows and lanes both read it): a
    /// held key maps to its factor, releasing it goes back to None.
    #[test]
    fn rate_follows_the_held_key() {
        let key = |key, pressed| egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let ctx = egui::Context::default();
        let mut down = egui::RawInput::default();
        down.events.push(key(Key::Z, true));
        let mut up = egui::RawInput::default();
        up.events.push(key(Key::Z, false));
        let (mut held, mut released) = (None, None);
        crate::snapshot::discard(ctx.run_ui(down, |ui| held = rate(ui)));
        crate::snapshot::discard(ctx.run_ui(up, |ui| released = rate(ui)));
        assert_eq!(held, Some(0.01));
        assert_eq!(released, None);
        // Three keys each way, mirrored in log space around 1: the inner
        // pair cancels exactly, and so does the middle pair.
        assert_eq!(RATE_KEYS.len(), 6);
        let f = |k: Key| RATE_KEYS.iter().find(|(rk, _)| *rk == k).unwrap().1;
        assert!((f(Key::C) * f(Key::V) - 1.0).abs() < 1e-6);
        assert!((f(Key::X) * f(Key::B) - 1.0).abs() < 1e-6);
        assert!(f(Key::Z) < f(Key::X) && f(Key::X) < f(Key::C));
        assert!(f(Key::V) < f(Key::B) && f(Key::B) < f(Key::N));
        // cmd+Z is undo: a chord must not arm a rate, or an undo fired
        // mid-drag would silently drop the throw to a hundredth.
        let mut chord = egui::RawInput::default();
        chord.events.push(egui::Event::ModifiersChanged(egui::Modifiers::COMMAND));
        chord.events.push(egui::Event::Key {
            key: Key::Z,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        });
        let mut under_cmd = Some(1.0);
        crate::snapshot::discard(ctx.run_ui(chord, |ui| under_cmd = rate(ui)));
        assert_eq!(under_cmd, None);
    }

    /// The lane NUMBERS move together when the widget is ganged — one
    /// field dragged carries the other two and the triple keeps its shape.
    /// Drives the real `show`, because the arithmetic was never the broken
    /// half (`gang_is_relative` covers that): the fields were bound
    /// straight to `v[lane]`, so they were the one part of the instrument
    /// that could not be ganged, keyed to G like everything else here.
    #[test]
    fn ganged_number_fields_carry_all_three() {
        for ganged in [true, false] {
            let ctx = egui::Context::default();
            let spec = scale();
            let start = [1.0f32, 1.4, 0.6];
            let mut v = start;
            let mut gang = ganged;
            let frame = |input: egui::RawInput, v: &mut [f32; 3], gang: &mut bool| {
                crate::snapshot::discard(ctx.run_ui(input, |ui| {
                    show(ui, "t", v, gang, spec, None, false);
                }));
            };
            // One pass to place the widget, then aim at lane 0's field the
            // way --test-rgb does: off the geometry it reports, never a
            // hard-coded point.
            frame(egui::RawInput::default(), &mut v, &mut gang);
            let g = last_geom(&ctx, "t").expect("the widget reports its geometry");
            let at = egui::pos2(g.track_x1 + GAP + NUM_W * 0.5, g.lane_y[0]);
            let mut press = egui::RawInput::default();
            press.events.push(egui::Event::PointerMoved(at));
            press.events.push(egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            });
            frame(press, &mut v, &mut gang);
            for i in 1..=6 {
                let mut mv = egui::RawInput::default();
                mv.events.push(egui::Event::PointerMoved(at + egui::vec2(4.0 * i as f32, 0.0)));
                frame(mv, &mut v, &mut gang);
            }

            let d = [v[0] - start[0], v[1] - start[1], v[2] - start[2]];
            assert!(d[0] > 0.0, "ganged={ganged}: the field did not drag at all ({v:?})");
            if ganged {
                assert!(
                    (d[1] - d[0]).abs() < 1e-5 && (d[2] - d[0]).abs() < 1e-5,
                    "ganged: every lane moves by the same amount, got {d:?}"
                );
            } else {
                assert_eq!([d[1], d[2]], [0.0, 0.0], "unganged: lane 0 moves alone");
            }
        }
    }

    #[test]
    fn gang_is_relative() {
        // The whole point of the gang: the triple keeps its shape.
        let v = [1.0, 1.4, 0.6];
        let o = nudge(v, Axis::Rgb, 0.5, scale());
        assert_eq!(o, [1.5, 1.9, 1.1]);
        assert!((o[1] - o[0] - (v[1] - v[0])).abs() < 1e-6);
    }

    #[test]
    fn lane_moves_alone() {
        let o = nudge([1.0, 1.0, 1.0], Axis::Lane(2), -0.25, scale());
        assert_eq!(o, [1.0, 1.0, 0.75]);
    }

    #[test]
    fn colour_axes_preserve_luma_exactly() {
        for (spec, v) in [
            (scale(), [1.0, 1.0, 1.0]),
            (scale(), [2.0, 0.8, 1.3]),
            (offset(), [0.0, 0.0, 0.0]),
            (offset(), [0.2, -0.1, 0.05]),
        ] {
            let l0 = wmean(spec.luma, v);
            for axis in [Axis::Warm, Axis::Tint] {
                for d in [0.35_f32, -0.2] {
                    let o = nudge(v, axis, d, spec);
                    let l1 = wmean(spec.luma, o);
                    assert!(
                        (l1 - l0).abs() < 1e-5,
                        "{axis:?} d={d} moved luma {l0} -> {l1} ({v:?} -> {o:?})"
                    );
                    assert!(o != v, "{axis:?} d={d} did nothing to {v:?}");
                }
            }
        }
    }

    #[test]
    fn warm_goes_orange_right_cyan_left() {
        let v = [1.0, 1.0, 1.0];
        let warm = nudge(v, Axis::Warm, 0.3, scale());
        assert!(warm[0] > v[0] && warm[2] < v[2], "right must warm: {warm:?}");
        let cool = nudge(v, Axis::Warm, -0.3, scale());
        assert!(cool[0] < v[0] && cool[2] > v[2], "left must cool: {cool:?}");
    }

    #[test]
    fn tint_goes_magenta_right_green_left() {
        let v = [1.0, 1.0, 1.0];
        let mag = nudge(v, Axis::Tint, 0.3, scale());
        assert!(mag[1] < v[1] && mag[0] > v[0], "right must go magenta: {mag:?}");
        let grn = nudge(v, Axis::Tint, -0.3, scale());
        assert!(grn[1] > v[1] && grn[0] < v[0], "left must go green: {grn:?}");
    }

    #[test]
    fn saturation_needs_chroma_to_work_on() {
        // A neutral triple has no chroma: S is honestly a no-op there.
        let n = [2.0, 2.0, 2.0];
        assert_eq!(nudge(n, Axis::Sat, 0.5, scale()), n);
        // With a cast, it pushes the channels apart and keeps the luma.
        let v = nudge(n, Axis::Warm, 0.4, scale());
        let spread = |x: [f32; 3]| x[0] - x[2];
        let up = nudge(v, Axis::Sat, 0.6, scale());
        let down = nudge(v, Axis::Sat, -0.6, scale());
        assert!(spread(up) > spread(v), "S right must saturate: {up:?}");
        assert!(spread(down) < spread(v), "S left must desaturate: {down:?}");
        let l0 = wmean(AP1, v);
        assert!((wmean(AP1, up) - l0).abs() < 1e-5);
        assert!((wmean(AP1, down) - l0).abs() < 1e-5);
    }

    #[test]
    fn scale_mode_never_goes_through_zero() {
        // A multiplier pushed hard along a colour axis stays a multiplier.
        let mut v = [1.0, 1.0, 1.0];
        for _ in 0..40 {
            v = nudge(v, Axis::Warm, 0.5, scale());
        }
        assert!(v.iter().all(|c| *c >= 0.0), "{v:?}");
        assert!(v[2] > 0.0, "log space must keep blue alive: {v:?}");
    }

    #[test]
    fn offset_mode_may_cross_zero() {
        // ...whereas an offset-type triple is allowed to go negative: that
        // is what an offset is for.
        let v = nudge([0.0, 0.0, 0.0], Axis::Warm, 0.4, offset());
        assert!(v[2] < 0.0, "{v:?}");
    }

    #[test]
    fn range_is_clamped() {
        let hot = nudge([15.9, 15.9, 15.9], Axis::Rgb, 4.0, scale());
        assert_eq!(hot, [16.0, 16.0, 16.0]);
        let cold = nudge([0.1, 0.1, 0.1], Axis::Rgb, -4.0, scale());
        assert_eq!(cold, [0.0, 0.0, 0.0]);
    }

    #[test]
    fn luma_weights_matter() {
        // The correction is done in the working space's frame, so the same
        // gesture lands differently in ACEScg than in Rec.709. If this ever
        // stops being true, the weights have been hardcoded somewhere.
        let v = [1.0, 1.0, 1.0];
        let a = nudge(v, Axis::Warm, 0.4, RgbSpec::scale(0.0, 16.0, AP1));
        let b = nudge(v, Axis::Warm, 0.4, RgbSpec::scale(0.0, 16.0, W709));
        assert!((a[1] - b[1]).abs() > 1e-3, "{a:?} vs {b:?}");
    }

    #[test]
    fn modifier_precedence_and_gang_invert() {
        let none = Mods::default();
        assert_eq!(none.axis(1, true), Axis::Rgb);
        assert_eq!(none.axis(1, false), Axis::Lane(1));
        // G inverts, both ways.
        let g = Mods { gang_invert: true, ..none };
        assert_eq!(g.axis(1, true), Axis::Lane(1));
        assert_eq!(g.axis(1, false), Axis::Rgb);
        // S beats D beats F, and a colour axis beats the gang state.
        let all = Mods { gang_invert: true, sat: true, warm: true, tint: true };
        assert_eq!(all.axis(0, true), Axis::Sat);
        assert_eq!(Mods { sat: false, ..all }.axis(0, true), Axis::Warm);
        assert_eq!(Mods { sat: false, warm: false, ..all }.axis(0, true), Axis::Tint);
    }

    #[test]
    fn intensity_tracks_the_pointer() {
        // A full-track drag must cover the full range, or the knob leaves
        // the finger behind.
        let spec = scale();
        let d = 100.0 * per_px(Axis::Rgb, 100.0, spec);
        assert!((d - (spec.hi - spec.lo)).abs() < 1e-4);
    }
}
