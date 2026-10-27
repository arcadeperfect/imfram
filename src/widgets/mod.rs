//! Shared controls.
//!
//! | module     | what it owns                                                 |
//! |------------|--------------------------------------------------------------|
//! | `rate`     | drag-rate keys: hold Z X C to slow a drag, V B N to speed it |
//! | `slider`   | the float slider row: rail, value box, label                 |
//! | `range`    | re-aimable slider ranges: A shows every slider's bounds      |
//! | `group`    | a foldable group of controls, with or without a switch       |
//! | `tip`      | a tooltip over a whole row (or anything egui won't hover)    |
//! | `keystrip` | key caps along a view's top; hover: what each does           |
//! | `offline`  | a global "locked" flag: settings that cannot change now      |
//! | `span`     | an in/out span on a bar: drag its ends, slide it whole       |
//!
//! Every float slider goes through `slider`, so the drag rates work
//! everywhere.

pub mod group;
pub mod keystrip;
pub mod offline;
pub mod range;
pub mod rate;
pub mod slider;
pub mod span;
pub mod tip;

pub use rate::{drag_rate, rated};
pub use range::range_slider;
pub use slider::{rects, slider, SliderOut};
pub use span::{span_bar, SpanOpts, SpanOut};
pub use tip::row as tipped;
