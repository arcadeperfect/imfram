//! The UI colour theme: every colour an interface paints that is chrome
//! rather than data, in ONE table a user can recolour.
//!
//! An app declares its table with [`slots!`](crate::slots) — one slot per
//! row: a stable id (what prefs store), the group and label the colours…
//! window lists it under, a doc comment for the hover tip, and a default.
//! A default is a literal or a LINK to another slot, so recolouring a role
//! carries the colours that belong to it: move `bg` and the windows, fields
//! and canvases move with it; move `accent` and the highlights built on it
//! follow. Links come in two strengths:
//!
//! * `like(Slot::x)` — the same colour as `x`, whatever that is.
//! * `shade(Slot::x, c)` — its own colour `c`, shifted per channel by however
//!   far `x` has been moved from ITS default. A canvas a little darker than
//!   the panels stays a little darker in any theme, and with nothing
//!   overridden every slot is exactly its literal.
//!
//! Sites read LIVE through the generated accessors (`theme::accent()`): one
//! atomic load, so an edit repaints the same frame with nothing to
//! invalidate. The statics are const-initialised to the defaults, so code
//! that paints before `install` sees the real colours. Prefs hold only the
//! OVERRIDES, as hex (`"accent": "#ff8800"`): an untouched slot follows any
//! future default, and an id a build does not know is skipped with a warning.
//!
//! egui's own widgets follow through `sync`, which writes the interface
//! slots into a context's dark `Visuals` whenever the theme has moved.
//!
//! **One live table per app.** This module invokes the macro with the CORE
//! table (interface, status, canvases: what the crate's own widgets need),
//! so a crate user with no theme of its own has one. An app with more
//! colours declares a superset — `slots! { core; "my group" { … } }` — and
//! from then on its table is the live one: the crate's widgets never call
//! the core accessors here; they read `ui.visuals()`, which the app's `sync`
//! fills by slot id, so a bigger table drives them the same.
//!
//! Deliberately NOT a theme's business: colours that ARE data — channel
//! lanes and their tints, swatches showing a value, user-picked colours.
//!
//! Adding UI colour? Add a slot and call it; do not paint a literal.

mod engine;
mod macros;
mod ui;
mod visuals;

#[cfg(test)]
mod tests;

pub use egui::Color32;
pub use engine::{
    default_of, default_rgb, hex, lift, pack, parse_hex, shade_of, shift, unpack, Base, Def, Table,
};

/// The table literals the macro brings into the invoking module.
#[doc(hidden)]
pub mod lit {
    pub use super::engine::{gray, rgb, rgba};
}
pub use ui::ThemeUi;

#[doc(hidden)]
pub use egui as __egui;

/// What the macro declares its override store as; empty until the first edit.
pub type Overrides = std::sync::RwLock<Vec<Option<Color32>>>;

crate::slots! { core; }
