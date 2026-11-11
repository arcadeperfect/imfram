//! The window shell: a GPU handle, egui painted into a view, an OS window
//! drawn entirely with egui that repaints on demand.
//!
//! | module   | what it owns                                               |
//! |----------|------------------------------------------------------------|
//! | `gpu`    | instance + adapter + device + queue, shared by every window |
//! | `layer`  | one window's egui: input, the frame, painting into a view   |
//! | `window` | an OS window + surface + layer; on-demand repaint; offscreen |
//! | `capture`| a texture read back to pixels, saved as a PNG                |

//!
//! In a page (wasm) only [`UiApp`] is here: the page's host (eframe) owns
//! the canvas, and the dock draws into its egui frame.

#[cfg(not(target_arch = "wasm32"))]
pub mod capture;
#[cfg(not(target_arch = "wasm32"))]
mod gpu;
#[cfg(not(target_arch = "wasm32"))]
mod layer;
#[cfg(not(target_arch = "wasm32"))]
mod window;

#[cfg(not(target_arch = "wasm32"))]
pub use gpu::Gpu;
#[cfg(not(target_arch = "wasm32"))]
pub use layer::EguiLayer;
#[cfg(not(target_arch = "wasm32"))]
pub use window::UiWindow;

/// The UI drawn in a layer each frame. A plain `FnMut(&mut egui::Ui)` works;
/// implement the trait to also see the raw input before egui does.
pub trait UiApp {
    /// Runs once per frame before egui: claim or rewrite input events.
    /// `ctx` is the layer's context (what has focus, the last pointer).
    fn raw_input(&mut self, _ctx: &egui::Context, _raw: &mut egui::RawInput) {}
    fn ui(&mut self, ui: &mut egui::Ui);
}

impl<F: FnMut(&mut egui::Ui)> UiApp for F {
    fn ui(&mut self, ui: &mut egui::Ui) {
        self(ui)
    }
}
