//! Modal dialogs. Each is app-owned state shown every frame it is up: a
//! dimmed scrim over the whole window that eats clicks, a centred card,
//! Enter for the main answer and Escape to back out. [`Confirm`] asks
//! yes/no, [`Progress`] reports a long job (Cancel when asked),
//! [`Prompt`] asks for one line of text. [`SettingsFrame`] is not a modal
//! but the layout a settings window wants: a filterable page list on the
//! left, the chosen page on the right. Colours come from `Visuals`.

use egui::{Align2, Area, Frame, Id, Key, Modifiers, Order, Response, Sense, Ui};

const CARD_W: f32 = 360.0;
const CARD_GAP: f32 = 12.0;
const PAGE_LIST_W: f32 = 150.0;
/// Alpha of the scrim over the window (its hue is the theme's shadow).
const SCRIM_ALPHA: u8 = 110;

/// A dimmed, click-eating layer over the whole window. Returns its response
/// (a click on it is a click on nothing).
pub fn scrim(ctx: &egui::Context, id: Id) -> Response {
    let rect = ctx.content_rect();
    Area::new(id)
        .order(Order::Foreground)
        .fixed_pos(rect.min)
        .sense(Sense::click())
        .show(ctx, |ui| {
            let (r, resp) = ui.allocate_exact_size(rect.size(), Sense::click());
            let c = ui.visuals().window_shadow.color;
            ui.painter().rect_filled(r, 0.0, egui::Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), SCRIM_ALPHA));
            resp
        })
        .inner
}

/// A centred card above a scrim; `body` fills it.
pub fn card<R>(ctx: &egui::Context, id: Id, body: impl FnOnce(&mut Ui) -> R) -> R {
    scrim(ctx, id.with("scrim"));
    let out = Area::new(id)
        .order(Order::Foreground)
        .anchor(Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            Frame::window(ui.style()).show(ui, |ui| {
                ui.set_width(CARD_W);
                body(ui)
            })
        });
    ctx.move_to_top(out.response.layer_id);
    out.inner.inner
}

/// Reads and consumes Enter / Escape, in that order.
fn enter_esc(ctx: &egui::Context) -> (bool, bool) {
    ctx.input_mut(|i| (i.consume_key(Modifiers::NONE, Key::Enter), i.consume_key(Modifiers::NONE, Key::Escape)))
}

/// A yes/no question. `show` answers `Some(true)` for yes (button or
/// Enter), `Some(false)` for no (button or Escape), `None` while open.
pub struct Confirm<'a> {
    title: &'a str,
    body: &'a str,
    yes: &'a str,
    no: &'a str,
}

impl<'a> Confirm<'a> {
    pub fn new(title: &'a str, body: &'a str) -> Self {
        Self { title, body, yes: "OK", no: "Cancel" }
    }
    pub fn yes(mut self, label: &'a str) -> Self {
        self.yes = label;
        self
    }
    pub fn no(mut self, label: &'a str) -> Self {
        self.no = label;
        self
    }
    pub fn show(self, ctx: &egui::Context) -> Option<bool> {
        let (enter, esc) = enter_esc(ctx);
        let mut answer = if enter { Some(true) } else if esc { Some(false) } else { None };
        card(ctx, Id::new("imfram.confirm"), |ui| {
            ui.heading(self.title);
            ui.add_space(CARD_GAP * 0.5);
            ui.label(self.body);
            ui.add_space(CARD_GAP);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(self.yes).clicked() {
                    answer = Some(true);
                }
                if ui.button(self.no).clicked() {
                    answer = Some(false);
                }
            });
        });
        answer
    }
}

/// What the user did to a progress card this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgressEvent {
    None,
    Cancel,
}

/// A long job's card: a bar at `fraction` (0..=1), a label, a Cancel button
/// (and Escape) when `cancellable`.
pub struct Progress {
    pub fraction: f32,
    pub label: String,
    pub cancellable: bool,
}

