//! General-purpose colour picker: a swatch that opens a popup with four
//! ways to say the same colour — wheel, rgb, hsl, lch (Oklch).
//!
//! This is the "pick a colour" counterpart to `rgbslider`, for the values
//! that widget deliberately does not serve (see CLAUDE.md): a Grid's line
//! and background, a paint stroke, a 2dini style's fill — colours with an
//! alpha, picked absolutely rather than pushed relatively. It replaces
//! egui's stock `color_edit_button_*` everywhere in the app.
//!
//! ## Ergonomics
//!
//! - **Every lane is a picture of its own axis.** A track is a live
//!   gradient of what the colour BECOMES at each position, the other
//!   coordinates held, so you aim at a colour rather than at a number. The
//!   one licence taken: a HUE lane never goes flat. At zero chroma an honest
//!   hue lane is a grey bar, and a hue you cannot see is a hue you cannot
//!   aim at, so it is painted at a small floor chroma instead.
//! - **Modes are views over one value.** One rgba value is authored; the
//!   modes are coordinate systems over it. The last mode used anywhere is
//!   the mode every picker opens in — someone who thinks in lch thinks in
//!   it at every swatch.
//! - **Coordinates survive their degenerate points.** Drag saturation to
//!   zero and back and the hue is still there; the same for chroma, and for
//!   everything at black. The view being edited keeps its exact
//!   coordinates; the other views follow the value, falling back to their
//!   previous coordinates wherever the value cannot say.
//! - **lch is honest about the gamut.** L, C and H are kept as authored,
//!   and the colour written is that one with its chroma pulled back to the
//!   gamut edge. Every lane UNDERLINES the stretch where that pull-back is
//!   happening, so a knob sitting over an underline is a colour the working
//!   space cannot make — and moving L back restores the chroma you asked for.
//! - **Drags are the house drags.** A plain press puts the knob under the
//!   pointer; after that the pointer's travel is added at the host's live
//!   drag rate (wobble: z/x/c/v, `widgets::rated`), so a rate key steps fine
//!   or coarse mid-gesture and releasing it never snaps the value back.
//! - **⌘C / ⌘V over the swatch or the popup** copy the stored numbers
//!   (`[r, g, b, a]`, lossless) and paste either that or a hex colour.
//! - Hex entry, old/new chips (click old to restore), numbers on every lane.
//! - **A palette the whole app shares.** The + slot keeps the current
//!   colour; click a swatch to use it (colour only — the value keeps its
//!   alpha), shift-click to replace it with the current colour, alt-click
//!   to drop it. Swatches are kept in linear Rec.709, so one means the same
//!   colour in every comp whatever its working space, and the host persists
//!   them ([`take_changed_swatches`]).
//!
//! ## Size
//!
//! The popup opens at three times the picker's original geometry and
//! resizes on BOTH axes independently: drag its right edge for width, its
//! free edge (the one away from the swatch) for height, or the corner
//! between them for both; pinch / ⌘-scroll scales it evenly, and a grip's
//! double-click restores the default. x runs everything measured ACROSS —
//! the lanes' length, the chips, the wheel's row — and y everything
//! STACKED: track thickness, lane height, the gaps. So a wide picker has
//! long sliders and a tall one fat sliders, and the wheel, whose geometry
//! is an angle and a radius, stays ROUND on the smaller axis. Like
//! properties zoom, what you AIM at grows while type and number fields
//! stay put, because they are read. There is one size for the whole app
//! ([`scale`], a `Vec2`); the host persists it ([`take_settled_scale`]).
//!
//! It is fitted to the room on the side of the swatch the popup hangs on,
//! so a big picker near a screen edge comes up smaller rather than off
//! screen, and a resize stops where the room ends — until it is MOVED, and
//! then the room is the screen. Dragging the frame margin moves it
//! ([`move_zones`]); the offset lives as long as the popup is open, so the
//! next open comes back on its swatch.
//!
//! ## What the numbers mean
//!
//! The value is the STORED rgba — scene-linear, in the comp's working
//! primaries, 0..1 (these params feed shaders directly, and that is the
//! range the stock picker always held them to).
//!
//! - **rgb** lanes are those stored numbers, 1:1.
//! - **wheel** (hsv) and **hsl** are the stored numbers under the picker's
//!   display encode, so a lightness of 0.5 looks mid rather than near white.
//! - **lch** and **hex** mean something OUTSIDE the comp, so they go through
//!   `PickSpec::display` (working -> Rec.709): Oklab is defined over linear
//!   Rec.709, and a hex code is sRGB.
//!
//! The display encode is exact piecewise sRGB and there is only one: the
//! previews, hsv/hsl and hex all use it, so the swatch on screen and the
//! hex are the same bytes, a pasted `#808080` lands on the linear value it
//! means everywhere else, and hsl agrees with any other app about the same
//! hex. Deliberately NOT `rgbslider`'s 2.2 (off by nearly 2x in linear near
//! black) and NOT the viewer's display transform (which lives at the blit).
//!
//! Deliberately egui-only (no `engine`, no `crate::`) so it can be lifted
//! out whole, on the `rgbslider` precedent: the host's colour matrices and
//! input conventions come in through [`PickSpec`].
//!
//! Owned by instance P (see .agents/board.md).

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering::Relaxed};

use egui::{
    epaint, pos2, vec2, Color32, CursorIcon, Event, Id, Pos2, Rect, Response, Sense, Stroke,
    StrokeKind, TextStyle, Ui, Vec2,
};

// ---------------------------------------------------------------------------
// Size
// ---------------------------------------------------------------------------

/// The picker opens at three times its original geometry.
pub const SCALE_DEFAULT: f32 = 3.0;
pub const SCALE_MIN: f32 = 1.0;
pub const SCALE_MAX: f32 = 6.0;

// Aimed geometry at scale 1 — the picker's original size.
/// A lane's aimable width, the knob's overhang at each end included.
const LANE_W: f32 = 164.0;
const TRACK_H: f32 = 12.0;
/// A lane's height beyond its track: the knob's overhang and the gamut
/// underline. An offset, not a size — it stays put.
const LANE_PAD: f32 = 10.0;
const DISC_D: f32 = 144.0;
/// Old/new chip size in the footer.
const CHIP: Vec2 = vec2(40.0, 18.0);
const GAP: Vec2 = vec2(4.0, 3.0);
/// Swatches per palette row, at every scale: the slots grow, the count
/// does not.
pub const SWATCH_ROW: usize = 12;
/// The palette's size: four rows.
pub const SWATCH_MAX: usize = 48;

// Read, not aimed: the same at every scale.
const NUM_W: f32 = 46.0;
const NUM_H: f32 = 18.0;
const LAB_W: f32 = 14.0;
/// The h/s/v labels beside the wheel's number column.
const WHEEL_NUM_LAB: f32 = 10.0;
const HEX_W: f32 = 76.0;
const HEX_H: f32 = 18.0;
/// Swatch button size — egui's stock button's footprint, so swapping the
/// widget into an existing row does not reflow it.
const SWATCH: Vec2 = vec2(30.0, 18.0);
/// Between the swatch and its popup, and between the popup and the screen.
const POP_GAP: f32 = 4.0;
/// How far a resize grip reaches OUTSIDE the popup's edge (inside, it
/// reaches across the frame margin and no further).
const GRIP_OUT: f32 = 5.0;
/// A resize persists this long after the last frame of the gesture — one
/// prefs write per resize, not sixty a second.
const SETTLE_MS: u64 = 250;

/// Gradient samples per lane: no visible banding across a lane at the
/// largest scale, and few enough that one gamut bisection per sample is
/// still nothing.
const GRAD_N: usize = 96;

/// Hue-lane picture floors (see the module docs): the chroma a hue lane is
/// painted with when the real colour has less. hsl in encoded chroma, lch
/// in Oklab chroma (Rec.709's most saturated colour is ~0.32).
const HSL_HUE_FLOOR: f32 = 0.3;
const LCH_HUE_FLOOR: f32 = 0.06;
/// Below this Oklab chroma a colour has no hue worth keeping — float noise
/// through a working-space matrix lives well under it.
const CHROMA_EPS: f32 = 2e-4;

/// One picker's geometry at a scale. [`Geo::size`] is the layout's own
/// arithmetic: the screen fit, the popup's placement and the resize grips
/// all need the size BEFORE anything is laid out, and `size_is_the_layout`
/// holds the two together.
#[derive(Clone, Copy, Debug)]
struct Geo {
    gap: Vec2,
    lane_w: f32,
    track_h: f32,
    /// The value strip's THICKNESS in wheel mode. A track_h turned on its
    /// side, so it answers to the width like everything else measured
    /// across — the height scale must not widen the wheel row.
    strip_w: f32,
    lane_h: f32,
    knob_r: f32,
    disc_d: f32,
    chip: Vec2,
    /// A palette slot's side and the gap between slots: a row fills the
    /// width.
    slot: f32,
    slot_gap: f32,
    /// Rows the palette takes: its swatches plus the + slot.
    swatch_rows: usize,
    /// The popup frame's margin: egui's own 6 at scale 1, growing like a
    /// gap after that. Whole points, because egui's `Margin` is.
    margin: i8,
    /// Content width, the same for every mode.
    width: f32,
}

impl Geo {
    /// `swatches`: how many the palette holds, which decides its rows.
    fn new(scale: Vec2, swatches: usize) -> Self {
        let s = vec2(scale.x.max(0.1), scale.y.max(0.1));
        let gap = vec2(GAP.x * s.x, GAP.y * s.y);
        let lane_w = LANE_W * s.x;
        let track_h = TRACK_H * s.y;
        let width = LAB_W + gap.x + lane_w + gap.x + NUM_W;
        let slot_gap = gap.x * 0.5;
        Self {
            gap,
            lane_w,
            track_h,
            strip_w: TRACK_H * s.x,
            lane_h: (track_h + LANE_PAD).max(NUM_H),
            // A knob is round, so a very fat track on a very short lane
            // would eat the track it slides along. A quarter of the lane
            // is as much as it may take.
            knob_r: (track_h * 0.5 + 1.0).min(lane_w * 0.25),
            // A wheel stays ROUND: hue is an angle and saturation a
            // radius, so an ellipse would be a lie about the geometry the
            // hand is aiming at. It takes the smaller axis.
            disc_d: DISC_D * s.x.min(s.y),
            chip: vec2(CHIP.x * s.x, CHIP.y * s.y),
            slot: (width - (SWATCH_ROW - 1) as f32 * slot_gap) / SWATCH_ROW as f32,
            slot_gap,
            swatch_rows: slot_count(swatches).div_ceil(SWATCH_ROW),
            // Off the WIDTH alone, which is what keeps the two axes
            // separable: the outer width is then a function of `s.x` only,
            // so `fit_scale` can solve x and then y instead of chasing a
            // margin that moves with both.
            margin: (6.0 + GAP.x * (s.x - 1.0)).round().clamp(6.0, 64.0) as i8,
            width,
        }
    }

    fn footer_h(&self) -> f32 {
        self.chip.y.max(HEX_H)
    }

    fn grid_h(&self) -> f32 {
        let rows = self.swatch_rows as f32;
        rows * self.slot + (rows - 1.0).max(0.0) * self.slot_gap
    }

    /// Content size in `mode`: rows at their heights, a gap between
    /// neighbours and one more above the palette.
    fn size(&self, mode: Mode, alpha: bool) -> Vec2 {
        let (body, body_rows) = match mode {
            Mode::Wheel => (self.disc_d, 1),
            _ => (3.0 * self.lane_h, 3),
        };
        let (a, a_rows) = if alpha { (self.lane_h, 1) } else { (0.0, 0) };
        let rows = 1 + body_rows + a_rows + 2;
        let h = NUM_H + body + a + self.grid_h() + self.footer_h() + self.gap.y * rows as f32;
        vec2(self.width, h)
    }

    /// The tallest mode: what a popup must have room for, so switching
    /// modes never refits it.
    fn size_max(&self, alpha: bool) -> Vec2 {
        Mode::ALL.iter().fold(Vec2::ZERO, |m, &mode| m.max(self.size(mode, alpha)))
    }

    /// What `Frame::popup` with this margin adds around the content.
    fn frame_extra(&self, style: &egui::Style) -> Vec2 {
        Vec2::splat(2.0 * (self.margin as f32 + style.visuals.window_stroke().width))
    }

    fn outer(&self, mode: Mode, alpha: bool, style: &egui::Style) -> Vec2 {
        self.size(mode, alpha) + self.frame_extra(style)
    }

    fn outer_max(&self, alpha: bool, style: &egui::Style) -> Vec2 {
        self.size_max(alpha) + self.frame_extra(style)
    }
}

/// A lane's track length at `scale` — what a drawn lane measures.
pub fn track_len(scale: Vec2) -> f32 {
    let g = Geo::new(scale, 0);
    g.lane_w - 2.0 * g.knob_r
}

/// Palette slots on show: every swatch, plus the + slot while there is room.
fn slot_count(swatches: usize) -> usize {
    (swatches + usize::from(swatches < SWATCH_MAX)).min(SWATCH_MAX)
}

