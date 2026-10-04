use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec4};
use wgpu::util::DeviceExt;
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingType, Buffer, BufferBindingType, BufferUsages, ColorTargetState,
    ColorWrites, CompareFunction, DepthBiasState, DepthStencilState, Device, Face, FragmentState,
    FrontFace, MultisampleState, PipelineCompilationOptions, PipelineLayout,
    PipelineLayoutDescriptor, PrimitiveState, PrimitiveTopology, Queue, RenderPipeline,
    RenderPipelineDescriptor, SamplerBindingType, ShaderStages, StencilState, TextureFormat,
    TextureSampleType, TextureViewDimension, VertexState,
};

use crate::camera::CameraUniform;
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
pub struct LightUniform {
    pub direction: [f32; 3],
    pub _pad0: f32,
    pub color: [f32; 3],
    pub _pad1: f32,
    pub ambient_color: [f32; 3],
    pub _pad2: f32,
}

impl Default for LightUniform {
    fn default() -> Self {
        Self {
            direction: [0.5, 1.0, 0.8],
            _pad0: 0.0,
            color: [1.0, 1.0, 1.0],
            _pad1: 0.0,
            ambient_color: [0.35, 0.35, 0.4],
            _pad2: 0.0,
        }
    }
}

/// Holds camera and lighting buffers for Group 0 (scene globals).
pub struct SceneBuffer {
    pub camera_buffer: Buffer,
    pub light_buffer: Buffer,
    pub layout: BindGroupLayout,
    pub bind_group: BindGroup,
}

impl SceneBuffer {
    pub fn new(device: &Device, camera: &CameraUniform, light: &LightUniform) -> Self {
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Scene Camera Buffer"),
            contents: bytemuck::cast_slice(&[*camera]),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });

        let light_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Scene Light Buffer"),
            contents: bytemuck::cast_slice(&[*light]),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });

        let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("Scene Bind Group Layout (Group 0)"),
            entries: &[
                // binding 0: CameraUniform
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // binding 1: LightUniform
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("Scene Bind Group (Group 0)"),
            layout: &layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: light_buffer.as_entire_binding(),
                },
            ],
        });

        Self {
            camera_buffer,
            light_buffer,
            layout,
            bind_group,
        }
    }

    pub fn update_camera(&self, queue: &Queue, uniform: &CameraUniform) {
        queue.write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(&[*uniform]));
    }

    pub fn update_light(&self, queue: &Queue, uniform: &LightUniform) {
        queue.write_buffer(&self.light_buffer, 0, bytemuck::cast_slice(&[*uniform]));
    }
}

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

        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("Model Bind Group"),
            layout,
            entries: &[BindGroupEntry {
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

/// Render pipeline implementing lilToon 2-step toon shading, rimlight, emission,
/// and inverted hull outline passes.
pub struct LilToonPipeline {
    pub main_pipeline: RenderPipeline,
    pub outline_pipeline: RenderPipeline,
    pub layout: PipelineLayout,
    pub scene_layout: BindGroupLayout,
    pub material_layout: BindGroupLayout,
    pub model_skeleton_layout: BindGroupLayout,
}

impl LilToonPipeline {
    pub fn new(device: &Device, surface_format: TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("liltoon.wgsl"));

        // Group 0: Scene globals (Camera + Light)
        let scene_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("LilToon Scene Layout (Group 0)"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        // Group 1: Material (LilToonUniform + BaseTexture + Sampler)
        let material_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("LilToon Material Layout (Group 1)"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // Group 2: Model & Skeleton (ModelUniform + Bone Matrices Storage Buffer)
        let model_skeleton_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("LilToon Model & Skeleton Layout (Group 2)"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::VERTEX,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::VERTEX,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("LilToon Pipeline Layout"),
            bind_group_layouts: &[
                Some(&scene_layout),
                Some(&material_layout),
                Some(&model_skeleton_layout),
            ],
            immediate_size: 0,
        });

        // Pass 1: Main Toon shading (Backface culling, Less depth test)
        let main_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("LilToon Main Render Pipeline"),
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

        // Pass 2: Inverted Hull outline (Frontface culling => backfaces rendered, LessEqual depth test)
        let outline_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("LilToon Outline Render Pipeline"),
            layout: Some(&layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs_outline"),
                buffers: &[Some(GpuMesh::vertex_layout())],
                compilation_options: PipelineCompilationOptions::default(),
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("fs_outline"),
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
                cull_mode: Some(Face::Front), // Cull front => renders only backfaces pushed along normals!
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(CompareFunction::LessEqual),
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
            main_pipeline,
            outline_pipeline,
            layout,
            scene_layout,
            material_layout,
            model_skeleton_layout,
        }
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

        let model_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
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
