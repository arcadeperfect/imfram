//! A command palette: ⌘K-style. A centred card with a focused search field,
//! fuzzy-matched commands under group headers, recent picks floated to the
//! top. The app owns a [`Palette`], hands its commands in as any slice of
//! [`Command`]s and gets the chosen id back the frame it is picked.
//!
//! The matcher ([`fuzzy`]) is a plain subsequence match: every query char
//! in order somewhere in the title. Runs of consecutive hits and hits on
//! word starts score higher, so "sv" finds "Save" over "Preferences".
//! Colours come from the context's `Visuals`.

use egui::text::LayoutJob;
use egui::{Align2, Area, Frame, Id, Key, Modifiers, Order, RichText, TextFormat, Ui};

/// One entry the palette can offer.
pub trait Command {
    /// What the app gets back when this is picked.
    type Id: Clone + std::hash::Hash + std::fmt::Debug;
    fn id(&self) -> Self::Id;
    fn title(&self) -> &str;
    /// Header the command lists under; "" for none.
    fn group(&self) -> &str {
        ""
    }
    /// A key hint shown at the right, e.g. "⌘S".
    fn keys(&self) -> &str {
        ""
    }
}

const PALETTE_W: f32 = 480.0;
const LIST_H: f32 = 320.0;
const ROW_H: f32 = 24.0;
/// How far down the window the card sits (a fraction of its height).
const TOP_FRACTION: f32 = 0.14;
const RECENT_MAX: usize = 6;
const RECENT_LABEL: &str = "Recent";

/// Open/closed state, the typed query and the keyboard cursor.
#[derive(Default)]
pub struct Palette {
    open: bool,
    query: String,
    cursor: usize,
    /// Most recent pick first, as egui ids of the commands' ids.
    recent: Vec<Id>,
    /// The field wants focus on the next frame (just opened).
    focus: bool,
    /// The cursor moved by key this frame: scroll it into view.
    scroll: bool,
}

/// A hit of [`fuzzy`]: its score and which chars (by char index) hit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match {
    pub score: i32,
    pub hits: Vec<usize>,
}

/// One drawn row: a group header or the command at `index` in the slice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Row {
    Header(String),
    Item { index: usize, hits: Vec<usize> },
}

