//! Several composers under names (layouts), one shown at a time: a bar
//! above the rows picks it, and makes, copies, renames and deletes them.
//! Every layout keeps its edits as they are made; switching loses nothing.

use egui::{Id, Ui};
use serde::{Deserialize, Serialize};

use super::{Composer, Entry};

/// One named composer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Layout {
    pub name: String,
    #[serde(flatten)]
    pub rows: Composer,
}

/// Named layouts and the one shown. Save it (serde) to keep them all. It
/// reads a lone [`Composer`]'s file too, as one layout, "main".
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "LibraryFile")]
pub struct Library {
    /// The shown layout's name.
    pub current: String,
    /// Never empty.
    pub layouts: Vec<Layout>,
}

#[derive(Deserialize)]
struct LibraryFile {
    #[serde(default)]
    current: String,
    #[serde(default)]
    layouts: Vec<Layout>,
    /// A lone composer's rows (the file before layouts).
    #[serde(default)]
    entries: Vec<Entry>,
}

impl From<LibraryFile> for Library {
    fn from(f: LibraryFile) -> Self {
        let mut layouts = f.layouts;
        if !f.entries.is_empty() || layouts.is_empty() {
            layouts.insert(0, Layout { name: "main".into(), rows: Composer { entries: f.entries } });
        }
        let mut lib = Library { current: f.current, layouts };
        lib.settle();
        lib
    }
}

impl Default for Library {
    fn default() -> Self {
        Self { current: "main".into(), layouts: vec![Layout { name: "main".into(), rows: Composer::default() }] }
    }
}

impl std::ops::Deref for Library {
    type Target = Composer;
    fn deref(&self) -> &Composer {
        &self.layouts[self.index()].rows
    }
}

impl std::ops::DerefMut for Library {
    fn deref_mut(&mut self) -> &mut Composer {
        let i = self.index();
        &mut self.layouts[i].rows
    }
}

/// What the bar asked for this frame.
enum Act {
    Switch(String),
    New,
    Copy,
    Rename(String),
    Delete,
}

impl Library {
    /// The shown layout's place in `layouts`.
    pub fn index(&self) -> usize {
        self.layouts.iter().position(|l| l.name == self.current).unwrap_or(0)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.layouts.iter().map(|l| l.name.as_str())
    }

    /// Show the layout `name`; false when there is none.
    pub fn switch(&mut self, name: &str) -> bool {
        let there = self.layouts.iter().any(|l| l.name == name);
        if there {
            self.current = name.to_owned();
        }
        there
    }

    /// Add a layout holding `rows`, named `base` (numbered if taken), and
    /// show it; its name.
    pub fn add(&mut self, base: &str, rows: Composer) -> String {
        let name = self.unique(base);
        let at = self.index() + 1;
        self.layouts.insert(at.min(self.layouts.len()), Layout { name: name.clone(), rows });
        self.current = name.clone();
        name
    }

    /// Rename the shown layout (numbered if the name is taken); its name.
    pub fn rename(&mut self, to: &str) -> String {
        let to = to.trim();
        let i = self.index();
        if to.is_empty() || to == self.layouts[i].name {
            return self.layouts[i].name.clone();
        }
        let name = self.unique(to);
        self.layouts[i].name = name.clone();
        self.current = name.clone();
        name
    }

    /// Delete the shown layout, unless it is the last; the one before it
    /// is shown then. True when deleted.
    pub fn delete(&mut self) -> bool {
        if self.layouts.len() < 2 {
            return false;
        }
        let i = self.index();
        self.layouts.remove(i);
        self.current = self.layouts[i.saturating_sub(1)].name.clone();
        true
    }

    fn unique(&self, base: &str) -> String {
        let base = if base.trim().is_empty() { "layout" } else { base.trim() };
        let taken = |n: &str| self.layouts.iter().any(|l| l.name == n);
        if !taken(base) {
            return base.to_owned();
        }
        (2..).map(|n| format!("{base} {n}")).find(|n| !taken(n)).unwrap()
    }

    /// Fix a file read by hand: no layouts, names repeated, or the shown
    /// one not there.
    fn settle(&mut self) {
        if self.layouts.is_empty() {
            *self = Self::default();
            return;
        }
        let mut seen = Vec::<String>::new();
        for l in &mut self.layouts {
            let mut name = if l.name.trim().is_empty() { "layout".to_owned() } else { l.name.clone() };
            if seen.contains(&name) {
                name = (2..).map(|n| format!("{name} {n}")).find(|n| !seen.contains(n)).unwrap();
            }
            l.name = name.clone();
            seen.push(name);
        }
        if !seen.contains(&self.current) {
            self.current = seen[0].clone();
        }
    }

