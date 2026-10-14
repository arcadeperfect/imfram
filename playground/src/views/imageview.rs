//! Playground tab for `imfram::imageview`: a generated 256×256 gradient
//! with a hole punched in its alpha, uploaded once, sampled for the readout
//! from the same pixels.

use egui::{TextureHandle, TextureOptions, Ui};
use imfram::imageview::ImageView;

const N: usize = 256;
const HOLE_R: f32 = 60.0;

#[derive(Default)]
pub struct State {
    view: ImageView,
    tex: Option<TextureHandle>,
    pixels: Vec<[f32; 4]>,
}

/// A red-over-x, green-over-y gradient, half blue, alpha fading out
/// inside a soft disc at the centre.
fn generate() -> Vec<[f32; 4]> {
    let c = N as f32 * 0.5;
    (0..N * N)
        .map(|i| {
            let (x, y) = ((i % N) as f32, (i / N) as f32);
            let d = ((x - c).powi(2) + (y - c).powi(2)).sqrt();
            let a = ((d - HOLE_R) / 8.0).clamp(0.0, 1.0);
            [x / (N - 1) as f32, y / (N - 1) as f32, 0.5, a]
        })
        .collect()
}

pub fn ui(ui: &mut Ui, s: &mut State) {
    if s.tex.is_none() {
        s.pixels = generate();
        let bytes: Vec<u8> = s.pixels.iter().flat_map(|p| p.map(|v| (v * 255.0).round() as u8)).collect();
        let img = egui::ColorImage::from_rgba_unmultiplied([N, N], &bytes);
        s.tex = Some(ui.ctx().load_texture("imageview-demo", img, TextureOptions::NEAREST));
    }
    let Some(tex) = &s.tex else { return };
    ui.weak("MMB / alt+LMB pan · ctrl+MMB drag-zoom · wheel zoom · F fit · 1 is 1:1 · hover for the readout");
    let px = &s.pixels;
    let sample = |p: [usize; 2]| px[p[1] * N + p[0]];
    s.view.show(ui, tex.id(), [N, N], Some(&sample));
}
