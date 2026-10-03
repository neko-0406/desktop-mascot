pub mod error;
pub mod header;
pub mod material;
pub mod mesh;
pub mod metadata;
pub mod morph;
pub mod physics;
pub mod reader;
pub mod skeleton;
pub mod validate;
pub mod writer;

// Re-export common types
pub use error::{DmaError, ValidationError};
pub use header::{
    DmaHeader, TocEntry, CHUNK_ANIM, CHUNK_MAT, CHUNK_MESH, CHUNK_META, CHUNK_MORPH, CHUNK_PHYS,
    CHUNK_SKEL, DMA_MAGIC, DMA_VERSION, FLAG_COMPRESSED, FLAG_LITTLE_ENDIAN,
};
pub use material::{
    DmaMaterial, DmaTexture, LilToonMaterialParams, RawMaterial, RawTextureHeader,
    TEXTURE_FORMAT_DDS, TEXTURE_FORMAT_KTX2, TEXTURE_FORMAT_PNG, TEXTURE_FORMAT_RAW_RGBA,
};
pub use mesh::{DmaMesh, DmaVertex, MeshChunkHeader, SubmeshInfo};
pub use metadata::AvatarMetadata;
pub use morph::{DeltaVertex, MorphChunkHeader, MorphTarget, MorphTargetHeader};
pub use physics::{ColliderShape, PhysBoneChain, PhysChunkHeader, PhysCollider};
pub use reader::DmaFile;
pub use skeleton::{RawBone, SkeletonChunkHeader};
pub use validate::validate_avatar;
pub use writer::DmaWriter;
