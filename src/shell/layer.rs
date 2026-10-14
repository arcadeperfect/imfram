//! egui plumbing for one window: input translation, the frame, and painting
//! into whatever target it is given. Tool windows clear and fill the target;
//! a game window draws it as an overlay on top of the world.

use std::time::Duration;

use winit::event::WindowEvent;
use winit::window::Window;

use super::Gpu;

/// The UI drawn in a layer each frame. A plain `FnMut(&mut egui::Ui)` works;
/// implement the trait to also see the raw input before egui does.
pub trait UiApp {
    /// Runs once per frame before egui: claim or rewrite input events.
    fn raw_input(&mut self, _raw: &mut egui::RawInput) {}
    fn ui(&mut self, ui: &mut egui::Ui);
}

impl<F: FnMut(&mut egui::Ui)> UiApp for F {
    fn ui(&mut self, ui: &mut egui::Ui) {
        self(ui)
    }
}

pub struct EguiLayer {
    ctx: egui::Context,
    state: egui_winit::State,
    renderer: egui_wgpu::Renderer,
}

impl EguiLayer {
    /// `format` is the format of the views this layer will paint into
    /// (non-sRGB preferred: egui blends in gamma space).
    pub fn new(gpu: &Gpu, window: &Window, format: wgpu::TextureFormat) -> Self {
        let ctx = egui::Context::default();
        let state = egui_winit::State::new(
            ctx.clone(),
            egui::ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            None,
            Some(gpu.device.limits().max_texture_dimension_2d as usize),
        );
        let renderer = egui_wgpu::Renderer::new(&gpu.device, format, egui_wgpu::RendererOptions::default());
        Self { ctx, state, renderer }
    }

    pub fn on_event(&mut self, window: &Window, event: &WindowEvent) -> egui_winit::EventResponse {
        self.state.on_window_event(window, event)
    }

    /// A text field has keyboard focus (so letter keys are being typed).
    /// Not `egui_wants_keyboard_input`: that is true whenever any widget has
    /// focus, which a clicked checkbox or a Tab press leaves behind.
    pub fn text_edit_focused(&self) -> bool {
        self.ctx.text_edit_focused()
    }

    /// The layer's egui context (scripted tests read widget state from it).
    pub fn ctx(&self) -> &egui::Context {
        &self.ctx
    }

    /// The renderer, for apps that register paint-callback resources.
    pub fn renderer_mut(&mut self) -> &mut egui_wgpu::Renderer {
        &mut self.renderer
    }

    /// Queue synthetic input for the next frame (scripted tests).
    pub fn inject(&mut self, events: impl IntoIterator<Item = egui::Event>) {
        self.state.egui_input_mut().events.extend(events);
    }

    /// Run one egui frame and paint it into `target` (`size` in pixels).
    /// `clear` wipes the target first; otherwise the UI is drawn over it.
    /// Returns how soon egui wants another frame, if it does.
    pub fn paint(
        &mut self,
        gpu: &Gpu,
        window: &Window,
        target: &wgpu::TextureView,
        size: [u32; 2],
        clear: Option<wgpu::Color>,
        app: &mut dyn UiApp,
    ) -> Option<Duration> {
        let mut input = self.state.take_egui_input(window);
        app.raw_input(&mut input);
        let mut out = self.ctx.run_ui(input, |ui| app.ui(ui));
        self.state.handle_platform_output(window, std::mem::take(&mut out.platform_output));

        let repaint = out
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map(|vp| vp.repaint_delay)
            .filter(|d| *d < Duration::from_secs(3600));

        let prims = self.ctx.tessellate(std::mem::take(&mut out.shapes), out.pixels_per_point);
        let screen = egui_wgpu::ScreenDescriptor { size_in_pixels: size, pixels_per_point: out.pixels_per_point };
        // egui insists every delta is consumed, hence drain.
        for (id, deltas) in out.textures_delta.set.drain() {
            for delta in deltas {
                self.renderer.update_texture(&gpu.device, &gpu.queue, id, &delta);
            }
        }
        let mut enc = gpu.device.create_command_encoder(&Default::default());
        let extra = self.renderer.update_buffers(&gpu.device, &gpu.queue, &mut enc, &prims, &screen);
        {
            let load = match clear {
                Some(c) => wgpu::LoadOp::Clear(c),
                None => wgpu::LoadOp::Load,
            };
            let mut pass = enc
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("egui"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: target,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
                    })],
                    ..Default::default()
                })
                .forget_lifetime();
            self.renderer.render(&mut pass, &prims, &screen);
        }
        gpu.queue.submit(extra.into_iter().chain([enc.finish()]));
        for id in out.textures_delta.free.drain() {
            self.renderer.free_texture(&id);
        }
        repaint
    }
}
