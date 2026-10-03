use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec4};
use wgpu::util::DeviceExt;
use wgpu::{
    BindGroup, BindGroupLayout, BindGroupLayoutEntry, BindingType, Buffer, BufferBindingType,
    BufferUsages, ColorTargetState, ColorWrites, CompareFunction, DepthBiasState,
    DepthStencilState, Device, Face, FragmentState, FrontFace, MultisampleState,
    PipelineCompilationOptions, PipelineLayout, PipelineLayoutDescriptor, PrimitiveState,
    PrimitiveTopology, Queue, RenderPipeline, RenderPipelineDescriptor, ShaderStages,
    StencilState, TextureFormat, VertexState,
};

use crate::context::DEPTH_FORMAT;
use crate::mesh::GpuMesh;

pub const PREMULTIPLIED_ALPHA_BLEND: wgpu::BlendState = wgpu::BlendState {
    color: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
        operation: wgpu::BlendOperation::Add,
    },
    alpha: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
        operation: wgpu::BlendOperation::Add,
    },
};

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct ModelUniform {
    pub model_matrix: [[f32; 4]; 4],
    pub color: [f32; 4],
}

impl Default for ModelUniform {
    fn default() -> Self {
        Self {
            model_matrix: Mat4::IDENTITY.to_cols_array_2d(),
            color: Vec4::new(1.0, 1.0, 1.0, 1.0).to_array(),
        }
    }
}

pub struct ModelBuffer {
    pub buffer: Buffer,
    pub bind_group: BindGroup,
}

impl ModelBuffer {
    pub fn new(device: &Device, layout: &BindGroupLayout, initial: &ModelUniform) -> Self {
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Model Buffer"),
            contents: bytemuck::cast_slice(&[*initial]),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Model Bind Group"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });

        Self { buffer, bind_group }
    }

    pub fn update(&self, queue: &Queue, uniform: &ModelUniform) {
        queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&[*uniform]));
    }
}

pub struct BasicPipeline {
    pub pipeline: RenderPipeline,
    pub layout: PipelineLayout,
    pub model_layout: BindGroupLayout,
}

impl BasicPipeline {
    pub fn new(
        device: &Device,
        surface_format: TextureFormat,
        camera_layout: &BindGroupLayout,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));

        let model_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Model Bind Group Layout"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Basic Pipeline Layout"),
            bind_group_layouts: &[Some(camera_layout), Some(&model_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("Basic 3D Render Pipeline"),
            layout: Some(&layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(GpuMesh::vertex_layout())],
                compilation_options: PipelineCompilationOptions::default(),
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(ColorTargetState {
                    format: surface_format,
                    blend: Some(PREMULTIPLIED_ALPHA_BLEND),
                    write_mask: ColorWrites::ALL,
                })],
                compilation_options: PipelineCompilationOptions::default(),
            }),
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: FrontFace::Ccw,
                cull_mode: Some(Face::Back),
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(CompareFunction::Less),
                stencil: StencilState::default(),
                bias: DepthBiasState::default(),
            }),
            multisample: MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        });

        Self {
            pipeline,
            layout,
            model_layout,
        }
    }
}