    /// The layout bar, then the shown layout's composer (`Composer::show`).
    /// True when anything changed (worth saving).
    pub fn show(&mut self, ui: &mut Ui, sources: impl FnOnce(&mut Ui)) -> bool {
        let me = ui.id().with("composer-library");
        let mut act = None;
        ui.horizontal(|ui| {
            ui.label("layout");
            egui::ComboBox::from_id_salt(me.with("pick")).selected_text(self.current.as_str()).show_ui(ui, |ui| {
                for l in &self.layouts {
                    if ui.selectable_label(l.name == self.current, &l.name).clicked() && l.name != self.current {
                        act = Some(Act::Switch(l.name.clone()));
                    }
                }
            });
            if ui.small_button("new").on_hover_text("a new, empty layout").clicked() {
                act = Some(Act::New);
            }
            if ui.small_button("copy").on_hover_text("a new layout holding this one's rows").clicked() {
                act = Some(Act::Copy);
            }
            ui.menu_button("…", |ui| {
                if let Some(to) = rename_field(ui, me.with(("rename", &self.current)), &self.current) {
                    act = Some(Act::Rename(to));
                }
                ui.separator();
                if ui.add_enabled(self.layouts.len() > 1, egui::Button::new("delete this layout")).on_disabled_hover_text("the last one stays").clicked() {
                    act = Some(Act::Delete);
                    ui.close();
                }
            })
            .response
            .on_hover_text("rename or delete this layout");
        });
        let mut changed = match act {
            Some(Act::Switch(name)) => self.switch(&name),
            Some(Act::New) => {
                self.add("layout", Composer::default());
                true
            }
            Some(Act::Copy) => {
                let (name, rows) = (format!("{} copy", self.current), (**self).clone());
                self.add(&name, rows);
                true
            }
            Some(Act::Rename(to)) => {
                self.rename(&to);
                true
            }
            Some(Act::Delete) => self.delete(),
            None => false,
        };
        // (Each layout's rows under its own id: row heights and drags kept apart.)
        let i = self.index();
        let salt = self.layouts[i].name.clone();
        changed |= ui.push_id(("layout", salt), |ui| self.layouts[i].rows.show(ui, sources)).inner;
        changed
    }
}

/// A rename field in a menu: Enter or the button takes it. `Some` with the
/// new name.
fn rename_field(ui: &mut Ui, id: Id, now: &str) -> Option<String> {
    let mut text: String = ui.data(|d| d.get_temp(id)).unwrap_or_else(|| now.to_owned());
    ui.label("name");
    let field = ui.text_edit_singleline(&mut text);
    let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    let mut out = None;
    if ui.button("rename").clicked() || enter {
        let t = text.trim();
        if !t.is_empty() && t != now {
            out = Some(t.to_owned());
        }
        ui.data_mut(|d| d.remove::<String>(id));
        ui.close();
    } else {
        ui.data_mut(|d| d.insert_temp(id, text));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(k: &str) -> Entry {
        Entry { key: k.into(), name: k.into(), label: None }
    }

    #[test]
    fn an_old_composer_file_reads_as_main() {
        let lib: Library = serde_json::from_str(r#"{"entries":[{"key":"A/x","name":"x"}]}"#).unwrap();
        assert_eq!(lib.current, "main");
        assert_eq!(lib.layouts.len(), 1);
        assert_eq!(lib.entries, vec![Entry { key: "A/x".into(), name: "x".into(), label: None }]);
    }

    #[test]
    fn layouts_round_trip_and_keep_their_rows_apart() {
        let mut lib = Library::default();
        lib.push(row("A/x"));
        lib.add("tuning", Composer::default());
        lib.push(row("B/y"));
        lib.push_divider("sky");
        let text = serde_json::to_string(&lib).unwrap();
        let back: Library = serde_json::from_str(&text).unwrap();
        assert_eq!(back, lib);
        assert_eq!(back.current, "tuning");
        assert_eq!(back.entries.len(), 2);
        assert!(back.entries[1].is_divider());
        let mut back = back;
        assert!(back.switch("main"));
        assert_eq!(back.entries, vec![row("A/x")]);
    }

    #[test]
    fn names_stay_unique_and_the_last_layout_stays() {
        let mut lib = Library::default();
        assert_eq!(lib.add("main", Composer::default()), "main 2");
        assert_eq!(lib.rename("main"), "main 3");
        assert!(lib.delete());
        assert_eq!(lib.current, "main");
        assert!(!lib.delete());
        let lib: Library = serde_json::from_str(r#"{"current":"gone","layouts":[{"name":"a","entries":[]},{"name":"a","entries":[]}]}"#).unwrap();
        assert_eq!(lib.names().collect::<Vec<_>>(), vec!["a", "a 2"]);
        assert_eq!(lib.current, "a");
    }
}
