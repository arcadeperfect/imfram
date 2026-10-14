//! Test-only: render a piece of the REAL UI through egui-wgpu on a headless
//! device and write it to `target/<name>.png`.
//!
//! [`crate::snapshot`] rasterizes egui's tessellated output on the CPU, which
//! is enough to assert geometry but skips text and anything drawn from the
//! font atlas (glyphs, small discs). This goes the other way: a real device,
//! the app's own renderer, everything on screen. Slower and GPU-bound, so it
//! belongs in `#[ignore]`d tests you run to LOOK at something:
//!
//! ```ignore
//! uishot::render("keys-window", 860.0, 420.0, 2.0, 4, |ui, _frame| { … });
//! ```
//!
//! `build` runs once per frame (egui needs a second pass for anything that
//! sizes itself from the last one), with the frame index, so a test can park
//! the pointer once the layout it is aiming at exists.

use std::cell::RefCell;
use std::path::PathBuf;

thread_local! {
    static STYLE: RefCell<Option<Box<dyn Fn(&egui::Context)>>> = const { RefCell::new(None) };
}

/// What to run on each fresh context before rendering (an app's theme
/// sync). Set once per test thread; unset, egui's dark theme is used.
pub fn set_style(f: impl Fn(&egui::Context) + 'static) {
    STYLE.with(|s| *s.borrow_mut() = Some(Box::new(f)));
}

/// Render `frames` egui passes and save the last one. `None` when the
/// machine has no adapter — the caller prints a SKIP and returns.
pub fn render(
    name: &str,
    w_pt: f32,
    h_pt: f32,
    ppp: f32,
    frames: usize,
    mut build: impl FnMut(&mut egui::Ui, usize),
) -> Option<PathBuf> {
    let Some(gpu) = crate::shell::Gpu::standalone() else {
        eprintln!("SKIP {name}: no adapter");
        return None;
    };
    let ctx = egui::Context::default();
    ctx.set_theme(egui::Theme::Dark);
    STYLE.with(|s| if let Some(f) = &*s.borrow() { f(&ctx) });
    let mut deltas = Vec::new();
    let mut last = None;
    for frame in 0..frames.max(1) {
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(w_pt, h_pt),
            )),
            // Time advances a frame at a time, so anything that breathes or
            // twinkles is caught mid-cycle rather than at t = 0.
            time: Some(frame as f64 / 60.0),
            ..Default::default()
        };
        input.viewports.values_mut().for_each(|v| v.native_pixels_per_point = Some(ppp));
        let mut out = ctx.run_ui(input, |ui| build(ui, frame));
        deltas.extend(out.textures_delta.set.drain());
        out.textures_delta.clear();
        last = Some(out);
    }
    let mut out = last?;
    let prims = ctx.tessellate(std::mem::take(&mut out.shapes), out.pixels_per_point);
    let (w, h) = ((w_pt * ppp) as u32, (h_pt * ppp) as u32);
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("uishot"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = tex.create_view(&Default::default());
    let mut renderer =
        egui_wgpu::Renderer::new(&gpu.device, format, egui_wgpu::RendererOptions::default());
    for (id, ds) in &deltas {
        for d in ds {
            renderer.update_texture(&gpu.device, &gpu.queue, *id, d);
        }
    }
    let screen = egui_wgpu::ScreenDescriptor { size_in_pixels: [w, h], pixels_per_point: ppp };
    let mut enc = gpu.device.create_command_encoder(&Default::default());
    let user = renderer.update_buffers(&gpu.device, &gpu.queue, &mut enc, &prims, &screen);
    {
        let mut rp = enc
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("uishot"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.043,
                            g: 0.043,
                            b: 0.043,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            })
            .forget_lifetime();
        renderer.render(&mut rp, &prims, &screen);
    }
    let unpadded = w as usize * 4;
    let padded = unpadded.div_ceil(256) * 256;
    let buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("uishot-rb"),
        size: (padded * h as usize) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buf,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded as u32),
                rows_per_image: Some(h),
            },
        },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    gpu.queue.submit(user.into_iter().chain([enc.finish()]));
    let (tx, rx) = std::sync::mpsc::channel();
    buf.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
    rx.recv().ok()?.ok()?;
    let mut rgb = Vec::with_capacity(w as usize * h as usize * 3);
    {
        let data = buf.slice(..).get_mapped_range().ok()?;
        for y in 0..h as usize {
            for px in data[y * padded..y * padded + unpadded].chunks_exact(4) {
                rgb.extend_from_slice(&px[..3]);
            }
        }
    }
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("target/{name}.png"));
    let file = std::fs::File::create(&path).unwrap();
    let mut png = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    png.set_color(png::ColorType::Rgb);
    png.set_depth(png::BitDepth::Eight);
    png.write_header().unwrap().write_image_data(&rgb).unwrap();
    eprintln!("wrote {}", path.display());
    Some(path)
}
