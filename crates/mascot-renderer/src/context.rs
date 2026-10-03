use std::sync::Arc;
use wgpu::{
    CompositeAlphaMode, Device, DeviceDescriptor, Instance, PowerPreference, PresentMode,
    Queue, RequestAdapterOptions, Surface, SurfaceColorSpace, SurfaceConfiguration,
    Texture, TextureFormat, TextureUsages, TextureView, TextureViewDescriptor,
};
use winit::window::Window;

use crate::error::RendererError;

pub const DEPTH_FORMAT: TextureFormat = TextureFormat::Depth32Float;

pub struct RenderContext<'w> {
    pub instance: Instance,
    pub adapter: wgpu::Adapter,
    pub device: Device,
    pub queue: Queue,
    pub surface: Surface<'w>,
    pub config: SurfaceConfiguration,
    pub depth_texture: Texture,
    pub depth_view: TextureView,
}

impl<'w> RenderContext<'w> {
    /// Initialize WGPU context with transparent surface support (PreMultiplied alpha mode).
    pub async fn new(
        window: Arc<Window>,
        width: u32,
        height: u32,
    ) -> Result<Self, RendererError> {
        let instance = Instance::default();

        let surface = instance
            .create_surface(window)
            .map_err(RendererError::CreateSurfaceError)?;

        let adapter = instance
            .request_adapter(&RequestAdapterOptions {
                power_preference: PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await
            .map_err(|_| RendererError::AdapterNotFound)?;

        let (device, queue) = adapter
            .request_device(&DeviceDescriptor {
                label: Some("Mascot Device"),
                required_features: Default::default(),
                required_limits: Default::default(),
                experimental_features: Default::default(),
                memory_hints: Default::default(),
                trace: Default::default(),
            })
            .await
            .map_err(RendererError::DeviceRequestFailed)?;

        let caps = surface.get_capabilities(&adapter);

        // Select texture format (prefer sRGB formats)
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);

        // Select alpha mode: strictly prefer PreMultiplied for transparent window composition
        let alpha_mode = if caps.alpha_modes.contains(&CompositeAlphaMode::PreMultiplied) {
            CompositeAlphaMode::PreMultiplied
        } else if caps.alpha_modes.contains(&CompositeAlphaMode::PostMultiplied) {
            CompositeAlphaMode::PostMultiplied
        } else {
            caps.alpha_modes[0]
        };

        let width = width.max(1);
        let height = height.max(1);

        let config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: SurfaceColorSpace::Auto,
            width,
            height,
            present_mode: PresentMode::AutoVsync,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        surface.configure(&device, &config);

        let (depth_texture, depth_view) = Self::create_depth_texture(&device, width, height);

        Ok(Self {
            instance,
            adapter,
            device,
            queue,
            surface,
            config,
            depth_texture,
            depth_view,
        })
    }

    /// Resize surface and recreate depth buffer.
    pub fn resize(&mut self, new_width: u32, new_height: u32) {
        if new_width > 0 && new_height > 0 {
            self.config.width = new_width;
            self.config.height = new_height;
            self.surface.configure(&self.device, &self.config);

            let (depth_texture, depth_view) =
                Self::create_depth_texture(&self.device, new_width, new_height);
            self.depth_texture = depth_texture;
            self.depth_view = depth_view;
        }
    }

    fn create_depth_texture(
        device: &Device,
        width: u32,
        height: u32,
    ) -> (Texture, TextureView) {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let desc = wgpu::TextureDescriptor {
            label: Some("Mascot Depth Texture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        };
        let texture = device.create_texture(&desc);
        let view = texture.create_view(&TextureViewDescriptor::default());
        (texture, view)
    }
}