impl Palette {
    pub fn open(&mut self) {
        self.open = true;
        self.focus = true;
        self.query.clear();
        self.cursor = 0;
    }
    pub fn close(&mut self) {
        self.open = false;
    }
    pub fn toggle(&mut self) {
        if self.open {
            self.close();
        } else {
            self.open();
        }
    }
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Draws the palette if open; the picked command's id, the frame it is picked.
    pub fn show<C: Command>(&mut self, ctx: &egui::Context, items: &[C]) -> Option<C::Id> {
        if !self.open {
            return None;
        }
        // Keys are read before the field sees them: it would eat Enter and Escape.
        let [down, up, enter, esc] = ctx.input_mut(|i| [Key::ArrowDown, Key::ArrowUp, Key::Enter, Key::Escape].map(|k| i.consume_key(Modifiers::NONE, k)));
        if esc {
            self.close();
            return None;
        }
        let rows = self.rows(items);
        let count = rows.iter().filter(|r| matches!(r, Row::Item { .. })).count().max(1);
        if down || up {
            self.cursor = (self.cursor + count + usize::from(down) - usize::from(up)) % count;
            self.scroll = true;
        }
        self.cursor = self.cursor.min(count - 1);

        let content = ctx.content_rect();
        let mut picked: Option<usize> = if enter { nth_item(&rows, self.cursor) } else { None };
        let area = Area::new(Id::new("imfram.palette"))
            .order(Order::Foreground)
            .anchor(Align2::CENTER_TOP, egui::vec2(0.0, content.height() * TOP_FRACTION))
            .show(ctx, |ui| {
                Frame::window(ui.style()).show(ui, |ui| {
                    ui.set_width(PALETTE_W);
                    let field = egui::TextEdit::singleline(&mut self.query)
                        .hint_text("Type a command…")
                        .desired_width(f32::INFINITY)
                        .show(ui)
                        .response
                        .response;
                    if self.focus {
                        field.request_focus();
                        self.focus = false;
                    }
                    if field.changed() {
                        self.cursor = 0;
                    }
                    ui.separator();
                    egui::ScrollArea::vertical().max_height(LIST_H).show(ui, |ui| {
                        if rows.is_empty() {
                            ui.weak("No matches");
                        }
                        let mut nth = 0;
                        for row in &rows {
                            match row {
                                Row::Header(name) => {
                                    ui.add_space(4.0);
                                    ui.label(RichText::new(name).small().weak());
                                }
                                Row::Item { index, hits } => {
                                    let selected = nth == self.cursor;
                                    let r = item_row(ui, &items[*index], hits, selected);
                                    if selected && self.scroll {
                                        ui.scroll_to_rect(r.rect, None);
                                    }
                                    if r.clicked() {
                                        picked = Some(*index);
                                    }
                                    nth += 1;
                                }
                            }
                        }
                    });
                });
            });
        self.scroll = false;
        // A click anywhere else dismisses it.
        let outside = ctx.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !area.response.rect.contains(p)));
        if outside {
            self.close();
        }
        picked.map(|i| {
            let id = items[i].id();
            self.remember(Id::new(&id));
            self.close();
            id
        })
    }

    fn remember(&mut self, id: Id) {
        self.recent.retain(|r| *r != id);
        self.recent.insert(0, id);
        self.recent.truncate(RECENT_MAX);
    }

    /// The rows for the current query: recents first, then each group under
    /// its header, best matches first within a group; groups ordered by
    /// their best match.
    pub fn rows<C: Command>(&self, items: &[C]) -> Vec<Row> {
        let mut scored: Vec<(usize, Match, bool)> = items
            .iter()
            .enumerate()
            .filter_map(|(i, c)| fuzzy(&self.query, c.title()).map(|m| (i, m, self.recent.contains(&Id::new(c.id())))))
            .collect();
        // Recents keep their MRU order; the rest sort by score.
        scored.sort_by_key(|(i, m, recent)| {
            let rank = if *recent { self.recent.iter().position(|r| *r == Id::new(items[*i].id())).unwrap_or(0) as i32 } else { i32::MAX };
            (rank, -m.score, *i)
        });
        let mut rows = vec![];
        let mut groups: Vec<(&str, Vec<(usize, Vec<usize>)>)> = vec![];
        for (i, m, recent) in scored {
            let g = if recent { RECENT_LABEL } else { items[i].group() };
            match groups.iter_mut().find(|(name, _)| *name == g) {
                Some((_, list)) => list.push((i, m.hits)),
                None => groups.push((g, vec![(i, m.hits)])),
            }
        }
        for (name, list) in groups {
            if !name.is_empty() {
                rows.push(Row::Header(name.to_string()));
            }
            for (index, hits) in list {
                rows.push(Row::Item { index, hits });
            }
        }
        rows
    }
}

fn nth_item(rows: &[Row], n: usize) -> Option<usize> {
    rows.iter().filter_map(|r| if let Row::Item { index, .. } = r { Some(*index) } else { None }).nth(n)
}

fn item_row<C: Command>(ui: &mut Ui, c: &C, hits: &[usize], selected: bool) -> egui::Response {
    let vis = ui.visuals().clone();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), ROW_H), egui::Sense::click());
    if selected || resp.hovered() {
        let fill = if selected { vis.selection.bg_fill } else { vis.widgets.hovered.weak_bg_fill };
        ui.painter().rect_filled(rect, vis.widgets.inactive.corner_radius, fill);
    }
    let font = egui::TextStyle::Body.resolve(ui.style());
    let plain = if selected { vis.selection.stroke.color } else { vis.text_color() };
    let hit = if selected { vis.strong_text_color() } else { vis.selection.stroke.color };
    let mut job = LayoutJob::default();
    for (i, ch) in c.title().chars().enumerate() {
        let color = if hits.contains(&i) { hit } else { plain };
        job.append(&ch.to_string(), 0.0, TextFormat { font_id: font.clone(), color, ..Default::default() });
    }
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    let pad = 6.0;
    ui.painter().galley(egui::pos2(rect.min.x + pad, rect.center().y - galley.size().y * 0.5), galley, plain);
    if !c.keys().is_empty() {
        ui.painter().text(
            egui::pos2(rect.max.x - pad, rect.center().y),
            Align2::RIGHT_CENTER,
            c.keys(),
            egui::TextStyle::Small.resolve(ui.style()),
            if selected { plain } else { vis.weak_text_color() },
        );
    }
    resp
}

