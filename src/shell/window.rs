//! A tool window: its own OS window + surface, drawn entirely with egui.
//! Repaints on demand (input, egui animations, or an explicit request) so
//! tool windows never hold up a game window's frame.

use std::sync::Arc;

use web_time::Instant;

use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

use super::{EguiLayer, Gpu, UiApp};

pub struct UiWindow {
    pub window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    egui: EguiLayer,
    repaint_at: Option<Instant>,
    /// Never shown (a capture's or a test's window): `show` and
    /// `toggle_visible` leave it be.
    headless: bool,
    /// What a frame starts as.
    pub clear: wgpu::Color,
}

impl UiWindow {
    /// `shown` false makes a window that stays hidden (captures, tests).
    pub fn new(el: &ActiveEventLoop, gpu: &Gpu, title: &str, size: (u32, u32), shown: bool) -> Self {
        let attrs = Window::default_attributes()
            .with_title(title)
            .with_inner_size(LogicalSize::new(size.0, size.1))
            .with_visible(shown)
            .with_active(shown);
        let window = Arc::new(el.create_window(attrs).expect("create tool window"));
        let surface = gpu.instance.create_surface(window.clone()).expect("create surface");

        let px = window.inner_size();
        let mut config = surface
            .get_default_config(&gpu.adapter, px.width.max(1), px.height.max(1))
            .expect("surface config");
        // egui blends in gamma space; give it a non-sRGB target when we can.
        let linear = config.format.remove_srgb_suffix();
        if surface.get_capabilities(&gpu.adapter).formats.contains(&linear) {
            config.format = linear;
        }
        config.present_mode = wgpu::PresentMode::AutoVsync;
        surface.configure(&gpu.device, &config);

        let egui = EguiLayer::new(gpu, &window, config.format);
        Self {
            window,
            surface,
            config,
            egui,
            repaint_at: Some(Instant::now()),
            headless: !shown,
            clear: wgpu::Color::BLACK,
        }
    }

    pub fn id(&self) -> WindowId {
        self.window.id()
    }

    pub fn format(&self) -> wgpu::TextureFormat {
        self.config.format
    }

    pub fn size(&self) -> [u32; 2] {
        [self.config.width, self.config.height]
    }

    pub fn is_visible(&self) -> bool {
        self.window.is_visible().unwrap_or(true)
    }

    pub fn set_visible(&mut self, show: bool) {
        if self.headless {
            return;
        }
        self.window.set_visible(show);
        if show {
            self.request_repaint();
        }
    }

    pub fn toggle_visible(&mut self) {
        if self.headless {
            return;
        }
        let show = !self.is_visible();
        self.window.set_visible(show);
        if show {
            self.window.focus_window();
            self.request_repaint();
        }
    }

    /// Bring the window back, wherever it went: closed, minimised or
    /// behind the others.
    pub fn show(&mut self) {
        if self.headless {
            return;
        }
        self.window.set_visible(true);
        self.window.set_minimized(false);
        self.window.focus_window();
        self.request_repaint();
    }

    pub fn request_repaint(&mut self) {
        self.repaint_at = Some(Instant::now());
    }

    /// Feed a window event to egui. Closing a tool window only hides it.
    pub fn on_event(&mut self, gpu: &Gpu, event: &WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                self.window.set_visible(false);
                return;
            }
            WindowEvent::Resized(size) if size.width > 0 && size.height > 0 => {
                self.config.width = size.width;
                self.config.height = size.height;
                self.surface.configure(&gpu.device, &self.config);
                self.request_repaint();
            }
            _ => {}
        }
        if self.egui.on_event(&self.window, event).repaint {
            self.request_repaint();
        }
    }

    /// Call once per event-loop turn: asks winit for a redraw when one is due.
    pub fn poll(&mut self) {
        if self.repaint_at.is_some_and(|t| Instant::now() >= t) {
            self.repaint_at = None;
            self.window.request_redraw();
        }
    }

    /// Run one egui frame and present it.
    pub fn draw(&mut self, gpu: &Gpu, app: &mut dyn UiApp) {
        use wgpu::CurrentSurfaceTexture as Cst;
        let frame = match self.surface.get_current_texture() {
            Cst::Success(f) | Cst::Suboptimal(f) => f,
            Cst::Timeout | Cst::Occluded => return,
            _ => {
                self.surface.configure(&gpu.device, &self.config);
                self.request_repaint();
                return;
            }
        };
        let target = frame.texture.create_view(&Default::default());
        self.paint(gpu, &target, app);
        gpu.queue.present(frame);
    }

    /// Run one egui frame into an offscreen texture (captures, scripted
    /// tests); the texture can be copied out.
    pub fn offscreen(&mut self, gpu: &Gpu, app: &mut dyn UiApp) -> wgpu::Texture {
        let tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ui offscreen"),
            size: wgpu::Extent3d { width: self.config.width, height: self.config.height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        self.paint(gpu, &tex.create_view(&Default::default()), app);
        tex
    }

    /// The window's egui context (scripted tests read widget state from it).
    pub fn ctx(&self) -> &egui::Context {
        self.egui.ctx()
    }

    pub fn layer_mut(&mut self) -> &mut EguiLayer {
        &mut self.egui
    }

    /// Queue synthetic input for the next frame (scripted tests).
    pub fn inject(&mut self, events: impl IntoIterator<Item = egui::Event>) {
        self.egui.inject(events);
    }

    fn paint(&mut self, gpu: &Gpu, target: &wgpu::TextureView, app: &mut dyn UiApp) {
        let size = [self.config.width, self.config.height];
        // egui asks for follow-up frames while animating / dragging.
        if let Some(delay) = self.egui.paint(gpu, &self.window, target, size, Some(self.clear), app) {
            let at = Instant::now() + delay;
            self.repaint_at = Some(self.repaint_at.map_or(at, |t| t.min(at)));
        }
    }
}
