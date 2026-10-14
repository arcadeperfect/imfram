//! The storage and derivation engine the [`slots!`](crate::slots) expansion
//! calls: slot definitions over plain indices, link resolution, the live
//! atomic table, prefs as hex, egui's `Visuals`. Nothing here knows an
//! app's `Slot` enum; the macro turns `Slot` into `usize` at the boundary.

use egui::Color32;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{RwLock, RwLockWriteGuard};

/// How a slot gets its colour when nothing overrides it. Parents are slot
/// indices into the same table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Base {
    /// A fixed colour.
    Rgb(Color32),
    /// Exactly another slot's colour.
    Like(usize),
    /// Its own colour, shifted by however far the linked slot has moved from
    /// its default.
    Shade(usize, Color32),
}

impl Base {
    pub const fn parent(&self) -> Option<usize> {
        match *self {
            Base::Rgb(_) => None,
            Base::Like(p) | Base::Shade(p, _) => Some(p),
        }
    }
}

/// One slot's declaration: what the table row says about it.
#[derive(Clone, Copy, Debug)]
pub struct Def {
    /// The prefs key. Renaming one orphans every stored override of it.
    pub id: &'static str,
    pub group: &'static str,
    pub label: &'static str,
    /// The slot's doc comment, shown on hover.
    pub tip: &'static str,
    pub base: Base,
    /// The picker offers an alpha lane. Always true for a translucent
    /// default; `[alpha]` in the table grants it to an opaque one whose
    /// sites fade it themselves (`gamma_multiply_u8`), so a theme's alpha
    /// scales every one of them.
    pub alpha: bool,
}

// --- table literals --------------------------------------------------------

pub const fn rgb(r: u8, g: u8, b: u8) -> Base {
    Base::Rgb(Color32::from_rgb(r, g, b))
}
pub const fn gray(l: u8) -> Base {
    Base::Rgb(Color32::from_gray(l))
}
/// Unmultiplied alpha, like a hex code.
pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Base {
    Base::Rgb(Color32::from_rgba_unmultiplied_const(r, g, b, a))
}
/// `shade(parent, own)` with the parent already an index.
pub const fn shade_of(parent: usize, own: Base) -> Base {
    match own {
        Base::Rgb(c) => Base::Shade(parent, c),
        _ => panic!("shade() takes a literal colour"),
    }
}
/// A literal's colour, for the table's alpha flag (`like` answers opaque;
/// the flag is read off the slot's OWN literal).
pub const fn default_rgb(b: Base) -> Color32 {
    match b {
        Base::Rgb(c) | Base::Shade(_, c) => c,
        Base::Like(_) => Color32::BLACK,
    }
}

// --- packing + arithmetic ---------------------------------------------------

/// Premultiplied rgba in one word, for the atomic table.
pub const fn pack(c: Color32) -> u32 {
    u32::from_le_bytes(c.to_array())
}

pub fn unpack(u: u32) -> Color32 {
    let [r, g, b, a] = u.to_le_bytes();
    Color32::from_rgba_premultiplied(r, g, b, a)
}

/// Slot `i`'s colour with nothing overridden anywhere.
pub const fn default_of(defs: &[Def], i: usize) -> Color32 {
    match defs[i].base {
        Base::Rgb(c) | Base::Shade(_, c) => c,
        Base::Like(p) => default_of(defs, p),
    }
}

/// `c` lighter by `by` levels per channel (darker when negative), alpha kept.
/// For a site that sits a fixed step off a slot — an unbound keycap a shade
/// under a bound one — so the step holds in any theme.
pub fn lift(c: Color32, by: i16) -> Color32 {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    let m = |x: u8| (x as i16 + by).clamp(0, 255) as u8;
    Color32::from_rgba_unmultiplied(m(r), m(g), m(b), a)
}

/// `c` moved by however far `to` is from `from`, per channel, in the sRGB
/// bytes the eye judges; alpha stays `c`'s.
pub fn shift(c: Color32, from: Color32, to: Color32) -> Color32 {
    if from == to {
        return c;
    }
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    let [fr, fg, fb, _] = from.to_srgba_unmultiplied();
    let [tr, tg, tb, _] = to.to_srgba_unmultiplied();
    let m = |x: u8, f: u8, t: u8| (x as i16 + t as i16 - f as i16).clamp(0, 255) as u8;
    Color32::from_rgba_unmultiplied(m(r, fr, tr), m(g, fg, tg), m(b, fb, tb), a)
}

/// `#rrggbb`, or `#rrggbbaa` when there is an alpha to say.
pub fn hex(c: Color32) -> String {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    if a == 255 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
    }
}

/// A prefs string back to a colour; the `#` is optional.
pub fn parse_hex(s: &str) -> Option<Color32> {
    let t = s.trim();
    let t = if t.starts_with('#') { t.to_string() } else { format!("#{t}") };
    Color32::from_hex(&t).ok()
}

// --- the live table ----------------------------------------------------------

