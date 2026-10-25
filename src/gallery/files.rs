//! A folder as gallery items: frame sequences grouped into one item
//! (`plate.1001.exr` .. `plate.1100.exr` is `plate.####.exr`, 1001-1100),
//! movies probed for their frame count, folders first. Listing and probing
//! run off the UI thread ([`crate::bg`]); [`Listing::poll`] takes what
//! has landed. Native only.
//!
//! A [`FileSrc`] is what a thumbnail loader gets: [`FileSrc::frame_path`]
//! names the file for a frame of a sequence; a movie's loader decodes
//! frame `n` itself. [`load_png`] is a loader for PNG stills and
//! sequences; an app chains its own decoders (EXR, movies) in front of it.

use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::SystemTime;

use egui::{Color32, ColorImage};

use super::{Details, Items, Request};

/// Extensions read as movies (probed for a frame count).
pub const MOVIE_EXTS: &[&str] = &["mov", "mp4", "m4v", "mkv", "avi", "webm", "mxf"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    Dir,
    Sequence,
    Movie,
    File,
}

/// A run of numbered files: `prefix` + the number padded to `pad` + `suffix`.
#[derive(Clone, Debug)]
pub struct Seq {
    pub dir: PathBuf,
    pub prefix: String,
    pub suffix: String,
    pub pad: usize,
    /// The numbers present, ascending (gaps are left out, so a scrub
    /// lands only on frames that exist).
    pub frames: Arc<[i64]>,
}

impl Seq {
    pub fn first(&self) -> i64 {
        self.frames[0]
    }
    pub fn last(&self) -> i64 {
        self.frames[self.frames.len() - 1]
    }
    /// Numbers missing between first and last.
    pub fn missing(&self) -> i64 {
        self.last() - self.first() + 1 - self.frames.len() as i64
    }
    pub fn path_of(&self, number: i64) -> PathBuf {
        self.dir.join(format!("{}{:0pad$}{}", self.prefix, number, self.suffix, pad = self.pad))
    }
    /// `plate.####.exr`.
    pub fn pattern(&self) -> String {
        format!("{}{}{}", self.prefix, "#".repeat(self.pad), self.suffix)
    }
}

impl PartialEq for Seq {
    fn eq(&self, o: &Self) -> bool {
        self.dir == o.dir && self.prefix == o.prefix && self.suffix == o.suffix && self.pad == o.pad && self.frames == o.frames
    }
}
impl Eq for Seq {}
impl Hash for Seq {
    fn hash<H: Hasher>(&self, h: &mut H) {
        (&self.dir, &self.prefix, &self.suffix, self.pad, self.frames.len(), self.frames.first()).hash(h);
    }
}

/// What a thumbnail loader is handed.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FileSrc {
    /// The file, the folder, or a sequence's first frame.
    pub path: PathBuf,
    pub kind: Kind,
    pub seq: Option<Seq>,
}

impl FileSrc {
    /// The file to read for `frame` (an index, `0..frames`): a sequence's
    /// frame file, else the path itself.
    pub fn frame_path(&self, frame: u32) -> PathBuf {
        match &self.seq {
            Some(s) => s.path_of(s.frames[(frame as usize).min(s.frames.len() - 1)]),
            None => self.path.clone(),
        }
    }

