//! A file browser: pinned and recent places down the left, the path as
//! crumbs along the top, a filterable listing (folders first), and a pick.
//! Listing runs off the UI thread ([`crate::bg`]) so a slow disk never
//! holds a frame. Native only: the web build has no directory to walk.
//!
//! The caller owns a [`Browser`] and shows it where it likes (a tab, a
//! window); it answers with an [`Action`] each frame. Filters are by
//! extension; a `Pick` is a file, `PickDir` the shown directory.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::time::SystemTime;

use egui::{RichText, Ui};

/// What the user did this frame.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    None,
    Pick(PathBuf),
    PickDir(PathBuf),
    Close,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub dir: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
}

pub struct Browser {
    pub dir: PathBuf,
    /// The path field as typed (committed with Enter).
    pub dir_edit: String,
    pub filter: String,
    /// Extensions shown ("exr", "png"); empty shows all.
    pub exts: Vec<String>,
    pub show_hidden: bool,
    pub pinned: Vec<PathBuf>,
    pub recent: Vec<PathBuf>,
    pub selected: Option<PathBuf>,
    /// Picking a folder is the point (a save location, a project root).
    pub dirs_only: bool,
    entries: Vec<Entry>,
    error: Option<String>,
    /// A listing in flight for `dir`.
    pending: Option<Receiver<Result<Vec<Entry>, String>>>,
    listed: Option<PathBuf>,
}

const RECENT_MAX: usize = 8;

