//! imfram: the egui UI layer wobble and graav share. Blender muscle memory on
//! every canvas, one set of controls, one window shell.
//!
//! | module        | from   | what it owns                                                  |
//! |---------------|--------|---------------------------------------------------------------|
//! | `shell`       | graav  | a GPU handle, an egui layer painted into any view, an OS window of egui |
//! | `dock`        | graav  | panels as dock tabs across tool windows; drag a tab out to detach it |
//! | `tiltmenu`    | wobble | the menubar's swung dropdowns                                 |
//! | `hudkeys`     | wobble | hotkey caps fanned along a view's top                         |
//! | `nav`         | both   | Blender-style pan / zoom classifiers for any 2D canvas        |
//! | `widgets`     | graav  | sliders with drag rates, folding groups, row tooltips, key strips |
//! | `numfield`    | wobble | a DragValue in a fixed box, decimals dropping to fit          |
//! | `rgbslider`   | wobble | the RGB triple slider                                         |
//! | `colorpicker` | wobble | swatch + popup picker (wheel, rgb, hsl, Oklch), hex, eyedropper hook |
//! | `eyedrop`     | wobble | hover a colour control, press E, sample a texture             |
//! | `textedit`    | new    | a line, a paragraph, or code: gutter, tabs, a small lexer       |
//! | `history`     | graav  | snapshot undo that settles an edit burst into one step        |
//! | `filebrowser` | new    | an in-app file browser: places, crumbs, filter, pick (native)  |
//! | `fonts`       | new    | font size and a user's font file, applied to a context         |
//! | `filedialog`  | wobble | rfd on native, a stand-in on the web                          |
//! | `bg`          | wobble | fire-and-forget work: a thread, or inline on the web          |
//! | `snapshot`    | wobble | test: CPU-rasterized egui output for geometry asserts         |
//! | `uishot`      | wobble | test: the real renderer to a PNG, to look at                  |
//!
//! Everything here depends on egui (and wgpu / winit for the shell) only:
//! an app hands in its own state through closures and traits. Theme slots
//! and key maps stay in the apps for now; see the README.

pub mod bg;
pub mod colorpicker;
pub mod eyedrop;
pub mod filedialog;
pub mod fonts;
pub mod history;
pub mod hudkeys;
pub mod nav;
pub mod numfield;
pub mod rgbslider;
pub mod snapshot;
pub mod textedit;
pub mod tiltmenu;
pub mod widgets;

#[cfg(not(target_arch = "wasm32"))]
pub mod dock;
#[cfg(not(target_arch = "wasm32"))]
pub mod filebrowser;
#[cfg(not(target_arch = "wasm32"))]
pub mod shell;
#[cfg(not(target_arch = "wasm32"))]
pub mod uishot;

#[cfg(not(target_arch = "wasm32"))]
pub use shell::{EguiLayer, Gpu, UiApp, UiWindow};