/// The largest scale PER AXIS whose tallest mode, frame and all, fits
/// `room`. Never under `SCALE_MIN`: on a screen too small even for that,
/// the popup overflows rather than disappear.
///
/// x is solved first and y against the x that won, because the two are not
/// quite independent — the palette's slots are squares sized by the WIDTH,
/// so they carry a little of x into the height. Width itself answers to x
/// alone (that is what pinning the margin to `s.x` buys), so the order is
/// the one that has an answer.
fn fit_scale(room: Vec2, alpha: bool, swatches: usize, style: &egui::Style) -> Vec2 {
    let solve = |fits: &dyn Fn(f32) -> bool| -> f32 {
        if fits(SCALE_MAX) {
            return SCALE_MAX;
        }
        if !fits(SCALE_MIN) {
            return SCALE_MIN;
        }
        let (mut lo, mut hi) = (SCALE_MIN, SCALE_MAX);
        for _ in 0..24 {
            let mid = 0.5 * (lo + hi);
            if fits(mid) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        lo
    };
    // x is judged against BOTH rooms at the shortest possible height: the
    // palette's slots are squares sized by the width, so a width that only
    // fits sideways can still overflow downwards, and then no height would
    // save it.
    let x = solve(&|s| {
        let o = Geo::new(vec2(s, SCALE_MIN), swatches).outer_max(alpha, style);
        o.x <= room.x && o.y <= room.y
    });
    let y = solve(&|s| Geo::new(vec2(x, s), swatches).outer_max(alpha, style).y <= room.y);
    vec2(x, y)
}

/// Component-wise, against a per-axis ceiling.
fn clamp_scale(s: Vec2, fit: Vec2) -> Vec2 {
    vec2(s.x.clamp(SCALE_MIN, fit.x), s.y.clamp(SCALE_MIN, fit.y))
}

/// The scale a grip drag has reached. `axis` is which way the grip grows
/// the popup: +x for the right edge, ±y for the free horizontal edge (up
/// when the popup hangs above its swatch), both for the corner — and each
/// axis moves its OWN scale, which is what makes the popup resizable in
/// width and height independently. The outer size is linear in scale (to
/// the point its whole-point margin rounds), so a unit of scale moves an
/// edge by a fixed amount and the travel divided by that rate keeps the
/// edge under the hand.
fn grip_scale(s0: Vec2, travel: Vec2, axis: Vec2, mode: Mode, alpha: bool, swatches: usize) -> Vec2 {
    let outer = |s: Vec2| {
        let g = Geo::new(s, swatches);
        g.size(mode, alpha) + Vec2::splat(2.0 * g.margin as f32)
    };
    let base = outer(s0);
    let mut out = s0;
    if axis.x != 0.0 {
        let rate = outer(s0 + vec2(1.0, 0.0)).x - base.x;
        if rate.abs() > 1e-6 {
            out.x = s0.x + travel.x * axis.x / rate;
        }
    }
    if axis.y != 0.0 {
        let rate = outer(s0 + vec2(0.0, 1.0)).y - base.y;
        if rate.abs() > 1e-6 {
            out.y = s0.y + travel.y * axis.y / rate;
        }
    }
    out
}

// ---------------------------------------------------------------------------
// App-wide memory: the mode and the size are ONE memory for the whole app,
// every egui context included (a detached pane has its own).
// ---------------------------------------------------------------------------

static MODE: AtomicU8 = AtomicU8::new(0);
/// f32 bits, one per axis; 0 means never set, which is the default.
static SCALE_X: AtomicU32 = AtomicU32::new(0);
static SCALE_Y: AtomicU32 = AtomicU32::new(0);
/// A user resize the host has not persisted yet.
static SCALE_DIRTY: AtomicBool = AtomicBool::new(false);
/// When a resize gesture was last live, in `now_ms` time.
static GESTURE_MS: AtomicU64 = AtomicU64::new(0);

fn now_ms() -> u64 {
    static EPOCH: std::sync::OnceLock<web_time::Instant> = std::sync::OnceLock::new();
    EPOCH.get_or_init(web_time::Instant::now).elapsed().as_millis() as u64
}

pub fn mode() -> Mode {
    Mode::ALL.get(MODE.load(Relaxed) as usize).copied().unwrap_or(Mode::Wheel)
}

pub fn set_mode(m: Mode) {
    MODE.store(Mode::ALL.iter().position(|x| *x == m).unwrap_or(0) as u8, Relaxed);
}

/// The size every picker opens at, per axis: x runs the lanes' length and
/// everything measured across, y their thickness and everything stacked.
pub fn scale() -> Vec2 {
    let one = |bits: u32| if bits == 0 { SCALE_DEFAULT } else { f32::from_bits(bits) };
    vec2(one(SCALE_X.load(Relaxed)), one(SCALE_Y.load(Relaxed)))
}

/// The size a picker opens at with nothing saved, and what a grip's
/// double-click goes back to.
pub fn scale_default() -> Vec2 {
    Vec2::splat(SCALE_DEFAULT)
}

/// Seed the size — the host's saved preference. Not an edit: there is
/// nothing to persist.
pub fn set_scale(s: Vec2) {
    SCALE_X.store(s.x.clamp(SCALE_MIN, SCALE_MAX).to_bits(), Relaxed);
    SCALE_Y.store(s.y.clamp(SCALE_MIN, SCALE_MAX).to_bits(), Relaxed);
}

/// A resize by the user: the host will be told once it settles.
fn resize_to(s: Vec2) {
    set_scale(s);
    SCALE_DIRTY.store(true, Relaxed);
    GESTURE_MS.store(now_ms(), Relaxed);
}

/// The size, once a user's resize has settled — and only once: the host's
/// cue to persist it.
pub fn take_settled_scale() -> Option<Vec2> {
    settled_at(now_ms())
}

fn settled_at(now: u64) -> Option<Vec2> {
    let quiet = now.saturating_sub(GESTURE_MS.load(Relaxed)) > SETTLE_MS;
    (quiet && SCALE_DIRTY.swap(false, Relaxed)).then(scale)
}

/// The app's swatches, in linear Rec.709.
static PALETTE: std::sync::Mutex<Vec<[f32; 3]>> = std::sync::Mutex::new(Vec::new());
/// A palette edit the host has not persisted yet.
static PALETTE_DIRTY: AtomicBool = AtomicBool::new(false);

fn palette_lock() -> std::sync::MutexGuard<'static, Vec<[f32; 3]>> {
    PALETTE.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The swatches every picker offers, in linear Rec.709.
pub fn swatches() -> Vec<[f32; 3]> {
    palette_lock().clone()
}

/// Seed the swatches — the host's saved preference. Not an edit. Anything
/// non-finite (a hand-edited prefs file) is dropped, and so is anything
/// past `SWATCH_MAX`.
pub fn set_swatches(mut v: Vec<[f32; 3]>) {
    v.retain(|c| c.iter().all(|x| x.is_finite()));
    v.truncate(SWATCH_MAX);
    *palette_lock() = v;
}

/// A palette edit by the user: the host will be told.
fn store_swatches(v: Vec<[f32; 3]>) {
    set_swatches(v);
    PALETTE_DIRTY.store(true, Relaxed);
}

/// The swatches, once after they change: the host's cue to persist them.
pub fn take_changed_swatches() -> Option<Vec<[f32; 3]>> {
    PALETTE_DIRTY.swap(false, Relaxed).then(swatches)
}

// ---------------------------------------------------------------------------
// Spec
// ---------------------------------------------------------------------------

const IDENTITY3: [[f32; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

fn no_rate(_: &Ui) -> Option<f32> {
    None
}
fn no_claim(_: &egui::Context) {}
fn no_eyedrop(_: &Ui, _: Id, _: bool) -> (bool, Option<[f32; 3]>) {
    (false, None)
}

/// Everything the picker needs that is not the value itself.
#[derive(Clone, Copy, Debug)]
pub struct PickSpec {
    /// Working primaries -> Rec.709 (`color::primaries_matrix`): painted
    /// previews, lch and hex. Identity in a Rec.709 comp.
    pub display: [[f32; 3]; 3],
    /// Rec.709 -> working, the inverse of `display`. Carried rather than
    /// inverted here so both come from the host's own derivation and cannot
    /// drift apart.
    pub display_inv: [[f32; 3]; 3],
    /// Draw the alpha lane and edit `v[3]`.
    pub alpha: bool,
    /// The host's held drag-rate factor (None = 1:1), read live every frame.
    /// A plain fn keeps the spec Copy and the widget egui-only.
    pub rate: fn(&Ui) -> Option<f32>,
    /// Called every frame the pointer is over the popup: the host's chance
    /// to stand down single-letter hotkeys its rate keys share (in wobble
    /// V hides the very panel the swatch lives in).
    pub claim: fn(&egui::Context),
    /// The host's eyedropper, called every frame with the swatch's id and
    /// whether the pointer is over the swatch or its popup. Returns (armed
    /// for this swatch, an rgb sample to take). While armed the popup stays
    /// up through clicks elsewhere — they are the sampling.
    pub eyedrop: fn(&Ui, Id, bool) -> (bool, Option<[f32; 3]>),
}

impl PickSpec {
    /// Numbers treated as Rec.709, no alpha lane, no host conventions.
    pub fn rgb() -> Self {
        Self {
            display: IDENTITY3,
            display_inv: IDENTITY3,
            alpha: false,
            rate: no_rate,
            claim: no_claim,
            eyedrop: no_eyedrop,
        }
    }
    /// [`PickSpec::rgb`] with the alpha lane.
    pub fn rgba() -> Self {
        Self { alpha: true, ..Self::rgb() }
    }
    /// Working primaries: `to709` = working -> Rec.709, `from709` its inverse.
    pub fn with_display(mut self, to709: [[f32; 3]; 3], from709: [[f32; 3]; 3]) -> Self {
        self.display = to709;
        self.display_inv = from709;
        self
    }
    /// The host's drag-rate factor and hotkey claim.
    pub fn with_input(mut self, rate: fn(&Ui) -> Option<f32>, claim: fn(&egui::Context)) -> Self {
        self.rate = rate;
        self.claim = claim;
        self
    }
    /// The host's eyedropper (see [`PickSpec::eyedrop`]).
    pub fn with_eyedrop(mut self, f: fn(&Ui, Id, bool) -> (bool, Option<[f32; 3]>)) -> Self {
        self.eyedrop = f;
        self
    }
}

// ---------------------------------------------------------------------------
// Modes
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Wheel,
    Rgb,
    Hsl,
    Lch,
}

impl Mode {
    pub const ALL: [Mode; 4] = [Mode::Wheel, Mode::Rgb, Mode::Hsl, Mode::Lch];
    fn label(self) -> &'static str {
        match self {
            Mode::Wheel => "wheel",
            Mode::Rgb => "rgb",
            Mode::Hsl => "hsl",
            Mode::Lch => "lch",
        }
    }
    fn hint(self) -> &'static str {
        match self {
            Mode::Wheel => "hue and saturation on the disc, value on the strip",
            Mode::Rgb => "the stored linear numbers",
            Mode::Hsl => "hue, saturation, lightness",
            Mode::Lch => {
                "Oklch: perceptual lightness, chroma and hue — equal steps look \
                 equal. Underlined: out of gamut there, pulled back to the edge"
            }
        }
    }
}

/// How one picker is shown. [`button`] keeps the app's mode and size here;
/// an embedder of [`picker_ui`] keeps its own.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub mode: Mode,
    /// Multiplier over the picker's original geometry, per axis.
    pub scale: Vec2,
    /// Tabs on the bottom edge. The popup sets it when it hangs ABOVE its
    /// swatch: the edge facing the swatch is the pinned one, so the tabs
    /// stay put while modes of different heights swap in, and they sit
    /// right by the pointer that just opened the picker.
    pub tabs_last: bool,
}

// ---------------------------------------------------------------------------
// Conversions. All pure, all unit-tested. Angles in degrees, 0..360.
// ---------------------------------------------------------------------------

/// The picker's ONE display encode, exact piecewise sRGB: every preview,
/// hsv/hsl and hex go through it, so the swatch you see and the hex you
/// read are the same bytes, and a hex typed in comes back out unchanged.
pub fn enc1(x: f32) -> f32 {
    let x = x.max(0.0);
    if x <= 0.003_130_8 {
        12.92 * x
    } else {
        1.055 * x.powf(1.0 / 2.4) - 0.055
    }
}
pub fn dec1(x: f32) -> f32 {
    let x = x.max(0.0);
    if x <= 0.040_45 {
        x / 12.92
    } else {
        ((x + 0.055) / 1.055).powf(2.4)
    }
}
pub fn enc3(v: [f32; 3]) -> [f32; 3] {
    v.map(enc1)
}
pub fn dec3(v: [f32; 3]) -> [f32; 3] {
    v.map(dec1)
}

fn mat3(m: [[f32; 3]; 3], v: [f32; 3]) -> [f32; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

fn clamp01(v: [f32; 3]) -> [f32; 3] {
    v.map(|x| x.clamp(0.0, 1.0))
}

/// Stored rgb -> a screen colour: preview matrix, then the display encode.
fn to_color32(v: [f32; 3], spec: &PickSpec) -> Color32 {
    let d = mat3(spec.display, v);
    let e = |x: f32| (enc1(x).clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgb(e(d[0]), e(d[1]), e(d[2]))
}

/// A stored colour as it is painted on screen, with its alpha — for a
/// widget outside the picker (a gradient's ramp) that must show exactly
/// what the swatch shows.
pub fn preview(v: [f32; 3], a: f32, spec: &PickSpec) -> Color32 {
    with_alpha(to_color32(v, spec), a)
}

/// The checkerboard the picker lays under anything translucent.
pub fn paint_checker(p: &egui::Painter, rect: Rect, cell: f32) {
    checker(p, rect, cell)
}

fn with_alpha(c: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a.clamp(0.0, 1.0) * 255.0) as u8)
}

/// Encoded rgb -> [hue, sat, value].
pub fn rgb_to_hsv(c: [f32; 3]) -> [f32; 3] {
    let max = c[0].max(c[1]).max(c[2]);
    let min = c[0].min(c[1]).min(c[2]);
    let d = max - min;
    let s = if max > 1e-6 { d / max } else { 0.0 };
    [hue_of(c, max, d), s, max]
}

pub fn hsv_to_rgb(h: [f32; 3]) -> [f32; 3] {
    let [hue, s, v] = h;
    let c = v * s;
    base_hue(hue, c).map(|x| x + (v - c))
}

/// Encoded rgb -> [hue, sat, lightness].
pub fn rgb_to_hsl(c: [f32; 3]) -> [f32; 3] {
    let max = c[0].max(c[1]).max(c[2]);
    let min = c[0].min(c[1]).min(c[2]);
    let d = max - min;
    let l = (max + min) * 0.5;
    let s = if d < 1e-6 { 0.0 } else { d / (1.0 - (2.0 * l - 1.0).abs()).max(1e-6) };
    [hue_of(c, max, d), s.clamp(0.0, 1.0), l]
}

pub fn hsl_to_rgb(h: [f32; 3]) -> [f32; 3] {
    let [hue, s, l] = h;
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    base_hue(hue, c).map(|x| x + (l - c * 0.5))
}

fn hue_of(c: [f32; 3], max: f32, d: f32) -> f32 {
    if d < 1e-6 {
        return 0.0;
    }
    let h = if max == c[0] {
        ((c[1] - c[2]) / d).rem_euclid(6.0)
    } else if max == c[1] {
        (c[2] - c[0]) / d + 2.0
    } else {
        (c[0] - c[1]) / d + 4.0
    };
    (h * 60.0).rem_euclid(360.0)
}

/// The hue's rgb shape at chroma `c`, before the lightness offset.
fn base_hue(hue: f32, c: f32) -> [f32; 3] {
    let hp = hue.rem_euclid(360.0) / 60.0;
    let x = c * (1.0 - (hp.rem_euclid(2.0) - 1.0).abs());
    match hp as u32 {
        0 => [c, x, 0.0],
        1 => [x, c, 0.0],
        2 => [0.0, c, x],
        3 => [0.0, x, c],
        4 => [x, 0.0, c],
        _ => [c, 0.0, x],
    }
}

// --- Oklab (Björn Ottosson's matrices, defined over LINEAR Rec.709) -------

pub fn lin709_to_oklab(c: [f32; 3]) -> [f32; 3] {
    let l = 0.412_221_47 * c[0] + 0.536_332_54 * c[1] + 0.051_445_995 * c[2];
    let m = 0.211_903_5 * c[0] + 0.680_699_55 * c[1] + 0.107_396_96 * c[2];
    let s = 0.088_302_46 * c[0] + 0.281_718_85 * c[1] + 0.629_978_7 * c[2];
    let (l, m, s) = (l.cbrt(), m.cbrt(), s.cbrt());
    [
        0.210_454_26 * l + 0.793_617_79 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    ]
}

pub fn oklab_to_lin709(lab: [f32; 3]) -> [f32; 3] {
    let l = lab[0] + 0.396_337_78 * lab[1] + 0.215_803_76 * lab[2];
    let m = lab[0] - 0.105_561_346 * lab[1] - 0.063_854_17 * lab[2];
    let s = lab[0] - 0.089_484_18 * lab[1] - 1.291_485_5 * lab[2];
    let (l, m, s) = (l * l * l, m * m * m, s * s * s);
    [
        4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s,
        -1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s,
        -0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s,
    ]
}

/// Stored rgb -> [L, C, H] (Oklch), through the Rec.709 matrix.
pub fn value_to_lch(v: [f32; 3], spec: &PickSpec) -> [f32; 3] {
    let lab = lin709_to_oklab(mat3(spec.display, v));
    let c = (lab[1] * lab[1] + lab[2] * lab[2]).sqrt();
    let h = lab[2].atan2(lab[1]).to_degrees().rem_euclid(360.0);
    [lab[0], c, h]
}

/// [L, C, H] -> stored rgb. NOT gamut-clamped: see [`clamp_lch`].
pub fn lch_to_value(lch: [f32; 3], spec: &PickSpec) -> [f32; 3] {
    let (s, c) = lch[2].to_radians().sin_cos();
    mat3(spec.display_inv, oklab_to_lin709([lch[0], lch[1] * c, lch[1] * s]))
}

