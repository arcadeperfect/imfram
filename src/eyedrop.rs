//! Eyedropper: sample the viewer's pixels into a colour parameter.
//!
//! Blender's hands (manifesto 11): hover any colour control — a picker
//! swatch or its open popup, an rgb slider row — and press **E**. The viewer
//! arms: a click samples the pixel under the pointer (the pixel readout's
//! box size), a drag averages the box it draws. Esc, right-click or a click
//! outside the viewer stands it down.
//!
//! What lands is the comp's SCENE-LINEAR value in the working space — the
//! numbers the node's shader sees — never the display value. Alpha is not
//! sampled: a swatch keeps its own.
//!
//! Two halves:
//! - the **protocol**, egui-temp state keyed by the control's own `Id`: a
//!   control calls [`hook`] every frame and applies whatever it returns, so
//!   no parameter plumbing runs between the viewer and the panels. The
//!   liftable widgets (`colorpicker`) reach it through a plain fn in their
//!   spec, the way they reach the rate keys.
//! - the **readback** ([`Sampler`]): the pixel picker's shape, one read in
//!   flight, landing a frame or two later.

use egui::{Context, Id, Ui};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// The key that arms it over a colour control.
pub const KEY: egui::Key = egui::Key::E;
/// Largest box edge a drag reads, in comp pixels; a bigger drag is read
/// around its centre. An average over a million pixels says nothing the
/// quarter-million around its middle does not.
const MAX_EDGE: u32 = 512;

#[derive(Clone, Copy, Debug, Default)]
struct State {
    /// The control the next viewer sample goes to.
    target: Option<Id>,
    /// A landed sample waiting for its control to pick it up.
    delivered: Option<(Id, [f32; 4])>,
}

fn sid() -> Id {
    Id::new("eyedrop")
}

fn state(ctx: &Context) -> State {
    ctx.data(|d| d.get_temp(sid())).unwrap_or_default()
}

fn set(ctx: &Context, s: State) {
    ctx.data_mut(|d| d.insert_temp(sid(), s));
}

/// What a control's [`hook`] reports this frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct Hook {
    /// The viewer is armed for THIS control.
    pub armed: bool,
    /// A sample landed for this control: scene-linear rgba.
    pub sample: Option<[f32; 4]>,
}

/// Call once a frame from a colour control. `hovered`: the pointer is over
/// the control (or its popup) — E there arms the viewer for `target`, and
/// E again stands it down.
pub fn hook(ui: &Ui, target: Id, hovered: bool) -> Hook {
    let ctx = ui.ctx();
    let mut s = state(ctx);
    if hovered && ctx.memory(|m| m.focused().is_none()) {
        let hit = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, KEY));
        if hit {
            s.target = if s.target == Some(target) { None } else { Some(target) };
            set(ctx, s);
        }
    }
    // Esc stands the armed control down before anything else (an open
    // picker popup would take it as "close") can see it.
    if s.target == Some(target)
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        s.target = None;
        set(ctx, s);
    }
    let sample = match s.delivered {
        Some((id, rgba)) if id == target => {
            s.delivered = None;
            set(ctx, s);
            Some(rgba)
        }
        _ => None,
    };
    if s.target == Some(target) {
        ctx.request_repaint();
    }
    Hook { armed: s.target == Some(target), sample }
}

/// For `colorpicker::PickSpec`: the same hook, rgb only.
pub fn pick_hook(ui: &Ui, target: Id, hovered: bool) -> (bool, Option<[f32; 3]>) {
    let h = hook(ui, target, hovered);
    (h.armed, h.sample.map(|s| [s[0], s[1], s[2]]))
}

/// Is the viewer armed (for anyone)?
pub fn armed(ctx: &Context) -> bool {
    state(ctx).target.is_some()
}

pub fn cancel(ctx: &Context) {
    let mut s = state(ctx);
    s.target = None;
    set(ctx, s);
}

/// A sample landed: hand it to the armed control and disarm.
pub fn deliver(ctx: &Context, rgba: [f32; 4]) {
    let mut s = state(ctx);
    if let Some(t) = s.target.take() {
        s.delivered = Some((t, rgba));
        set(ctx, s);
        ctx.request_repaint();
    }
}

/// A read box in comp pixels: min corner and size, clamped to the frame
/// and to [`MAX_EDGE`] around its centre.
pub fn read_box(a: (i32, i32), b: (i32, i32), out: (u32, u32), click_n: u32) -> Option<[u32; 4]> {
    let (ow, oh) = (out.0 as i32, out.1 as i32);
    if ow <= 0 || oh <= 0 {
        return None;
    }
    let (mut x0, mut y0) = (a.0.min(b.0), a.1.min(b.1));
    let (mut x1, mut y1) = (a.0.max(b.0) + 1, a.1.max(b.1) + 1);
    // A click (or a drag too short to mean a box) reads the readout's box
    // around the press, like the pixel readout does.
    if x1 - x0 <= 2 && y1 - y0 <= 2 {
        let n = click_n.max(1) as i32;
        x0 = b.0 - n / 2;
        y0 = b.1 - n / 2;
        x1 = x0 + n;
        y1 = y0 + n;
    }
    let cap = MAX_EDGE as i32;
    let (cx, cy) = ((x0 + x1) / 2, (y0 + y1) / 2);
    if x1 - x0 > cap {
        x0 = cx - cap / 2;
        x1 = x0 + cap;
    }
    if y1 - y0 > cap {
        y0 = cy - cap / 2;
        y1 = y0 + cap;
    }
    let (x0, y0) = (x0.clamp(0, ow - 1), y0.clamp(0, oh - 1));
    let (x1, y1) = (x1.clamp(x0 + 1, ow), y1.clamp(y0 + 1, oh));
    Some([x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32])
}

