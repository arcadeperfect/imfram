//! Type a name to jump to it: letters and digits typed over a view (while
//! no text field has the keyboard) collect into a buffer that lapses after
//! a pause, and the first name starting with the buffer is the answer.
//! The Finder's hand, and seq_manip's.

use web_time::Instant;

/// How long a pause starts a fresh buffer.
pub const LAPSE_SECS: f32 = 2.0;

#[derive(Default)]
pub struct TypeAhead {
    typed: String,
    at: Option<Instant>,
}

impl TypeAhead {
    pub fn new() -> Self {
        Self::default()
    }

    /// What has been typed so far (empty once lapsed).
    pub fn typed(&self) -> &str {
        &self.typed
    }

    /// This frame's typing over `ui` (letters and digits, no command
    /// modifier, none of `skip`'s characters: those are the view's own
    /// hotkeys). Answers the first of `names`, in order, starting with the
    /// buffer (case-blind) when something was typed. Does nothing while a
    /// text field wants the keyboard or the pointer is elsewhere.
    pub fn feed<'a>(&mut self, ui: &egui::Ui, skip: &str, names: impl IntoIterator<Item = &'a str>) -> Option<usize> {
        if ui.ctx().egui_wants_keyboard_input() || !ui.ui_contains_pointer() {
            return None;
        }
        let typed: String = ui.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Text(t) if !i.modifiers.command && !i.modifiers.ctrl => Some(t.clone()),
                    _ => None,
                })
                .collect()
        });
        let typed: String = typed.chars().filter(|c| c.is_alphanumeric() && !skip.contains(*c)).collect();
        self.push(&typed)?;
        first_match(names, &self.typed)
    }

    /// Add `s` to the buffer (a fresh one after a lapse). None when `s` is
    /// empty: nothing to look for.
    pub fn push(&mut self, s: &str) -> Option<()> {
        if s.is_empty() {
            return None;
        }
        if self.at.is_none_or(|t| t.elapsed().as_secs_f32() > LAPSE_SECS) {
            self.typed.clear();
        }
        self.typed.push_str(s);
        self.at = Some(Instant::now());
        Some(())
    }
}

/// The index of the first name starting with `typed`, case-blind.
pub fn first_match<'a>(names: impl IntoIterator<Item = &'a str>, typed: &str) -> Option<usize> {
    let t = typed.to_lowercase();
    names.into_iter().position(|n| n.to_lowercase().starts_with(&t))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_name_with_the_prefix_wins_case_blind() {
        let names = ["alpha", "Beta", "bravo", "charlie"];
        assert_eq!(first_match(names, "b"), Some(1));
        assert_eq!(first_match(names, "BR"), Some(2));
        assert_eq!(first_match(names, "z"), None);
    }

    #[test]
    fn typing_builds_up_until_it_lapses() {
        let mut t = TypeAhead::new();
        assert!(t.push("").is_none());
        t.push("b").unwrap();
        t.push("r").unwrap();
        assert_eq!(t.typed(), "br");
        t.at = Some(Instant::now() - std::time::Duration::from_secs(3));
        t.push("c").unwrap();
        assert_eq!(t.typed(), "c");
    }
}
