//! Getting around a file browser: a path bar (back, forward, up, rescan,
//! favourite, and the path itself with terminal-style Tab completion), a
//! folder tree read lazily, and a list of places (favourites, recents).
//! seq_manip's chrome, for any app that browses folders. It builds for a
//! page too, where there is no disk: every listing there is empty.
//!
//! The app owns the folder shown; these widgets only ever answer "go
//! there". The path bar keeps its own back / forward history by watching
//! the folder it is handed change, so a jump from the tree, a double-click
//! or a typed path all land in history the same way.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use egui::{Key, Modifiers, Ui};

use super::Dragged;

// ---------------------------------------------------------------- path bar

/// What the path bar asked for this frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PathEvent {
    /// Show this folder (back, forward, up, Enter or Go on a typed path).
    pub go: Option<PathBuf>,
    /// The rescan button.
    pub rescan: bool,
    /// The star: Some(true) add the folder shown to favourites, Some(false)
    /// remove it.
    pub favourite: Option<bool>,
    /// Items dropped on the up button (move them a folder up), with that
    /// folder.
    pub dropped_up: Option<(Dragged, PathBuf)>,
}

#[derive(Default)]
pub struct PathBar {
    /// The text in the field (the folder shown, unless being typed in).
    pub edit: String,
    back: Vec<PathBuf>,
    fwd: Vec<PathBuf>,
    /// The folder last shown, to notice a change.
    current: Option<PathBuf>,
    /// The next change is a back / forward step, not a new visit.
    stepping: bool,
    /// Tab completions being cycled, and which one is in the field.
    completion: Option<(Vec<String>, usize)>,
    /// Shown dimmed in the empty field.
    pub hint: String,
}

impl PathBar {
    pub fn new() -> Self {
        Self { hint: "path · Tab completes · ~ is home".into(), ..Self::default() }
    }

    pub fn can_back(&self) -> bool {
        !self.back.is_empty()
    }
    pub fn can_forward(&self) -> bool {
        !self.fwd.is_empty()
    }

    /// Note the folder shown this frame: a change from the last one is a
    /// visit (history grows, forward is dropped) unless it was our own step.
    pub fn track(&mut self, dir: &Path) {
        if self.current.as_deref() == Some(dir) {
            return;
        }
        if let Some(old) = self.current.take() {
            if !self.stepping {
                self.back.push(old);
                self.fwd.clear();
            }
        }
        self.stepping = false;
        self.current = Some(dir.to_path_buf());
        self.completion = None;
    }

    /// One step back (`true`) or forward: the folder to show, if any.
    pub fn step(&mut self, back: bool) -> Option<PathBuf> {
        let cur = self.current.clone()?;
        let to = if back { self.back.pop()? } else { self.fwd.pop()? };
        if back {
            self.fwd.push(cur);
        } else {
            self.back.push(cur);
        }
        self.stepping = true;
        Some(to)
    }

