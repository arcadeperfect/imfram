//! The keys window: search on top, the command list left (grouped by
//! scope, fixed gestures last), the drawn keyboard and its inspector right.
//! A row's key button arms a capture; the next key pressed (or cap clicked)
//! becomes the binding. The board part is in `caps.rs`.

use super::board::{board_size, keyboard_rows, Cap, INSPECT_H, LIST_W, ROW_H};
use super::{display, format, Action, Binding, Keymap, MatchMode};
use egui::{Color32, Key, Modifiers, Visuals};

/// A command that is a key or a modifier gesture but deliberately NOT
/// remappable (navigation chords, a held colour axis). Listed read-only so
/// the list and the board tell the whole truth about a key; `how` stands
/// where a remappable row shows its binding.
#[derive(Clone, Copy, Debug)]
pub struct Fixed {
    pub how: &'static str,
    pub label: &'static str,
    pub key: Option<Key>,
    pub mods: Modifiers,
}

/// One command in the list: a remappable action or a fixed one. The list,
/// the lit caps, the inspector and click-to-select all go through this, so
/// they cannot disagree about what a key does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Pick<A> {
    Act(A),
    Fixed(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Capture<A> {
    None,
    /// Waiting for a key to bind to this action.
    Armed(A),
    /// Bound; waiting for that key to come back UP so its release cannot
    /// reach the app (releasing a captured Space must not start playback).
    WaitRelease(Key),
}

/// The window's colours, read off the context's `Visuals` once a frame.
#[derive(Clone, Copy)]
pub(super) struct Tone {
    pub cap: Color32,
    pub border: Color32,
    pub text: Color32,
    pub weak: Color32,
    pub strong: Color32,
    pub hover: Color32,
    pub raised: Color32,
    pub warn: Color32,
}

impl Tone {
    pub(super) fn of(v: &Visuals) -> Self {
        Self {
            cap: v.widgets.inactive.bg_fill,
            border: v.widgets.noninteractive.bg_stroke.color,
            text: v.text_color(),
            weak: v.weak_text_color(),
            strong: v.strong_text_color(),
            hover: v.widgets.hovered.weak_bg_fill,
            raised: v.faint_bg_color,
            warn: v.warn_fg_color,
        }
    }
}

type ScopeColour = Box<dyn Fn(&str) -> Color32 + Send + Sync>;

pub struct KeysUi<A: Action> {
    pub open: bool,
    pub(super) capture: Capture<A>,
    pub(super) search: String,
    /// The command whose chord the board lights.
    pub(super) selected: Option<Pick<A>>,
    /// The list row under the pointer this frame; the board previews it.
    pub(super) list_hover: Option<Pick<A>>,
    /// The cap under the pointer this frame; the inspector describes it.
    pub(super) cap_hover: Option<(usize, usize)>,
    /// Where the board was last drawn, for the pixel test.
    pub(super) board: egui::Rect,
    pub(super) fixed: Vec<Fixed>,
    colour: Option<ScopeColour>,
}

impl<A: Action> Default for KeysUi<A> {
    fn default() -> Self {
        Self::new()
    }
}

impl<A: Action> KeysUi<A> {
    pub fn new() -> Self {
        Self {
            open: false,
            capture: Capture::None,
            search: String::new(),
            selected: None,
            list_hover: None,
            cap_hover: None,
            board: egui::Rect::NOTHING,
            fixed: Vec::new(),
            colour: None,
        }
    }

    /// The scope tint: the dots on a keycap, the ring on a selected
    /// command's key, the legend chips and the list headers. Without one,
    /// each scope gets the `Visuals` selection colour turned round the hue
    /// wheel.
    pub fn with_scope_colour(mut self, f: impl Fn(&str) -> Color32 + Send + Sync + 'static) -> Self {
        self.colour = Some(Box::new(f));
        self
    }

    /// The gestures the window lists but will not remap.
    pub fn with_fixed(mut self, fixed: Vec<Fixed>) -> Self {
        self.fixed = fixed;
        self
    }

    pub fn scope_colour(&self, v: &Visuals, scope: &str) -> Color32 {
        if let Some(f) = &self.colour {
            return f(scope);
        }
        let scopes = Keymap::<A>::scopes();
        let n = scopes.iter().position(|s| *s == scope).unwrap_or(0);
        let mut h = egui::ecolor::Hsva::from(v.selection.bg_fill);
        h.h = (h.h + n as f32 / scopes.len().max(1) as f32).fract();
        h.s = h.s.max(0.55);
        h.v = h.v.max(0.75);
        h.a = 1.0;
        Color32::from(h)
    }

    /// Arm a capture for `a`: the next key pressed becomes its binding.
    pub fn begin_capture(&mut self, a: A) {
        self.capture = Capture::Armed(a);
        self.selected = Some(Pick::Act(a));
    }

    /// The action waiting for a key, if a capture is armed.
    pub fn capturing(&self) -> Option<A> {
        match self.capture {
            Capture::Armed(a) => Some(a),
            _ => None,
        }
    }

    /// Light `a`'s chord on the board (what clicking its row does).
    pub fn select(&mut self, a: A) {
        self.selected = Some(Pick::Act(a));
    }

    /// Draw the window (when open) and run the capture. Returns true when a
    /// binding changed — the caller mirrors [`Keymap::overrides`] into
    /// prefs and saves.
    pub fn window(&mut self, ctx: &egui::Context, map: &Keymap<A>) -> bool {
        let mut changed = false;
        if !self.open {
            self.capture = Capture::None;
            map.set_capturing(false);
            return false;
        }
        changed |= self.run_capture(ctx, map);
        let mut open = self.open;
        egui::Window::new("keys").open(&mut open).resizable(false).show(ctx, |ui| changed |= self.show(ui, map));
        self.open = open;
        map.set_capturing(!matches!(self.capture, Capture::None));
        changed
    }

    /// The capture, BEFORE drawing so the row shows its new key this frame.
    fn run_capture(&mut self, ctx: &egui::Context, map: &Keymap<A>) -> bool {
        let mut changed = false;
        if let Capture::Armed(act) = self.capture {
            let hit = ctx.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Key { key, pressed: true, repeat: false, modifiers, .. } => Some((*key, *modifiers)),
                    _ => None,
                })
            });
            if let Some((key, mods)) = hit {
                if key != Key::Escape {
                    let mods = if act.mode() == MatchMode::KeyOnly { Modifiers::NONE } else { mods };
                    map.rebind(act, Binding { key, mods });
                    changed = true;
                }
                self.capture = Capture::WaitRelease(key);
            }
        }
        // Not an else: a tap delivers press and release in ONE frame, and
        // holding the guard past a key that is already up would eat the
        // next real keystroke.
        if let Capture::WaitRelease(k) = self.capture {
            let up = ctx.input(|i| {
                !i.key_down(k)
                    || i.events.iter().any(|e| matches!(e, egui::Event::Key { key, pressed: false, .. } if *key == k))
            });
            if up {
                self.capture = Capture::None;
            }
        }
        changed
    }

    /// The window's body, bare: search on top; the command list left, the
    /// board and its inspector right. For an app that docks it as a tab,
    /// and for tests. (Run the capture through [`window`](Self::window) or
    /// not at all: here a click on a cap still rebinds an armed row.)
    pub fn show(&mut self, ui: &mut egui::Ui, map: &Keymap<A>) -> bool {
        let mut changed = false;
        self.list_hover = None;
        self.cap_hover = None;
        let tone = Tone::of(ui.visuals());
        let conflicted = map.conflicts();
        let mut enter = false;
        ui.horizontal(|ui| {
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text("search commands: name, scope or key…")
                    .desired_width(LIST_W - 28.0),
            );
            // Enter commits the search: the first row shown is selected.
            enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            if !self.search.is_empty() && ui.small_button("×").on_hover_text("clear").clicked() {
                self.search.clear();
            }
            ui.add_space(12.0);
            if ui.small_button("reset all").on_hover_text("every remappable key back to its default").clicked() {
                map.reset_all();
                self.capture = Capture::None;
                changed = true;
            }
        });
        ui.separator();
        let right_h = board_size(&keyboard_rows()).y + ui.spacing().item_spacing.y + INSPECT_H;
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(LIST_W);
                let (c, first) = self.command_list(ui, map, &tone, &conflicted, right_h);
                changed |= c;
                if enter && first.is_some() {
                    self.selected = first;
                }
            });
            ui.separator();
            ui.vertical(|ui| {
                changed |= self.keyboard(ui, map, &tone);
                changed |= self.inspector(ui, map, &tone, &conflicted);
            });
        });
        changed
    }

    /// The searchable list, grouped by scope with the fixed commands last.
    /// Returns (a binding changed, the first row shown).
    fn command_list(
        &mut self,
        ui: &mut egui::Ui,
        map: &Keymap<A>,
        tone: &Tone,
        conflicted: &[(A, A)],
        height: f32,
    ) -> (bool, Option<Pick<A>>) {
        let mut changed = false;
        let mut first = None;
        let q = self.search.trim().to_lowercase();
        let mut groups: Vec<(&str, Color32, Vec<Pick<A>>)> = Keymap::<A>::scopes()
            .into_iter()
            .map(|s| {
                let picks = A::all().iter().filter(|a| a.scope() == s).map(|a| Pick::Act(*a)).collect();
                (s, self.scope_colour(ui.visuals(), s), picks)
            })
            .collect();
        if !self.fixed.is_empty() {
            groups.push(("fixed — not remappable", tone.text, (0..self.fixed.len()).map(Pick::Fixed).collect()));
        }
        egui::ScrollArea::vertical().id_salt("keys-list").max_height(height).auto_shrink([false, false]).show(ui, |ui| {
            for (title, color, picks) in groups {
                let shown: Vec<Pick<A>> = picks.into_iter().filter(|p| self.pick_matches(map, *p, &q)).collect();
                if shown.is_empty() {
                    continue;
                }
                first = first.or(shown.first().copied());
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    scope_dot(ui, color);
                    ui.strong(title);
                });
                for (n, p) in shown.into_iter().enumerate() {
                    changed |= self.row(ui, map, tone, p, n % 2 == 1, conflicted);
                }
            }
            if first.is_none() {
                ui.add_space(8.0);
                ui.weak("nothing matches");
            }
        });
        (changed, first)
    }

    fn row(&mut self, ui: &mut egui::Ui, map: &Keymap<A>, tone: &Tone, p: Pick<A>, stripe: bool, conflicted: &[(A, A)]) -> bool {
        let mut changed = false;
        // The row's own click is registered BEFORE its button, so the
        // button on top still takes its click.
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), ROW_H), egui::Sense::click());
        let sel = self.selected == Some(p);
        let hover = ui.rect_contains_pointer(rect);
        if hover {
            self.list_hover = Some(p);
        }
        let col = self.pick_colour(ui.visuals(), tone, p);
        if sel {
            ui.painter().rect_filled(rect, 3.0, col.gamma_multiply(0.22));
            let bar = egui::Rect::from_min_size(rect.min, egui::vec2(3.0, ROW_H));
            ui.painter().rect_filled(bar, 1.5, col);
        } else if hover {
            ui.painter().rect_filled(rect, 3.0, tone.hover);
        } else if stripe {
            ui.painter().rect_filled(rect, 0.0, tone.raised);
        }
        if resp.clicked() {
            self.selected = if sel { None } else { Some(p) };
        }
        let inner = egui::UiBuilder::new()
            .max_rect(rect.shrink2(egui::vec2(8.0, 0.0)))
            .layout(egui::Layout::right_to_left(egui::Align::Center));
        ui.scope_builder(inner, |ui| {
            // Right to left: reset, key, then the name fills what is left.
            match p {
                Pick::Act(a) => {
                    if map.is_overridden(a) {
                        if ui.small_button("⟲").on_hover_text(format!("back to {}", format(a.default()))).clicked() {
                            map.reset(a);
                            changed = true;
                        }
                    } else {
                        ui.add_space(18.0);
                    }
                    let armed = matches!(self.capture, Capture::Armed(x) if x == a);
                    let clash = conflicted.iter().find(|(x, y)| *x == a || *y == a);
                    let mut text = if armed {
                        egui::RichText::new("press a key…").italics()
                    } else {
                        egui::RichText::new(self.pick_chord(ui.ctx(), map, p)).strong()
                    };
                    if clash.is_some() && !armed {
                        text = text.color(tone.warn);
                    }
                    let mut btn = ui.button(text);
                    btn = match clash {
                        Some((x, y)) => {
                            let other = if *x == a { *y } else { *x };
                            btn.on_hover_text(format!("collides with \"{}\" in the same scope", other.label()))
                        }
                        None => btn.on_hover_text("click, then press the new key (or click one on the board)"),
                    };
                    if btn.clicked() {
                        self.capture = if armed { Capture::None } else { Capture::Armed(a) };
                        self.selected = Some(p);
                    }
                }
                Pick::Fixed(_) => {
                    ui.add_space(18.0);
                    ui.weak(self.pick_chord(ui.ctx(), map, p));
                }
            }
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                let mut name = egui::RichText::new(self.pick_label(p));
                if sel {
                    name = name.strong();
                }
                ui.add(egui::Label::new(name).truncate().selectable(false));
            });
        });
        changed
    }

    // -- Pick helpers ------------------------------------------------------

    pub(super) fn pick_colour(&self, v: &Visuals, tone: &Tone, p: Pick<A>) -> Color32 {
        match p {
            Pick::Act(a) => self.scope_colour(v, a.scope()),
            Pick::Fixed(_) => tone.text,
        }
    }

    pub(super) fn pick_label(&self, p: Pick<A>) -> &'static str {
        match p {
            Pick::Act(a) => a.label(),
            Pick::Fixed(i) => self.fixed[i].label,
        }
    }

    /// The chord as the list shows it — the LIVE binding for an action,
    /// with its fixed synonym and second chord after a slash.
    pub(super) fn pick_chord(&self, ctx: &egui::Context, map: &Keymap<A>, p: Pick<A>) -> String {
        match p {
            Pick::Act(a) => {
                let mut t = map.label(ctx, a);
                if let Some(k) = a.also() {
                    t = format!("{t} / {}", k.name());
                }
                if let Some(c) = map.also_chord(a) {
                    t = format!("{t} / {}", display(ctx, c));
                }
                t
            }
            Pick::Fixed(i) => self.fixed[i].how.to_string(),
        }
    }

    /// Does this command's chord use `cap` — its key, its fixed synonym,
    /// or a modifier it holds?
    pub(super) fn pick_lights(&self, map: &Keymap<A>, p: Pick<A>, cap: &Cap) -> bool {
        let (key, also, mods) = match p {
            Pick::Act(a) => {
                let b = map.binding(a);
                // Light the modifiers of the second chord as well: both
                // are how this command is reached.
                let mut mods = b.mods;
                if let Some(c) = map.also_chord(a) {
                    mods |= c.mods;
                }
                (Some(b.key), a.also(), mods)
            }
            Pick::Fixed(i) => (self.fixed[i].key, None, self.fixed[i].mods),
        };
        match (cap.key, cap.modifier) {
            (Some(k), _) => key == Some(k) || also == Some(k),
            (None, Some(m)) => super::board::mod_in(mods, m),
            _ => false,
        }
    }

    /// The list filter. `q` arrives lowercased; see [`row_matches`].
    pub(super) fn pick_matches(&self, map: &Keymap<A>, p: Pick<A>, q: &str) -> bool {
        match p {
            Pick::Act(a) => row_matches(map, a, q),
            Pick::Fixed(i) => {
                let f = &self.fixed[i];
                q.is_empty() || f.label.to_lowercase().contains(q) || f.how.to_lowercase().contains(q) || "fixed".contains(q)
            }
        }
    }

    /// Every command that uses `cap`: remappable ones in list order, then
    /// the fixed ones.
    pub(super) fn picks_on(&self, map: &Keymap<A>, cap: &Cap) -> Vec<Pick<A>> {
        A::all()
            .iter()
            .map(|a| Pick::Act(*a))
            .chain((0..self.fixed.len()).map(Pick::Fixed))
            .filter(|p| self.pick_lights(map, *p, cap))
            .collect()
    }
}

/// The list filter. Label, prefs name, scope and the CURRENT key's
/// spelling all match, so "tidy", "graph" and "cmd+t" each find their
/// rows. `q` arrives lowercased.
pub(super) fn row_matches<A: Action>(map: &Keymap<A>, a: A, q: &str) -> bool {
    if q.is_empty() {
        return true;
    }
    a.label().to_lowercase().contains(q)
        || a.name().contains(q)
        || a.scope().contains(q)
        || format(map.binding(a)).to_lowercase().contains(q)
}

pub(super) fn scope_dot(ui: &mut egui::Ui, col: Color32) {
    let (r, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
    ui.painter().circle_filled(r.center(), 3.0, col);
}