impl Progress {
    pub fn show(&self, ctx: &egui::Context) -> ProgressEvent {
        let (_, esc) = enter_esc(ctx);
        let mut ev = if esc && self.cancellable { ProgressEvent::Cancel } else { ProgressEvent::None };
        card(ctx, Id::new("imfram.progress"), |ui| {
            ui.label(&self.label);
            ui.add(egui::ProgressBar::new(self.fraction.clamp(0.0, 1.0)).show_percentage());
            if self.cancellable {
                ui.add_space(CARD_GAP * 0.5);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Cancel").clicked() {
                        ev = ProgressEvent::Cancel;
                    }
                });
            }
        });
        ev
    }
}

/// A one-line text ask. `show` answers `Some(Some(text))` on OK / Enter,
/// `Some(None)` on Cancel / Escape, `None` while open. The field takes
/// focus the first frame it is shown.
pub struct Prompt {
    pub title: String,
    pub text: String,
    pub hint: String,
    pub ok: String,
    fresh: bool,
}

impl Prompt {
    pub fn new(title: &str) -> Self {
        Self { title: title.into(), text: String::new(), hint: String::new(), ok: "OK".into(), fresh: true }
    }
    pub fn text(mut self, text: &str) -> Self {
        self.text = text.into();
        self
    }
    pub fn hint(mut self, hint: &str) -> Self {
        self.hint = hint.into();
        self
    }
    pub fn ok(mut self, label: &str) -> Self {
        self.ok = label.into();
        self
    }
    pub fn show(&mut self, ctx: &egui::Context) -> Option<Option<String>> {
        let (enter, esc) = enter_esc(ctx);
        let mut answer = if enter { Some(true) } else if esc { Some(false) } else { None };
        card(ctx, Id::new("imfram.prompt"), |ui| {
            ui.heading(&self.title);
            ui.add_space(CARD_GAP * 0.5);
            let field = egui::TextEdit::singleline(&mut self.text).hint_text(&self.hint).desired_width(f32::INFINITY).show(ui).response.response;
            if self.fresh {
                field.request_focus();
                self.fresh = false;
            }
            ui.add_space(CARD_GAP);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(&self.ok).clicked() {
                    answer = Some(true);
                }
                if ui.button("Cancel").clicked() {
                    answer = Some(false);
                }
            });
        });
        answer.map(|yes| yes.then(|| self.text.clone()))
    }
}

/// A settings window's layout: a page list on the left filtered by a
/// search field, the chosen page on the right. The app owns the state and
/// draws each page through `content(ui, page_index)`.
#[derive(Default, Clone, Debug)]
pub struct SettingsFrame {
    pub page: usize,
    pub search: String,
}

impl SettingsFrame {
    pub fn show(&mut self, ui: &mut Ui, pages: &[&str], content: impl FnOnce(&mut Ui, usize)) {
        let shown = filtered(pages, &self.search);
        egui::Panel::left(ui.id().with("imfram.settings.pages")).exact_size(PAGE_LIST_W).resizable(false).show(ui, |ui| {
            egui::TextEdit::singleline(&mut self.search).hint_text("Search").desired_width(f32::INFINITY).show(ui);
            ui.add_space(CARD_GAP * 0.5);
            if shown.is_empty() {
                ui.weak("No pages");
            }
            for &i in &shown {
                if ui.selectable_label(self.page == i, pages[i]).clicked() {
                    self.page = i;
                }
            }
        });
        if !pages.is_empty() {
            let page = if shown.contains(&self.page) || shown.is_empty() { self.page.min(pages.len() - 1) } else { shown[0] };
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading(pages[page]);
                ui.separator();
                content(ui, page);
            });
        }
    }
}