    /// Back, forward, up, rescan, the star (`favourite`: None hides it) and
    /// the path field, along one row, for the folder `dir`.
    pub fn show(&mut self, ui: &mut Ui, dir: &Path, favourite: Option<bool>) -> PathEvent {
        self.track(dir);
        let mut ev = PathEvent::default();
        // Cmd-[ / cmd-] and cmd-up, as the Finder.
        let (k_back, k_fwd, k_up) = ui.input_mut(|i| {
            (i.consume_key(Modifiers::COMMAND, Key::OpenBracket), i.consume_key(Modifiers::COMMAND, Key::CloseBracket), i.consume_key(Modifiers::COMMAND, Key::ArrowUp))
        });
        ui.horizontal(|ui| {
            let mut hist = None;
            if ui.add_enabled(self.can_back(), egui::Button::new("⏴")).on_hover_text("back (cmd-[)").clicked() || (k_back && self.can_back()) {
                hist = Some(true);
            }
            if ui.add_enabled(self.can_forward(), egui::Button::new("⏵")).on_hover_text("forward (cmd-])").clicked() || (k_fwd && self.can_forward()) {
                hist = Some(false);
            }
            if let Some(b) = hist {
                ev.go = self.step(b);
            }
            let parent = dir.parent().map(Path::to_path_buf);
            let up = ui.add_enabled(parent.is_some(), egui::Button::new("⏶")).on_hover_text("up (cmd-up)");
            if up.clicked() || (k_up && parent.is_some()) {
                ev.go = parent.clone();
            }
            if let (Some(d), Some(p)) = (up.dnd_release_payload::<Dragged>(), parent) {
                ev.dropped_up = Some(((*d).clone(), p));
            }
            if up.dnd_hover_payload::<Dragged>().is_some() {
                ui.painter().rect_stroke(up.rect.expand(2.0), 3.0, egui::Stroke::new(2.0, ui.visuals().selection.bg_fill), egui::StrokeKind::Outside);
            }
            ev.rescan = ui.button("⟳").on_hover_text("scan again").clicked();
            if let Some(fav) = favourite {
                let r = ui.button(if fav { "★" } else { "☆" }).on_hover_text(if fav { "remove from favourites" } else { "add this folder to favourites" });
                if r.clicked() {
                    ev.favourite = Some(!fav);
                }
            }
            let id = ui.make_persistent_id("imfram.pathbar");
            let focused = ui.memory(|m| m.has_focus(id));
            // Tab completes (as a terminal does) instead of moving focus.
            if focused && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Tab)) {
                self.complete();
            }
            let w = (ui.available_width() - 40.0).max(80.0);
            let r = ui.add(egui::TextEdit::singleline(&mut self.edit).id(id).hint_text(self.hint.as_str()).desired_width(w).font(egui::TextStyle::Monospace).lock_focus(true));
            if r.changed() {
                self.completion = None;
            }
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            let escape = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Escape));
            if ui.button("Go").clicked() || enter {
                let p = expand_home(&self.edit);
                if p.is_dir() {
                    ev.go = Some(p);
                } else {
                    self.edit = dir.display().to_string();
                }
            } else if escape || !r.has_focus() {
                self.edit = dir.display().to_string();
            }
        });
        ev
    }

    /// Tab: complete to what the candidates share, or (again) cycle them.
    fn complete(&mut self) {
        if let Some((cands, i)) = &mut self.completion {
            *i = (*i + 1) % cands.len();
            self.edit = with_slash(&cands[*i]);
            return;
        }
        let typed = expand_home(&self.edit).display().to_string();
        let cands = complete_path(&typed);
        match complete(&typed, &cands) {
            Some(t) => self.edit = t,
            None if cands.len() > 1 => {
                self.edit = with_slash(&cands[0]);
                self.completion = Some((cands, 0));
            }
            None => {}
        }
    }
}

/// `~` and `~/x` to the home directory (a leading tilde only).
pub fn expand_home(s: &str) -> PathBuf {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix('~').filter(|r| r.is_empty() || r.starts_with(['/', '\\'])) {
        if let Some(h) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
            return PathBuf::from(h).join(rest.trim_start_matches(['/', '\\']));
        }
    }
    PathBuf::from(s)
}

fn with_slash(p: &str) -> String {
    let sep = std::path::MAIN_SEPARATOR;
    format!("{}{sep}", p.trim_end_matches(['/', '\\']))
}

/// The folders `partial` could be completing: everything in a folder named
/// with a trailing separator, else the folders beside its last part whose
/// names start with it (case-sensitively if any do, else case-blind).
/// Hidden folders only when the part asks with a leading dot. Sorted.
pub fn complete_path(partial: &str) -> Vec<String> {
    let p = Path::new(partial);
    let ends = partial.ends_with(['/', '\\']);
    let (dir, leaf) = if ends || partial.is_empty() {
        (p.to_path_buf(), String::new())
    } else {
        (p.parent().map(Path::to_path_buf).unwrap_or_default(), p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
    };
    let dir = if dir.as_os_str().is_empty() { PathBuf::from(".") } else { dir };
    let names: Vec<(String, PathBuf)> = list_dirs_hidden(&dir, leaf.starts_with('.'));
    let exact: Vec<String> = names.iter().filter(|(n, _)| n.starts_with(&leaf)).map(|(_, p)| p.display().to_string()).collect();
    if !exact.is_empty() {
        return exact;
    }
    let low = leaf.to_lowercase();
    names.iter().filter(|(n, _)| n.to_lowercase().starts_with(&low)).map(|(_, p)| p.display().to_string()).collect()
}

/// The longest prefix all candidates share.
pub fn common_prefix(cands: &[String]) -> String {
    let Some(first) = cands.first() else { return String::new() };
    let mut end = first.len();
    for c in &cands[1..] {
        end = end.min(first.bytes().zip(c.bytes()).take_while(|(a, b)| a == b).count());
    }
    while !first.is_char_boundary(end) {
        end -= 1;
    }
    first[..end].to_string()
}

/// Terminal-style completion of `typed` from `cands`: a single match gets a
/// trailing separator; several complete to what they share. None: nothing
/// to add (the caller starts cycling).
pub fn complete(typed: &str, cands: &[String]) -> Option<String> {
    match cands {
        [] => None,
        [one] => Some(with_slash(one)),
        many => {
            let p = common_prefix(many);
            (p.len() > typed.len()).then_some(p)
        }
    }
}

// ---------------------------------------------------------------- folders

/// The folders in `dir` (not hidden ones), naturally sorted, as (name, path).
pub fn list_dirs(dir: &Path) -> Vec<(String, PathBuf)> {
    list_dirs_hidden(dir, false)
}

fn list_dirs_hidden(dir: &Path, hidden: bool) -> Vec<(String, PathBuf)> {
    let Ok(rd) = std::fs::read_dir(dir) else { return vec![] };
    let mut out: Vec<(String, PathBuf)> = rd
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false) || e.path().is_dir())
        .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
        .filter(|(n, _)| hidden || !n.starts_with('.'))
        .collect();
    #[cfg(not(target_arch = "wasm32"))]
    out.sort_by(|a, b| super::files::natural(&a.0.to_lowercase(), &b.0.to_lowercase()));
    #[cfg(target_arch = "wasm32")]
    out.sort_by_key(|a| a.0.to_lowercase());
    out
}