    pub fn ext(&self) -> String {
        ext_of(&self.frame_path(0))
    }
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub id: u64,
    pub name: String,
    pub src: FileSrc,
    /// Bytes (a sequence: all its frames).
    pub size: u64,
    pub modified: Option<SystemTime>,
    /// Frames to scrub (a movie: once probed).
    pub frames: u32,
    pub hidden: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortKey {
    Name,
    Kind,
    Size,
    Modified,
}

/// Counts a movie's frames, on a worker thread; None if it cannot.
pub type Probe = dyn Fn(&Path) -> Option<u32> + Send + Sync;

pub struct Listing {
    pub dir: PathBuf,
    pub show_hidden: bool,
    /// Case-blind substring over names.
    pub filter: String,
    pub sort: SortKey,
    pub ascending: bool,
    entries: Vec<Entry>,
    /// Indices into `entries`: filtered and sorted.
    shown: Vec<usize>,
    error: Option<String>,
    loading: bool,
    pending: Option<Receiver<Result<Vec<Entry>, String>>>,
    probe: Option<Arc<Probe>>,
    probes: Option<Receiver<(u64, u32)>>,
    /// Frame counts probed, kept across re-listings (by id).
    probed: HashMap<u64, u32>,
    listed: Option<(PathBuf, bool)>,
    /// What `shown` was built from.
    shaped: Option<(String, SortKey, bool, usize)>,
}

impl Listing {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            show_hidden: false,
            filter: String::new(),
            sort: SortKey::Name,
            ascending: true,
            entries: vec![],
            shown: vec![],
            error: None,
            loading: false,
            pending: None,
            probe: None,
            probes: None,
            probed: HashMap::new(),
            listed: None,
            shaped: None,
        }
    }

    /// Movies get their frame counts from `probe` (on a worker).
    pub fn with_probe(mut self, probe: impl Fn(&Path) -> Option<u32> + Send + Sync + 'static) -> Self {
        self.probe = Some(Arc::new(probe));
        self
    }

    pub fn go(&mut self, dir: impl Into<PathBuf>) {
        self.dir = dir.into();
        self.listed = None;
    }

    pub fn refresh(&mut self) {
        self.listed = None;
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn loading(&self) -> bool {
        self.loading
    }

    /// Every entry, unfiltered.
    pub fn all(&self) -> &[Entry] {
        &self.entries
    }

    /// The shown entry i.
    pub fn entry(&self, i: usize) -> &Entry {
        &self.entries[self.shown[i]]
    }

    pub fn by_id(&self, id: u64) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// Start a listing if the folder changed, take what has landed,
    /// re-filter and re-sort if asked. Call once a frame.
    pub fn poll(&mut self, ctx: &egui::Context) {
        let want = (self.dir.clone(), self.show_hidden);
        if self.listed.as_ref() != Some(&want) {
            let (tx, rx) = channel();
            let (dir, hidden) = want.clone();
            crate::bg::spawn(move || {
                let _ = tx.send(scan(&dir, hidden));
            });
            self.pending = Some(rx);
            self.listed = Some(want);
            self.loading = true;
        }
        if let Some(rx) = &self.pending {
            match rx.try_recv() {
                Ok(res) => {
                    self.pending = None;
                    self.loading = false;
                    match res {
                        Ok(es) => {
                            self.entries = es;
                            self.error = None;
                            for e in &mut self.entries {
                                if let Some(&n) = self.probed.get(&e.id) {
                                    e.frames = n;
                                }
                            }
                            self.start_probes();
                        }
                        Err(e) => {
                            self.entries.clear();
                            self.error = Some(e);
                        }
                    }
                    self.shaped = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(30)),
                Err(_) => {
                    self.pending = None;
                    self.loading = false;
                }
            }
        }
        if let Some(rx) = &self.probes {
            let mut any = false;
            while let Ok((id, n)) = rx.try_recv() {
                self.probed.insert(id, n);
                if let Some(e) = self.entries.iter_mut().find(|e| e.id == id) {
                    e.frames = n;
                }
                any = true;
            }
            if any {
                ctx.request_repaint();
            }
        }
        let shape = (self.filter.clone(), self.sort, self.ascending, self.entries.len());
        if self.shaped.as_ref() != Some(&shape) {
            self.reshape();
            self.shaped = Some(shape);
        }
    }

    fn start_probes(&mut self) {
        let Some(probe) = self.probe.clone() else { return };
        let todo: Vec<(u64, PathBuf)> = self.entries.iter().filter(|e| e.src.kind == Kind::Movie && !self.probed.contains_key(&e.id)).map(|e| (e.id, e.src.path.clone())).collect();
        if todo.is_empty() {
            return;
        }
        let (tx, rx) = channel();
        crate::bg::spawn(move || {
            for (id, p) in todo {
                if let Some(n) = probe(&p) {
                    if tx.send((id, n.max(1))).is_err() {
                        return;
                    }
                }
            }
        });
        self.probes = Some(rx);
    }

    fn reshape(&mut self) {
        let f = self.filter.to_lowercase();
        let mut shown: Vec<usize> = (0..self.entries.len()).filter(|&i| f.is_empty() || self.entries[i].name.to_lowercase().contains(&f)).collect();
        let es = &self.entries;
        let (key, asc) = (self.sort, self.ascending);
        shown.sort_by(|&a, &b| order(&es[a], &es[b], key, asc));
        self.shown = shown;
    }
}