/// Inside the stored range, which is the working gamut.
fn in_gamut(v: [f32; 3]) -> bool {
    v.iter().all(|x| (-1e-4..=1.0 + 1e-4).contains(x))
}

/// The most chroma this L and hue reach inside the working gamut.
/// 16 bisection steps on 0..1 land within 2e-5 — far below a pixel of lane.
pub fn max_chroma(l: f32, h: f32, spec: &PickSpec) -> f32 {
    if l <= 1e-4 || l >= 1.0 - 1e-4 {
        return 0.0;
    }
    let fits = |c: f32| in_gamut(lch_to_value([l, c, h], spec));
    if !fits(0.0) {
        return 0.0;
    }
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..16 {
        let mid = 0.5 * (lo + hi);
        if fits(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

/// Pull the chroma back to the gamut edge, L and hue held.
pub fn clamp_lch(lch: [f32; 3], spec: &PickSpec) -> [f32; 3] {
    [lch[0], lch[1].min(max_chroma(lch[0], lch[2], spec)), lch[2]]
}

/// The C lane's fixed range: the gamut's most saturated colour, plus a
/// little air. A gamut's cusp for every hue sits on the six edges of its
/// cube that run through the primaries and secondaries (one channel at 1,
/// one at 0), so sampling those finds the maximum;
/// `lane_reaches_every_edge` holds it to a brute-force sweep.
pub fn lane_chroma_max(spec: &PickSpec) -> f32 {
    const RING: [[f32; 3]; 6] = [
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 1.0, 1.0],
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 1.0],
    ];
    let mut m: f32 = 0.0;
    for k in 0..6 {
        let (a, b) = (RING[k], RING[(k + 1) % 6]);
        for i in 0..=32 {
            let t = i as f32 / 32.0;
            let v = [0, 1, 2].map(|j| a[j] + (b[j] - a[j]) * t);
            m = m.max(value_to_lch(v, spec)[1]);
        }
    }
    m * 1.05
}

// --- hex + copy/paste --------------------------------------------------------

/// Stored -> sRGB hex. Eight digits only when there is an alpha to say.
pub fn hex_of(v: [f32; 4], spec: &PickSpec) -> String {
    let d = mat3(spec.display, [v[0], v[1], v[2]]);
    let e = |x: f32| (enc1(x.clamp(0.0, 1.0)) * 255.0).round() as u8;
    let mut s = format!("#{:02x}{:02x}{:02x}", e(d[0]), e(d[1]), e(d[2]));
    if spec.alpha && v[3] < 1.0 {
        s += &format!("{:02x}", (v[3].clamp(0.0, 1.0) * 255.0).round() as u8);
    }
    s
}

/// `#rgb` / `#rgba` / `#rrggbb` / `#rrggbbaa`, hash optional. The alpha is
/// kept when the code does not carry one.
pub fn hex_parse(s: &str, cur_a: f32, spec: &PickSpec) -> Option<[f32; 4]> {
    let t = s.trim().trim_start_matches('#');
    if !t.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize, n: usize| -> Option<f32> {
        let x = u8::from_str_radix(&t[i..i + n], 16).ok()?;
        Some(if n == 1 { (x * 17) as f32 / 255.0 } else { x as f32 / 255.0 })
    };
    let (n, has_a) = match t.len() {
        3 => (1, false),
        4 => (1, true),
        6 => (2, false),
        8 => (2, true),
        _ => return None,
    };
    let enc = [byte(0, n)?, byte(n, n)?, byte(2 * n, n)?];
    let a = if has_a { byte(3 * n, n)? } else { cur_a };
    let v = clamp01(mat3(spec.display_inv, enc.map(dec1)));
    Some([v[0], v[1], v[2], a])
}

/// The clipboard form: the stored numbers, lossless to four places.
/// Blender's float-list shape, since that is where they will get pasted.
pub fn floats_of(v: [f32; 4], alpha: bool) -> String {
    if alpha {
        format!("[{:.4}, {:.4}, {:.4}, {:.4}]", v[0], v[1], v[2], v[3])
    } else {
        format!("[{:.4}, {:.4}, {:.4}]", v[0], v[1], v[2])
    }
}

/// Three or four numbers, any of `[..]` / `(..)` / bare, comma or space
/// separated. Stored numbers go straight in (clamped to the stored range).
pub fn floats_parse(s: &str, cur_a: f32) -> Option<[f32; 4]> {
    let t = s
        .trim()
        .trim_start_matches(|c| c == '[' || c == '(')
        .trim_end_matches(|c| c == ']' || c == ')');
    let xs: Vec<f32> = t
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|p| !p.is_empty())
        .map(|p| p.parse::<f32>())
        .collect::<Result<_, _>>()
        .ok()?;
    let a = match xs.len() {
        3 => cur_a,
        4 => xs[3],
        _ => return None,
    };
    if xs.iter().any(|x| !x.is_finite()) {
        return None;
    }
    let c = |x: f32| x.clamp(0.0, 1.0);
    Some([c(xs[0]), c(xs[1]), c(xs[2]), c(a)])
}

/// Whatever came off the clipboard: numbers first, then hex.
pub fn paste_parse(s: &str, cur_a: f32, spec: &PickSpec) -> Option<[f32; 4]> {
    floats_parse(s, cur_a).or_else(|| hex_parse(s, cur_a, spec))
}

// ---------------------------------------------------------------------------
// State: the coordinates being edited, kept while the popup is open so a
// degenerate value (grey, black) cannot eat the hue you had.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
struct State {
    /// The value when the popup opened — the old chip.
    orig: [f32; 4],
    hsv: [f32; 3],
    hsl: [f32; 3],
    lch: [f32; 3],
    /// The value the coordinates agree with. When the value moves without
    /// us (a patch, undo, another panel) every view re-derives.
    last: [f32; 4],
    /// The C lane's range, fixed for the popup's life.
    c_lane: f32,
}

impl State {
    fn fresh(v: [f32; 4], spec: &PickSpec) -> Self {
        let mut st = Self {
            orig: v,
            hsv: [0.0; 3],
            hsl: [0.0; 3],
            lch: [0.0; 3],
            last: v,
            c_lane: lane_chroma_max(spec),
        };
        st.follow(v, None, spec);
        st
    }

    /// Re-derive every view except `keep` from `v`. Wherever the value
    /// cannot say (hue at zero chroma, saturation at black, hsl saturation
    /// at white) a view keeps its previous coordinate.
    fn follow(&mut self, v: [f32; 4], keep: Option<Mode>, spec: &PickSpec) {
        let rgb = [v[0], v[1], v[2]];
        let e = enc3(rgb);
        let (max, min) = (e[0].max(e[1]).max(e[2]), e[0].min(e[1]).min(e[2]));
        let neutral = max - min < 1e-5;
        if keep != Some(Mode::Wheel) {
            let mut hsv = rgb_to_hsv(e);
            if neutral {
                hsv[0] = self.hsv[0];
                if max < 1e-5 {
                    hsv[1] = self.hsv[1];
                }
            }
            self.hsv = hsv;
        }
        if keep != Some(Mode::Hsl) {
            let mut hsl = rgb_to_hsl(e);
            if neutral {
                hsl[0] = self.hsl[0];
                if max < 1e-5 || min > 1.0 - 1e-5 {
                    hsl[1] = self.hsl[1];
                }
            }
            self.hsl = hsl;
        }
        if keep != Some(Mode::Lch) {
            let mut lch = value_to_lch(rgb, spec);
            if lch[1] < CHROMA_EPS {
                lch[2] = self.lch[2];
            }
            self.lch = lch;
        }
        self.last = v;
    }
}

// ---------------------------------------------------------------------------
// Test geometry, published every frame on the `rgbslider::last_geom`
// pattern so a suite can aim real input at the real layout.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub struct Geom {
    pub open: bool,
    /// The scale it was drawn at, per axis: the app's, fitted to the room.
    pub scale: Vec2,
    pub swatch: Rect,
    pub popup: Rect,
    /// Resize grips: the right edge, the free horizontal edge, the corner.
    pub grips: [Rect; 3],
    pub tabs: [Rect; 4],
    /// Wheel mode only.
    pub disc: Rect,
    pub vstrip: Rect,
    /// The slider mode's three tracks, top to bottom.
    pub lanes: [Rect; 3],
    pub alpha_lane: Rect,
    pub old_chip: Rect,
    pub hex: Rect,
    /// The palette: its grid, a slot's side, the gap between slots, and how
    /// many swatches it holds — slot `swatch_count` is the + slot.
    pub swatch_grid: Rect,
    pub swatch_slot: f32,
    pub swatch_gap: f32,
    pub swatch_count: usize,
    /// A palette slot is armed for the eyedropper this frame. The popup
    /// must not take the viewer click that follows as "clicked elsewhere,
    /// close" — the sample is on its way to that slot.
    pub swatch_dropping: bool,
}

impl Geom {
    /// Palette slot `i`, row by row; slot `swatch_count` is the + slot.
    pub fn swatch_rect(&self, i: usize) -> Rect {
        let pitch = self.swatch_slot + self.swatch_gap;
        let (row, col) = (i / SWATCH_ROW, i % SWATCH_ROW);
        Rect::from_min_size(
            self.swatch_grid.min + vec2(col as f32 * pitch, row as f32 * pitch),
            Vec2::splat(self.swatch_slot),
        )
    }
}

impl Default for Geom {
    fn default() -> Self {
        Self {
            open: false,
            scale: Vec2::ZERO,
            swatch: Rect::NOTHING,
            popup: Rect::NOTHING,
            grips: [Rect::NOTHING; 3],
            tabs: [Rect::NOTHING; 4],
            disc: Rect::NOTHING,
            vstrip: Rect::NOTHING,
            lanes: [Rect::NOTHING; 3],
            alpha_lane: Rect::NOTHING,
            old_chip: Rect::NOTHING,
            hex: Rect::NOTHING,
            swatch_grid: Rect::NOTHING,
            swatch_slot: 0.0,
            swatch_gap: 0.0,
            swatch_count: 0,
            swatch_dropping: false,
        }
    }
}

fn base_id(id_salt: impl std::hash::Hash + std::fmt::Debug) -> Id {
    Id::new("colorpicker").with(id_salt)
}

pub fn last_geom(ctx: &egui::Context, id_salt: impl std::hash::Hash + std::fmt::Debug) -> Option<Geom> {
    ctx.data(|d| d.get_temp::<Geom>(base_id(id_salt).with("geom")))
}

// ---------------------------------------------------------------------------
// Painting
// ---------------------------------------------------------------------------

/// Checkerboard under anything translucent.
fn checker(p: &egui::Painter, rect: Rect, cell: f32) {
    let cell = cell.max(2.0);
    p.rect_filled(rect, 2.0, Color32::from_gray(96));
    let cols = (rect.width() / cell).ceil() as i32;
    let rows = (rect.height() / cell).ceil() as i32;
    for y in 0..rows {
        for x in (0..cols).filter(|x| (x + y) % 2 == 1) {
            let r = Rect::from_min_size(
                rect.min + vec2(x as f32 * cell, y as f32 * cell),
                vec2(cell, cell),
            );
            p.rect_filled(r.intersect(rect), 0.0, Color32::from_gray(150));
        }
    }
}

/// A strip of `GRAD_N + 1` colour samples, left to right or bottom to top.
fn gradient(p: &egui::Painter, rect: Rect, vertical: bool, sample: &dyn Fn(usize) -> Color32) {
    let mut mesh = epaint::Mesh::default();
    for i in 0..=GRAD_N {
        let t = i as f32 / GRAD_N as f32;
        let c = sample(i);
        let ends = if vertical {
            let y = rect.max.y - t * rect.height();
            [pos2(rect.min.x, y), pos2(rect.max.x, y)]
        } else {
            let x = rect.min.x + t * rect.width();
            [pos2(x, rect.min.y), pos2(x, rect.max.y)]
        };
        for pos in ends {
            mesh.vertices.push(epaint::Vertex { pos, uv: epaint::WHITE_UV, color: c });
        }
        if i > 0 {
            let k = (i as u32) * 2;
            mesh.indices.extend_from_slice(&[k - 2, k - 1, k, k - 1, k + 1, k]);
        }
    }
    p.add(egui::Shape::mesh(mesh));
}

fn frame_stroke(p: &egui::Painter, rect: Rect) {
    p.rect_stroke(rect, 2.0, Stroke::new(1.0, Color32::from_black_alpha(110)), StrokeKind::Outside);
}

/// The knob every lane, the value strip and the wheel share: the colour it
/// sits on, in a white ring that reads on any gradient. Sized with the
/// track it rides, so it stays a knob at every scale.
fn thumb(p: &egui::Painter, at: Pos2, r: f32, fill: Color32) {
    p.circle_filled(at, r, Color32::from_gray(128));
    p.circle_filled(at, r, fill);
    p.circle_stroke(at, r - 1.0, Stroke::new(2.0, Color32::WHITE));
    p.circle_stroke(at, r, Stroke::new(1.0, Color32::from_black_alpha(200)));
}

/// Underline every run of `over` samples beneath a track.
fn underline_runs(p: &egui::Painter, track: Rect, over: &[bool], color: Color32) {
    let n = over.len().saturating_sub(1).max(1) as f32;
    let half = 0.5 / n;
    let y = track.max.y + 3.0;
    let mut i = 0;
    while i < over.len() {
        if !over[i] {
            i += 1;
            continue;
        }
        let start = i;
        while i + 1 < over.len() && over[i + 1] {
            i += 1;
        }
        let a = (start as f32 / n - half).max(0.0);
        let b = (i as f32 / n + half).min(1.0);
        p.line_segment(
            [
                pos2(track.min.x + a * track.width(), y),
                pos2(track.min.x + b * track.width(), y),
            ],
            Stroke::new(2.0, color),
        );
        i += 1;
    }
}

// ---------------------------------------------------------------------------
// Drags
// ---------------------------------------------------------------------------

/// Absolute on the press, relative after it — the house rail contract
/// (`widgets::sens_drag`). A plain press puts the knob under the pointer; a
/// press with a rate key already down does not jump. From then on the
/// pointer's travel is added at the LIVE rate, so fine -> coarse ->
/// released steps inside one gesture and a release never snaps. The
/// accumulator is unclamped, so at 1:1 the knob stays exactly under the
/// finger even after running off an end and coming back.
fn drag_1d(
    ui: &Ui,
    r: &Response,
    id: Id,
    cur: f32,
    spec: &PickSpec,
    to_v: impl Fn(Pos2) -> f32,
) -> Option<f32> {
    let acc: Option<f32> = ui.data(|d| d.get_temp(id));
    if !r.is_pointer_button_down_on() {
        if acc.is_some() {
            ui.data_mut(|d| d.remove::<f32>(id));
        }
        return None;
    }
    let pos = r.interact_pointer_pos()?;
    let rate = (spec.rate)(ui);
    let nv = match acc {
        None if rate.is_none() => to_v(pos),
        None => cur,
        Some(a) => {
            let d = ui.input(|i| i.pointer.delta());
            a + (to_v(pos) - to_v(pos - d)) * rate.unwrap_or(1.0)
        }
    };
    ui.data_mut(|d| d.insert_temp(id, nv));
    Some(nv)
}