/// Where a folder tree starts: home, the filesystem's root(s), and mounted
/// volumes (`/Volumes` on macOS; `/media/<user>` and `/mnt` elsewhere on
/// unix; drive letters on Windows).
pub fn roots() -> Vec<(String, PathBuf)> {
    let mut out = vec![];
    if let Some(h) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from) {
        out.push(("~".to_string(), h));
    }
    #[cfg(windows)]
    for c in b'A'..=b'Z' {
        let p = PathBuf::from(format!("{}:\\", c as char));
        if p.exists() {
            out.push((format!("{}:", c as char), p));
        }
    }
    #[cfg(not(windows))]
    {
        out.push(("/".to_string(), PathBuf::from("/")));
        let mut mounts = vec![PathBuf::from("/Volumes"), PathBuf::from("/mnt")];
        if let Some(u) = std::env::var_os("USER") {
            mounts.push(Path::new("/media").join(u));
        }
        for m in mounts {
            out.extend(list_dirs(&m));
        }
    }
    out
}

/// A folder tree: children read when a folder is first unfolded (and
/// cached), the folder shown highlighted with its ancestors unfolded.
#[derive(Default)]
pub struct FolderTree {
    open: BTreeSet<PathBuf>,
    kids: HashMap<PathBuf, Vec<(String, PathBuf)>>,
    roots: Option<Vec<(String, PathBuf)>>,
    revealed: Option<PathBuf>,
}

impl FolderTree {
    pub fn new() -> Self {
        Self::default()
    }

    /// Forget what has been read (a rescan).
    pub fn refresh(&mut self) {
        self.kids.clear();
        self.roots = None;
    }

    /// Unfold every ancestor of `dir`, re-reading `dir` itself.
    pub fn reveal(&mut self, dir: &Path) {
        for a in dir.ancestors().skip(1) {
            self.open.insert(a.to_path_buf());
        }
        self.kids.remove(dir);
        self.revealed = Some(dir.to_path_buf());
    }

    fn kids(&mut self, p: &Path) -> &Vec<(String, PathBuf)> {
        self.kids.entry(p.to_path_buf()).or_insert_with(|| list_dirs(p))
    }

    /// The tree from [`roots`], with `current` lit (and revealed when it
    /// changes). A click answers that folder.
    pub fn show(&mut self, ui: &mut Ui, current: &Path) -> Option<PathBuf> {
        if self.revealed.as_deref() != Some(current) {
            self.reveal(current);
        }
        let roots = self.roots.get_or_insert_with(roots).clone();
        let mut go = None;
        for (name, path) in roots {
            self.node(ui, &name, &path, 0, current, &mut go);
        }
        go
    }

    fn node(&mut self, ui: &mut Ui, name: &str, path: &Path, depth: usize, current: &Path, go: &mut Option<PathBuf>) {
        let open = self.open.contains(path);
        let mut toggle = false;
        let r = ui.horizontal(|ui| {
            ui.add_space(depth as f32 * 12.0);
            if ui.add(egui::Button::new(if open { "⏷" } else { "⏵" }).frame(false).small()).clicked() {
                toggle = true;
            }
            let l = ui.selectable_label(path == current, name).on_hover_text(path.display().to_string());
            if l.double_clicked() {
                toggle = true;
            } else if l.clicked() {
                *go = Some(path.to_path_buf());
            }
            l
        });
        if path == current && self.revealed.as_deref() == Some(current) && !ui.is_rect_visible(r.inner.rect) {
            r.inner.scroll_to_me(Some(egui::Align::Center));
        }
        if toggle {
            if open {
                self.open.remove(path);
            } else {
                self.open.insert(path.to_path_buf());
            }
        }
        if self.open.contains(path) {
            let kids = self.kids(path).clone();
            if kids.is_empty() {
                ui.horizontal(|ui| {
                    ui.add_space((depth + 2) as f32 * 12.0);
                    ui.weak("no folders");
                });
            }
            for (n, p) in kids {
                self.node(ui, &n, &p, depth + 1, current, go);
            }
        }
    }
}

