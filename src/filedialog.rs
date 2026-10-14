//! The file dialog: `rfd::FileDialog` on native. In the web build (`web/`)
//! a stand-in with the same builder that picks nothing — a browser has no
//! synchronous dialog to block on, so every "pick" site behaves as if the
//! user pressed Cancel, which each of them already handles.

#[cfg(not(target_arch = "wasm32"))]
pub use rfd::FileDialog;

#[cfg(target_arch = "wasm32")]
pub use web::FileDialog;

#[cfg(target_arch = "wasm32")]
mod web {
    use std::path::{Path, PathBuf};

    #[derive(Default)]
    pub struct FileDialog;

    impl FileDialog {
        pub fn new() -> Self {
            Self
        }
        pub fn add_filter(self, _name: impl Into<String>, _exts: &[impl ToString]) -> Self {
            self
        }
        pub fn set_directory<P: AsRef<Path>>(self, _dir: P) -> Self {
            self
        }
        pub fn set_file_name(self, _name: impl Into<String>) -> Self {
            self
        }
        pub fn pick_file(self) -> Option<PathBuf> {
            None
        }
        pub fn pick_folder(self) -> Option<PathBuf> {
            None
        }
        pub fn save_file(self) -> Option<PathBuf> {
            None
        }
    }
}
