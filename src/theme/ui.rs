//! The colours… window: every slot of a [`Table`] by group, a picker
//! swatch each, the links shown ("follows accent"), reset per slot and all.

use super::engine::Table;
use egui::Color32;

#[derive(Default)]
pub struct ThemeUi {
    pub open: bool,
    search: String,
    /// Edited but not yet written to prefs: a picker drag recolours every
    /// frame, and prefs are written once, when the pointer lets go.
    dirty: bool,
    /// List every slot, not just the roots the rest follow.
    show_all: bool,
}

/// A slot the short list shows: one with a colour of its own (nothing to
/// follow), or one somebody has already set.
fn is_root(t: &Table, i: usize) -> bool {
    t.defs[i].base.parent().is_none() || t.is_overridden(i)
}

/// How many slots move when slot `i` does, following links transitively.
fn carried(t: &Table, i: usize) -> usize {
    let mut moves = vec![false; t.len()];
    moves[i] = true;
    let mut n = 0;
    // Links only point backwards, so one forward pass sees every chain.
    for j in i + 1..t.len() {
        if t.defs[j].base.parent().is_some_and(|p| moves[p]) {
            moves[j] = true;
            n += 1;
        }
    }
    n
}

fn row_matches(d: &super::Def, q: &str) -> bool {
    q.is_empty()
        || d.label.to_lowercase().contains(q)
        || d.group.to_lowercase().contains(q)
        || d.id.replace('_', " ").contains(q)
}

/// The table's groups in first-appearance order; a group may be declared
/// in more than one run (an app extending a core group).
fn groups(t: &Table) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for d in t.defs {
        if !out.contains(&d.group) {
            out.push(d.group);
        }
    }
    out
}

impl ThemeUi {
    /// Draw the window (when open). Returns true when the overrides should
    /// be written to prefs — the caller mirrors `overrides()` and saves.
    pub fn window(&mut self, ctx: &egui::Context, table: &Table, fullscreen: bool) -> bool {
        if self.open && !fullscreen {
            let mut open = self.open;
            egui::Window::new("colours")
                .open(&mut open)
                .default_width(360.0)
                .show(ctx, |ui| self.contents(ui, table));
            self.open = open;
        }
        if self.dirty {
            // egui's own widgets pick the change up at the next `sync`.
            ctx.request_repaint();
        }
        let settled = !self.open || ctx.input(|i| !i.pointer.any_down());
        if self.dirty && settled {
            self.dirty = false;
            return true;
        }
        false
    }

    /// The window's body, for embedding in a panel of the caller's.
    pub fn contents(&mut self, ui: &mut egui::Ui, t: &Table) {
        let n = t.len();
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text("search colours…")
                    .desired_width(200.0),
            );
            if !self.search.is_empty() && ui.small_button("×").clicked() {
                self.search.clear();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(t.any_overridden(), egui::Button::new("reset all").small())
                    .on_hover_text("every colour back to its default")
                    .clicked()
                {
                    t.reset_all();
                    self.dirty = true;
                }
            });
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.show_all, format!("all {n}"))
                .on_hover_text("also list the colours that follow another one");
            ui.weak("setting a colour moves everything that follows it");
        });
        ui.separator();
        let q = self.search.trim().to_lowercase();
        // A search looks through everything; otherwise the short list.
        let short = q.is_empty() && !self.show_all;
        let mut any = false;
        egui::ScrollArea::vertical().id_salt("theme-list").max_height(480.0).show(ui, |ui| {
            for (g, group) in groups(t).into_iter().enumerate() {
                let rows: Vec<usize> = (0..n)
                    .filter(|&i| t.defs[i].group == group)
                    .filter(|&i| row_matches(&t.defs[i], &q) && (!short || is_root(t, i)))
                    .collect();
                if rows.is_empty() {
                    continue;
                }
                any = true;
                let header = egui::CollapsingHeader::new(group)
                    .id_salt(("theme-group", group))
                    .default_open(g == 0);
                // A search opens every group it hits, and closes nothing.
                let header = if q.is_empty() { header } else { header.open(Some(true)) };
                header.show(ui, |ui| {
                    egui::Grid::new(("theme-grid", group))
                        .num_columns(3)
                        .spacing([8.0, 4.0])
                        .show(ui, |ui| {
                            for i in rows {
                                self.row(ui, t, i);
                                ui.end_row();
                            }
                        });
                });
            }
        });
        if !any {
            ui.weak("no colour matches");
        }
    }

    fn row(&mut self, ui: &mut egui::Ui, t: &Table, i: usize) {
        use crate::colorpicker::{dec1, enc1, PickSpec};
        let d = &t.defs[i];
        let cur = t.get(i);
        let [r, g, b, a] = cur.to_srgba_unmultiplied();
        let byte = |x: u8| dec1(x as f32 / 255.0);
        let mut v = [byte(r), byte(g), byte(b), a as f32 / 255.0];
        let spec = if d.alpha { PickSpec::rgba() } else { PickSpec::rgb() };
        if crate::colorpicker::button(ui, ("theme", i), &mut v, spec).changed() {
            let e = |x: f32| (enc1(x).clamp(0.0, 1.0) * 255.0).round() as u8;
            let alpha = if spec.alpha { (v[3].clamp(0.0, 1.0) * 255.0).round() as u8 } else { a };
            let c = Color32::from_rgba_unmultiplied(e(v[0]), e(v[1]), e(v[2]), alpha);
            if c != cur {
                t.set(i, c);
                self.dirty = true;
            }
        }
        let label = ui.label(d.label);
        if !d.tip.trim().is_empty() {
            label.on_hover_ui(|ui| {
                ui.label(d.tip.lines().map(str::trim).collect::<Vec<_>>().join(" ").trim());
            });
        }
        ui.horizontal(|ui| {
            if t.is_overridden(i) {
                if ui.small_button("⟲").on_hover_text("back to its default").clicked() {
                    t.reset(i);
                    self.dirty = true;
                }
            } else if let Some(p) = d.base.parent() {
                let p = &t.defs[p];
                if p.group == d.group {
                    ui.weak(format!("follows {}", p.label));
                } else {
                    ui.weak(format!("follows {} › {}", p.group, p.label));
                }
            } else {
                let n = carried(t, i);
                if n > 0 {
                    ui.weak(format!("+{n} follow"));
                }
            }
        });
    }
}