/// [`drag_1d`] in two dimensions, for the wheel's puck.
fn drag_2d(ui: &Ui, r: &Response, id: Id, cur: Pos2, spec: &PickSpec) -> Option<Pos2> {
    let acc: Option<Pos2> = ui.data(|d| d.get_temp(id));
    if !r.is_pointer_button_down_on() {
        if acc.is_some() {
            ui.data_mut(|d| d.remove::<Pos2>(id));
        }
        return None;
    }
    let pos = r.interact_pointer_pos()?;
    let rate = (spec.rate)(ui);
    let np = match acc {
        None if rate.is_none() => pos,
        None => cur,
        Some(a) => a + ui.input(|i| i.pointer.delta()) * rate.unwrap_or(1.0),
    };
    ui.data_mut(|d| d.insert_temp(id, np));
    Some(np)
}

/// A number field at the host's rate. True when it changed the value.
fn number(ui: &mut Ui, v: &mut f32, lo: f32, hi: f32, speed: f32, decimals: usize, spec: &PickSpec) -> bool {
    let before = *v;
    let rate = (spec.rate)(ui).unwrap_or(1.0);
    crate::numfield::fixed(ui, NUM_W, NUM_H, egui::DragValue::new(v)
            .range(lo..=hi)
            .speed(speed * rate)
            .max_decimals(decimals),
    );
    *v != before
}

// ---------------------------------------------------------------------------
// Lanes
// ---------------------------------------------------------------------------

/// One lane: label, gradient track, knob, number. `sample(i)` paints
/// gradient sample `i` of `GRAD_N`; `over` marks samples to underline.
/// Returns (changed, track rect).
#[allow(clippy::too_many_arguments)]
fn lane(
    ui: &mut Ui,
    geo: &Geo,
    id: Id,
    label: &str,
    val: &mut f32,
    (lo, hi): (f32, f32),
    (speed, decimals): (f32, usize),
    spec: &PickSpec,
    sample: &dyn Fn(usize) -> Color32,
    over: Option<&[bool]>,
    checkered: bool,
) -> (bool, Rect) {
    let mut changed = false;
    let mut track = Rect::NOTHING;
    ui.horizontal(|ui| {
        ui.add_sized([LAB_W, geo.lane_h], egui::Label::new(egui::RichText::new(label).weak()));
        let (rect, r) = ui.allocate_exact_size(vec2(geo.lane_w, geo.lane_h), Sense::click_and_drag());
        track = Rect::from_center_size(
            rect.center(),
            vec2(geo.lane_w - 2.0 * geo.knob_r, geo.track_h),
        );
        let span = (hi - lo).max(1e-6);
        let to_v = |p: Pos2| lo + (p.x - track.min.x) / track.width() * span;
        if let Some(nv) = drag_1d(ui, &r, id.with("drag"), *val, spec, to_v) {
            let nv = nv.clamp(lo, hi);
            if nv != *val {
                *val = nv;
                changed = true;
            }
        }
        if r.hovered() || r.is_pointer_button_down_on() {
            ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
        }
        let p = ui.painter_at(rect);
        if checkered {
            checker(&p, track, geo.track_h * 0.5);
        }
        gradient(&p, track, false, sample);
        frame_stroke(&p, track);
        if let Some(over) = over {
            underline_runs(&p, track, over, ui.visuals().text_color());
        }
        let t = ((*val - lo) / span).clamp(0.0, 1.0);
        let fill = sample((t * GRAD_N as f32).round() as usize);
        thumb(&p, pos2(track.min.x + t * track.width(), track.center().y), geo.knob_r, fill);
        changed |= number(ui, val, lo, hi, speed, decimals, spec);
    });
    (changed, track)
}

// ---------------------------------------------------------------------------
// The wheel
// ---------------------------------------------------------------------------

/// Screen direction of a hue, matching the grade wheels (`wheels.rs`): red
/// up, green lower-left, blue lower-right. One hue geometry in the app.
fn hue_dir(h_deg: f32) -> Vec2 {
    let r = h_deg.to_radians();
    vec2(-r.sin(), -r.cos())
}

fn hue_at(c: Pos2, pos: Pos2) -> f32 {
    let d = pos - c;
    (-d.x).atan2(-d.y).to_degrees().rem_euclid(360.0)
}

/// The disc dims with value but floors well above black: it is an aiming
/// surface, and the chips tell the truth. Same licence as the hue lanes.
fn disc_value(v: f32) -> f32 {
    0.3 + 0.7 * v.clamp(0.0, 1.0)
}

/// Disc + value strip + h/s/v numbers. Edits `st.hsv`; true on change.
fn wheel_ui(ui: &mut Ui, geo: &Geo, id: Id, st: &mut State, spec: &PickSpec, geom: &mut Geom) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        let d = geo.disc_d;
        let rad = d * 0.5;
        let (disc, dr) = ui.allocate_exact_size(vec2(d, d), Sense::click_and_drag());
        geom.disc = disc;
        let c = disc.center();

        let puck_at = |hsv: [f32; 3]| c + hue_dir(hsv[0]) * (hsv[1].clamp(0.0, 1.0) * rad);
        if let Some(np) = drag_2d(ui, &dr, id.with("puck"), puck_at(st.hsv), spec) {
            let h = if (np - c).length() > 1e-3 { hue_at(c, np) } else { st.hsv[0] };
            let s = ((np - c).length() / rad).clamp(0.0, 1.0);
            if h != st.hsv[0] || s != st.hsv[1] {
                (st.hsv[0], st.hsv[1]) = (h, s);
                changed = true;
            }
        }
        if dr.hovered() || dr.is_pointer_button_down_on() {
            ui.ctx().set_cursor_icon(CursorIcon::Crosshair);
        }

        // Rings x spokes, coloured at the current value (floored). A puck
        // at the rim may hang into the gap beside the disc, never beyond.
        let p = ui.painter_at(disc.expand(geo.knob_r.min(geo.gap.x)));
        let vd = disc_value(st.hsv[2]);
        const RINGS: usize = 16;
        const SPOKES: usize = 72;
        let mut mesh = epaint::Mesh::default();
        for ring in 0..=RINGS {
            let s = ring as f32 / RINGS as f32;
            for k in 0..=SPOKES {
                let h = k as f32 / SPOKES as f32 * 360.0;
                mesh.vertices.push(epaint::Vertex {
                    pos: c + hue_dir(h) * (s * rad),
                    uv: epaint::WHITE_UV,
                    color: to_color32(dec3(hsv_to_rgb([h, s, vd])), spec),
                });
            }
        }
        for ring in 0..RINGS {
            for k in 0..SPOKES {
                let a = (ring * (SPOKES + 1) + k) as u32;
                let b = a + (SPOKES + 1) as u32;
                mesh.indices.extend_from_slice(&[a, b, a + 1, b, b + 1, a + 1]);
            }
        }
        p.add(egui::Shape::mesh(mesh));
        p.circle_stroke(c, rad, Stroke::new(1.0, Color32::from_black_alpha(120)));
        let now = to_color32(dec3(hsv_to_rgb(st.hsv)), spec);
        thumb(&p, puck_at(st.hsv), geo.knob_r, now);

        // Value strip: 0 at the bottom, 1 at the top, at the current hue
        // and saturation — the axis the disc cannot draw. A vertical lane.
        let (strip, vr) = ui.allocate_exact_size(vec2(geo.strip_w, d), Sense::click_and_drag());
        geom.vstrip = strip;
        let run = strip.shrink2(vec2(0.0, geo.knob_r));
        let to_v = |pos: Pos2| (run.max.y - pos.y) / run.height();
        if let Some(nv) = drag_1d(ui, &vr, id.with("value"), st.hsv[2], spec, to_v) {
            let nv = nv.clamp(0.0, 1.0);
            if nv != st.hsv[2] {
                st.hsv[2] = nv;
                changed = true;
            }
        }
        if vr.hovered() || vr.is_pointer_button_down_on() {
            ui.ctx().set_cursor_icon(CursorIcon::ResizeVertical);
        }
        let sp = ui.painter_at(strip.expand2(vec2(geo.gap.x.min(2.0), 0.0)));
        let (h, s) = (st.hsv[0], st.hsv[1]);
        gradient(&sp, run, true, &|i| {
            to_color32(dec3(hsv_to_rgb([h, s, i as f32 / GRAD_N as f32])), spec)
        });
        frame_stroke(&sp, run);
        let ky = run.max.y - st.hsv[2].clamp(0.0, 1.0) * run.height();
        thumb(&sp, pos2(strip.center().x, ky), geo.knob_r, now);

        ui.vertical(|ui| {
            let rows = [("h", 360.0, 1.0, 0), ("s", 1.0, 0.005, 3), ("v", 1.0, 0.005, 3)];
            for (i, (lab, hi, speed, dec)) in rows.into_iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.add_sized(
                        [WHEEL_NUM_LAB, NUM_H],
                        egui::Label::new(egui::RichText::new(lab).weak()),
                    );
                    changed |= number(ui, &mut st.hsv[i], 0.0, hi, speed, dec, spec);
                });
            }
        });
    });
    changed
}

// ---------------------------------------------------------------------------
// The palette
// ---------------------------------------------------------------------------

/// Stored -> the palette's space, linear Rec.709: a swatch has to mean one
/// colour in every comp, whatever its working space.
fn to_palette(v: [f32; 4], spec: &PickSpec) -> [f32; 3] {
    mat3(spec.display, [v[0], v[1], v[2]])
}

/// The palette's space -> stored, clamped to the stored range (a wide-gamut
/// swatch in a narrower comp lands on the nearest colour it can make).
fn from_palette(c: [f32; 3], spec: &PickSpec) -> [f32; 3] {
    clamp01(mat3(spec.display_inv, c))
}

