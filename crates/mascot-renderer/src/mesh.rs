use mascot_format::{DmaMesh, DmaVertex, SubmeshInfo};
use wgpu::util::DeviceExt;
use wgpu::{Buffer, BufferAddress, BufferUsages, Device, IndexFormat, VertexBufferLayout};

pub struct GpuMesh {
    pub vertex_buffer: Buffer,
    pub index_buffer: Buffer,
    pub index_count: u32,
    pub submeshes: Vec<SubmeshInfo>,
}

impl GpuMesh {
    pub const VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 7] = wgpu::vertex_attr_array![
        0 => Float32x3, // position
        1 => Float32x3, // normal
        2 => Float32x4, // tangent
        3 => Float32x2, // uv0
        4 => Float32x2, // uv1
        5 => Uint16x4,  // bone_indices
        6 => Float32x4, // bone_weights
    ];

    pub fn vertex_layout() -> VertexBufferLayout<'static> {
        VertexBufferLayout {
            array_stride: std::mem::size_of::<DmaVertex>() as BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::VERTEX_ATTRIBUTES,
        }
    }

    pub fn from_dma(device: &Device, mesh: &DmaMesh) -> Self {
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Mesh Vertex Buffer"),
            contents: bytemuck::cast_slice(&mesh.vertices),
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Mesh Index Buffer"),
            contents: bytemuck::cast_slice(&mesh.indices),
            usage: BufferUsages::INDEX,
        });

        Self {
            vertex_buffer,
            index_buffer,
            index_count: mesh.indices.len() as u32,
            submeshes: mesh.submeshes.clone(),
        }
    }

    pub fn create_cube(device: &Device, size: f32) -> Self {
        let dma = create_cube_mesh(size);
        Self::from_dma(device, &dma)
    }

    pub const INDEX_FORMAT: IndexFormat = IndexFormat::Uint32;
}

/// Generates a standard DmaMesh cube with normal, tangent, and UV coordinates.
pub fn create_cube_mesh(size: f32) -> DmaMesh {
    let h = size * 0.5;

    // 6 faces: +Z, -Z, +X, -X, +Y, -Y
    let face_data = [
        // (+Z front)
        ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0], [
            [-h, -h,  h], [ h, -h,  h], [ h,  h,  h], [-h,  h,  h]
        ]),
        // (-Z back)
        ([0.0, 0.0, -1.0], [-1.0, 0.0, 0.0, 1.0], [
            [ h, -h, -h], [-h, -h, -h], [-h,  h, -h], [ h,  h, -h]
        ]),
        // (+X right)
        ([1.0, 0.0, 0.0], [0.0, 0.0, -1.0, 1.0], [
            [ h, -h,  h], [ h, -h, -h], [ h,  h, -h], [ h,  h,  h]
        ]),
        // (-X left)
        ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 1.0], [
            [-h, -h, -h], [-h, -h,  h], [-h,  h,  h], [-h,  h, -h]
        ]),
        // (+Y top)
        ([0.0, 1.0, 0.0], [1.0, 0.0, 0.0, 1.0], [
            [-h,  h,  h], [ h,  h,  h], [ h,  h, -h], [-h,  h, -h]
        ]),
        // (-Y bottom)
        ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0, 1.0], [
            [-h, -h, -h], [ h, -h, -h], [ h, -h,  h], [-h, -h,  h]
        ]),
    ];

    let uv_coords = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];

    let mut vertices = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);

    for (normal, tangent, positions) in &face_data {
        let base_idx = vertices.len() as u32;
        for i in 0..4 {
            vertices.push(DmaVertex {
                position: positions[i],
                normal: *normal,
                tangent: *tangent,
                uv0: uv_coords[i],
                uv1: [0.0, 0.0],
                bone_indices: [0, 0, 0, 0],
                bone_weights: [1.0, 0.0, 0.0, 0.0],
            });
        }
        indices.extend_from_slice(&[
            base_idx,
            base_idx + 1,
            base_idx + 2,
            base_idx,
            base_idx + 2,
            base_idx + 3,
        ]);
    }

    DmaMesh {
        submeshes: vec![SubmeshInfo::new(0, indices.len() as u32, 0)],
        vertices,
        indices,
    }
}