/// Folders first whichever way; then the key, then the name.
pub fn order(a: &Entry, b: &Entry, key: SortKey, ascending: bool) -> Ordering {
    let dirs = (b.src.kind == Kind::Dir).cmp(&(a.src.kind == Kind::Dir));
    let by = match key {
        SortKey::Name => Ordering::Equal,
        SortKey::Kind => kind_text(a).cmp(&kind_text(b)),
        SortKey::Size => a.size.cmp(&b.size),
        SortKey::Modified => a.modified.cmp(&b.modified),
    };
    let by = by.then_with(|| natural(&a.name, &b.name));
    dirs.then(if ascending { by } else { by.reverse() })
}

/// Case-blind, with digit runs compared as numbers (`shot2` before `shot10`).
pub fn natural(a: &str, b: &str) -> Ordering {
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return a.cmp(b),
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(p), Some(q)) if p.is_ascii_digit() && q.is_ascii_digit() => {
                let take = |it: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut s = String::new();
                    while let Some(c) = it.peek().copied().filter(char::is_ascii_digit) {
                        s.push(c);
                        it.next();
                    }
                    s
                };
                let (m, n) = (take(&mut x), take(&mut y));
                let (mt, nt) = (m.trim_start_matches('0'), n.trim_start_matches('0'));
                let o = mt.len().cmp(&nt.len()).then_with(|| mt.cmp(nt));
                if o != Ordering::Equal {
                    return o;
                }
            }
            (Some(p), Some(q)) => {
                let o = p.to_lowercase().cmp(q.to_lowercase());
                if o != Ordering::Equal {
                    return o;
                }
                x.next();
                y.next();
            }
        }
    }
}

fn kind_text(e: &Entry) -> String {
    match e.src.kind {
        Kind::Dir => "folder".into(),
        Kind::Sequence => format!("{} seq", e.src.ext()),
        _ => e.src.ext(),
    }
}

fn ext_of(p: &Path) -> String {
    p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

fn id_of(p: &Path) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    p.hash(&mut h);
    h.finish()
}

/// `name` split around its last digit run before the extension:
/// `plate.1001.exr` is ("plate.", "1001", ".exr").
pub fn split_number(name: &str) -> Option<(&str, &str, &str)> {
    let stem_end = name.rfind('.').filter(|&d| d > 0).unwrap_or(name.len());
    let stem = &name[..stem_end];
    let end = stem.rfind(|c: char| c.is_ascii_digit())? + 1;
    let start = stem[..end].rfind(|c: char| !c.is_ascii_digit()).map_or(0, |i| i + 1);
    Some((&name[..start], &name[start..end], &name[end..]))
}

/// List `dir`, grouping numbered files that share a prefix, suffix and
/// width into sequences (two or more frames; one alone stays a file).
pub fn scan(dir: &Path, hidden: bool) -> Result<Vec<Entry>, String> {
    let rd = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut out = vec![];
    // (prefix, suffix, width) -> [(number, path, size, mtime)]
    type Run = Vec<(i64, PathBuf, u64, Option<SystemTime>)>;
    let mut runs: BTreeMap<(String, String, usize), Run> = BTreeMap::new();
    for de in rd.flatten() {
        let name = de.file_name().to_string_lossy().into_owned();
        let dot = name.starts_with('.');
        if dot && !hidden {
            continue;
        }
        let path = de.path();
        let md = std::fs::metadata(&path).ok();
        let is_dir = md.as_ref().is_some_and(|m| m.is_dir());
        let size = md.as_ref().map_or(0, |m| m.len());
        let modified = md.as_ref().and_then(|m| m.modified().ok());
        if !is_dir {
            if let Some((pre, num, suf)) = split_number(&name) {
                if let Ok(n) = num.parse::<i64>() {
                    runs.entry((pre.to_owned(), suf.to_owned(), num.len())).or_default().push((n, path, size, modified));
                    continue;
                }
            }
        }
        out.push(plain(path, name, is_dir, size, modified, dot));
    }
    for ((prefix, suffix, pad), mut run) in runs {
        if run.len() == 1 {
            let (_, path, size, modified) = run.pop().unwrap();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let dot = name.starts_with('.');
            out.push(plain(path, name, false, size, modified, dot));
            continue;
        }
        run.sort_by_key(|r| r.0);
        let seq = Seq { dir: dir.to_path_buf(), prefix, suffix, pad, frames: run.iter().map(|r| r.0).collect() };
        let first = run[0].1.clone();
        out.push(Entry {
            id: id_of(&dir.join(seq.pattern())),
            name: seq.pattern(),
            frames: run.len() as u32,
            size: run.iter().map(|r| r.2).sum(),
            modified: run.iter().filter_map(|r| r.3).max(),
            hidden: seq.prefix.starts_with('.'),
            src: FileSrc { path: first, kind: Kind::Sequence, seq: Some(seq) },
        });
    }
    out.sort_by(|a, b| order(a, b, SortKey::Name, true));
    Ok(out)
}