impl Browser {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        let mut pinned = vec![];
        if let Some(h) = home() {
            pinned.push(h.clone());
            for sub in ["Desktop", "Documents", "Downloads"] {
                let p = h.join(sub);
                if p.is_dir() {
                    pinned.push(p);
                }
            }
        }
        Self {
            dir_edit: dir.display().to_string(),
            dir,
            filter: String::new(),
            exts: vec![],
            show_hidden: false,
            pinned,
            recent: vec![],
            selected: None,
            dirs_only: false,
            entries: vec![],
            error: None,
            pending: None,
            listed: None,
        }
    }

    pub fn exts(mut self, exts: &[&str]) -> Self {
        self.exts = exts.iter().map(|e| e.trim_start_matches('.').to_ascii_lowercase()).collect();
        self
    }

    pub fn dirs_only(mut self) -> Self {
        self.dirs_only = true;
        self
    }

    /// Go to `dir`; the listing follows on another thread.
    pub fn go(&mut self, dir: impl Into<PathBuf>) {
        let dir = dir.into();
        if dir != self.dir {
            self.recent.retain(|p| *p != self.dir);
            self.recent.insert(0, self.dir.clone());
            self.recent.truncate(RECENT_MAX);
        }
        self.dir = dir;
        self.dir_edit = self.dir.display().to_string();
        self.selected = None;
        self.listed = None;
    }

    /// List again (something was written there).
    pub fn refresh(&mut self) {
        self.listed = None;
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    fn poll(&mut self, ctx: &egui::Context) {
        if self.listed.as_ref() != Some(&self.dir) && self.pending.is_none() {
            let (tx, rx) = channel();
            let dir = self.dir.clone();
            let hidden = self.show_hidden;
            crate::bg::spawn(move || {
                let _ = tx.send(list(&dir, hidden));
            });
            self.pending = Some(rx);
            self.listed = Some(self.dir.clone());
        }
        if let Some(rx) = &self.pending {
            match rx.try_recv() {
                Ok(Ok(es)) => {
                    self.entries = es;
                    self.error = None;
                    self.pending = None;
                }
                Ok(Err(e)) => {
                    self.entries.clear();
                    self.error = Some(e);
                    self.pending = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(30)),
                Err(_) => self.pending = None,
            }
        }
    }

    /// The whole browser in `ui`. What was done this frame comes back.
    pub fn ui(&mut self, ui: &mut Ui) -> Action {
        self.poll(ui.ctx());
        let mut action = Action::None;
        egui::Panel::left(ui.id().with("places")).resizable(true).default_size(150.0).show(ui, |ui| {
            self.places(ui);
        });
        egui::Panel::bottom(ui.id().with("foot")).show(ui, |ui| {
            ui.horizontal(|ui| {
                let picked = match &self.selected {
                    Some(p) => p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
                    None if self.dirs_only => self.dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
                    None => String::new(),
                };
                ui.label(RichText::new(picked).monospace());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("cancel").clicked() {
                        action = Action::Close;
                    }
                    let can = self.selected.is_some() || self.dirs_only;
                    if ui.add_enabled(can, egui::Button::new("pick")).clicked() {
                        action = self.pick();
                    }
                });
            });
        });
        egui::Panel::top(ui.id().with("head")).show(ui, |ui| {
            if let Some(a) = self.head(ui) {
                action = a;
            }
        });
        if let Some(a) = self.listing(ui) {
            action = a;
        }
        action
    }

    fn pick(&mut self) -> Action {
        match &self.selected {
            Some(p) if p.is_dir() => {
                let p = p.clone();
                self.go(p);
                Action::None
            }
            Some(p) => Action::Pick(p.clone()),
            None if self.dirs_only => Action::PickDir(self.dir.clone()),
            None => Action::None,
        }
    }

    fn places(&mut self, ui: &mut Ui) {
        let mut go = None;
        ui.label(RichText::new("places").weak());
        for p in &self.pinned {
            let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| p.display().to_string());
            if ui.selectable_label(*p == self.dir, name).on_hover_text(p.display().to_string()).clicked() {
                go = Some(p.clone());
            }
        }
        if !self.recent.is_empty() {
            ui.separator();
            ui.label(RichText::new("recent").weak());
            for p in &self.recent {
                let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| p.display().to_string());
                if ui.selectable_label(false, name).on_hover_text(p.display().to_string()).clicked() {
                    go = Some(p.clone());
                }
            }
        }
        ui.separator();
        let here = self.dir.clone();
        let pinned = self.pinned.contains(&here);
        if ui.selectable_label(pinned, if pinned { "unpin this folder" } else { "pin this folder" }).clicked() {
            if pinned {
                self.pinned.retain(|p| *p != here);
            } else {
                self.pinned.push(here);
            }
        }
        if let Some(p) = go {
            self.go(p);
        }
    }

    fn head(&mut self, ui: &mut Ui) -> Option<Action> {
        let mut go = None;
        ui.horizontal(|ui| {
            if ui.button("↑").on_hover_text("up a folder (backspace)").clicked() {
                go = self.dir.parent().map(Path::to_path_buf);
            }
            // Crumbs: each ancestor is a button.
            let parts: Vec<PathBuf> = self.dir.ancestors().map(Path::to_path_buf).collect();
            for p in parts.iter().rev() {
                let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "/".into());
                if ui.small_button(name).clicked() {
                    go = Some(p.clone());
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.checkbox(&mut self.show_hidden, "hidden").changed().then(|| self.refresh());
                let r = ui.add(egui::TextEdit::singleline(&mut self.filter).hint_text("filter").desired_width(120.0));
                if r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    self.filter.clear();
                }
            });
        });
        ui.horizontal(|ui| {
            let r = ui.add(egui::TextEdit::singleline(&mut self.dir_edit).desired_width(f32::INFINITY).font(egui::TextStyle::Monospace));
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                let p = expand_home(&self.dir_edit);
                if p.is_dir() {
                    go = Some(p);
                } else {
                    self.error = Some(format!("not a folder: {}", p.display()));
                }
            }
        });
        if let Some(e) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, e);
        }
        if ui.input(|i| i.key_pressed(egui::Key::Backspace)) && !ui.ctx().text_edit_focused() {
            go = self.dir.parent().map(Path::to_path_buf);
        }
        if let Some(p) = go {
            self.go(p);
        }
        None
    }

    fn listing(&mut self, ui: &mut Ui) -> Option<Action> {
        let mut action = None;
        let mut enter = None;
        let filter = self.filter.to_lowercase();
        let shown: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| filter.is_empty() || e.name.to_lowercase().contains(&filter))
            .filter(|(_, e)| e.dir || (!self.dirs_only && self.exts.is_empty()) || ext_ok(&e.path, &self.exts) && !self.dirs_only)
            .map(|(i, _)| i)
            .collect();
        let row_h = ui.text_style_height(&egui::TextStyle::Body) + 4.0;
        let dim = ui.visuals().weak_text_color();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, row_h, shown.len(), |ui, range| {
            for &i in &shown[range] {
                let e = &self.entries[i];
                let selected = self.selected.as_ref() == Some(&e.path);
                let text = if e.dir { RichText::new(format!("▸ {}", e.name)).strong() } else { RichText::new(format!("  {}", e.name)) };
                let r = ui.horizontal(|ui| {
                    let r = ui.selectable_label(selected, text);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if !e.dir {
                            ui.label(RichText::new(size_text(e.size)).color(dim).monospace());
                        }
                        ui.label(RichText::new(when(e.modified)).color(dim).small());
                    });
                    r
                });
                let r = r.inner;
                if r.clicked() {
                    self.selected = Some(e.path.clone());
                }
                if r.double_clicked() {
                    if e.dir {
                        // (Entered after the loop: `go` wants the whole of self.)
                        enter = Some(e.path.clone());
                    } else if !self.dirs_only {
                        action = Some(Action::Pick(e.path.clone()));
                    }
                }
            }
        });
        if shown.is_empty() && self.pending.is_none() && self.error.is_none() {
            ui.label(RichText::new("nothing here").color(dim));
        }
        // A folder double-clicked is entered, not picked.
        if let Some(p) = enter {
            self.go(p);
            return Some(Action::None);
        }
        if ui.input(|i| i.key_pressed(egui::Key::Enter)) && !ui.ctx().text_edit_focused() {
            return Some(self.pick());
        }
        action
    }
}