/// A Rec.709 swatch as a screen colour.
fn paint709(c: [f32; 3]) -> Color32 {
    let e = |x: f32| (enc1(x).clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgb(e(c[0]), e(c[1]), e(c[2]))
}

/// What a click on a palette slot does, by the modifiers held.
enum SwatchOp {
    Use(usize),
    Keep,
    Replace(usize),
    Remove(usize),
    /// A viewer sample landed on this slot (already in the palette's
    /// space): it fills the + slot or overwrites a swatch.
    Sample(usize, [f32; 3]),
}

/// The palette: click a swatch to use it, the + slot keeps the current
/// colour, shift-click replaces a swatch with the current colour, alt-click
/// removes one. Colour only: the value keeps its alpha. True when `v`
/// changed; edits to `palette` are the caller's to notice.
///
/// Every slot is also an EYEDROPPER TARGET in its own right (`E` over it,
/// then click the viewer, same as any colour control): the sample lands in
/// that slot — in the + slot, as a new swatch — without going through the
/// value first. That is how a palette gets built from the shot rather than
/// from the picker.
fn swatch_grid(
    ui: &mut Ui,
    geo: &Geo,
    id: Id,
    v: &mut [f32; 4],
    palette: &mut Vec<[f32; 3]>,
    spec: &PickSpec,
    geom: &mut Geom,
) -> bool {
    let (grid, _) = ui.allocate_exact_size(vec2(geo.width, geo.grid_h()), Sense::hover());
    geom.swatch_grid = grid;
    geom.swatch_dropping = false;
    geom.swatch_slot = geo.slot;
    geom.swatch_gap = geo.slot_gap;
    geom.swatch_count = palette.len();
    let now = to_palette(*v, spec);
    let mut op = None;
    for i in 0..slot_count(palette.len()) {
        let r = geom.swatch_rect(i);
        let resp = ui.interact(r, id.with(("swatch", i)), Sense::click());
        let hot = resp.hovered();
        if hot {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        // Each slot is its own eyedropper target, so E over one arms the
        // viewer for THAT slot. The swatch's own hook (which arms from
        // anywhere over the popup) runs earlier in the frame and stands
        // down over this grid, or it would consume the key first.
        let (dropping, drop) = (spec.eyedrop)(ui, id.with(("swatch-drop", i)), hot);
        geom.swatch_dropping |= dropping;
        if let Some(c) = drop {
            op = Some(SwatchOp::Sample(i, to_palette([c[0], c[1], c[2], 1.0], spec)));
        }
        let w = &ui.visuals().widgets;
        let edge = if hot { w.hovered.fg_stroke.color } else { w.noninteractive.fg_stroke.color };
        let p = ui.painter();
        if let Some(&c) = palette.get(i) {
            p.rect_filled(r, 2.0, paint709(c));
            // The swatch the value is on right now: a double ring that reads
            // on white and on black alike.
            if (0..3).all(|k| (c[k] - now[k]).abs() < 1e-4) {
                p.rect_stroke(r, 2.0, Stroke::new(2.0, Color32::WHITE), StrokeKind::Inside);
                p.rect_stroke(r.shrink(2.0), 1.0, Stroke::new(1.0, Color32::from_black_alpha(170)), StrokeKind::Inside);
            }
            let outline = if hot { edge } else { Color32::from_black_alpha(110) };
            p.rect_stroke(r, 2.0, Stroke::new(1.0, outline), StrokeKind::Outside);
            if resp.clicked() {
                let m = ui.input(|i| i.modifiers);
                op = Some(if m.alt {
                    SwatchOp::Remove(i)
                } else if m.shift {
                    SwatchOp::Replace(i)
                } else {
                    SwatchOp::Use(i)
                });
            }
            resp.on_hover_text(format!(
                "{} — click to use · shift-click to replace it with the current colour · \
                 alt-click to remove · E samples the viewer into it",
                hex_of([c[0], c[1], c[2], 1.0], &PickSpec::rgb())
            ));
        } else {
            // The + slot.
            p.rect_stroke(r.shrink(1.0), 2.0, Stroke::new(1.0, edge), StrokeKind::Inside);
            let (c, arm) = (r.center(), r.width() * 0.22);
            p.line_segment([c - vec2(arm, 0.0), c + vec2(arm, 0.0)], Stroke::new(1.5, edge));
            p.line_segment([c - vec2(0.0, arm), c + vec2(0.0, arm)], Stroke::new(1.5, edge));
            if resp.clicked() {
                op = Some(SwatchOp::Keep);
            }
            resp.on_hover_text(
                "keep the current colour as a swatch · E samples the viewer straight into it \
                 — every picker in the app shares them",
            );
        }
        if dropping {
            ui.painter().rect_stroke(
                r.expand(2.0),
                3.0,
                Stroke::new(1.5, ui.visuals().selection.stroke.color),
                StrokeKind::Outside,
            );
        }
    }
    match op {
        Some(SwatchOp::Use(i)) => {
            let c = from_palette(palette[i], spec);
            if [v[0], v[1], v[2]] != c {
                (v[0], v[1], v[2]) = (c[0], c[1], c[2]);
                return true;
            }
        }
        Some(SwatchOp::Keep) => palette.push(now),
        Some(SwatchOp::Sample(i, c)) => {
            if i < palette.len() {
                palette[i] = c;
            } else if palette.len() < SWATCH_MAX {
                palette.push(c);
            }
        }
        Some(SwatchOp::Replace(i)) => palette[i] = now,
        Some(SwatchOp::Remove(i)) => {
            palette.remove(i);
        }
        None => {}
    }
    false
}

// ---------------------------------------------------------------------------
// The picker body
// ---------------------------------------------------------------------------

/// The mode tabs.
fn tabs_row(ui: &mut Ui, geo: &Geo, mode: &mut Mode, geom: &mut Geom) {
    ui.horizontal(|ui| {
        let w = (geo.width - 3.0 * geo.gap.x) / 4.0;
        for (i, m) in Mode::ALL.into_iter().enumerate() {
            let r = ui
                .add_sized([w, NUM_H], egui::Button::selectable(*mode == m, m.label()))
                .on_hover_text(m.hint());
            geom.tabs[i] = r.rect;
            if r.clicked() {
                *mode = m;
            }
        }
    });
}

/// The whole picker, embeddable anywhere; the popup is this in a frame.
/// `id` must be stable per value. True when `v` changed this frame.
/// `palette` is the swatches it offers, which it may edit.
pub fn picker_ui(
    ui: &mut Ui,
    id: Id,
    v: &mut [f32; 4],
    spec: PickSpec,
    view: &mut View,
    palette: &mut Vec<[f32; 3]>,
    geom: &mut Geom,
) -> bool {
    let geo = Geo::new(view.scale, palette.len());
    let sid = id.with("state");
    let mut st: State = ui.data(|d| d.get_temp(sid)).unwrap_or_else(|| State::fresh(*v, &spec));
    if st.last != *v {
        st.follow(*v, None, &spec);
    }
    let spec = &spec;
    let mut changed = false;
    geom.scale = view.scale;

    ui.spacing_mut().item_spacing = geo.gap;
    ui.set_width(geo.width);

    if !view.tabs_last {
        tabs_row(ui, &geo, &mut view.mode, geom);
    }

    match view.mode {
        Mode::Wheel => {
            if wheel_ui(ui, &geo, id.with("wheel"), &mut st, spec, geom) {
                let nv = clamp01(dec3(hsv_to_rgb(st.hsv)));
                (v[0], v[1], v[2]) = (nv[0], nv[1], nv[2]);
                st.follow(*v, Some(Mode::Wheel), spec);
                changed = true;
            }
        }
        Mode::Rgb => {
            let cur = [v[0], v[1], v[2]];
            let mut edited = false;
            for i in 0..3 {
                let sample = move |k: usize| {
                    let mut c = cur;
                    c[i] = k as f32 / GRAD_N as f32;
                    to_color32(c, spec)
                };
                let (ch, track) = lane(
                    ui,
                    &geo,
                    id.with(("rgb", i)),
                    ["r", "g", "b"][i],
                    &mut v[i],
                    (0.0, 1.0),
                    (0.003, 3),
                    spec,
                    &sample,
                    None,
                    false,
                );
                geom.lanes[i] = track;
                edited |= ch;
            }
            if edited {
                st.follow(*v, None, spec);
                changed = true;
            }
        }
        Mode::Hsl => {
            let cur = st.hsl;
            let mut edited = false;
            let rows = [("h", 360.0, 1.0, 0), ("s", 1.0, 0.005, 3), ("l", 1.0, 0.005, 3)];
            for (i, (lab, hi, speed, dec)) in rows.into_iter().enumerate() {
                let sample = move |k: usize| {
                    let t = k as f32 / GRAD_N as f32;
                    let mut c = cur;
                    if i == 0 {
                        c[0] = t * 360.0;
                        // The hue lane's floor: never a grey bar.
                        let chroma = (1.0 - (2.0 * c[2] - 1.0).abs()) * c[1];
                        if chroma < HSL_HUE_FLOOR {
                            c[2] = c[2].clamp(0.25, 0.75);
                            c[1] = (HSL_HUE_FLOOR / (1.0 - (2.0 * c[2] - 1.0).abs())).min(1.0);
                        }
                    } else {
                        c[i] = t;
                    }
                    to_color32(dec3(hsl_to_rgb(c)), spec)
                };
                let (ch, track) = lane(
                    ui,
                    &geo,
                    id.with(("hsl", i)),
                    lab,
                    &mut st.hsl[i],
                    (0.0, hi),
                    (speed, dec),
                    spec,
                    &sample,
                    None,
                    false,
                );
                geom.lanes[i] = track;
                edited |= ch;
            }
            if edited {
                let nv = clamp01(dec3(hsl_to_rgb(st.hsl)));
                (v[0], v[1], v[2]) = (nv[0], nv[1], nv[2]);
                st.follow(*v, Some(Mode::Hsl), spec);
                changed = true;
            }
        }
        Mode::Lch => {
            let cur = st.lch;
            let c_hi = st.c_lane;
            let t = |k: usize| k as f32 / GRAD_N as f32;
            // The gamut edge along the L and H lanes: one bisection per
            // gradient sample serves both the picture and the underline.
            let edge_l: [f32; GRAD_N + 1] =
                std::array::from_fn(|k| max_chroma(t(k), cur[2], spec));
            let edge_h: [f32; GRAD_N + 1] =
                std::array::from_fn(|k| max_chroma(cur[0], t(k) * 360.0, spec));
            let edge_c = max_chroma(cur[0], cur[2], spec);
            let over_l: [bool; GRAD_N + 1] =
                std::array::from_fn(|k| cur[1] > edge_l[k] + CHROMA_EPS);
            let over_h: [bool; GRAD_N + 1] =
                std::array::from_fn(|k| cur[1] > edge_h[k] + CHROMA_EPS);
            let over_c: [bool; GRAD_N + 1] =
                std::array::from_fn(|k| t(k) * c_hi > edge_c + CHROMA_EPS);
            let paint = |lch: [f32; 3]| to_color32(clamp01(lch_to_value(lch, spec)), spec);
            let samples: [&dyn Fn(usize) -> Color32; 3] = [
                &|k| paint([t(k), cur[1].min(edge_l[k]), cur[2]]),
                &|k| paint([cur[0], (t(k) * c_hi).min(edge_c), cur[2]]),
                &|k| {
                    let h = t(k) * 360.0;
                    if cur[1] < LCH_HUE_FLOOR {
                        let l = cur[0].clamp(0.45, 0.85);
                        paint(clamp_lch([l, LCH_HUE_FLOOR, h], spec))
                    } else {
                        paint([cur[0], cur[1].min(edge_h[k]), h])
                    }
                },
            ];
            let overs: [&[bool]; 3] = [&over_l, &over_c, &over_h];
            let rows = [("L", 1.0, 0.005, 3), ("c", c_hi, 0.001, 3), ("h", 360.0, 1.0, 0)];
            let mut edited = false;
            for (i, (lab, hi, speed, dec)) in rows.into_iter().enumerate() {
                let (ch, track) = lane(
                    ui,
                    &geo,
                    id.with(("lch", i)),
                    lab,
                    &mut st.lch[i],
                    (0.0, hi),
                    (speed, dec),
                    spec,
                    samples[i],
                    Some(overs[i]),
                    false,
                );
                geom.lanes[i] = track;
                edited |= ch;
            }
            if edited {
                let nv = clamp01(lch_to_value(clamp_lch(st.lch, spec), spec));
                (v[0], v[1], v[2]) = (nv[0], nv[1], nv[2]);
                st.follow(*v, Some(Mode::Lch), spec);
                changed = true;
            }
        }
    }

    if spec.alpha {
        let cur = [v[0], v[1], v[2]];
        let sample = move |k: usize| with_alpha(to_color32(cur, spec), k as f32 / GRAD_N as f32);
        let (ch, track) = lane(
            ui,
            &geo,
            id.with("alpha"),
            "a",
            &mut v[3],
            (0.0, 1.0),
            (0.003, 3),
            spec,
            &sample,
            None,
            true,
        );
        geom.alpha_lane = track;
        if ch {
            st.last = *v;
            changed = true;
        }
    }

    ui.add_space(geo.gap.y);
    if swatch_grid(ui, &geo, id, v, palette, spec, geom) {
        st.follow(*v, None, spec);
        changed = true;
    }
    ui.horizontal(|ui| {
        let (chips, cr) = ui.allocate_exact_size(vec2(geo.chip.x * 2.0, geo.chip.y), Sense::click());
        let old = Rect::from_min_size(chips.min, geo.chip);
        let new = Rect::from_min_size(chips.min + vec2(geo.chip.x, 0.0), geo.chip);
        geom.old_chip = old;
        let p = ui.painter();
        if spec.alpha {
            checker(p, chips, geo.track_h * 0.5);
        }
        let a = |x: f32| if spec.alpha { x } else { 1.0 };
        let oc = to_color32([st.orig[0], st.orig[1], st.orig[2]], spec);
        let nc = to_color32([v[0], v[1], v[2]], spec);
        p.rect_filled(old, 0.0, with_alpha(oc, a(st.orig[3])));
        p.rect_filled(new, 0.0, with_alpha(nc, a(v[3])));
        frame_stroke(p, chips);
        // Old and new read as two chips even while they are equal.
        p.line_segment(
            [new.left_top(), new.left_bottom()],
            Stroke::new(1.0, Color32::from_black_alpha(110)),
        );
        let over_old = cr.hover_pos().is_some_and(|q| old.contains(q));
        if over_old {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        let cr = cr.on_hover_text("old | new — click old to put it back");
        if cr.clicked() && over_old && *v != st.orig {
            *v = st.orig;
            st.follow(*v, None, spec);
            changed = true;
        }

        ui.add_space((ui.available_width() - HEX_W).max(0.0));
        let hid = id.with("hex");
        let live: Option<String> = ui.data(|d| d.get_temp(hid));
        let mut s = live.clone().unwrap_or_else(|| hex_of(*v, spec));
        // Exactly HEX_H tall, whatever the font's row height: the footer's
        // height is arithmetic the popup is placed by.
        let (_, hex_rect) = ui.allocate_space(vec2(HEX_W, HEX_H));
        let hex_edit = egui::TextEdit::singleline(&mut s)
            .font(TextStyle::Monospace)
            .margin(egui::Margin::symmetric(4, 1));
        let te = ui
            .put(hex_rect, hex_edit)
            .on_hover_text("sRGB hex — what this colour means outside the comp");
        geom.hex = te.rect;
        if te.has_focus() {
            ui.data_mut(|d| d.insert_temp(hid, s.clone()));
        } else if live.is_some() {
            ui.data_mut(|d| d.remove::<String>(hid));
            if te.lost_focus() {
                if let Some(nv) = hex_parse(&s, v[3], spec) {
                    if nv != *v {
                        *v = nv;
                        st.follow(*v, None, spec);
                        changed = true;
                    }
                }
            }
        }
    });

    if view.tabs_last {
        tabs_row(ui, &geo, &mut view.mode, geom);
    }
    ui.data_mut(|d| d.insert_temp(sid, st));
    changed
}

// ---------------------------------------------------------------------------
// The swatch + popup
// ---------------------------------------------------------------------------

/// ⌘C / ⌘V against `v`, for whoever owns the hover. True when pasted.
fn clipboard(ui: &Ui, v: &mut [f32; 4], spec: &PickSpec) -> bool {
    if ui.ctx().egui_wants_keyboard_input() {
        return false;
    }
    let (copy, paste) = ui.input(|i| {
        let mut out = (false, None);
        for e in &i.events {
            match e {
                Event::Copy => out.0 = true,
                Event::Paste(s) => out.1 = Some(s.clone()),
                _ => {}
            }
        }
        out
    });
    if copy {
        ui.ctx().copy_text(floats_of(*v, spec.alpha));
    }
    match paste.and_then(|s| paste_parse(&s, v[3], spec)) {
        Some(nv) if nv != *v => {
            *v = nv;
            true
        }
        _ => false,
    }
}

/// A popup's life: whether it is open, the frame it was last drawn on (a
/// swatch that stops being drawn comes back closed), and which side of the
/// swatch it hangs on — kept while open, so a resize never flips it.
#[derive(Clone, Copy, Debug)]
struct Open {
    open: bool,
    seen: u64,
    above: bool,
    /// How far the user has DRAGGED it off the spot it hangs on. A delta,
    /// not a position: a resize still pins the swatch-side edge, and the
    /// popup still follows its swatch if the panel under it moves. Cleared
    /// on every fresh open, so a picker always comes up where it belongs.
    off: Vec2,
}

/// A grip drag in flight: the scale it started from and which way it grows.
#[derive(Clone, Copy, Debug)]
struct Resize {
    s0: Vec2,
    axis: Vec2,
}

/// The resize grips: the right edge, the free horizontal edge (away from
/// the swatch) and the corner between them. Each reaches a little outside
/// the frame and across its margin; the corner comes last so it wins where
/// it overlaps an edge.
fn grip_zones(pop: Rect, above: bool, reach_in: f32) -> [Rect; 3] {
    let right = Rect::from_min_max(
        pos2(pop.right() - reach_in, pop.top()),
        pos2(pop.right() + GRIP_OUT, pop.bottom()),
    );
    let (y0, y1) = if above {
        (pop.top() - GRIP_OUT, pop.top() + reach_in)
    } else {
        (pop.bottom() - reach_in, pop.bottom() + GRIP_OUT)
    };
    let free = Rect::from_min_max(pos2(pop.left(), y0), pos2(pop.right(), y1));
    let corner_y = if above { pop.top() } else { pop.bottom() };
    let corner =
        Rect::from_center_size(pos2(pop.right(), corner_y), Vec2::splat(2.0 * reach_in.max(8.0)));
    [right, free, corner]
}

/// The bands a popup can be DRAGGED by: its frame margin, all four sides.
/// The chrome and nothing else — a drag rect over the content would take
/// the hover off every widget under it the moment the button went down,
/// and the old|new chip would stop answering clicks. Registered before the
/// grips, so a grip sharing an edge keeps its drag.
fn move_zones(pop: Rect, margin: f32) -> [Rect; 4] {
    let m = margin.max(4.0);
    [
        Rect::from_min_max(pop.min, pos2(pop.max.x, pop.top() + m)),
        Rect::from_min_max(pos2(pop.min.x, pop.bottom() - m), pop.max),
        Rect::from_min_max(pop.min, pos2(pop.left() + m, pop.max.y)),
        Rect::from_min_max(pos2(pop.right() - m, pop.min.y), pop.max),
    ]
}

/// Nudge a popup back inside the screen, keeping its size. Only a popup
/// the user has MOVED is fitted this way — one hanging off its swatch is
/// placed by the swatch and must not be shoved over it.
fn fit_on_screen(r: Rect, screen: Rect) -> Rect {
    let x = r.min.x.min(screen.right() - POP_GAP - r.width()).max(screen.left() + POP_GAP);
    let y = r.min.y.min(screen.bottom() - POP_GAP - r.height()).max(screen.top() + POP_GAP);
    Rect::from_min_size(pos2(x, y), r.size())
}

/// The corner glyph: two strokes across the free corner, inside the frame
/// margin, brighter while a grip is hovered or held.
fn paint_grip(ui: &Ui, pop: Rect, above: bool, margin: f32, hot: bool) {
    let w = &ui.visuals().widgets;
    let color = if hot { w.hovered.fg_stroke.color } else { w.noninteractive.fg_stroke.color };
    let inset = margin * 0.3;
    let (y, down) = if above { (pop.top() + inset, 1.0) } else { (pop.bottom() - inset, -1.0) };
    let corner = pos2(pop.right() - inset, y);
    for len in [0.35, 0.7] {
        let l = margin * len;
        ui.painter().line_segment(
            [corner - vec2(l, 0.0), corner + vec2(0.0, down * l)],
            Stroke::new(1.0, color),
        );
    }
}

/// The frame a picker popup was last open on, in egui memory.
fn open_frame_id() -> Id {
    Id::new("colorpicker::open-frame")
}

/// True while any picker popup is open (as of this frame or the last, so a
/// site that runs before the swatch draws still gets the answer). The
/// host's Escape stands down on it: the popup takes Escape itself, and an
/// app-level Escape that fired first would change the view under a popup
/// the user only meant to dismiss.
pub fn any_open(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<u64>(open_frame_id()))
        .is_some_and(|f| f + 1 >= ctx.cumulative_frame_nr())
}

/// Drop-in for `color_edit_button_rgba_unmultiplied`: a swatch that opens
/// the picker. `id_salt` must be stable and unique per swatch.
pub fn button(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug + Copy,
    v: &mut [f32; 4],
    spec: PickSpec,
) -> Response {
    button_sized(
        ui,
        id_salt,
        v,
        spec,
        SWATCH,
        "click to pick · ⌘C / ⌘V copy and paste the colour · E: eyedropper",
        None,
    )
}

/// [`button`] at a size of the caller's choosing, with its own hover text
/// — a chip's dot-sized swatch, say. `hit`, when given, is the rect that
/// takes the click and the hover instead of the swatch itself, for a
/// swatch too small to be a target on its own; it must be laid out by the
/// caller so nothing it covers is registered later and steals the click.
/// The popup hangs off the swatch's rect as ever, whatever its size.
pub fn button_sized(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug + Copy,
    v: &mut [f32; 4],
    spec: PickSpec,
    size: Vec2,
    hint: &str,
    hit: Option<Rect>,
) -> Response {
    let id = base_id(id_salt);
    let ctx = ui.ctx().clone();
    let mut geom = Geom::default();

    let (rect, mut resp) = match hit {
        None => ui.allocate_exact_size(size, Sense::click()),
        Some(h) => {
            let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
            (rect, ui.interact(h, id.with("hit"), Sense::click()))
        }
    };
    geom.swatch = rect;
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        if spec.alpha && v[3] < 1.0 {
            checker(p, rect, 5.0);
        }
        let a = if spec.alpha { v[3] } else { 1.0 };
        p.rect_filled(rect, 2.0, with_alpha(to_color32([v[0], v[1], v[2]], &spec), a));
        let stroke = if resp.hovered() {
            Stroke::new(1.5, ui.visuals().widgets.hovered.fg_stroke.color)
        } else {
            Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color)
        };
        p.rect_stroke(rect, 2.0, stroke, StrokeKind::Outside);
    }
    if resp.hovered() && clipboard(ui, v, &spec) {
        resp.mark_changed();
    }
    // Eyedropper: over the swatch, or over last frame's popup.
    let over_pop = ctx
        .data(|d| d.get_temp::<Geom>(id.with("geom")))
        .is_some_and(|g| {
            g.open
                && ctx.pointer_latest_pos().is_some_and(|q| {
                    // Not over the palette: those slots are eyedropper
                    // targets themselves, and whoever runs first takes E.
                    g.popup.contains(q) && !g.swatch_grid.contains(q)
                })
        });
    let (dropping, drop) = (spec.eyedrop)(ui, id, resp.hovered() || over_pop);
    if let Some(c) = drop {
        v[..3].copy_from_slice(&c);
        resp.mark_changed();
    }
    if dropping && ui.is_rect_visible(rect) {
        ui.painter().rect_stroke(
            rect.expand(2.0),
            3.0,
            Stroke::new(1.5, ui.visuals().selection.stroke.color),
            StrokeKind::Outside,
        );
    }
    let resp = resp.on_hover_text(hint);

    let oid = id.with("open");
    let now = ctx.cumulative_frame_nr();
    let prev: Option<Open> = ctx.data(|d| d.get_temp(oid));
    let was_open = prev.is_some_and(|o| o.open && o.seen + 2 >= now);
    let mut open = was_open ^ resp.clicked();
    let mut above = prev.is_some_and(|o| o.above);
    // How far the user has DRAGGED this popup, kept across frames while it
    // stays open. A fresh open starts at zero: a picker belongs to its
    // swatch until a hand says otherwise.
    let mut off = if was_open { prev.map(|o| o.off).unwrap_or_default() } else { Vec2::ZERO };
    let mut resp = resp;

    if open {
        if !was_open {
            ctx.data_mut(|d| {
                d.insert_temp(id.with("state"), State::fresh(*v, &spec));
                d.remove::<String>(id.with("hex"));
            });
        }
        let mut palette = swatches();
        let n_sw = palette.len();
        let style = ctx.global_style();
        let screen = ctx.content_rect();
        let room_above = rect.top() - screen.top() - 2.0 * POP_GAP;
        let room_below = screen.bottom() - rect.bottom() - 2.0 * POP_GAP;
        if !was_open {
            // Hang it where it fits, preferring below — the properties strip
            // lives at the bottom of the window, where below rarely fits.
            let need = Geo::new(scale(), n_sw).outer_max(spec.alpha, &style).y;
            above = need > room_below && room_above > room_below;
        }
        // A popup still hanging off its swatch may only grow into the room
        // on that side. One the user has MOVED answers to the screen.
        let room = if off == Vec2::ZERO {
            vec2(screen.width() - 2.0 * POP_GAP, if above { room_above } else { room_below })
        } else {
            screen.size() - Vec2::splat(2.0 * POP_GAP)
        };
        let fit = fit_scale(room, spec.alpha, n_sw, &style);
        let mut view = View { mode: mode(), scale: scale().min(fit), tabs_last: above };
        let geo = Geo::new(view.scale, n_sw);
        // Placed from the size it WILL have, not the size it had: a resize
        // keeps the swatch-side edge exactly where it was, every frame.
        let size = geo.outer(view.mode, spec.alpha, &style);
        let x = rect.left().min(screen.right() - POP_GAP - size.x).max(screen.left() + POP_GAP);
        let y = if above { rect.top() - POP_GAP - size.y } else { rect.bottom() + POP_GAP };
        let anchor = pos2(x, y);
        let pop = if off == Vec2::ZERO {
            Rect::from_min_size(anchor, size)
        } else {
            let p = fit_on_screen(Rect::from_min_size(anchor + off, size), screen);
            // Whatever the screen took back stays taken: the offset IS
            // where the popup is, or the next frame would drift it again.
            off = p.min - anchor;
            p
        };
        let reach_in = geo.frame_extra(&style).x * 0.5;
        let grips = grip_zones(pop, above, reach_in);
        let up = if above { -1.0 } else { 1.0 };
        let axes = [vec2(1.0, 0.0), vec2(0.0, up), vec2(1.0, up)];
        let rid = id.with("resize");
        let mut resized: Option<Vec2> = None;
        let mut moved: Option<Vec2> = None;
        let (mut grip_hot, mut grip_click, mut live) = (false, false, false);

        let shown = egui::Area::new(id.with("popup"))
            .order(egui::Order::Foreground)
            .fixed_pos(pop.min)
            .constrain(false)
            .show(&ctx, |ui| {
                // The frame margin is a MOVE handle — the chrome a hand
                // reaches for to shift a window, and it costs no layout.
                // First of all, so the grips and every widget inside win
                // where they overlap.
                let mid = id.with("moving");
                for (k, band) in move_zones(pop, geo.margin as f32).into_iter().enumerate() {
                    let mv = ui.interact(band, id.with(("move", k)), Sense::drag());
                    if mv.hovered() || mv.dragged() {
                        ui.ctx().set_cursor_icon(if mv.dragged() {
                            CursorIcon::Grabbing
                        } else {
                            CursorIcon::Grab
                        });
                    }
                    // Snapshot at the press and compute ABSOLUTELY, the
                    // house rule: summed `drag_delta`s would swallow the
                    // pointer's jump onto the handle on the first frame —
                    // egui's pre-press teleport, measured here at 200pt.
                    if mv.drag_started() {
                        ui.data_mut(|d| d.insert_temp(mid, off));
                    }
                    if mv.dragged() {
                        let start: Option<Vec2> = ui.data(|d| d.get_temp(mid));
                        let travel = ui.input(|i| -> Option<Vec2> {
                            Some(i.pointer.interact_pos()? - i.pointer.press_origin()?)
                        });
                        if let (Some(o0), Some(t)) = (start, travel) {
                            moved = Some(o0 + t);
                        }
                    }
                    if mv.drag_stopped() {
                        ui.data_mut(|d| d.remove::<Vec2>(mid));
                    }
                }
                // The grips go in BEFORE the picker: where a zone overlaps a
                // widget, the widget keeps its clicks (CLAUDE.md's drag trap).
                for (k, zone) in grips.into_iter().enumerate() {
                    let r = ui.interact(zone, id.with(("grip", k)), Sense::click_and_drag());
                    if r.hovered() || r.is_pointer_button_down_on() {
                        grip_hot = true;
                        ui.ctx().set_cursor_icon(match k {
                            0 => CursorIcon::ResizeHorizontal,
                            1 => CursorIcon::ResizeVertical,
                            _ if above => CursorIcon::ResizeNeSw,
                            _ => CursorIcon::ResizeNwSe,
                        });
                    }
                    grip_click |= r.clicked();
                    // A quick hand's double-click lands as a TRIPLE when a
                    // click came just before it (the one that opened the
                    // popup, say); both mean "reset".
                    if r.double_clicked() || r.triple_clicked() {
                        resized = Some(scale_default());
                    }
                    if r.drag_started() {
                        let start = Resize { s0: view.scale, axis: axes[k] };
                        ui.data_mut(|d| d.insert_temp(rid, start));
                    }
                    if r.dragged() {
                        live = true;
                        let start: Option<Resize> = ui.data(|d| d.get_temp(rid));
                        let travel = ui.input(|i| -> Option<Vec2> {
                            Some(i.pointer.interact_pos()? - i.pointer.press_origin()?)
                        });
                        if let (Some(start), Some(t)) = (start, travel) {
                            let s = grip_scale(start.s0, t, start.axis, view.mode, spec.alpha, n_sw);
                            resized = Some(clamp_scale(s, fit));
                        }
                    }
                    if r.drag_stopped() {
                        ui.data_mut(|d| d.remove::<Resize>(rid));
                    }
                    r.on_hover_text(
                        "drag to resize · pinch or ⌘-scroll over the picker · \
                         double-click for the default size",
                    );
                }
                let changed = egui::Frame::popup(ui.style())
                    .inner_margin(egui::Margin::same(geo.margin))
                    .show(ui, |ui| {
                        let mut ch = picker_ui(ui, id, v, spec, &mut view, &mut palette, &mut geom);
                        if ui.ui_contains_pointer() {
                            ch |= clipboard(ui, v, &spec);
                        }
                        ch
                    })
                    .inner;
                paint_grip(ui, pop, above, geo.margin as f32, grip_hot);
                changed
            });
        geom.popup = shown.response.rect;
        geom.grips = grips;
        if shown.inner {
            resp.mark_changed();
        }
        if ctx.pointer_latest_pos().is_some_and(|q| pop.expand(GRIP_OUT).contains(q)) {
            (spec.claim)(&ctx);
        }
        if ctx.rect_contains_pointer(shown.response.layer_id, pop) {
            let z = ctx.input(|i| i.zoom_delta());
            if (z - 1.0).abs() > 1e-4 {
                live = true;
                resized = Some(clamp_scale(view.scale * z, fit));
            }
        }
        if let Some(s) = resized {
            if (s - scale()).length() > 1e-5 {
                resize_to(s);
            }
        }
        if let Some(o) = moved {
            off = o;
        }
        if live {
            GESTURE_MS.store(now_ms(), Relaxed);
        }
        if SCALE_DIRTY.load(Relaxed) {
            // Wake the host once the resize has settled, so it can persist.
            ctx.request_repaint_after(web_time::Duration::from_millis(SETTLE_MS + 20));
        }
        set_mode(view.mode);
        if palette != swatches() {
            store_swatches(palette);
        }
        let esc = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        if esc
            || (!dropping
                && !geom.swatch_dropping
                && !resp.clicked()
                && !grip_click
                && shown.response.clicked_elsewhere())
        {
            open = false;
        }
    }

    if open != was_open || open {
        ctx.data_mut(|d| d.insert_temp(oid, Open { open, seen: now, above, off }));
    }
    if open {
        ctx.data_mut(|d| d.insert_temp(open_frame_id(), now));
    }
    if was_open && !open {
        ctx.data_mut(|d| {
            d.remove::<State>(id.with("state"));
            d.remove::<String>(id.with("hex"));
        });
    }
    geom.open = open;
    ctx.data_mut(|d| d.insert_temp(id.with("geom"), geom));
    resp
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn close3(a: [f32; 3], b: [f32; 3], eps: f32) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < eps)
    }

    #[test]
    fn hsv_hsl_round_trip() {
        for r in 0..=4 {
            for g in 0..=4 {
                for b in 0..=4 {
                    let c = [r as f32 / 4.0, g as f32 / 4.0, b as f32 / 4.0];
                    assert!(close3(hsv_to_rgb(rgb_to_hsv(c)), c, 1e-4), "hsv {c:?}");
                    assert!(close3(hsl_to_rgb(rgb_to_hsl(c)), c, 1e-4), "hsl {c:?}");
                }
            }
        }
    }

    #[test]
    fn hue_landmarks() {
        assert!(rgb_to_hsv([1.0, 0.0, 0.0])[0].abs() < 1e-3);
        assert!((rgb_to_hsv([0.0, 1.0, 0.0])[0] - 120.0).abs() < 1e-3);
        assert!((rgb_to_hsv([0.0, 0.0, 1.0])[0] - 240.0).abs() < 1e-3);
        let g = rgb_to_hsl([0.5, 0.5, 0.5]);
        assert!(g[1].abs() < 1e-5 && (g[2] - 0.5).abs() < 1e-5);
    }

    #[test]
    fn oklch_round_trip() {
        let spec = PickSpec::rgb();
        for r in 0..=4 {
            for g in 0..=4 {
                for b in 0..=4 {
                    let c = [r as f32 / 4.0, g as f32 / 4.0, b as f32 / 4.0];
                    let back = lch_to_value(value_to_lch(c, &spec), &spec);
                    assert!(close3(back, c, 2e-3), "{c:?} -> {back:?}");
                }
            }
        }
    }

    /// Published anchors: white is L 1 with no chroma, and sRGB red is
    /// oklch(0.628 0.258 29.2) — the reference every CSS oklch() agrees on.
    #[test]
    fn oklab_anchors() {
        let w = lin709_to_oklab([1.0; 3]);
        assert!((w[0] - 1.0).abs() < 1e-3 && w[1].abs() < 1e-4 && w[2].abs() < 1e-4);
        assert!(lin709_to_oklab([0.0; 3])[0].abs() < 1e-4);
        let red = value_to_lch([1.0, 0.0, 0.0], &PickSpec::rgb());
        assert!(close3(red, [0.628, 0.2577, 29.23], 0.01), "{red:?}");
    }

    #[test]
    fn chroma_edge() {
        let spec = PickSpec::rgb();
        for (l, h) in [(0.3, 20.0), (0.55, 140.0), (0.75, 260.0), (0.9, 100.0)] {
            let cm = max_chroma(l, h, &spec);
            assert!(cm > 0.0, "no chroma at L={l} h={h}");
            assert!(in_gamut(lch_to_value([l, cm, h], &spec)));
            assert!(!in_gamut(lch_to_value([l, cm + 0.01, h], &spec)));
            let v = lch_to_value(clamp_lch([l, 0.5, h], &spec), &spec);
            assert!(in_gamut(v), "clamped {v:?}");
        }
        assert_eq!(max_chroma(0.0, 90.0, &spec), 0.0);
        assert_eq!(max_chroma(1.0, 90.0, &spec), 0.0);
    }

    /// The C lane reaches the gamut edge everywhere: a brute-force sweep
    /// never finds more chroma than the lane's range.
    #[test]
    fn lane_reaches_every_edge() {
        let swap = [[0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
        for spec in [PickSpec::rgb(), PickSpec::rgb().with_display(swap, swap)] {
            let lane = lane_chroma_max(&spec);
            let mut most: f32 = 0.0;
            for hi in 0..72 {
                for li in 1..40 {
                    most = most.max(max_chroma(li as f32 / 40.0, hi as f32 * 5.0, &spec));
                }
            }
            assert!(most <= lane, "sweep found C {most} past the lane's {lane}");
            assert!(lane < most * 1.1, "lane {lane} is far wider than the gamut {most}");
        }
    }

    #[test]
    fn hex_round_trip() {
        let spec = PickSpec::rgba();
        let v = hex_parse("#3fa64b", 1.0, &spec).unwrap();
        assert_eq!(hex_of(v, &spec), "#3fa64b");
        let va = hex_parse("80ff0040", 1.0, &spec).unwrap();
        assert!((va[3] - 64.0 / 255.0).abs() < 1e-3);
        assert_eq!(hex_of(va, &spec), "#80ff0040");
        let short = hex_parse("#fa3", 0.5, &spec).unwrap();
        assert_eq!(hex_of([short[0], short[1], short[2], 1.0], &spec), "#ffaa33");
        assert!((short[3] - 0.5).abs() < 1e-6, "no alpha digit keeps the alpha");
        assert!(hex_parse("#12345", 1.0, &spec).is_none());
        assert!(hex_parse("nope", 1.0, &spec).is_none());
    }

    /// Hex is EXACT sRGB: #808080 is 0.2159 linear everywhere else, and a
    /// 2.2 would say 0.2176. And since previews share the curve, bytes
    /// survive the round trip a display-referred host makes (the theme
    /// window decodes a byte with `dec1` and re-encodes with `enc1`).
    #[test]
    fn hex_is_exact_srgb() {
        let v = hex_parse("#808080", 1.0, &PickSpec::rgb()).unwrap();
        assert!((v[0] - 0.215_861).abs() < 1e-5, "{}", v[0]);
        for byte in 0..=255u8 {
            let lin = dec1(byte as f32 / 255.0);
            let back = (enc1(lin) * 255.0).round() as u8;
            assert_eq!(back, byte, "byte {byte} drifted through dec1/enc1");
            let hex = hex_of([lin, lin, lin, 1.0], &PickSpec::rgb());
            assert_eq!(hex, format!("#{byte:02x}{byte:02x}{byte:02x}"));
        }
    }

    #[test]
    fn clipboard_formats() {
        let spec = PickSpec::rgba();
        let v = [0.2140, 0.5, 0.8, 0.25];
        assert_eq!(floats_parse(&floats_of(v, true), 1.0), Some(v));
        assert_eq!(floats_parse("0.1 0.2 0.3", 0.7), Some([0.1, 0.2, 0.3, 0.7]));
        assert_eq!(floats_parse("(1, 0, 2)", 1.0), Some([1.0, 0.0, 1.0, 1.0]));
        assert_eq!(floats_parse("[1, 2]", 1.0), None);
        assert_eq!(floats_parse("[nan, 0, 0]", 1.0), None);
        // Hex comes through the same door.
        let h = paste_parse("#ffffff", 0.5, &spec).unwrap();
        assert!(close3([h[0], h[1], h[2]], [1.0; 3], 1e-5) && h[3] == 0.5);
        assert!(paste_parse("hello", 1.0, &spec).is_none());
    }

    /// The ergonomic contract: a coordinate survives a trip through its
    /// own degenerate point, in the view being edited AND in the others.
    #[test]
    fn degenerate_coords_survive() {
        let spec = PickSpec::rgb();
        let start = clamp01(dec3(hsl_to_rgb([210.0, 0.8, 0.5])));
        let mut st = State::fresh([start[0], start[1], start[2], 1.0], &spec);
        assert!((st.hsl[0] - 210.0).abs() < 1.0);
        st.hsl[1] = 0.0;
        let grey = clamp01(dec3(hsl_to_rgb(st.hsl)));
        st.follow([grey[0], grey[1], grey[2], 1.0], Some(Mode::Hsl), &spec);
        assert!((st.hsv[0] - 210.0).abs() < 1.0, "wheel hue lost: {}", st.hsv[0]);
        st.hsl[1] = 0.8;
        let back = clamp01(dec3(hsl_to_rgb(st.hsl)));
        let h = rgb_to_hsl(enc3(back))[0];
        assert!((h - 210.0).abs() < 1.0, "hue after the round trip: {h}");

        // Black keeps the wheel's saturation as well as its hue.
        let mut st = State::fresh([0.1, 0.3, 0.8, 1.0], &spec);
        let (h0, s0) = (st.hsv[0], st.hsv[1]);
        st.follow([0.0, 0.0, 0.0, 1.0], None, &spec);
        assert!((st.hsv[0] - h0).abs() < 1e-3 && (st.hsv[1] - s0).abs() < 1e-3);

        // Chroma to zero in lch keeps the hue in every view.
        let mut st = State::fresh([0.1, 0.3, 0.8, 1.0], &spec);
        let h0 = st.lch[2];
        st.follow([0.5, 0.5, 0.5, 1.0], None, &spec);
        assert!((st.lch[2] - h0).abs() < 1e-3, "lch hue lost");
    }

    #[test]
    fn wheel_matches_grade_wheels() {
        assert!((hue_dir(0.0) - vec2(0.0, -1.0)).length() < 1e-4);
        assert!((hue_dir(120.0) - vec2(-0.866_025_4, 0.5)).length() < 1e-4);
        assert!((hue_dir(240.0) - vec2(0.866_025_4, 0.5)).length() < 1e-4);
        for h in [0.0f32, 45.0, 120.0, 200.0, 300.0] {
            let p = Pos2::ZERO + hue_dir(h) * 10.0;
            let back = hue_at(Pos2::ZERO, p);
            let err = (back - h).rem_euclid(360.0);
            assert!(err.min(360.0 - err) < 1e-2, "{h} -> {back}");
        }
    }

    /// A display matrix routes hex and lch through Rec.709 and back.
    #[test]
    fn display_matrix_round_trips() {
        let swap = [[0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
        let spec = PickSpec::rgb().with_display(swap, swap);
        assert_eq!(hex_of([1.0, 0.0, 0.0, 1.0], &spec), "#00ff00");
        let v = hex_parse("#00ff00", 1.0, &spec).unwrap();
        assert!(close3([v[0], v[1], v[2]], [1.0, 0.0, 0.0], 1e-4));
        let lch = value_to_lch([1.0, 0.0, 0.0], &spec);
        assert!(close3(lch, value_to_lch([0.0, 1.0, 0.0], &PickSpec::rgb()), 1e-5));
    }

    /// Every non-ASCII character in a string literal here has a glyph in
    /// egui's default proportional font — a missing one draws as a box
    /// (`keymap::tests::every_drawn_character_has_a_glyph` is the original).
    #[test]
    fn every_drawn_character_has_a_glyph() {
        let ctx = egui::Context::default();
        crate::snapshot::discard(ctx.run_ui(Default::default(), |_| {}));
        let prop = egui::FontId::proportional(12.0);
        let src = include_str!("colorpicker.rs");
        let (mut in_str, mut in_comment, mut escaped) = (false, false, false);
        let mut prev = '\0';
        let mut missing = Vec::new();
        for c in src.chars() {
            if in_comment {
                in_comment = c != '\n';
            } else if in_str {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == '\u{22}' {
                    in_str = false;
                } else if !c.is_ascii() && !ctx.fonts_mut(|f| f.has_glyph(&prop, c)) {
                    missing.push(c);
                }
            } else if c == '/' && prev == '/' {
                in_comment = true;
            } else if c == '\u{22}' {
                in_str = true;
            }
            prev = c;
        }
        assert!(missing.is_empty(), "no glyph for {missing:?}");
    }

    // --- Size ---------------------------------------------------------------

    /// A uniform scale, for the tests that do not care about the axes.
    fn sc(s: f32) -> Vec2 {
        Vec2::splat(s)
    }

    /// What you aim at grows with the scale; type and number fields do not.
    /// Scale 1 is the original picker, 232 wide.
    #[test]
    fn aimed_geometry_scales_type_does_not() {
        let (a, b) = (Geo::new(sc(1.0), 0), Geo::new(sc(3.0), 0));
        assert_eq!(a.width, 232.0);
        for (x, y, what) in [
            (a.lane_w, b.lane_w, "lane"),
            (a.track_h, b.track_h, "track"),
            (a.disc_d, b.disc_d, "disc"),
            (a.chip.x, b.chip.x, "chip"),
            (a.gap.y, b.gap.y, "gap"),
        ] {
            assert!((y - 3.0 * x).abs() < 1e-4, "{what}: {x} at 1, {y} at 3");
        }
        // The fixed parts are exactly what the width does not triple.
        let fixed = LAB_W + NUM_W;
        assert!(((b.width - fixed) - 3.0 * (a.width - fixed)).abs() < 1e-3);
        assert!((track_len(sc(3.0)) - (b.lane_w - 2.0 * b.knob_r)).abs() < 1e-4);
        // A palette row fills the width, and the + slot needs a row too.
        assert!((12.0 * a.slot + 11.0 * a.slot_gap - a.width).abs() < 1e-3);
        for (n, rows) in [(0, 1), (11, 1), (12, 2), (35, 3), (47, 4), (48, 4)] {
            assert_eq!(Geo::new(sc(1.0), n).swatch_rows, rows, "{n} swatches");
        }
    }

    /// The two axes are INDEPENDENT: width runs the lanes' length, height
    /// their thickness, and neither reaches into the other. That is what a
    /// popup resized by one edge has to do — stretch the sliders it was
    /// dragged along, and leave the rest where it was.
    #[test]
    fn each_axis_scales_its_own_geometry() {
        let one = Geo::new(sc(1.0), SWATCH_ROW);
        let wide = Geo::new(vec2(3.0, 1.0), SWATCH_ROW);
        let tall = Geo::new(vec2(1.0, 3.0), SWATCH_ROW);
        assert!((wide.lane_w - 3.0 * one.lane_w).abs() < 1e-4, "width must stretch the lanes");
        assert_eq!(wide.track_h, one.track_h, "width must not fatten a track");
        assert!((tall.track_h - 3.0 * one.track_h).abs() < 1e-4, "height must fatten the tracks");
        assert_eq!(tall.lane_w, one.lane_w, "height must not stretch a lane");
        // The popup's own outer width answers to x alone — what lets the
        // fit solve one axis and then the other.
        let style = egui::Style::default();
        let w = |g: &Geo| g.outer_max(true, &style).x;
        assert!((w(&tall) - w(&one)).abs() < 1e-3, "height changed the width");
        assert!(w(&wide) > w(&one) + 50.0, "width did not change the width");
        // A wheel stays round whatever the axes do.
        assert_eq!(wide.disc_d, one.disc_d);
        assert_eq!(tall.disc_d, one.disc_d);
    }

    /// `Geo::size` is the layout's own arithmetic, measured off real egui
    /// passes: every mode, with and without alpha, tabs at either edge, an
    /// empty palette, a full row and a full palette, at several scales,
    /// frame included. The screen fit, the placement and the grips all
    /// trust this number before anything is drawn.
    #[test]
    fn size_is_the_layout() {
        let ctx = egui::Context::default();
        for scale in [sc(1.0), sc(1.7), sc(3.0), sc(4.5), vec2(4.0, 1.4), vec2(1.2, 3.6)] {
            for swatches in [0, SWATCH_ROW, SWATCH_MAX] {
                let geo = Geo::new(scale, swatches);
                for alpha in [false, true] {
                    let spec = if alpha { PickSpec::rgba() } else { PickSpec::rgb() };
                    for mode in Mode::ALL {
                        for tabs_last in [false, true] {
                            let input = egui::RawInput {
                                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(4000.0, 4000.0))),
                                ..Default::default()
                            };
                            let (mut content, mut outer) = (Vec2::ZERO, Vec2::ZERO);
                            crate::snapshot::discard(ctx.run_ui(input, |ui| {
                                let mut v = [0.2, 0.45, 0.8, 0.6];
                                let mut view = View { mode, scale, tabs_last };
                                let mut palette = vec![[0.5, 0.25, 0.1]; swatches];
                                let mut geom = Geom::default();
                                let fr = egui::Frame::popup(ui.style())
                                    .inner_margin(egui::Margin::same(geo.margin))
                                    .show(ui, |ui| {
                                        picker_ui(
                                            ui,
                                            Id::new("size"),
                                            &mut v,
                                            spec,
                                            &mut view,
                                            &mut palette,
                                            &mut geom,
                                        );
                                        ui.min_rect().size()
                                    });
                                (content, outer) = (fr.inner, fr.response.rect.size());
                            }));
                            let what = format!(
                                "{mode:?} at {scale:?}, {swatches} swatches, alpha {alpha}, tabs last {tabs_last}"
                            );
                            let want = geo.size(mode, alpha);
                            assert!((content - want).length() < 0.5, "{what}: laid out {content:?}, sized {want:?}");
                            let want = geo.outer(mode, alpha, &ctx.global_style());
                            assert!((outer - want).length() < 0.5, "{what}: framed {outer:?}, sized {want:?}");
                        }
                    }
                }
            }
        }
    }

    /// A grip keeps the popup's edge under the pointer — the frame's edge,
    /// margin included, to within the margin's whole-point rounding: right
    /// edge, the top edge of a popup hanging above its swatch, the corner.
    #[test]
    fn grips_track_the_pointer() {
        let style = egui::Style::default();
        for n in [0, 20] {
            for mode in Mode::ALL {
                for s0 in [sc(2.0), sc(3.0), vec2(3.5, 1.5)] {
                    let outer = |s: Vec2| Geo::new(s, n).outer(mode, true, &style);
                    let grip = |t: Vec2, axis: Vec2| grip_scale(s0, t, axis, mode, true, n);
                    // The right edge moves the WIDTH, and only the width —
                    // the sideways travel is the whole gesture.
                    let s = grip(vec2(90.0, 7.0), vec2(1.0, 0.0));
                    let moved = outer(s).x - outer(s0).x;
                    assert!((moved - 90.0).abs() < 2.5, "{mode:?} at {s0:?}: right edge moved {moved}");
                    assert_eq!(s.y, s0.y, "{mode:?} at {s0:?}: the right edge changed the height");
                    let s = grip(vec2(-172.0, 0.0), vec2(1.0, 0.0));
                    let moved = outer(s).x - outer(s0).x;
                    assert!((moved + 172.0).abs() < 2.5, "{mode:?} at {s0:?}: shrink moved {moved}");
                    // The free edge moves the HEIGHT, and only the height.
                    let s = grip(vec2(5.0, -60.0), vec2(0.0, -1.0));
                    let moved = outer(s).y - outer(s0).y;
                    assert!((moved - 60.0).abs() < 2.5, "{mode:?} at {s0:?}: top edge moved {moved}");
                    assert_eq!(s.x, s0.x, "{mode:?} at {s0:?}: the free edge changed the width");
                    // The corner takes both at once.
                    let per = vec2(
                        outer(s0 + vec2(1.0, 0.0)).x - outer(s0).x,
                        outer(s0 + vec2(0.0, 1.0)).y - outer(s0).y,
                    );
                    let s = grip(vec2(per.x, -per.y) * 0.5, vec2(1.0, -1.0));
                    let want = s0 + Vec2::splat(0.5);
                    assert!((s - want).length() < 1e-3, "{mode:?} at {s0:?}: corner reached {s:?}");
                }
            }
        }
    }

    /// The fit keeps the whole popup — tallest mode, frame, palette and all —
    /// inside its room, and uses the room.
    #[test]
    fn fit_fills_the_room() {
        let style = egui::Style::default();
        for n in [0, SWATCH_MAX] {
            for room in [vec2(1600.0, 700.0), vec2(700.0, 2000.0), vec2(500.0, 360.0)] {
                let s = fit_scale(room, true, n, &style);
                let o = Geo::new(s, n).outer_max(true, &style);
                assert!(o.x <= room.x && o.y <= room.y, "{room:?}: scale {s:?} is {o:?}");
                // Each axis uses its own room: a hair more overflows it.
                if s.x < SCALE_MAX - 1e-3 {
                    let bigger = Geo::new(vec2(s.x + 0.02, SCALE_MIN), n).outer_max(true, &style);
                    assert!(
                        bigger.x > room.x || bigger.y > room.y,
                        "{room:?}: width {s:?} leaves room"
                    );
                }
                if s.y < SCALE_MAX - 1e-3 {
                    let bigger = Geo::new(s + vec2(0.0, 0.02), n).outer_max(true, &style);
                    assert!(bigger.y > room.y, "{room:?}: height {s:?} leaves room");
                }
            }
            assert_eq!(fit_scale(vec2(100.0, 100.0), true, n, &style), sc(SCALE_MIN));
        }
        assert_eq!(fit_scale(vec2(9000.0, 9000.0), true, 0, &style), sc(SCALE_MAX));
    }

    /// A resize reaches the host once, and only after the gesture settles;
    /// the host seeding its saved size is not a resize at all.
    #[test]
    fn a_resize_settles_once() {
        set_scale(sc(2.0));
        assert!(settled_at(u64::MAX).is_none(), "seeding counted as an edit");
        resize_to(vec2(2.5, 1.5));
        let at = GESTURE_MS.load(Relaxed);
        assert!(settled_at(at + SETTLE_MS / 2).is_none(), "persisted mid-gesture");
        assert_eq!(settled_at(at + SETTLE_MS + 1), Some(vec2(2.5, 1.5)));
        assert!(settled_at(at + SETTLE_MS + 2).is_none(), "persisted twice");
        set_scale(scale_default());
    }

    // --- Palette --------------------------------------------------------------

    /// A swatch is kept in linear Rec.709, so it is ONE colour in every comp:
    /// stored and used in the same working space it comes back as the value
    /// it was, and used in another it lands on the same Rec.709 colour.
    #[test]
    fn a_swatch_is_one_colour_in_every_working_space() {
        let swap = [[0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
        let odd = PickSpec::rgb().with_display(swap, swap);
        let c = to_palette([0.9, 0.2, 0.05, 1.0], &odd);
        assert!(close3(from_palette(c, &odd), [0.9, 0.2, 0.05], 1e-6));
        // The swap put working red in Rec.709's green; a Rec.709 comp gets
        // the Rec.709 colour itself.
        assert!(close3(from_palette(c, &PickSpec::rgb()), [0.2, 0.9, 0.05], 1e-6));
        // Wider than the comp can make: clamped into the stored range.
        assert!(close3(from_palette([1.4, -0.2, 0.5], &PickSpec::rgb()), [1.0, 0.0, 0.5], 1e-6));
    }

    /// The host hears about the palette once per edit, never about its own
    /// seeding, and junk from a hand-edited prefs file never gets in.
    #[test]
    fn palette_edits_reach_the_host_once() {
        set_swatches(vec![[0.1, 0.2, 0.3], [f32::NAN, 0.0, 0.0]]);
        assert_eq!(swatches(), vec![[0.1, 0.2, 0.3]]);
        assert!(take_changed_swatches().is_none(), "seeding counted as an edit");
        store_swatches(vec![[0.4, 0.5, 0.6]]);
        assert_eq!(take_changed_swatches(), Some(vec![[0.4, 0.5, 0.6]]));
        assert!(take_changed_swatches().is_none(), "reported twice");
        set_swatches(vec![[0.0; 3]; SWATCH_MAX + 5]);
        assert_eq!(swatches().len(), SWATCH_MAX);
        set_swatches(Vec::new());
    }

    /// One egui pass of the palette test's picker, with `events` and `mods`.
    fn palette_pass(
        ctx: &egui::Context,
        events: Vec<Event>,
        mods: egui::Modifiers,
        v: &mut [f32; 4],
        palette: &mut Vec<[f32; 3]>,
        geom: &mut Geom,
    ) {
        let mut all = vec![Event::ModifiersChanged(mods)];
        all.extend(events);
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900.0, 900.0))),
            events: all,
            ..Default::default()
        };
        crate::snapshot::discard(ctx.run_ui(input, |ui| {
            let mut view = View { mode: Mode::Rgb, scale: sc(1.0), tabs_last: false };
            picker_ui(ui, Id::new("palette"), v, PickSpec::rgba(), &mut view, palette, geom);
        }));
    }

    /// A real click (press, release, settle) on palette slot `i`.
    fn palette_click(
        ctx: &egui::Context,
        i: usize,
        mods: egui::Modifiers,
        v: &mut [f32; 4],
        palette: &mut Vec<[f32; 3]>,
        geom: &mut Geom,
    ) {
        let at = geom.swatch_rect(i).center();
        let button = |pressed| Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: mods,
        };
        palette_pass(ctx, vec![Event::PointerMoved(at), button(true)], mods, v, palette, geom);
        palette_pass(ctx, vec![button(false)], mods, v, palette, geom);
        palette_pass(ctx, vec![], egui::Modifiers::NONE, v, palette, geom);
    }

    /// A viewer sample lands STRAIGHT in a palette slot: E over a slot
    /// arms the eyedropper for it, and the pixel that comes back fills
    /// that slot — the + slot as a new swatch, a filled one over the top —
    /// without passing through the picker's value. Driven through the real
    /// `eyedrop` protocol, not a stub, because the wiring is the thing
    /// worth pinning: the slot's own target id, the sample's conversion
    /// into the palette's space, and the popup NOT treating the viewer
    /// click as "clicked elsewhere, close".
    #[test]
    fn a_sample_lands_in_the_palette_slot_it_was_armed_from() {
        let spec = PickSpec::rgba().with_eyedrop(crate::eyedrop::pick_hook);
        let pass = |ctx: &egui::Context,
                    events: Vec<Event>,
                    v: &mut [f32; 4],
                    palette: &mut Vec<[f32; 3]>,
                    geom: &mut Geom| {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900.0, 900.0))),
                events,
                ..Default::default()
            };
            crate::snapshot::discard(ctx.run_ui(input, |ui| {
                let mut view = View { mode: Mode::Rgb, scale: sc(1.0), tabs_last: false };
                picker_ui(ui, Id::new("palette"), v, spec, &mut view, palette, geom);
            }));
        };
        let arm_over = |ctx: &egui::Context,
                        slot: usize,
                        v: &mut [f32; 4],
                        palette: &mut Vec<[f32; 3]>,
                        geom: &mut Geom| {
            let at = geom.swatch_rect(slot).center();
            pass(
                ctx,
                vec![
                    Event::PointerMoved(at),
                    Event::Key {
                        key: crate::eyedrop::KEY,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                v,
                palette,
                geom,
            );
        };

        let ctx = egui::Context::default();
        let mut v = [0.5f32, 0.5, 0.5, 1.0];
        let mut palette = vec![[0.1f32, 0.2, 0.3]];
        let mut geom = Geom::default();
        // A pass to place the grid, so the slots have rects to aim at.
        pass(&ctx, vec![], &mut v, &mut palette, &mut geom);

        // The + slot (index 1 here) takes a sample as a NEW swatch.
        arm_over(&ctx, 1, &mut v, &mut palette, &mut geom);
        assert!(crate::eyedrop::armed(&ctx), "E over the + slot did not arm the eyedropper");
        assert!(geom.swatch_dropping, "the popup was not told a slot is armed");
        let sampled = [0.8f32, 0.4, 0.2];
        crate::eyedrop::deliver(&ctx, [sampled[0], sampled[1], sampled[2], 1.0]);
        pass(&ctx, vec![], &mut v, &mut palette, &mut geom);
        assert_eq!(
            palette,
            vec![[0.1, 0.2, 0.3], sampled],
            "the + slot should have kept the sampled colour"
        );
        assert_eq!(v, [0.5, 0.5, 0.5, 1.0], "the value must not move — the sample went to the slot");

        // A filled slot takes one over the top.
        arm_over(&ctx, 0, &mut v, &mut palette, &mut geom);
        let second = [0.05f32, 0.6, 0.9];
        crate::eyedrop::deliver(&ctx, [second[0], second[1], second[2], 1.0]);
        pass(&ctx, vec![], &mut v, &mut palette, &mut geom);
        assert_eq!(palette, vec![second, sampled], "slot 0 should hold the new sample");
    }

    /// The palette under real clicks: + keeps the current colour, a click
    /// uses a swatch (colour only — the alpha stays), shift-click replaces a
    /// swatch with the current colour, alt-click removes it, and a full
    /// palette has no + slot.
    #[test]
    fn palette_clicks() {
        let ctx = egui::Context::default();
        let (mut v, mut palette, mut geom) = ([0.2, 0.45, 0.8, 0.6], Vec::new(), Geom::default());
        palette_pass(&ctx, vec![], egui::Modifiers::NONE, &mut v, &mut palette, &mut geom);
        assert_eq!(geom.swatch_count, 0);

        palette_click(&ctx, 0, egui::Modifiers::NONE, &mut v, &mut palette, &mut geom);
        assert_eq!(palette, vec![[0.2, 0.45, 0.8]], "the + slot did not keep the colour");

        v = [0.9, 0.1, 0.1, 0.3];
        palette_click(&ctx, 0, egui::Modifiers::NONE, &mut v, &mut palette, &mut geom);
        assert_eq!(v, [0.2, 0.45, 0.8, 0.3], "using a swatch: its colour, the value's own alpha");

        v = [0.1, 0.9, 0.1, 0.3];
        palette_click(&ctx, 0, egui::Modifiers::SHIFT, &mut v, &mut palette, &mut geom);
        assert_eq!(palette, vec![[0.1, 0.9, 0.1]], "shift-click did not replace the swatch");
        assert_eq!(v, [0.1, 0.9, 0.1, 0.3], "shift-click must not change the value");

        palette_click(&ctx, 0, egui::Modifiers::ALT, &mut v, &mut palette, &mut geom);
        assert!(palette.is_empty(), "alt-click did not remove the swatch");

        palette = vec![[0.3, 0.3, 0.3]; SWATCH_MAX - 1];
        palette_pass(&ctx, vec![], egui::Modifiers::NONE, &mut v, &mut palette, &mut geom);
        palette_click(&ctx, SWATCH_MAX - 1, egui::Modifiers::NONE, &mut v, &mut palette, &mut geom);
        assert_eq!(palette.len(), SWATCH_MAX, "the last + slot did not keep the colour");
        assert_eq!(palette[SWATCH_MAX - 1], [0.1, 0.9, 0.1]);
        assert_eq!(slot_count(palette.len()), SWATCH_MAX, "a full palette still offers +");
    }

    // --- Pictures: the popup body drawn headlessly, saved to
    // target/cpick_*.png so the layout can be LOOKED at, and pinned. ---

    /// A few swatches, so the pictures show the palette too.
    const SOME_SWATCHES: [[f32; 3]; 5] =
        [[0.8, 0.05, 0.05], [0.9, 0.6, 0.1], [0.1, 0.5, 0.2], [0.05, 0.2, 0.9], [0.18, 0.18, 0.18]];

    fn shoot(mode: Mode, v: [f32; 4], scale: Vec2) -> (crate::snapshot::Shot, Geom) {
        let mut val = v;
        let mut geom = Geom::default();
        let mut palette = SOME_SWATCHES.to_vec();
        let size = Geo::new(scale, palette.len()).size(mode, true) + vec2(16.0, 16.0);
        let shot = crate::snapshot::shoot(size.x, size.y, |ui| {
            let mut view = View { mode, scale, tabs_last: false };
            picker_ui(ui, Id::new("shot"), &mut val, PickSpec::rgba(), &mut view, &mut palette, &mut geom);
        });
        (shot, geom)
    }

    fn sum(c: [u8; 3]) -> u32 {
        c.iter().map(|&x| x as u32).sum()
    }

    #[test]
    fn snapshot_wheel() {
        let (shot, geom) = shoot(Mode::Wheel, [1.0, 0.1, 0.1, 1.0], scale_default());
        shot.save("cpick_wheel");
        assert!((geom.disc.width() - DISC_D * SCALE_DEFAULT).abs() < 0.5, "{:?}", geom.disc);
        let c = geom.disc.center();
        let top = shot.at(c.x, geom.disc.min.y + 24.0);
        assert!(top[0] > top[1] + 40 && top[0] > top[2] + 40, "top should be red: {top:?}");
        let ll = shot.at(geom.disc.min.x + 42.0, c.y + geom.disc.height() * 0.2);
        assert!(ll[1] > ll[2] && ll[1] > ll[0], "lower-left should be green: {ll:?}");
        let s = geom.vstrip.center();
        let (hi, lo) = (shot.at(s.x, geom.vstrip.min.y + 50.0), shot.at(s.x, geom.vstrip.max.y - 50.0));
        assert!(sum(hi) > sum(lo) + 60, "strip should run bright to dark: {hi:?} {lo:?}");
        // The palette paints its colours: red first, blue fourth.
        let (red, blue) = (geom.swatch_rect(0).center(), geom.swatch_rect(3).center());
        let (red, blue) = (shot.at(red.x, red.y), shot.at(blue.x, blue.y));
        assert!(red[0] > red[1] + 80 && red[0] > red[2] + 80, "first swatch not red: {red:?}");
        assert!(blue[2] > blue[0] + 80 && blue[2] > blue[1] + 40, "fourth swatch not blue: {blue:?}");
    }

    #[test]
    fn snapshot_lanes() {
        for scale in [sc(SCALE_MIN), sc(SCALE_DEFAULT), vec2(4.0, 1.6)] {
            for (mode, name) in [(Mode::Rgb, "rgb"), (Mode::Hsl, "hsl"), (Mode::Lch, "lch")] {
                let (shot, geom) = shoot(mode, [0.2, 0.45, 0.8, 0.6], scale);
                let name = format!("cpick_{name}_x{}x{}", scale.x, scale.y);
                shot.save(&name);
                for (i, r) in geom.lanes.iter().enumerate() {
                    assert!((r.width() - track_len(scale)).abs() < 0.5, "{name} lane {i}: {r:?}");
                    let (a, b) = (shot.at(r.min.x + 3.0, r.center().y), shot.at(r.max.x - 3.0, r.center().y));
                    assert_ne!(a, b, "{name} lane {i} painted flat");
                }
                assert!((geom.alpha_lane.width() - track_len(scale)).abs() < 0.5, "{name} alpha lane");
            }
        }
    }

    /// At zero chroma the hue lanes still show hues (the aiming floor): red
    /// near the start (clear of the knob parked at hue 0), then green, then
    /// blue along the lane.
    #[test]
    fn hue_lane_never_flat() {
        for (mode, name, hue_lane) in [(Mode::Hsl, "cpick_hsl_grey", 0), (Mode::Lch, "cpick_lch_grey", 2)] {
            let (shot, geom) = shoot(mode, [0.18, 0.18, 0.18, 1.0], scale_default());
            shot.save(name);
            let r = geom.lanes[hue_lane];
            let at = |t: f32| shot.at(r.min.x + t * r.width(), r.center().y);
            let (red, green, blue) = (at(0.06), at(0.36), at(0.72));
            assert!(red[0] > red[2] + 20, "{name}: start of hue lane not red: {red:?}");
            assert!(green[1] > green[0] && green[1] > green[2], "{name}: no green: {green:?}");
            assert!(blue[2] > blue[0] && blue[2] > blue[1], "{name}: no blue: {blue:?}");
        }
    }
}