/// One app's theme: its definitions and the storage the macro declared for
/// them. `LIVE` holds the resolved colours; the overrides are the only
/// authored state. A process has ONE table that drives egui; the crate's
/// own widgets read `ui.visuals()`, which [`Table::sync`] fills from the
/// interface slots found by id, so an app's bigger table drives them too.
pub struct Table {
    pub defs: &'static [Def],
    pub live: &'static [AtomicU32],
    /// Empty until the first edit; `None` per slot after.
    pub overrides: &'static RwLock<Vec<Option<Color32>>>,
    /// Bumped whenever `live` changes, so [`Table::sync`] knows a context is stale.
    pub generation: &'static AtomicU32,
}

impl Table {
    pub fn len(&self) -> usize {
        self.defs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }

    #[inline]
    pub fn get(&self, i: usize) -> Color32 {
        unpack(self.live[i].load(Ordering::Relaxed))
    }

    /// The slot with this prefs id.
    pub fn index(&self, id: &str) -> Option<usize> {
        self.defs.iter().position(|d| d.id == id)
    }

    pub fn default_of(&self, i: usize) -> Color32 {
        default_of(self.defs, i)
    }

    /// What slot `i` is without an override of its own, given every slot
    /// before it already resolved (links only point backwards — a test holds that).
    fn inherited(&self, i: usize, out: &[Color32]) -> Color32 {
        match self.defs[i].base {
            Base::Rgb(c) => c,
            Base::Like(p) => out[p],
            Base::Shade(p, c) => shift(c, self.default_of(p), out[p]),
        }
    }

    /// Every slot's colour under these overrides (short slices are padded).
    pub fn resolve(&self, over: &[Option<Color32>]) -> Vec<Color32> {
        let mut out = vec![Color32::TRANSPARENT; self.len()];
        for i in 0..self.len() {
            out[i] = over.get(i).copied().flatten().unwrap_or_else(|| self.inherited(i, &out));
        }
        out
    }

    fn apply(&self, over: &[Option<Color32>]) {
        for (slot, c) in self.live.iter().zip(self.resolve(over)) {
            slot.store(pack(c), Ordering::Relaxed);
        }
        self.generation.fetch_add(1, Ordering::Relaxed);
    }

    fn over_mut(&self) -> RwLockWriteGuard<'_, Vec<Option<Color32>>> {
        let mut o = self.overrides.write().unwrap();
        if o.len() < self.len() {
            o.resize(self.len(), None);
        }
        o
    }

    /// Recolour a slot. Setting it to what it would inherit anyway REMOVES
    /// the override, so prefs stay a diff and the link comes back.
    pub fn set(&self, i: usize, c: Color32) {
        let mut o = self.over_mut();
        o[i] = None;
        if self.resolve(&o)[i] != c {
            o[i] = Some(c);
        }
        self.apply(&o);
    }

    pub fn reset(&self, i: usize) {
        let mut o = self.over_mut();
        o[i] = None;
        self.apply(&o);
    }

    pub fn reset_all(&self) {
        let mut o = self.over_mut();
        o.fill(None);
        self.apply(&o);
    }

    pub fn is_overridden(&self, i: usize) -> bool {
        self.overrides.read().unwrap().get(i).copied().flatten().is_some()
    }

    pub fn any_overridden(&self) -> bool {
        self.overrides.read().unwrap().iter().any(Option::is_some)
    }

    // --- prefs ---

    pub fn format_overrides(&self, over: &[Option<Color32>]) -> BTreeMap<String, String> {
        over.iter()
            .enumerate()
            .filter_map(|(i, c)| c.map(|c| (self.defs[i].id.to_string(), hex(c))))
            .collect()
    }

    /// An id this build does not know, or a colour that does not parse, is
    /// skipped with a warning — a downgraded build must not wedge on a
    /// newer theme.
    pub fn parse_overrides(&self, map: &BTreeMap<String, String>) -> Vec<Option<Color32>> {
        let mut over = vec![None; self.len()];
        for (id, s) in map {
            let Some(i) = self.index(id) else {
                log::warn!("theme: unknown colour id {id:?} in prefs, skipped");
                continue;
            };
            match parse_hex(s) {
                Some(c) => over[i] = Some(c),
                None => log::warn!("theme: unparsable colour {s:?} for {id}, skipped"),
            }
        }
        // A stored value equal to what the slot inherits is no override at all.
        let mut o = vec![None; self.len()];
        for i in 0..self.len() {
            if let Some(c) = over[i] {
                if self.resolve(&o)[i] != c {
                    o[i] = Some(c);
                }
            }
        }
        o
    }

    /// The overrides as prefs strings, ready to serialize.
    pub fn overrides(&self) -> BTreeMap<String, String> {
        self.format_overrides(&self.overrides.read().unwrap())
    }

    /// Replace the overrides from prefs strings.
    pub fn install(&self, map: &BTreeMap<String, String>) {
        let mut o = self.over_mut();
        *o = self.parse_overrides(map);
        self.apply(&o);
    }

    /// Every slot as it is painted now.
    pub fn live(&self) -> Vec<Color32> {
        (0..self.len()).map(|i| self.get(i)).collect()
    }
}