/// Indices of the pages whose names contain every word of `search`, case-blind.
pub fn filtered(pages: &[&str], search: &str) -> Vec<usize> {
    let words: Vec<String> = search.split_whitespace().map(str::to_lowercase).collect();
    pages
        .iter()
        .enumerate()
        .filter(|(_, p)| {
            let l = p.to_lowercase();
            words.iter().all(|w| l.contains(w.as_str()))
        })
        .map(|(i, _)| i)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: egui::Vec2 = egui::vec2(800.0, 600.0);

    fn run<R>(ctx: &egui::Context, events: Vec<egui::Event>, mut f: impl FnMut(&egui::Context) -> R) -> R {
        let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SCREEN)), events, ..Default::default() };
        let mut got = None;
        let out = ctx.run_ui(input, |ui| got = Some(f(ui.ctx())));
        crate::snapshot::discard(out);
        got.unwrap()
    }
    fn key(k: Key) -> egui::Event {
        egui::Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }
    }
    fn press(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE }
    }

    #[test]
    fn enter_confirms_and_escape_declines() {
        let ctx = egui::Context::default();
        let ask = |ctx: &egui::Context| Confirm::new("Delete?", "It is gone for good.").yes("delete").no("keep").show(ctx);
        assert_eq!(run(&ctx, vec![], ask), None);
        assert_eq!(run(&ctx, vec![key(Key::Enter)], ask), Some(true));
        assert_eq!(run(&ctx, vec![key(Key::Escape)], ask), Some(false));
    }

    #[test]
    fn a_progress_card_cancels_on_escape_only_when_asked() {
        let ctx = egui::Context::default();
        let mut p = Progress { fraction: 0.4, label: "Baking".into(), cancellable: false };
        assert_eq!(run(&ctx, vec![key(Key::Escape)], |c| p.show(c)), ProgressEvent::None);
        p.cancellable = true;
        assert_eq!(run(&ctx, vec![key(Key::Escape)], |c| p.show(c)), ProgressEvent::Cancel);
    }

    #[test]
    fn a_prompt_hands_back_its_text_on_enter_and_nothing_on_escape() {
        let ctx = egui::Context::default();
        let mut p = Prompt::new("Name").text("shot_010");
        assert_eq!(run(&ctx, vec![], |c| p.show(c)), None);
        assert_eq!(run(&ctx, vec![key(Key::Enter)], |c| p.show(c)), Some(Some("shot_010".into())));
        assert_eq!(run(&ctx, vec![key(Key::Escape)], |c| p.show(c)), Some(None));
    }

    #[test]
    fn the_scrim_takes_a_click_meant_for_a_button_under_it() {
        // A button in the window, clicked at its centre, with and without a modal up.
        let click = |modal: bool| {
            let ctx = egui::Context::default();
            let frame = |events: Vec<egui::Event>| {
                run(&ctx, events, |ctx| {
                    let r = Area::new(Id::new("under")).fixed_pos(egui::pos2(40.0, 40.0)).show(ctx, |ui| ui.button("under")).inner;
                    if modal {
                        Confirm::new("over", "").show(ctx);
                    }
                    (r.rect.center(), r.clicked())
                })
            };
            frame(vec![]); // the areas' sizing frame
            let (at, _) = frame(vec![]);
            frame(vec![egui::Event::PointerMoved(at), press(at, true)]);
            frame(vec![press(at, false)]).1
        };
        assert!(click(false), "the button clicks when nothing covers it");
        assert!(!click(true), "the scrim let the click through");
    }

    #[test]
    fn the_search_filters_the_page_list_by_every_word() {
        let pages = ["General", "Colour management", "Key map", "Colour wheels"];
        assert_eq!(filtered(&pages, ""), vec![0, 1, 2, 3]);
        assert_eq!(filtered(&pages, "colour"), vec![1, 3]);
        assert_eq!(filtered(&pages, "colour wh"), vec![3]);
        assert!(filtered(&pages, "zzz").is_empty());
    }

    #[test]
    fn the_scrim_covers_the_window_and_the_card_is_centred() {
        let ctx = egui::Context::default();
        for _ in 0..2 {
            run(&ctx, vec![], |c| Confirm::new("Delete?", "Gone for good.").show(c));
        }
        let scrim = egui::AreaState::load(&ctx, Id::new("imfram.confirm").with("scrim")).unwrap().rect();
        assert_eq!(scrim, egui::Rect::from_min_size(egui::Pos2::ZERO, SCREEN));
        let card = egui::AreaState::load(&ctx, Id::new("imfram.confirm")).unwrap().rect();
        assert!((card.center() - egui::pos2(400.0, 300.0)).length() < 1.0, "{card:?}");
        assert!(card.width() >= CARD_W && card.height() < 200.0, "{card:?}");
    }
}