/// What a click in a [`places`] list asked for.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlacesEvent {
    pub go: Option<PathBuf>,
    /// The × on an entry (only when the list is removable).
    pub remove: Option<PathBuf>,
}

/// A titled list of folders (favourites, recents): the leaf name, the full
/// path on hover, `current` lit; `removable` adds an × to each. Nothing is
/// drawn for an empty list unless `empty_hint` says what would fill it.
pub fn places(ui: &mut Ui, title: &str, dirs: &[PathBuf], current: &Path, removable: bool, empty_hint: Option<&str>) -> PlacesEvent {
    let mut ev = PlacesEvent::default();
    if dirs.is_empty() && empty_hint.is_none() {
        return ev;
    }
    ui.strong(title);
    if dirs.is_empty() {
        if let Some(h) = empty_hint {
            ui.weak(h);
        }
    }
    for d in dirs {
        ui.horizontal(|ui| {
            let name = d.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| d.display().to_string());
            if ui.selectable_label(d == current, name).on_hover_text(d.display().to_string()).clicked() {
                ev.go = Some(d.clone());
            }
            if removable && ui.small_button("×").on_hover_text("remove").clicked() {
                ev.remove = Some(d.clone());
            }
        });
    }
    ui.add_space(6.0);
    ev
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("imfram-nav-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn completion_takes_the_shared_prefix_and_a_separator_for_one() {
        let c = vec!["/a/renders".to_string(), "/a/reports".to_string()];
        assert_eq!(complete("/a/r", &c), Some("/a/re".into()));
        assert_eq!(complete("/a/re", &c), None);
        let one = complete("/a/ren", &["/a/renders".to_string()]).unwrap();
        assert!(one.starts_with("/a/renders") && one.len() == "/a/renders/".len());
    }

    #[test]
    fn candidates_are_the_folders_that_start_with_the_last_part() {
        let d = tmp("complete");
        for n in ["renders", "Reports", "plates", ".hidden"] {
            std::fs::create_dir_all(d.join(n)).unwrap();
        }
        std::fs::write(d.join("readme.txt"), b"x").unwrap();
        let base = d.display().to_string();
        let leaf = |v: Vec<String>| v.into_iter().map(|p| Path::new(&p).file_name().unwrap().to_string_lossy().into_owned()).collect::<Vec<_>>();
        assert_eq!(leaf(complete_path(&format!("{base}/re"))), ["renders"], "case-sensitive first, files never");
        assert_eq!(leaf(complete_path(&format!("{base}/RE"))), ["renders", "Reports"], "else case-blind");
        assert_eq!(leaf(complete_path(&format!("{base}/"))), ["plates", "renders", "Reports"], "a trailing slash lists the folder");
        assert_eq!(leaf(complete_path(&format!("{base}/.h"))), [".hidden"], "a dot asks for hidden ones");
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn history_follows_the_folder_however_it_changed() {
        let mut b = PathBar::new();
        b.track(Path::new("/a"));
        b.track(Path::new("/b"));
        b.track(Path::new("/c"));
        assert_eq!(b.step(true), Some(PathBuf::from("/b")));
        b.track(Path::new("/b"));
        assert_eq!(b.step(true), Some(PathBuf::from("/a")));
        b.track(Path::new("/a"));
        assert!(!b.can_back());
        assert_eq!(b.step(false), Some(PathBuf::from("/b")));
        b.track(Path::new("/b"));
        // A new visit drops what was forward.
        b.track(Path::new("/z"));
        assert!(!b.can_forward());
        assert_eq!(b.step(true), Some(PathBuf::from("/b")));
    }

    #[test]
    fn a_tree_lists_folders_only_naturally_sorted() {
        let d = tmp("tree");
        for n in ["shot10", "shot2", ".git"] {
            std::fs::create_dir_all(d.join(n)).unwrap();
        }
        std::fs::write(d.join("a.exr"), b"x").unwrap();
        let names: Vec<String> = list_dirs(&d).into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, ["shot2", "shot10"]);
        let _ = std::fs::remove_dir_all(d);
    }
}