const HIT: i32 = 1;
const RUN_BONUS: i32 = 3;
const WORD_START_BONUS: i32 = 4;
/// Each char the first hit sits in from the start costs this.
const LEAD_PENALTY: i32 = 1;
/// Each skipped char between two hits costs this (capped per gap), so a
/// solid run beats letters picked from across a long title.
const GAP_PENALTY: i32 = 1;
const GAP_CAP: i32 = 3;

/// Case-blind subsequence match of `query` in `text`. `None` when a query
/// char is missing; an empty query matches everything at score 0.
pub fn fuzzy(query: &str, text: &str) -> Option<Match> {
    let q: Vec<char> = query.chars().filter(|c| !c.is_whitespace()).flat_map(char::to_lowercase).collect();
    let t: Vec<char> = text.chars().collect();
    let tl: Vec<char> = t.iter().flat_map(|c| c.to_lowercase()).collect();
    if q.is_empty() {
        return Some(Match { score: 0, hits: vec![] });
    }
    let word_start = |i: usize| i == 0 || !t[i - 1].is_alphanumeric() || (t[i - 1].is_lowercase() && t[i].is_uppercase());
    // Greedy, preferring a hit that starts a word or continues the run when
    // one is near; falls back to the first occurrence.
    let mut hits = Vec::with_capacity(q.len());
    let mut at = 0usize;
    for &qc in &q {
        let first = (at..tl.len()).find(|&i| tl[i] == qc)?;
        let good = (at..tl.len()).find(|&i| tl[i] == qc && (word_start(i) || hits.last() == Some(&(i.wrapping_sub(1)))));
        let pick = match good {
            Some(g) if remaining_fits(&tl, &q, hits.len() + 1, g + 1) => g,
            _ => first,
        };
        hits.push(pick);
        at = pick + 1;
    }
    let mut score = -(hits[0].min(10) as i32) * LEAD_PENALTY;
    for (k, &i) in hits.iter().enumerate() {
        score += HIT;
        if word_start(i) {
            score += WORD_START_BONUS;
        }
        if k > 0 {
            let gap = (i - hits[k - 1] - 1) as i32;
            score += if gap == 0 { RUN_BONUS } else { -gap.min(GAP_CAP) * GAP_PENALTY };
        }
    }
    Some(Match { score, hits })
}

