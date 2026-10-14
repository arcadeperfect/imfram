//! The window shell: a GPU handle, egui painted into a view, an OS window
//! drawn entirely with egui that repaints on demand.
//!
//! | module   | what it owns                                               |
//! |----------|------------------------------------------------------------|
//! | `gpu`    | instance + adapter + device + queue, shared by every window |
//! | `layer`  | one window's egui: input, the frame, painting into a view   |
//! | `window` | an OS window + surface + layer; on-demand repaint; offscreen |
//! | `capture`| a texture read back to pixels, saved as a PNG                |

pub mod capture;
mod gpu;
mod layer;
mod window;

pub use gpu::Gpu;
pub use layer::{EguiLayer, UiApp};
pub use window::UiWindow;