fn plain(path: PathBuf, name: String, is_dir: bool, size: u64, modified: Option<SystemTime>, hidden: bool) -> Entry {
    let kind = if is_dir {
        Kind::Dir
    } else if MOVIE_EXTS.contains(&ext_of(&path).as_str()) {
        Kind::Movie
    } else {
        Kind::File
    };
    Entry { id: id_of(&path), name, frames: 1, size: if is_dir { 0 } else { size }, modified, hidden, src: FileSrc { path, kind, seq: None } }
}

/// The columns [`Listing`]'s rows carry, in [`Details`] order.
pub fn columns() -> Vec<crate::table::Column> {
    use crate::table::Column;
    vec![Column::new("Name", 200.0), Column::new("Frames", 96.0).align(egui::Align::Max), Column::new("Kind", 80.0), Column::new("Size", 72.0).align(egui::Align::Max), Column::new("Modified", 96.0)]
}

/// The sort key a [`columns`] index stands for.
pub fn sort_key_of(col: usize) -> SortKey {
    match col {
        2 => SortKey::Kind,
        3 => SortKey::Size,
        4 => SortKey::Modified,
        _ => SortKey::Name,
    }
}

impl Items for Listing {
    type Src = FileSrc;
    fn len(&self) -> usize {
        self.shown.len()
    }
    fn id(&self, i: usize) -> u64 {
        self.entry(i).id
    }
    fn src(&self, i: usize) -> FileSrc {
        self.entry(i).src.clone()
    }
    fn label(&self, i: usize) -> String {
        self.entry(i).name.clone()
    }
    fn frames(&self, i: usize) -> u32 {
        self.entry(i).frames
    }
    fn poster(&self, i: usize) -> u32 {
        // Not the first frame: the middle shows what a shot is.
        self.entry(i).frames / 2
    }
    fn frame_name(&self, i: usize, f: u32) -> String {
        match &self.entry(i).src.seq {
            Some(s) => s.frames[(f as usize).min(s.frames.len() - 1)].to_string(),
            None => (f + 1).to_string(),
        }
    }
    fn badge(&self, i: usize) -> Option<String> {
        let e = self.entry(i);
        match &e.src.seq {
            Some(s) if s.missing() > 0 => Some(format!("{}-{} gaps", s.first(), s.last())),
            Some(s) => Some(format!("{}-{}", s.first(), s.last())),
            None if e.src.kind == Kind::Movie && e.frames > 1 => Some(format!("{}f", e.frames)),
            None => None,
        }
    }
    fn kind(&self, i: usize) -> Option<String> {
        let e = self.entry(i);
        Some(match e.src.kind {
            Kind::Dir => "folder".into(),
            _ => e.src.ext().to_uppercase(),
        })
    }
    fn has_picture(&self, i: usize) -> bool {
        self.entry(i).src.kind != Kind::Dir
    }
    fn dim(&self, i: usize) -> bool {
        self.entry(i).hidden
    }
}

impl Details for Listing {
    fn cell(&self, i: usize, col: usize) -> String {
        let e = self.entry(i);
        match col {
            1 => match &e.src.seq {
                Some(s) if s.missing() > 0 => format!("{}-{} ({} missing)", s.first(), s.last(), s.missing()),
                Some(s) => format!("{}-{}", s.first(), s.last()),
                None if e.src.kind == Kind::Movie => if e.frames > 1 { e.frames.to_string() } else { String::new() },
                None => String::new(),
            },
            2 => kind_text(e),
            3 if e.src.kind == Kind::Dir => "—".into(),
            3 => size_text(e.size),
            4 => when(e.modified),
            _ => String::new(),
        }
    }
}

pub fn size_text(n: u64) -> String {
    const K: f64 = 1024.0;
    let f = n as f64;
    if f < K {
        format!("{n} B")
    } else if f < K * K {
        format!("{:.0} KB", f / K)
    } else if f < K * K * K {
        format!("{:.1} MB", f / K / K)
    } else {
        format!("{:.2} GB", f / K / K / K)
    }
}

