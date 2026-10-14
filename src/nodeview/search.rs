//! The add-node popup, opened by Tab or a right-click on the canvas
//! background. Two ways through it:
//!
//! - browse: a submenu per kind of node; hover or click one and its
//!   nodes list beside it
//! - type: the list becomes every node matching what was typed, best first
//!
//! Arrows and Enter work in both (Right / Left step into and out of a
//! submenu); Escape or a click elsewhere closes it.

use egui::{Frame, Id, Modifiers, Pos2, Rect, Ui};

use super::graph::Entry;

/// Width of each of the popup's columns.
const COLUMN_W: f32 = 170.0;

pub struct Search {
    /// Where the new node goes, in canvas space.
    pub canvas: Pos2,
    /// Where the popup sits, in window space.
    window: Pos2,
    query: String,
    /// The highlighted row of whichever list has the keyboard.
    selected: usize,
    /// The submenu whose nodes are listed (browsing only).
    open: Option<String>,
    /// First frame: grab focus, and ignore the click that opened it.
    fresh: bool,
}

/// What the popup wants after a frame.
pub enum Outcome {
    Open,
    Closed,
    /// Add a node of this type at `Search::canvas`.
    Picked(String),
}

/// Entries matching `query`, best first: prefix, then substring, then
/// letters-in-order.
pub fn hits<'a>(query: &str, catalog: &'a [Entry]) -> Vec<&'a Entry> {
    let q = query.trim().to_lowercase();
    let rank = |d: &Entry| {
        let title = d.title.to_lowercase();
        if title.starts_with(&q) {
            Some(0)
        } else if title.contains(&q) {
            Some(1)
        } else {
            let mut chars = title.chars();
            q.chars().all(|c| chars.any(|t| t == c)).then_some(2)
        }
    };
    let mut hits: Vec<(u8, &Entry)> = catalog.iter().filter_map(|d| Some((rank(d)?, d))).collect();
    hits.sort_by_key(|h| h.0);
    hits.into_iter().map(|h| h.1).collect()
}

/// The submenus, in order of first appearance, each with its entries.
pub fn menus(catalog: &[Entry]) -> Vec<(String, Vec<&Entry>)> {
    let mut out: Vec<(String, Vec<&Entry>)> = Vec::new();
    for e in catalog {
        match out.iter_mut().find(|(m, _)| *m == e.menu) {
            Some((_, v)) => v.push(e),
            None => out.push((e.menu.clone(), vec![e])),
        }
    }
    out
}

/// The rows the popup last showed, with where they were (window
/// coordinates). Read by scripted UI self-tests.
pub type Rows = Vec<(String, Rect)>;

fn rows_id() -> Id {
    Id::new("node-search-rows")
}

pub fn rows(ctx: &egui::Context) -> Rows {
    ctx.data(|d| d.get_temp(rows_id())).unwrap_or_default()
}

impl Search {
    pub fn open(canvas: Pos2, window: Pos2) -> Self {
        Self { canvas, window, query: String::new(), selected: 0, open: None, fresh: true }
    }

