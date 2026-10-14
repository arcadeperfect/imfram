//! The keys window's right half: the QWERTY board, the inspector under it
//! and the legend beside esc.

use super::board::{board_size, cap_font, cap_rect, keyboard_rows, lift, mix, Cap, ModKey, INSPECT_H, KB_H, KB_SCALE};
use super::ui::{Capture, Pick, Tone};
use super::{format, Action, Binding, Keymap, KeysUi, MatchMode};
use egui::{Align2, Color32, Modifiers, Rect, Stroke, StrokeKind};

impl<A: Action> KeysUi<A> {
    /// The QWERTY board. The selected command's chord is lit in its
    /// scope's colour, and a hovered list row previews as an outline; the
    /// dots on a cap say which scopes use it; hovering a cap describes it
    /// in the inspector below. Clicking a cap selects what lives there
    /// (again = the next command on a shared key) — or, while a row waits
    /// for a key, assigns that key with whatever modifiers are held.
    pub(super) fn keyboard(&mut self, ui: &mut egui::Ui, map: &Keymap<A>, tone: &Tone) -> bool {
        let mut changed = false;
        let rows = keyboard_rows();
        let (board, _) = ui.allocate_exact_size(board_size(&rows), egui::Sense::hover());
        self.board = board;
        for (ri, row) in rows.iter().enumerate() {
            for (ci, cap) in row.iter().enumerate() {
                let r = cap_rect(&rows, ri, ci).translate(board.min.to_vec2());
                changed |= self.cap_ui(ui, map, tone, r, cap, (ri, ci));
            }
        }
        let esc = cap_rect(&rows, 0, rows[0].len() - 1);
        self.legend(ui, tone, board.min + egui::vec2(esc.max.x + 12.0 * KB_SCALE, 0.0), KB_H);
        changed
    }

    fn cap_ui(&mut self, ui: &mut egui::Ui, map: &Keymap<A>, tone: &Tone, r: Rect, cap: &Cap, at: (usize, usize)) -> bool {
        let dead = cap.key.is_none() && cap.modifier.is_none();
        let sense = if dead { egui::Sense::hover() } else { egui::Sense::click() };
        let resp = ui.interact(r, ui.id().with(("kbcap", at)), sense);
        let picks = self.picks_on(map, cap);
        if resp.hovered() {
            self.cap_hover = Some(at);
        }
        let lit = self.selected.filter(|p| self.pick_lights(map, *p, cap));
        let preview = self.list_hover.filter(|p| Some(*p) != self.selected && self.pick_lights(map, *p, cap));
        let colour = |p: Pick<A>| self.pick_colour(ui.visuals(), tone, p);

        let base = if dead {
            lift(tone.cap, -22)
        } else if picks.is_empty() {
            lift(tone.cap, -13)
        } else {
            tone.cap
        };
        let mut fill = base;
        if resp.hovered() && !dead {
            fill = mix(fill, Color32::WHITE, 0.10);
        }
        if let Some(p) = lit {
            fill = mix(base, colour(p), 0.38);
        }
        let stroke = match (lit, preview) {
            (Some(p), _) => Stroke::new(1.8 * KB_SCALE, colour(p)),
            (None, Some(p)) => Stroke::new(1.4 * KB_SCALE, mix(tone.border, colour(p), 0.8)),
            _ => Stroke::new(1.0 * KB_SCALE, tone.border),
        };
        let text = if dead {
            tone.weak
        } else if lit.is_some() || !picks.is_empty() {
            tone.strong
        } else {
            tone.text
        };
        let painter = ui.painter();
        painter.rect_filled(r, 4.0 * KB_SCALE, fill);
        painter.rect_stroke(r, 4.0 * KB_SCALE, stroke, StrokeKind::Inside);
        painter.text(r.center(), Align2::CENTER_CENTER, cap.label, cap_font(cap.label), text);
        // Dots along the top edge, right to left: which scopes use this cap.
        let mut dots: Vec<Color32> = Vec::new();
        for p in &picks {
            let c = colour(*p);
            if !dots.contains(&c) {
                dots.push(c);
            }
        }
        for (n, c) in dots.iter().enumerate() {
            let at = egui::pos2(r.max.x - (5.0 + n as f32 * 6.0) * KB_SCALE, r.min.y + 5.0 * KB_SCALE);
            painter.circle_filled(at, 2.2 * KB_SCALE, *c);
        }

        if !resp.clicked() {
            return false;
        }
        // A row waiting for a key takes the clicked one.
        if let (Capture::Armed(act), Some(key)) = (self.capture, cap.key) {
            let mods = if act.mode() == MatchMode::KeyOnly { Modifiers::NONE } else { ui.input(|i| i.modifiers) };
            map.rebind(act, Binding { key, mods });
            self.capture = Capture::None;
            return true;
        }
        if !picks.is_empty() {
            self.selected = Some(match self.selected.and_then(|s| picks.iter().position(|p| *p == s)) {
                Some(i) => picks[(i + 1) % picks.len()],
                None => picks[0],
            });
        }
        false
    }