/// Can `q[from..]` still be matched inside `t[start..]`?
fn remaining_fits(t: &[char], q: &[char], from: usize, start: usize) -> bool {
    let mut at = start;
    for &qc in &q[from..] {
        match (at..t.len()).find(|&i| t[i] == qc) {
            Some(i) => at = i + 1,
            None => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Cmd(&'static str, &'static str, &'static str);
    impl Command for Cmd {
        type Id = &'static str;
        fn id(&self) -> &'static str { self.0 }
        fn title(&self) -> &str { self.0 }
        fn group(&self) -> &str { self.1 }
        fn keys(&self) -> &str { self.2 }
    }
    fn toys() -> Vec<Cmd> {
        vec![Cmd("Save", "File", "⌘S"), Cmd("Save As", "File", ""), Cmd("Preferences", "Edit", ""), Cmd("Undo", "Edit", "⌘Z")]
    }

    #[test]
    fn a_subsequence_matches_and_out_of_order_letters_do_not() {
        assert!(fuzzy("sv", "Save").is_some());
        assert!(fuzzy("vs", "Save").is_none());
        assert_eq!(fuzzy("sv", "Save").unwrap().hits, vec![0, 2]);
    }

    #[test]
    fn consecutive_runs_beat_scattered_letters() {
        let run = fuzzy("sav", "Save").unwrap().score;
        let scattered = fuzzy("sav", "Stop all video").unwrap().score;
        assert!(run > scattered, "{run} vs {scattered}");
    }

    #[test]
    fn word_starts_beat_mid_word_hits() {
        let starts = fuzzy("as", "Save As").unwrap();
        assert_eq!(starts.hits, vec![5, 6], "picks the word start over the 'a' in Save");
        assert!(starts.score > fuzzy("as", "Bias").unwrap().score);
        assert_eq!(fuzzy("sa", "Save As").unwrap().hits, vec![0, 1], "a run is kept when it is right there");
    }

    #[test]
    fn matching_is_case_blind_and_an_empty_query_matches_everything() {
        assert!(fuzzy("PREF", "preferences").is_some());
        assert_eq!(fuzzy("", "anything"), Some(Match { score: 0, hits: vec![] }));
        assert_eq!(fuzzy("  ", "x").unwrap().score, 0);
    }

    #[test]
    fn rows_group_under_headers_with_recents_first() {
        let mut p = Palette::default();
        let items = toys();
        let rows = p.rows(&items);
        assert_eq!(rows[0], Row::Header("File".into()));
        assert_eq!(rows[3], Row::Header("Edit".into()));
        p.remember(Id::new("Undo"));
        let rows = p.rows(&items);
        assert_eq!(rows[0], Row::Header(RECENT_LABEL.into()));
        assert!(matches!(rows[1], Row::Item { index: 3, .. }));
        p.query = "sv".into();
        let rows = p.rows(&items);
        assert_eq!(rows.iter().filter(|r| matches!(r, Row::Item { .. })).count(), 2);
    }

    fn frame(ctx: &egui::Context, p: &mut Palette, items: &[Cmd], events: Vec<egui::Event>) -> Option<&'static str> {
        let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))), events, ..Default::default() };
        let mut got = None;
        let out = ctx.run_ui(input, |ui| got = p.show(ui.ctx(), items));
        crate::snapshot::discard(out);
        got
    }
    fn key(k: Key) -> egui::Event {
        egui::Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }
    }

    #[test]
    fn arrow_down_then_enter_picks_the_second_command_and_remembers_it() {
        let ctx = egui::Context::default();
        let items = toys();
        let mut p = Palette::default();
        p.open();
        assert_eq!(frame(&ctx, &mut p, &items, vec![]), None);
        assert_eq!(frame(&ctx, &mut p, &items, vec![key(Key::ArrowDown)]), None);
        assert_eq!(frame(&ctx, &mut p, &items, vec![key(Key::Enter)]), Some("Save As"));
        assert!(!p.is_open());
        assert_eq!(p.recent, vec![Id::new("Save As")]);
    }

    #[test]
    fn escape_closes_and_a_closed_palette_draws_nothing() {
        let ctx = egui::Context::default();
        let items = toys();
        let mut p = Palette::default();
        p.toggle();
        assert!(p.is_open());
        assert_eq!(frame(&ctx, &mut p, &items, vec![key(Key::Escape)]), None);
        assert!(!p.is_open());
        assert_eq!(frame(&ctx, &mut p, &items, vec![key(Key::Enter)]), None);
    }

    #[test]
    fn the_card_sits_near_the_top_centre_once_sized() {
        // An Area's first frame only measures; the second places it.
        let ctx = egui::Context::default();
        let items = toys();
        let mut p = Palette::default();
        p.open();
        frame(&ctx, &mut p, &items, vec![]);
        frame(&ctx, &mut p, &items, vec![]);
        let rect = egui::AreaState::load(&ctx, Id::new("imfram.palette")).unwrap().rect();
        assert!((rect.center().x - 400.0).abs() < 1.0, "{rect:?}");
        assert!((rect.min.y - 600.0 * TOP_FRACTION).abs() < 1.0, "{rect:?}");
        assert!(rect.width() >= PALETTE_W && rect.width() < PALETTE_W + 40.0, "{rect:?}");
    }
}
