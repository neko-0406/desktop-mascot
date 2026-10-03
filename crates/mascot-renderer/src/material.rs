use std::sync::Arc;
use bytemuck::{Pod, Zeroable};
use mascot_format::LilToonMaterialParams;
use wgpu::util::DeviceExt;
use wgpu::{
    AddressMode, BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindingResource,
    Buffer, BufferUsages, Device, Extent3d, FilterMode, MipmapFilterMode, Origin3d, Queue, Sampler,
    SamplerDescriptor, TexelCopyBufferLayout, TexelCopyTextureInfo, Texture, TextureAspect,
    TextureDescriptor, TextureDimension, TextureFormat, TextureUsages, TextureView,
    TextureViewDescriptor,
};

/// Uniform struct layout matching `LilToonUniform` in `liltoon.wgsl`.
/// Aligned to 16 bytes for standard uniform buffers. Total size: 144 bytes.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct LilToonUniform {
    pub base_color: [f32; 4],
    pub shade_color: [f32; 4],
    pub shade2_color: [f32; 4],
    pub shade_border: f32,
    pub shade_blur: f32,
    pub shade2_border: f32,
    pub shade2_blur: f32,
    pub rim_color: [f32; 4],
    pub rim_border: f32,
    pub rim_blur: f32,
    pub rim_fresnel_power: f32,
    pub _pad0: f32,
    pub emission_color: [f32; 4],
    pub outline_color: [f32; 4],
    pub outline_width: f32,
    pub outline_enable: u32,
    pub _pad1: [f32; 2],
}

impl Default for LilToonUniform {
    fn default() -> Self {
        Self {
            base_color: [1.0, 1.0, 1.0, 1.0],
            shade_color: [0.85, 0.85, 0.9, 1.0],
            shade2_color: [0.7, 0.7, 0.75, 1.0],
            shade_border: 0.5,
            shade_blur: 0.1,
            shade2_border: 0.3,
            shade2_blur: 0.1,
            rim_color: [1.0, 1.0, 1.0, 0.0],
            rim_border: 0.5,
            rim_blur: 0.2,
            rim_fresnel_power: 3.0,
            _pad0: 0.0,
            emission_color: [0.0, 0.0, 0.0, 0.0],
            outline_color: [0.0, 0.0, 0.0, 1.0],
            outline_width: 1.0,
            outline_enable: 1,
            _pad1: [0.0, 0.0],
        }
    }
}

impl From<&LilToonMaterialParams> for LilToonUniform {
    fn from(p: &LilToonMaterialParams) -> Self {
        Self {
            base_color: p.base_color,
            shade_color: p.shade_color,
            shade2_color: p.shade2_color,
            shade_border: p.shade_border,
            shade_blur: p.shade_blur.max(0.001),
            shade2_border: p.shade2_border,
            shade2_blur: p.shade2_blur.max(0.001),
            rim_color: p.rim_color,
            rim_border: p.rim_border,
            rim_blur: p.rim_blur.max(0.001),
            rim_fresnel_power: 3.0,
            _pad0: 0.0,
            emission_color: p.emission_color,
            outline_color: p.outline_color,
            outline_width: p.outline_width,
            outline_enable: p.outline_enable,
            _pad1: [0.0, 0.0],
        }
    }
}

/// A GPU texture with its view and sampler.
pub struct GpuTexture {
    pub texture: Texture,
    pub view: TextureView,
    pub sampler: Sampler,
    pub width: u32,
    pub height: u32,
}

impl GpuTexture {
    /// Creates a GPU texture from raw RGBA8 unorm pixel data.
    pub fn from_rgba(
        device: &Device,
        queue: &Queue,
        width: u32,
        height: u32,
        data: &[u8],
        label: Option<&str>,
    ) -> Self {
        let size = Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        let texture = device.create_texture(&TextureDescriptor {
            label,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8UnormSrgb,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            data,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: Some(height),
            },
            size,
        );

        let view = texture.create_view(&TextureViewDescriptor::default());
        let sampler = device.create_sampler(&SamplerDescriptor {
            label: label.map(|l| format!("{l} Sampler")).as_deref(),
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            address_mode_w: AddressMode::ClampToEdge,
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            mipmap_filter: MipmapFilterMode::Nearest,
            ..Default::default()
        });

        Self {
            texture,
            view,
            sampler,
            width,
            height,
        }
    }

    /// Creates a 1x1 solid-color texture (e.g. [255, 255, 255, 255] for white diffuse fallback).
    pub fn create_solid_color(
        device: &Device,
        queue: &Queue,
        color: [u8; 4],
        label: Option<&str>,
    ) -> Arc<Self> {
        Arc::new(Self::from_rgba(device, queue, 1, 1, &color, label))
    }
}

/// A material ready for rendering with lilToon shader pipelines.
pub struct GpuMaterial {
    pub name: String,
    pub uniform: LilToonUniform,
    pub uniform_buffer: Buffer,
    pub texture: Arc<GpuTexture>,
    pub bind_group: BindGroup,
}

impl GpuMaterial {
    pub fn new(
        device: &Device,
        layout: &BindGroupLayout,
        name: String,
        uniform: LilToonUniform,
        texture: Arc<GpuTexture>,
    ) -> Self {
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("Material Uniform: {name}")),
            contents: bytemuck::cast_slice(&[uniform]),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });

        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some(&format!("Material BindGroup: {name}")),
            layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::TextureView(&texture.view),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: BindingResource::Sampler(&texture.sampler),
                },
            ],
        });

        Self {
            name,
            uniform,
            uniform_buffer,
            texture,
            bind_group,
        }
    }

    pub fn update(&mut self, queue: &Queue, uniform: LilToonUniform) {
        self.uniform = uniform;
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniform]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_liltoon_uniform_size_alignment() {
        assert_eq!(std::mem::size_of::<LilToonUniform>(), 144);
        assert_eq!(std::mem::size_of::<LilToonUniform>() % 16, 0);
    }
}
