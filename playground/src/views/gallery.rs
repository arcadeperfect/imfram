//! The gallery over 600 made-up pictures in sections: stills of every
//! shape, clips to scrub (a dot crossing the frame), a few offline ones.
//! The loader paints each on a worker after a short sleep, so the lazy
//! loading and the per-frame uploads are visible.

use egui::{Color32, ColorImage, Ui};
use imfram::gallery::{Gallery, Items, Request, Thumbs};

const N: usize = 600;
const CLIP_FRAMES: u32 = 48;
/// How long a made-up load takes, to look like disk.
const LATENCY_MS: u64 = 12;

pub struct State {
    gallery: Gallery,
    thumbs: Thumbs<u32>,
    square: bool,
    last: String,
}

impl Default for State {
    fn default() -> Self {
        Self { gallery: Gallery::new(), thumbs: Thumbs::new(4, load), square: true, last: String::new() }
    }
}

struct Pictures;

fn is_clip(i: usize) -> bool {
    i % 7 == 3
}

/// Width over height of picture i.
fn aspect(i: usize) -> f32 {
    [16.0 / 9.0, 4.0 / 3.0, 1.0, 2.0 / 3.0, 2.39][i % 5]
}

impl Items for Pictures {
    type Src = u32;
    fn len(&self) -> usize {
        N
    }
    fn id(&self, i: usize) -> u64 {
        i as u64
    }
    fn src(&self, i: usize) -> u32 {
        i as u32
    }
    fn label(&self, i: usize) -> String {
        if is_clip(i) {
            format!("sh{:03}_plate.####.exr", i * 10)
        } else {
            format!("still_{i:04}.png")
        }
    }
    fn frames(&self, i: usize) -> u32 {
        if is_clip(i) { CLIP_FRAMES } else { 1 }
    }
    fn poster(&self, i: usize) -> u32 {
        if is_clip(i) { CLIP_FRAMES / 2 } else { 0 }
    }
    fn frame_name(&self, _i: usize, f: u32) -> String {
        (1001 + f).to_string()
    }
    fn badge(&self, i: usize) -> Option<String> {
        is_clip(i).then(|| format!("1001-{}", 1000 + CLIP_FRAMES))
    }
    fn kind(&self, i: usize) -> Option<String> {
        Some(if is_clip(i) { "EXR" } else { "PNG" }.into())
    }
    fn section(&self, i: usize) -> Option<String> {
        [0, 40, 110, 250, 420].iter().position(|&s| s == i).map(|k| ["plates", "renders", "reference", "stills", "archive"][k].into())
    }
    fn dim(&self, i: usize) -> bool {
        i >= 420 && i % 4 == 0
    }
}

/// Paints picture `src` (at `frame`, for clips) no bigger than `max_px`.
fn load(r: &Request<u32>) -> Result<ColorImage, String> {
    std::thread::sleep(std::time::Duration::from_millis(LATENCY_MS));
    let i = r.src as usize;
    if i % 97 == 50 {
        return Err("unreadable".into());
    }
    let a = aspect(i);
    let long = r.max_px as f32;
    let (w, h) = if a >= 1.0 { (long, long / a) } else { (long * a, long) };
    let (w, h) = (w.max(1.0) as usize, h.max(1.0) as usize);
    let hue = (i as f32 * 0.618_034).fract();
    let base = egui::ecolor::Hsva::new(hue, 0.55, 0.75, 1.0);
    let far = egui::ecolor::Hsva::new((hue + 0.15).fract(), 0.7, 0.25, 1.0);
    let t = if is_clip(i) { r.frame as f32 / (CLIP_FRAMES - 1) as f32 } else { 0.5 };
    let dot = (0.15 + 0.7 * t, 0.5 + 0.25 * (t * std::f32::consts::TAU).sin());
    let mut px = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let (u, v) = (x as f32 / w as f32, y as f32 / h as f32);
            let g = ((u - 0.3).powi(2) + (v - 0.3).powi(2)).sqrt().min(1.0);
            let c = lerp(base, far, g);
            let d = (((u - dot.0) * a).powi(2) + (v - dot.1).powi(2)).sqrt();
            px.push(if d < 0.08 { Color32::from_gray(240) } else { c });
        }
    }
    Ok(ColorImage::new([w, h], px))
}

fn lerp(a: egui::ecolor::Hsva, b: egui::ecolor::Hsva, t: f32) -> Color32 {
    let (a, b): (egui::Rgba, egui::Rgba) = (a.into(), b.into());
    (a * (1.0 - t) + b * t).into()
}

pub fn ui(ui: &mut Ui, s: &mut State) {
    ui.horizontal(|ui| {
        ui.add(egui::Slider::new(&mut s.gallery.size, imfram::gallery::MIN_SIZE..=imfram::gallery::MAX_SIZE).text("size"));
        ui.checkbox(&mut s.gallery.fill, "fill");
        ui.checkbox(&mut s.gallery.scrub, "scrub");
        if ui.checkbox(&mut s.square, "square").changed() {
            s.gallery.metrics.aspect = if s.square { 1.0 } else { 16.0 / 9.0 };
        }
        let st = s.thumbs.stats();
        ui.weak(format!("{} selected · {} textures · {} queued · {} loading · {}", s.gallery.selection.len(), st.textures, st.queued, st.running, s.last));
    });
    let ev = s.gallery.show(ui, &Pictures, &mut s.thumbs);
    if let Some(id) = ev.activated {
        s.last = format!("opened {}", Pictures.label(id as usize));
    }
    if let Some(id) = ev.preview {
        s.last = format!("preview {}", Pictures.label(id as usize));
    }
    if ev.drag_started {
        s.last = format!("dragging {}", s.gallery.selection.len());
    }
}