fn ext_ok(p: &Path, exts: &[String]) -> bool {
    exts.is_empty() || p.extension().and_then(|e| e.to_str()).is_some_and(|e| exts.iter().any(|x| x.eq_ignore_ascii_case(e)))
}

fn list(dir: &Path, hidden: bool) -> Result<Vec<Entry>, String> {
    let rd = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut out: Vec<Entry> = rd
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if !hidden && name.starts_with('.') {
                return None;
            }
            let md = e.metadata().ok()?;
            Some(Entry { path: e.path(), name, dir: md.is_dir(), size: md.len(), modified: md.modified().ok() })
        })
        .collect();
    out.sort_by(|a, b| b.dir.cmp(&a.dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(out)
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from)
}

/// `~/x` to the home directory.
pub fn expand_home(s: &str) -> PathBuf {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix('~') {
        if let Some(h) = home() {
            return h.join(rest.trim_start_matches(['/', '\\']));
        }
    }
    PathBuf::from(s)
}

fn size_text(n: u64) -> String {
    const K: f64 = 1024.0;
    let f = n as f64;
    if f < K {
        format!("{n} B")
    } else if f < K * K {
        format!("{:.0} KB", f / K)
    } else if f < K * K * K {
        format!("{:.1} MB", f / K / K)
    } else {
        format!("{:.2} GB", f / K / K / K)
    }
}

fn when(t: Option<SystemTime>) -> String {
    let Some(t) = t else { return String::new() };
    let Ok(age) = SystemTime::now().duration_since(t) else { return "just now".into() };
    let s = age.as_secs();
    match s {
        0..=59 => "just now".into(),
        60..=3599 => format!("{} min ago", s / 60),
        3600..=86399 => format!("{} h ago", s / 3600),
        _ => format!("{} d ago", s / 86400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_come_first_then_names_case_blind() {
        let d = std::env::temp_dir().join(format!("hands-fb-{}", std::process::id()));
        let _ = std::fs::create_dir_all(d.join("Zed"));
        std::fs::write(d.join("alpha.txt"), b"x").unwrap();
        std::fs::write(d.join("Beta.exr"), b"xy").unwrap();
        std::fs::write(d.join(".hidden"), b"").unwrap();
        let es = list(&d, false).unwrap();
        let names: Vec<&str> = es.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["Zed", "alpha.txt", "Beta.exr"]);
        assert_eq!(list(&d, true).unwrap().len(), 4);
        assert!(ext_ok(&d.join("Beta.exr"), &["exr".into()]));
        assert!(!ext_ok(&d.join("alpha.txt"), &["exr".into()]));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn a_missing_folder_is_an_error_not_a_panic() {
        assert!(list(Path::new("/definitely/not/here"), false).is_err());
    }

    #[test]
    fn sizes_read_as_people_write_them() {
        assert_eq!(size_text(10), "10 B");
        assert_eq!(size_text(2048), "2 KB");
        assert_eq!(size_text(3 * 1024 * 1024), "3.0 MB");
    }
}
