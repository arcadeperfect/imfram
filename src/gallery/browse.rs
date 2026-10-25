//! A folder in the grid or the list, with a path bar: the file pane of an
//! organisicator-style browser. Back / forward / up, the path (type and
//! Enter; `~` is home), a filter, hidden files, list or tiles, and the
//! size. Double-click a folder to go in; anything else comes back as
//! `opened`. Both views share one selection, so switching keeps it.
//!
//! The app owns the [`Thumbs`] cache (its loader decodes EXRs and
//! movies; [`super::files::load_png`] covers PNG) and hands it in.

use std::path::PathBuf;

use egui::Ui;

use super::files::{self, Entry, FileSrc, Kind, Listing};
use super::{Gallery, GalleryEvent, Items, List, Thumbs};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    List,
    Tiles,
}

#[derive(Clone, Debug, Default)]
pub struct FileEvent {
    /// A file or sequence was double-clicked or Entered.
    pub opened: Option<FileSrc>,
    /// The folder shown changed.
    pub went: Option<PathBuf>,
    /// The view's own event (selection, context menu, drag, preview).
    pub view: GalleryEvent,
}

pub struct FileView {
    pub listing: Listing,
    pub mode: Mode,
    pub gallery: Gallery,
    pub list: List,
    path_edit: String,
    back: Vec<PathBuf>,
    fwd: Vec<PathBuf>,
}

impl FileView {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        let mut list = List::new(files::columns());
        list.sort = Some((0, true));
        list.name_badge = false;
        Self { path_edit: dir.display().to_string(), listing: Listing::new(dir), mode: Mode::List, gallery: Gallery::new(), list, back: vec![], fwd: vec![] }
    }

    pub fn dir(&self) -> &std::path::Path {
        &self.listing.dir
    }

    /// The selected entries, in the order shown.
    pub fn selected(&self) -> Vec<&Entry> {
        let sel = self.selection();
        (0..self.listing.len()).map(|i| self.listing.entry(i)).filter(|e| sel.contains(&e.id)).collect()
    }

    fn selection(&self) -> &std::collections::BTreeSet<u64> {
        match self.mode {
            Mode::List => &self.list.selection,
            Mode::Tiles => &self.gallery.selection,
        }
    }

    pub fn go(&mut self, dir: impl Into<PathBuf>) {
        let dir = dir.into();
        if dir == self.listing.dir {
            return;
        }
        self.back.push(self.listing.dir.clone());
        self.fwd.clear();
        self.enter(dir);
    }

    fn enter(&mut self, dir: PathBuf) {
        self.path_edit = dir.display().to_string();
        self.listing.go(dir);
        self.list.selection.clear();
        self.gallery.selection.clear();
        self.list.scroll_to(0);
        self.gallery.scroll_to(0);
    }

    pub fn set_mode(&mut self, m: Mode) {
        if m == self.mode {
            return;
        }
        let sel = self.selection().clone();
        self.mode = m;
        self.list.selection = sel.clone();
        self.gallery.selection = sel;
    }

    pub fn show(&mut self, ui: &mut Ui, thumbs: &mut Thumbs<FileSrc>) -> FileEvent {
        let mut out = FileEvent::default();
        self.listing.poll(ui.ctx());
        let before = self.listing.dir.clone();
        self.bar(ui);
        if let Some(e) = self.listing.error() {
            ui.colored_label(ui.visuals().error_fg_color, e);
            return out;
        }
        let ev = match self.mode {
            Mode::List => {
                let ev = self.list.show(ui, &self.listing, thumbs);
                if ev.sort_changed {
                    if let Some((c, up)) = self.list.sort {
                        self.listing.sort = files::sort_key_of(c);
                        self.listing.ascending = up;
                    }
                }
                ev
            }
            Mode::Tiles => self.gallery.show(ui, &self.listing, thumbs),
        };
        if let Some(id) = ev.activated {
            if let Some(e) = self.listing.by_id(id) {
                if e.src.kind == Kind::Dir {
                    let p = e.src.path.clone();
                    self.go(p);
                } else {
                    out.opened = Some(e.src.clone());
                }
            }
        }
        if self.listing.dir != before {
            out.went = Some(self.listing.dir.clone());
        }
        out.view = ev;
        out
    }

    fn bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            if ui.add_enabled(!self.back.is_empty(), egui::Button::new("⏴")).on_hover_text("back").clicked() {
                let d = self.back.pop().unwrap();
                self.fwd.push(self.listing.dir.clone());
                self.enter(d);
            }
            if ui.add_enabled(!self.fwd.is_empty(), egui::Button::new("⏵")).on_hover_text("forward").clicked() {
                let d = self.fwd.pop().unwrap();
                self.back.push(self.listing.dir.clone());
                self.enter(d);
            }
            let parent = self.listing.dir.parent().map(|p| p.to_path_buf());
            if ui.add_enabled(parent.is_some(), egui::Button::new("⏶")).on_hover_text("up").clicked() {
                self.go(parent.unwrap());
            }
            let right = 260.0;
            let r = ui.add(egui::TextEdit::singleline(&mut self.path_edit).desired_width((ui.available_width() - right).max(80.0)).font(egui::TextStyle::Monospace));
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                let d = crate::filebrowser::expand_home(&self.path_edit);
                self.go(d);
            } else if !r.has_focus() {
                self.path_edit = self.listing.dir.display().to_string();
            }
            ui.add(egui::TextEdit::singleline(&mut self.listing.filter).hint_text("filter").desired_width(90.0));
            ui.toggle_value(&mut self.listing.show_hidden, "hidden").on_hover_text("show dot files");
            let mut m = self.mode;
            ui.selectable_value(&mut m, Mode::List, "list");
            ui.selectable_value(&mut m, Mode::Tiles, "tiles");
            self.set_mode(m);
        });
        ui.horizontal(|ui| {
            let n = self.listing.len();
            let s = self.selection().len();
            let mut t = if s > 0 { format!("{n} items · {s} selected") } else { format!("{n} items") };
            if self.listing.loading() {
                t.push_str(" · reading…");
            }
            ui.weak(t);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| match self.mode {
                Mode::List => {
                    ui.add(egui::Slider::new(&mut self.list.row_h, super::list::MIN_ROW..=super::list::MAX_ROW).show_value(false)).on_hover_text("row size (cmd-wheel)");
                }
                Mode::Tiles => {
                    ui.add(egui::Slider::new(&mut self.gallery.size, super::MIN_SIZE..=super::MAX_SIZE).show_value(false)).on_hover_text("tile size (cmd-wheel)");
                }
            });
        });
    }
}
