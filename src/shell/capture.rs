//! Reading a texture back: a window's offscreen frame to pixels, to a PNG.
//! Scripted tests and captures look at the UI this way.

use std::path::Path;

use super::Gpu;

/// The texture's pixels, tightly packed rows, as the format stores them
/// (4 bytes a pixel for the 8-bit formats). `None` if the map failed.
pub fn read_pixels(gpu: &Gpu, tex: &wgpu::Texture) -> Option<Vec<u8>> {
    let (w, h) = (tex.width(), tex.height());
    let bpp = tex.format().block_copy_size(None)? as usize;
    let unpadded = w as usize * bpp;
    let padded = unpadded.div_ceil(256) * 256;
    let buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: (padded * h as usize) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut enc = gpu.device.create_command_encoder(&Default::default());
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo { texture: tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        wgpu::TexelCopyBufferInfo {
            buffer: &buf,
            layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded as u32), rows_per_image: Some(h) },
        },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    gpu.queue.submit([enc.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buf.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
    rx.recv().ok()?.ok()?;
    let data = buf.slice(..).get_mapped_range().ok()?;
    let mut out = Vec::with_capacity(unpadded * h as usize);
    for y in 0..h as usize {
        out.extend_from_slice(&data[y * padded..y * padded + unpadded]);
    }
    Some(out)
}

/// Save an 8-bit 4-channel texture as an RGB PNG. BGRA formats are swizzled.
pub fn save_png(gpu: &Gpu, tex: &wgpu::Texture, path: &Path) -> Option<()> {
    let px = read_pixels(gpu, tex)?;
    let bgra = matches!(tex.format(), wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb);
    let mut rgb = Vec::with_capacity(px.len() / 4 * 3);
    for p in px.chunks_exact(4) {
        if bgra {
            rgb.extend_from_slice(&[p[2], p[1], p[0]]);
        } else {
            rgb.extend_from_slice(&p[..3]);
        }
    }
    let file = std::fs::File::create(path).ok()?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), tex.width(), tex.height());
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().ok()?.write_image_data(&rgb).ok()
}