fn when(t: Option<SystemTime>) -> String {
    let Some(t) = t else { return String::new() };
    let Ok(age) = SystemTime::now().duration_since(t) else { return "just now".into() };
    match age.as_secs() {
        0..=59 => "just now".into(),
        s @ 60..=3599 => format!("{} min ago", s / 60),
        s @ 3600..=86399 => format!("{} h ago", s / 3600),
        s => format!("{} d ago", s / 86400),
    }
}

/// Box-filters `rgba` (`w` x `h`, 8 bits a channel) down to fit `max_px`
/// on its long side (never up). Loaders hand their decode to this.
pub fn shrink(rgba: &[u8], w: usize, h: usize, max_px: u32) -> ColorImage {
    let long = w.max(h).max(1);
    let k = (long as f32 / max_px.max(1) as f32).max(1.0);
    let (nw, nh) = (((w as f32 / k).round() as usize).max(1), ((h as f32 / k).round() as usize).max(1));
    let mut px = Vec::with_capacity(nw * nh);
    for y in 0..nh {
        let (y0, y1) = (y * h / nh, ((y + 1) * h / nh).max(y * h / nh + 1));
        for x in 0..nw {
            let (x0, x1) = (x * w / nw, ((x + 1) * w / nw).max(x * w / nw + 1));
            let mut s = [0u32; 4];
            for yy in y0..y1 {
                for xx in x0..x1 {
                    let o = (yy * w + xx) * 4;
                    for c in 0..4 {
                        s[c] += rgba[o + c] as u32;
                    }
                }
            }
            let n = ((y1 - y0) * (x1 - x0)) as u32;
            px.push(Color32::from_rgba_unmultiplied((s[0] / n) as u8, (s[1] / n) as u8, (s[2] / n) as u8, (s[3] / n) as u8));
        }
    }
    ColorImage::new([nw, nh], px)
}

