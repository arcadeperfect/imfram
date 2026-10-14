//! Playground tab for `imfram::table`: 10 000 generated files (name, size,
//! modified, kind) behind a [`Rows`] source that sorts itself when the
//! table says so, with the last event shown under it.

use egui::{Align, Ui};
use imfram::table::{Column, Rows, Table};

struct File {
    id: u64,
    name: String,
    size: u64,
    /// Days since an epoch; shown as a date.
    modified: u32,
    kind: &'static str,
}

struct Files(Vec<File>);

const KINDS: &[&str] = &["exr", "png", "mov", "wav", "usd", "blend", "nk", "txt"];
const N: usize = 10_000;

fn generate() -> Files {
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        seed >> 33
    };
    let files = (0..N)
        .map(|i| {
            let kind = KINDS[next() as usize % KINDS.len()];
            let shot = 10 + next() % 90;
            let ver = 1 + next() % 40;
            File {
                id: i as u64,
                name: format!("sh{shot:03}_{}_v{ver:03}.{kind}", ["comp", "plate", "cg", "matte", "track"][next() as usize % 5]),
                size: (next() % 4_000_000_000) * (1 + next() % 3),
                modified: 19_000 + (next() % 1200) as u32,
                kind,
            }
        })
        .collect();
    Files(files)
}

impl Files {
    fn sort(&mut self, col: usize, ascending: bool) {
        match col {
            0 => self.0.sort_by(|a, b| a.name.cmp(&b.name)),
            1 => self.0.sort_by_key(|f| f.size),
            2 => self.0.sort_by_key(|f| f.modified),
            _ => self.0.sort_by(|a, b| a.kind.cmp(b.kind).then(a.name.cmp(&b.name))),
        }
        if !ascending {
            self.0.reverse();
        }
    }
}

impl Rows for Files {
    fn len(&self) -> usize {
        self.0.len()
    }
    fn cell(&self, ui: &mut Ui, row: usize, col: usize) {
        let f = &self.0[row];
        match col {
            0 => ui.label(&f.name),
            1 => ui.monospace(size_text(f.size)),
            2 => ui.label(date_text(f.modified)),
            _ => ui.weak(f.kind),
        };
    }
    fn id(&self, row: usize) -> u64 {
        self.0[row].id
    }
}

fn size_text(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = n as f64;
    let mut u = 0;
    while v >= 1000.0 && u < UNITS.len() - 1 {
        v /= 1000.0;
        u += 1;
    }
    if u == 0 { format!("{n} B") } else { format!("{v:.1} {}", UNITS[u]) }
}

/// Days since 1970-01-01 as y-m-d (civil-from-days, Howard Hinnant's).
fn date_text(days: u32) -> String {
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

pub struct State {
    files: Files,
    table: Table,
    last: String,
}

impl Default for State {
    fn default() -> Self {
        let table = Table::new(vec![
            Column::new("name", 260.0),
            Column::new("size", 90.0).align(Align::Max),
            Column::new("modified", 110.0),
            Column::new("kind", 70.0).fixed(),
        ]);
        Self { files: generate(), table, last: String::new() }
    }
}

const FOOT_H: f32 = 24.0;

pub fn ui(ui: &mut Ui, s: &mut State) {
    let h = (ui.available_height() - FOOT_H).max(60.0);
    let ev = ui.allocate_ui(egui::vec2(ui.available_width(), h), |ui| s.table.show(ui, &s.files)).inner;
    if ev.sort_changed {
        if let Some((col, up)) = s.table.sort {
            s.files.sort(col, up);
        }
    }
    if ev.sort_changed || ev.selection_changed || ev.activated.is_some() {
        s.last = format!("{ev:?}");
    }
    ui.horizontal(|ui| {
        ui.weak(format!("{} rows · {} selected", s.files.len(), s.table.selection.len()));
        ui.separator();
        ui.monospace(&s.last);
    });
}