    /// Under the board, and instant — tooltips wait half a second, far too
    /// slow for sweeping across a keyboard. The hovered cap's every use,
    /// else the selected command, else how to drive this.
    pub(super) fn inspector(&mut self, ui: &mut egui::Ui, map: &Keymap<A>, tone: &Tone, conflicted: &[(A, A)]) -> bool {
        let mut changed = false;
        let rows = keyboard_rows();
        let (rect, _) = ui.allocate_exact_size(egui::vec2(board_size(&rows).x, INSPECT_H), egui::Sense::hover());
        ui.painter().rect_filled(rect, 4.0, tone.raised);
        let inner = egui::UiBuilder::new().max_rect(rect.shrink(8.0));
        ui.scope_builder(inner, |ui| {
            ui.set_clip_rect(rect);
            if let Capture::Armed(a) = self.capture {
                let col = self.scope_colour(ui.visuals(), a.scope());
                ui.colored_label(col, format!("press a key for \"{}\" — or click one above", a.label()));
                ui.weak("esc cancels");
            } else if let Some((ri, ci)) = self.cap_hover {
                let cap = &rows[ri][ci];
                let title = match (cap.key, cap.modifier) {
                    (Some(k), _) => k.symbol_or_name().to_string(),
                    (_, Some(ModKey::Cmd)) => "cmd ⌘".into(),
                    (_, Some(ModKey::Shift)) => "shift".into(),
                    (_, Some(ModKey::Alt)) => "alt / option".into(),
                    (_, Some(ModKey::Ctrl)) => "ctrl".into(),
                    _ => cap.label.to_string(),
                };
                let picks = self.picks_on(map, cap);
                const SHOW: usize = 10;
                ui.horizontal(|ui| {
                    ui.strong(title);
                    if picks.len() > SHOW {
                        ui.weak(format!("+{} more", picks.len() - SHOW));
                    }
                });
                if picks.is_empty() {
                    ui.weak(if cap.key.is_none() && cap.modifier.is_none() { "not a key the app reads" } else { "unassigned" });
                } else if picks.len() <= 5 {
                    for p in &picks {
                        self.entry(ui, map, tone, *p);
                    }
                } else {
                    ui.columns(2, |cols| {
                        for (n, p) in picks.iter().take(SHOW).enumerate() {
                            self.entry(&mut cols[n % 2], map, tone, *p);
                        }
                    });
                }
            } else if let Some(p) = self.selected {
                self.entry(ui, map, tone, p);
                match p {
                    Pick::Act(a) => {
                        ui.horizontal(|ui| {
                            if ui.button("rebind").clicked() {
                                self.capture = Capture::Armed(a);
                            }
                            if map.is_overridden(a) && ui.button(format!("⟲ back to {}", format(a.default()))).clicked() {
                                map.reset(a);
                                changed = true;
                            }
                            ui.weak(a.scope());
                        });
                        if let Some((x, y)) = conflicted.iter().find(|(x, y)| *x == a || *y == a) {
                            let other = if *x == a { *y } else { *x };
                            ui.colored_label(tone.warn, format!("collides with \"{}\" in the same scope", other.label()));
                        }
                    }
                    Pick::Fixed(_) => {
                        ui.weak("fixed: not remappable");
                    }
                }
            } else {
                ui.weak("hover a key to see what it does");
                ui.weak("click a command, or a key, to light its chord");
                ui.weak("a row's key button rebinds it: press the new key, or click one");
            }
        });
        changed
    }

    /// One command, one line: its scope dot, its chord, its name.
    fn entry(&self, ui: &mut egui::Ui, map: &Keymap<A>, tone: &Tone, p: Pick<A>) {
        let col = self.pick_colour(ui.visuals(), tone, p);
        let chord = self.pick_chord(ui.ctx(), map, p);
        ui.horizontal(|ui| {
            super::ui::scope_dot(ui, col);
            ui.strong(chord);
            ui.add(egui::Label::new(self.pick_label(p)).truncate().selectable(false));
        });
    }

    /// The colour key for the cap dots, beside esc.
    fn legend(&self, ui: &egui::Ui, tone: &Tone, at: egui::Pos2, h: f32) {
        let painter = ui.painter();
        let y = at.y + h * 0.5;
        let mut x = at.x;
        let mut names: Vec<(&str, Color32)> =
            Keymap::<A>::scopes().into_iter().map(|s| (s, self.scope_colour(ui.visuals(), s))).collect();
        if !self.fixed.is_empty() {
            names.push(("fixed", tone.text));
        }
        for (name, col) in names {
            painter.circle_filled(egui::pos2(x + 3.0 * KB_SCALE, y), 3.0 * KB_SCALE, col);
            let g = painter.text(
                egui::pos2(x + 10.0 * KB_SCALE, y),
                Align2::LEFT_CENTER,
                name,
                egui::FontId::proportional(11.0 * KB_SCALE),
                tone.text,
            );
            x = g.max.x + 12.0 * KB_SCALE;
        }
    }
}