    pub fn show(&mut self, ui: &Ui, catalog: &[Entry]) -> Outcome {
        let browsing = self.query.trim().is_empty();
        let menus = menus(catalog);
        let hits = if browsing { Vec::new() } else { hits(&self.query, catalog) };
        let open_nodes = self.open.as_ref().filter(|_| browsing).and_then(|m| menus.iter().find(|(menu, _)| menu == m));
        let rows = match (browsing, open_nodes) {
            (false, _) => hits.len(),
            (true, Some((_, nodes))) => nodes.len(),
            (true, None) => menus.len(),
        };
        self.selected = self.selected.min(rows.saturating_sub(1));

        // Keys, taken before the text field sees them.
        let key = |k| ui.input_mut(|i| i.consume_key(Modifiers::NONE, k));
        let mut close = key(egui::Key::Escape);
        let mut pick = None;
        if rows > 0 {
            if key(egui::Key::ArrowDown) {
                self.selected = (self.selected + 1) % rows;
            }
            if key(egui::Key::ArrowUp) {
                self.selected = (self.selected + rows - 1) % rows;
            }
        }
        let enter = key(egui::Key::Enter);
        if browsing {
            match open_nodes {
                Some((menu, nodes)) => {
                    if enter {
                        pick = nodes.get(self.selected).map(|d| d.ty.clone());
                    } else if key(egui::Key::ArrowLeft) {
                        self.selected = menus.iter().position(|(m, _)| m == menu).unwrap_or(0);
                        self.open = None;
                    }
                }
                None => {
                    if (enter || key(egui::Key::ArrowRight)) && rows > 0 {
                        self.open = Some(menus[self.selected].0.clone());
                        self.selected = 0;
                    }
                }
            }
        } else if enter {
            pick = hits.get(self.selected).map(|d| d.ty.clone());
            close = true;
        }

        let mut shown: Rows = Vec::new();
        let area = egui::Area::new(Id::new("node-search")).order(egui::Order::Foreground).fixed_pos(self.window).show(ui.ctx(), |ui| {
            Frame::popup(ui.style()).show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(COLUMN_W);
                        let field = ui.add(egui::TextEdit::singleline(&mut self.query).hint_text("add node").desired_width(f32::INFINITY));
                        if self.fresh {
                            field.request_focus();
                        }
                        if field.changed() {
                            self.selected = 0;
                            self.open = None;
                        }
                        if browsing {
                            self.submenus(ui, &menus, &mut shown);
                        } else {
                            for (i, d) in hits.iter().enumerate() {
                                let row = ui.selectable_label(i == self.selected, &d.title);
                                shown.push((d.title.clone(), row.rect));
                                if row.clicked() {
                                    pick = Some(d.ty.clone());
                                }
                            }
                            if hits.is_empty() {
                                ui.weak("no match");
                            }
                        }
                    });
                    // The open submenu's nodes, beside the submenus.
                    if let Some((_, nodes)) = self.open.as_ref().filter(|_| browsing).and_then(|m| menus.iter().find(|(menu, _)| menu == m)) {
                        ui.separator();
                        ui.vertical(|ui| {
                            ui.set_width(COLUMN_W);
                            for (i, d) in nodes.iter().enumerate() {
                                let row = ui.selectable_label(i == self.selected, &d.title);
                                shown.push((d.title.clone(), row.rect));
                                if row.clicked() {
                                    pick = Some(d.ty.clone());
                                }
                            }
                        });
                    }
                });
            });
        });
        ui.data_mut(|d| d.insert_temp(rows_id(), shown));
        if !self.fresh && area.response.clicked_elsewhere() {
            close = true;
        }
        self.fresh = false;

        match pick {
            Some(ty) => Outcome::Picked(ty),
            None if close => Outcome::Closed,
            None => Outcome::Open,
        }
    }

    /// The submenu rows. Hovering or clicking one opens it.
    fn submenus(&mut self, ui: &mut Ui, menus: &[(String, Vec<&Entry>)], shown: &mut Rows) {
        for (i, (menu, _)) in menus.iter().enumerate() {
            let is_open = self.open.as_ref() == Some(menu);
            let highlighted = is_open || (self.open.is_none() && i == self.selected);
            let row = ui.selectable_label(highlighted, format!("{menu}  ›"));
            shown.push((menu.clone(), row.rect));
            // Only a pointer that MOVED onto the row opens it, so arrow-key
            // browsing is not undone by a pointer that happens to rest there.
            let moved = ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO);
            if (row.clicked() || (row.hovered() && moved)) && !is_open {
                self.open = Some(menu.clone());
                self.selected = 0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> Vec<Entry> {
        let e = |ty: &str, title: &str, menu: &str| Entry { ty: ty.into(), title: title.into(), menu: menu.into() };
        vec![e("perlin", "Perlin Noise", "Noise"), e("simplex", "Simplex Noise", "Noise"), e("ridged", "Ridged Multifractal", "Noise"), e("const", "Constant", "Inputs"), e("mix", "Mix", "Maths")]
    }

    #[test]
    fn prefix_beats_substring_beats_scattered_letters() {
        let c = catalog();
        let titles = |q: &str| hits(q, &c).into_iter().map(|d| d.title.as_str()).collect::<Vec<_>>();
        assert_eq!(titles("rid")[0], "Ridged Multifractal");
        assert!(titles("noise").contains(&"Perlin Noise"));
        let scattered = titles("pn");
        assert!(scattered.contains(&"Perlin Noise") && !scattered.contains(&"Constant"));
        assert!(titles("zzz").is_empty());
    }

    #[test]
    fn submenus_keep_first_appearance_order_and_every_entry() {
        let c = catalog();
        let m = menus(&c);
        assert_eq!(m.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), ["Noise", "Inputs", "Maths"]);
        assert_eq!(m.iter().map(|(_, v)| v.len()).sum::<usize>(), c.len());
    }
}