/// A thumbnail loader for PNG stills and PNG sequences. Anything else is
/// "no preview"; an app puts its own decoders in front.
pub fn load_png(r: &Request<FileSrc>) -> Result<ColorImage, String> {
    let path = r.src.frame_path(r.frame);
    if ext_of(&path) != "png" {
        return Err("no preview".into());
    }
    let file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
    let mut dec = png::Decoder::new(std::io::BufReader::new(file));
    dec.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = dec.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or("too big")?];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let (w, h) = (info.width as usize, info.height as usize);
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => buf[..w * h * 4].to_vec(),
        png::ColorType::Rgb => buf[..w * h * 3].chunks(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf[..w * h * 2].chunks(2).flat_map(|c| [c[0], c[0], c[0], c[1]]).collect(),
        png::ColorType::Grayscale => buf[..w * h].iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return Err("indexed PNG".into()),
    };
    Ok(shrink(&rgba, w, h, r.max_px))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("imfram-files-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn the_last_digit_run_before_the_extension_is_the_frame_number() {
        assert_eq!(split_number("plate.1001.exr"), Some(("plate.", "1001", ".exr")));
        assert_eq!(split_number("sh010_v2_0042.png"), Some(("sh010_v2_", "0042", ".png")));
        assert_eq!(split_number("render12"), Some(("render", "12", "")));
        assert_eq!(split_number("notes.txt"), None);
    }

    #[test]
    fn numbered_files_group_into_sequences_and_a_lone_one_stays_a_file() {
        let d = tmp("seq");
        for n in [1001, 1002, 1004] {
            std::fs::write(d.join(format!("plate.{n}.exr")), b"xx").unwrap();
        }
        std::fs::write(d.join("other.0001.png"), b"x").unwrap();
        std::fs::write(d.join("clip.mov"), b"x").unwrap();
        std::fs::create_dir(d.join("renders")).unwrap();
        std::fs::write(d.join(".hidden.1.exr"), b"").unwrap();
        let es = scan(&d, false).unwrap();
        let names: Vec<&str> = es.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["renders", "clip.mov", "other.0001.png", "plate.####.exr"]);
        let seq = &es[3];
        assert_eq!(seq.src.kind, Kind::Sequence);
        assert_eq!(seq.frames, 3);
        assert_eq!(seq.size, 6);
        let s = seq.src.seq.as_ref().unwrap();
        assert_eq!((s.first(), s.last(), s.missing()), (1001, 1004, 1));
        assert_eq!(seq.src.frame_path(2), d.join("plate.1004.exr"), "a scrub index skips the gap");
        assert_eq!(es[1].src.kind, Kind::Movie);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn names_sort_naturally_and_folders_stay_first_both_ways() {
        assert_eq!(natural("shot2", "shot10"), Ordering::Less);
        assert_eq!(natural("Alpha", "beta"), Ordering::Less);
        let d = tmp("sort");
        std::fs::create_dir(d.join("zz")).unwrap();
        std::fs::write(d.join("a.txt"), b"1").unwrap();
        std::fs::write(d.join("b.txt"), b"333").unwrap();
        let mut es = scan(&d, false).unwrap();
        es.sort_by(|a, b| order(a, b, SortKey::Size, false));
        let names: Vec<&str> = es.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["zz", "b.txt", "a.txt"]);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn a_listing_filters_and_probes_movies_off_thread() {
        let d = tmp("listing");
        std::fs::write(d.join("a.mov"), b"x").unwrap();
        std::fs::write(d.join("b.png"), b"x").unwrap();
        let mut l = Listing::new(&d).with_probe(|_| Some(48));
        let ctx = egui::Context::default();
        let t = std::time::Instant::now();
        while l.len() < 2 || l.frames(0) != 48 {
            l.poll(&ctx);
            assert!(t.elapsed().as_secs() < 5, "never listed / probed");
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(l.badge(0).as_deref(), Some("48f"));
        l.filter = "PNG".into();
        l.poll(&ctx);
        assert_eq!(l.len(), 1);
        assert_eq!(l.label(0), "b.png");
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn shrink_averages_down_and_never_grows() {
        let rgba: Vec<u8> = [[255, 0, 0, 255], [0, 0, 255, 255]].iter().cycle().take(4 * 2).flatten().copied().collect();
        let img = shrink(&rgba, 4, 2, 2);
        assert_eq!(img.size, [2, 1]);
        assert_eq!(img.pixels[0], Color32::from_rgb(127, 0, 127));
        assert_eq!(shrink(&rgba, 4, 2, 64).size, [4, 2]);
    }

    fn write_png(path: &Path, w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 3]) {
        let file = std::fs::File::create(path).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut wr = enc.write_header().unwrap();
        let data: Vec<u8> = (0..w * h).flat_map(|k| f(k % w, k / w)).collect();
        wr.write_image_data(&data).unwrap();
    }

    fn look_dir() -> PathBuf {
        let d = tmp("look");
        std::fs::create_dir(d.join("renders")).unwrap();
        std::fs::create_dir(d.join("reference")).unwrap();
        std::fs::write(d.join("notes.txt"), b"hello").unwrap();
        std::fs::write(d.join("edit_v3.mov"), b"x").unwrap();
        for (k, name) in ["sh010_plate", "sh020_comp", "sh030_bg"].iter().enumerate() {
            for n in 0..24u32 {
                write_png(&d.join(format!("{name}.{:04}.png", 1001 + n)), 128, 72, |x, y| {
                    let cx = 10.0 + n as f32 * 4.5;
                    let dd = ((x as f32 - cx).powi(2) + (y as f32 - 36.0).powi(2)).sqrt();
                    if dd < 9.0 { [240, 240, 240] } else { [(40 + 60 * k) as u8 + (x / 3) as u8, 60 + (y / 2) as u8, (160 - 40 * k) as u8] }
                });
            }
        }
        write_png(&d.join("still.png"), 64, 96, |x, y| [(x * 4) as u8, (y * 2) as u8, 128]);
        d
    }

    /// `cargo test files_to_look_at -- --ignored` → target/files-list.png, files-tiles.png
    #[test]
    #[ignore]
    fn files_to_look_at() {
        use super::super::browse::{FileView, Mode};
        let d = look_dir();
        for mode in [Mode::List, Mode::Tiles] {
            let mut v = FileView::new(&d);
            v.set_mode(mode);
            v.list.row_h = 44.0;
            let mut thumbs = super::super::Thumbs::new(4, load_png);
            let name = if mode == Mode::List { "files-list" } else { "files-tiles" };
            crate::uishot::render(name, 820.0, 420.0, 2.0, 80, |ui, frame| {
                // (No hover here: a pointer pushed mid-pass is not seen. The scrub is unit-tested.)
                if frame == 5 {
                    v.list.selection.insert(v.listing.all().iter().find(|e| e.name.starts_with("sh020")).map_or(0, |e| e.id));
                    v.gallery.selection = v.list.selection.clone();
                }
                v.show(ui, &mut thumbs);
                std::thread::sleep(std::time::Duration::from_millis(10));
            });
        }
        let _ = std::fs::remove_dir_all(d);
    }
}
