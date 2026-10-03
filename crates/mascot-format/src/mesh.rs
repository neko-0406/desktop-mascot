use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Pod, Zeroable)]
pub struct MeshChunkHeader {
    pub vertex_count: u32,
    pub index_count: u32,
    pub submesh_count: u32,
    pub reserved: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Pod, Zeroable)]
pub struct SubmeshInfo {
    pub index_offset: u32,
    pub index_count: u32,
    pub material_index: u32,
    pub reserved: u32,
}

impl SubmeshInfo {
    pub fn new(index_offset: u32, index_count: u32, material_index: u32) -> Self {
        Self {
            index_offset,
            index_count,
            material_index,
            reserved: 0,
        }
    }
}

/// Interleaved vertex format (80 bytes), ready for zero-copy GPU vertex buffers.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct DmaVertex {
    pub position: [f32; 3],     // Local position (x, y, z)
    pub normal: [f32; 3],       // Surface normal (x, y, z)
    pub tangent: [f32; 4],      // Tangent vector (x, y, z) and handedness w (+1.0 or -1.0)
    pub uv0: [f32; 2],          // Primary texture coordinates (u, v)
    pub uv1: [f32; 2],          // Secondary texture coordinates / lightmap (u, v)
    pub bone_indices: [u16; 4], // 4 bone indices for skinning
    pub bone_weights: [f32; 4], // 4 bone weights for skinning (sum ~= 1.0)
}

impl Default for DmaVertex {
    fn default() -> Self {
        Self {
            position: [0.0; 3],
            normal: [0.0, 1.0, 0.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            uv0: [0.0; 2],
            uv1: [0.0; 2],
            bone_indices: [0; 4],
            bone_weights: [1.0, 0.0, 0.0, 0.0],
        }
    }
}

/// Mesh structure combining submeshes, vertex buffer, and index buffer.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DmaMesh {
    pub submeshes: Vec<SubmeshInfo>,
    pub vertices: Vec<DmaVertex>,
    pub indices: Vec<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vertex_layout() {
        assert_eq!(std::mem::size_of::<DmaVertex>(), 80);
        assert_eq!(std::mem::size_of::<SubmeshInfo>(), 16);
        assert_eq!(std::mem::size_of::<MeshChunkHeader>(), 16);
    }
}