struct Read {
    buffer: wgpu::Buffer,
    w: u32,
    h: u32,
    stride: u32,
    ready: Arc<AtomicBool>,
    mapped: bool,
}

/// The viewer side: the press in flight and the readback.
#[derive(Default)]
pub struct Sampler {
    /// Where the press went down, comp pixels, while it is held.
    pub start: Option<(i32, i32)>,
    /// A box waiting to be read on the next encode.
    pub pending: Option<[u32; 4]>,
    read: Option<Read>,
}

impl Sampler {
    /// Queue the pending box's copy. `tex` is the comp output (rgba16f).
    pub fn encode(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        tex: &wgpu::Texture,
    ) {
        if self.read.is_some() {
            return;
        }
        let Some([x, y, w, h]) = self.pending.take() else { return };
        if x + w > tex.width() || y + h > tex.height() || w == 0 || h == 0 {
            return;
        }
        let stride = (w * 8).div_ceil(256) * 256;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("eyedrop-read"),
            size: (stride * h) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: tex,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(stride),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        self.read = Some(Read {
            buffer,
            w,
            h,
            stride,
            ready: Arc::new(AtomicBool::new(false)),
            mapped: false,
        });
    }

    /// The box's mean rgba once it has landed. Call once a frame.
    pub fn poll(&mut self) -> Option<[f32; 4]> {
        let r = self.read.as_mut()?;
        if !r.mapped {
            r.mapped = true;
            let flag = r.ready.clone();
            r.buffer.slice(..).map_async(wgpu::MapMode::Read, move |res| {
                if res.is_ok() {
                    flag.store(true, Ordering::Release);
                }
            });
        }
        if !r.ready.load(Ordering::Acquire) {
            return None;
        }
        let r = self.read.take()?;
        let mut sum = [0.0f64; 4];
        let mut count = 0.0f64;
        {
            let view = r.buffer.slice(..).get_mapped_range().ok()?;
            for row in 0..r.h as usize {
                let off = row * r.stride as usize;
                let px: &[half::f16] = bytemuck::cast_slice(&view[off..off + r.w as usize * 8]);
                for c in px.chunks_exact(4) {
                    // A NaN or inf pixel would poison the whole mean.
                    if c.iter().all(|v| v.to_f32().is_finite()) {
                        for k in 0..4 {
                            sum[k] += c[k].to_f32() as f64;
                        }
                        count += 1.0;
                    }
                }
            }
        }
        r.buffer.unmap();
        (count > 0.0).then(|| std::array::from_fn(|k| (sum[k] / count) as f32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_click_reads_the_readout_box_and_a_drag_its_own() {
        assert_eq!(read_box((10, 10), (10, 10), (100, 100), 3), Some([9, 9, 3, 3]));
        assert_eq!(read_box((10, 10), (10, 10), (100, 100), 1), Some([10, 10, 1, 1]));
        assert_eq!(read_box((40, 30), (20, 10), (100, 100), 3), Some([20, 10, 21, 21]));
    }

    #[test]
    fn boxes_stay_on_the_frame_and_under_the_cap() {
        assert_eq!(read_box((0, 0), (0, 0), (100, 100), 5), Some([0, 0, 3, 3]));
        assert_eq!(read_box((99, 99), (99, 99), (100, 100), 5), Some([97, 97, 3, 3]));
        let [_, _, w, h] = read_box((0, 0), (3999, 2999), (4000, 3000), 1).unwrap();
        assert!(w <= MAX_EDGE && h <= MAX_EDGE);
    }

    #[test]
    fn e_over_a_control_arms_it_and_a_delivery_lands_there_once() {
        let ctx = Context::default();
        let a = Id::new("a");
        let b = Id::new("b");
        let mut input = egui::RawInput::default();
        input.events.push(egui::Event::Key {
            key: KEY,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
        crate::snapshot::discard(ctx.run_ui(input, |ui| {
            assert!(hook(ui, a, true).armed);
            assert!(!hook(ui, b, false).armed);
        }));
        assert!(armed(&ctx));
        deliver(&ctx, [0.5, 0.25, 0.125, 1.0]);
        assert!(!armed(&ctx));
        crate::snapshot::discard(ctx.run_ui(Default::default(), |ui| {
            assert_eq!(hook(ui, b, false).sample, None);
            assert_eq!(hook(ui, a, false).sample, Some([0.5, 0.25, 0.125, 1.0]));
            assert_eq!(hook(ui, a, false).sample, None);
        }));
    }
}
