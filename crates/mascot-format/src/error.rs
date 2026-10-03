use thiserror::Error;

#[derive(Error, Debug)]
pub enum DmaError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid magic number: expected DMA1, got {0:?}")]
    InvalidMagic([u8; 4]),

    #[error("Unsupported version: {0} (supported: 1)")]
    UnsupportedVersion(u32),

    #[error("Invalid chunk offset or length: chunk '{0}', offset {1}, length {2}, total size {3}")]
    InvalidChunkBounds(String, u64, u64, u64),

    #[error("Duplicate chunk type: '{0}'")]
    DuplicateChunk(String),

    #[error("Failed to parse JSON metadata: {0}")]
    MetadataJson(#[from] serde_json::Error),

    #[error("Malformed chunk data: chunk '{0}', reason: {1}")]
    MalformedChunk(String, String),

    #[error("Validation error: {0}")]
    Validation(#[from] ValidationError),
}

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    #[error("Bone {bone_index} ('{bone_name}') has invalid parent index {parent_index} (bone count: {bone_count})")]
    InvalidBoneParent {
        bone_index: usize,
        bone_name: String,
        parent_index: i32,
        bone_count: usize,
    },

    #[error("Vertex {vertex_index} references bone index {bone_index}, which exceeds bone count {bone_count}")]
    VertexBoneIndexOutOfBounds {
        vertex_index: usize,
        bone_index: u16,
        bone_count: usize,
    },

    #[error("Morph target '{target_name}' references vertex index {vertex_index}, which exceeds vertex count {vertex_count}")]
    MorphVertexIndexOutOfBounds {
        target_name: String,
        vertex_index: u32,
        vertex_count: usize,
    },

    #[error("Submesh {submesh_index} index range [{offset}..{end}] exceeds total index buffer length {total_indices}")]
    SubmeshIndexOutOfBounds {
        submesh_index: usize,
        offset: u32,
        end: u32,
        total_indices: usize,
    },

    #[error("Submesh {submesh_index} references material index {material_index}, which exceeds material count {material_count}")]
    SubmeshMaterialOutOfBounds {
        submesh_index: usize,
        material_index: u32,
        material_count: usize,
    },

    #[error("Material {material_index} ('{material_name}') references texture index {texture_index}, which exceeds texture count {texture_count}")]
    MaterialTextureOutOfBounds {
        material_index: usize,
        material_name: String,
        texture_index: i32,
        texture_count: usize,
    },

    #[error("PhysBone chain {chain_index} references root bone {root_bone_index}, which exceeds bone count {bone_count}")]
    PhysBoneRootOutOfBounds {
        chain_index: usize,
        root_bone_index: u32,
        bone_count: usize,
    },

    #[error("PhysBone chain {chain_index} references collider index {collider_index}, which exceeds collider count {collider_count}")]
    PhysBoneColliderOutOfBounds {
        chain_index: usize,
        collider_index: u32,
        collider_count: usize,
    },

    #[error("Collider {collider_index} references root bone {root_bone_index}, which exceeds bone count {bone_count}")]
    ColliderRootOutOfBounds {
        collider_index: usize,
        root_bone_index: u32,
        bone_count: usize,
    },
}
