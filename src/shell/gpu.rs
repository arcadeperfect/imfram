//! The GPU handles every window shares. An app that owns its own device
//! wraps the same handles here (they are all `Clone`, Arc-backed); tests
//! and `uishot` open a headless one.

#[derive(Clone)]
pub struct Gpu {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

impl Gpu {
    pub fn new(instance: wgpu::Instance, adapter: wgpu::Adapter, device: wgpu::Device, queue: wgpu::Queue) -> Self {
        Self { instance, adapter, device, queue }
    }

    /// A device with no surface, default limits: tests, offscreen renders,
    /// and tool-only apps (their windows make surfaces from it after).
    /// `WGPU_BACKEND` and the other `WGPU_*` switches apply. `None` when
    /// the machine has no adapter.
    pub fn standalone() -> Option<Self> {
        pollster::block_on(Self::standalone_async())
    }

    pub async fn standalone_async() -> Option<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                ..Default::default()
            })
            .await
            .ok()?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor { label: Some("imfram headless"), ..Default::default() })
            .await
            .ok()?;
        Some(Self { instance, adapter, device, queue })
    }

    /// Largest texture side this device allows.
    pub fn max_texture_size(&self) -> u32 {
        self.device.limits().max_texture_dimension_2d
    }
}
