use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;
use wgpu::{BindGroup, BindGroupLayout, Buffer, BufferUsages, Device, Queue};

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct CameraUniform {
    pub view_proj: [[f32; 4]; 4],
    pub view_pos: [f32; 3],
    pub _padding: f32,
}

impl Default for CameraUniform {
    fn default() -> Self {
        Self {
            view_proj: Mat4::IDENTITY.to_cols_array_2d(),
            view_pos: [0.0; 3],
            _padding: 0.0,
        }
    }
}

pub struct Camera {
    pub eye: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub aspect: f32,
    pub fovy: f32,
    pub znear: f32,
    pub zfar: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            eye: Vec3::new(0.0, 1.0, 3.0),
            target: Vec3::new(0.0, 0.9, 0.0),
            up: Vec3::Y,
            aspect: 1.0,
            fovy: std::f32::consts::FRAC_PI_4, // 45 deg
            znear: 0.1,
            zfar: 100.0,
        }
    }
}

impl Camera {
    pub fn new(eye: Vec3, target: Vec3, aspect: f32) -> Self {
        Self {
            eye,
            target,
            aspect,
            ..Default::default()
        }
    }

    pub fn build_view_projection_matrix(&self) -> Mat4 {
        let view = Mat4::look_at_rh(self.eye, self.target, self.up);
        let proj = Mat4::perspective_rh(self.fovy, self.aspect, self.znear, self.zfar);
        proj * view
    }

    pub fn build_uniform(&self) -> CameraUniform {
        let view_proj = self.build_view_projection_matrix();
        CameraUniform {
            view_proj: view_proj.to_cols_array_2d(),
            view_pos: self.eye.to_array(),
            _padding: 0.0,
        }
    }
}

pub struct CameraBuffer {
    pub buffer: Buffer,
    pub bind_group_layout: BindGroupLayout,
    pub bind_group: BindGroup,
}

impl CameraBuffer {
    pub fn new(device: &Device, initial_uniform: &CameraUniform) -> Self {
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera Buffer"),
            contents: bytemuck::cast_slice(&[*initial_uniform]),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Camera Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Camera Bind Group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });

        Self {
            buffer,
            bind_group_layout,
            bind_group,
        }
    }

    pub fn update(&self, queue: &Queue, uniform: &CameraUniform) {
        queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&[*uniform]));
    }
}
