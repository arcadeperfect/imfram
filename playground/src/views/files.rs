//! A folder browsed in a list or tiles: frame sequences grouped, PNG
//! thumbnails (stills and sequences) loaded on workers, hover a sequence's
//! thumbnail to scrub it. Starts in the playground's folder (or
//! `$IMFRAM_FILES_DIR`).

use egui::Ui;
use imfram::gallery::browse::FileView;
use imfram::gallery::files::{load_png, FileSrc};
use imfram::gallery::Thumbs;

pub struct State {
    view: FileView,
    thumbs: Thumbs<FileSrc>,
    last: String,
}

impl Default for State {
    fn default() -> Self {
        let dir = std::env::var_os("IMFRAM_FILES_DIR").map(Into::into).or_else(|| std::env::current_dir().ok()).unwrap_or_default();
        Self { view: FileView::new(dir), thumbs: Thumbs::new(4, load_png), last: String::new() }
    }
}

pub fn ui(ui: &mut Ui, s: &mut State) {
    let ev = s.view.show(ui, &mut s.thumbs);
    if let Some(src) = ev.opened {
        s.last = format!("opened {}", src.path.display());
    }
    if !s.last.is_empty() {
        ui.weak(&s.last);
    }
}
