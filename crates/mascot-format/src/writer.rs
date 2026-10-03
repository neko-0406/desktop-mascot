use std::io::{Seek, SeekFrom, Write};

use crate::error::DmaError;
use crate::header::{
    DmaHeader, TocEntry, CHUNK_MAT, CHUNK_MESH, CHUNK_META, CHUNK_MORPH, CHUNK_PHYS, CHUNK_SKEL,
    DMA_MAGIC, DMA_VERSION, FLAG_LITTLE_ENDIAN,
};
use crate::material::{DmaMaterial, DmaTexture, MaterialChunkHeader, RawMaterial, RawTextureHeader};
use crate::metadata::AvatarMetadata;
use crate::mesh::{DmaMesh, MeshChunkHeader};
use crate::morph::{MorphChunkHeader, MorphTarget, MorphTargetHeader};
use crate::physics::{PhysBoneChain, PhysChunkHeader, PhysCollider};
use crate::skeleton::{RawBone, SkeletonChunkHeader};

pub struct DmaWriter;

impl DmaWriter {
    /// Writes complete avatar data to a seekable writer.
    pub fn write<W: Write + Seek>(
        writer: &mut W,
        metadata: Option<&AvatarMetadata>,
        bones: &[RawBone],
        mesh: Option<&DmaMesh>,
        morph_targets: &[MorphTarget],
        materials: &[DmaMaterial],
        textures: &[DmaTexture],
        phys_chains: &[PhysBoneChain],
        colliders: &[PhysCollider],
    ) -> Result<u64, DmaError> {
        // Collect active chunks to determine chunk count
        let mut active_chunk_types: Vec<[u8; 4]> = Vec::new();

        if metadata.is_some() {
            active_chunk_types.push(CHUNK_META);
        }
        if !bones.is_empty() {
            active_chunk_types.push(CHUNK_SKEL);
        }
        if mesh.is_some() {
            active_chunk_types.push(CHUNK_MESH);
        }
        if !morph_targets.is_empty() {
            active_chunk_types.push(CHUNK_MORPH);
        }
        if !materials.is_empty() || !textures.is_empty() {
            active_chunk_types.push(CHUNK_MAT);
        }
        if !phys_chains.is_empty() || !colliders.is_empty() {
            active_chunk_types.push(CHUNK_PHYS);
        }

        let chunk_count = active_chunk_types.len() as u32;

        // 1. Reserve placeholder header and TOC
        let placeholder_header = DmaHeader {
            magic: DMA_MAGIC,
            version: DMA_VERSION,
            flags: FLAG_LITTLE_ENDIAN,
            chunk_count,
            total_file_size: 0,
            reserved: [0; 8],
        };

        writer.seek(SeekFrom::Start(0))?;
        writer.write_all(bytemuck::bytes_of(&placeholder_header))?;

        let placeholder_toc = vec![TocEntry::new([0; 4], 0, 0); chunk_count as usize];
        if chunk_count > 0 {
            writer.write_all(bytemuck::cast_slice(&placeholder_toc))?;
        }

        // Align writer to 16 bytes for first chunk payload
        let current_offset = writer.stream_position()?;
        let pad = (16 - (current_offset % 16)) % 16;
        if pad > 0 {
            writer.write_all(&vec![0u8; pad as usize])?;
        }

        let mut final_toc: Vec<TocEntry> = Vec::with_capacity(chunk_count as usize);

        // Helper macro/closure for writing a chunk
        for chunk_type in active_chunk_types {
            let chunk_start_offset = writer.stream_position()?;

            match chunk_type {
                CHUNK_META => {
                    if let Some(meta) = metadata {
                        let json_bytes = serde_json::to_vec_pretty(meta)?;
                        writer.write_all(&json_bytes)?;
                    }
                }
                CHUNK_SKEL => {
                    let skel_hdr = SkeletonChunkHeader {
                        bone_count: bones.len() as u32,
                        reserved: 0,
                    };
                    writer.write_all(bytemuck::bytes_of(&skel_hdr))?;
                    writer.write_all(bytemuck::cast_slice(bones))?;
                }
                CHUNK_MESH => {
                    if let Some(m) = mesh {
                        let mesh_hdr = MeshChunkHeader {
                            vertex_count: m.vertices.len() as u32,
                            index_count: m.indices.len() as u32,
                            submesh_count: m.submeshes.len() as u32,
                            reserved: 0,
                        };
                        writer.write_all(bytemuck::bytes_of(&mesh_hdr))?;
                        writer.write_all(bytemuck::cast_slice(&m.submeshes))?;
                        writer.write_all(bytemuck::cast_slice(&m.vertices))?;
                        writer.write_all(bytemuck::cast_slice(&m.indices))?;
                    }
                }
                CHUNK_MORPH => {
                    let morph_hdr = MorphChunkHeader {
                        morph_target_count: morph_targets.len() as u32,
                        reserved: 0,
                    };
                    writer.write_all(bytemuck::bytes_of(&morph_hdr))?;
                    for target in morph_targets {
                        let target_hdr =
                            MorphTargetHeader::new(&target.name, target.deltas.len() as u32);
                        writer.write_all(bytemuck::bytes_of(&target_hdr))?;
                        writer.write_all(bytemuck::cast_slice(&target.deltas))?;
                    }
                }
                CHUNK_MAT => {
                    let mat_hdr = MaterialChunkHeader {
                        material_count: materials.len() as u32,
                        texture_count: textures.len() as u32,
                    };
                    writer.write_all(bytemuck::bytes_of(&mat_hdr))?;
                    for mat in materials {
                        let raw_mat = RawMaterial::new(&mat.name, mat.params);
                        writer.write_all(bytemuck::bytes_of(&raw_mat))?;
                    }
                    for tex in textures {
                        let tex_hdr = RawTextureHeader::new(
                            &tex.name,
                            tex.format,
                            tex.width,
                            tex.height,
                            tex.data.len() as u32,
                        );
                        writer.write_all(bytemuck::bytes_of(&tex_hdr))?;
                        writer.write_all(&tex.data)?;
                        // Pad texture data to 4-byte alignment
                        let pad_len = (4 - (tex.data.len() % 4)) % 4;
                        if pad_len > 0 {
                            writer.write_all(&vec![0u8; pad_len])?;
                        }
                    }
                }
                CHUNK_PHYS => {
                    let phys_hdr = PhysChunkHeader {
                        chain_count: phys_chains.len() as u32,
                        collider_count: colliders.len() as u32,
                    };
                    writer.write_all(bytemuck::bytes_of(&phys_hdr))?;
                    writer.write_all(bytemuck::cast_slice(phys_chains))?;
                    writer.write_all(bytemuck::cast_slice(colliders))?;
                }
                _ => {}
            }

            let chunk_end_offset = writer.stream_position()?;
            let chunk_len = chunk_end_offset - chunk_start_offset;
            final_toc.push(TocEntry::new(chunk_type, chunk_start_offset, chunk_len));

            // Align next chunk to 16 bytes
            let pad = (16 - (chunk_end_offset % 16)) % 16;
            if pad > 0 {
                writer.write_all(&vec![0u8; pad as usize])?;
            }
        }

        let total_file_size = writer.stream_position()?;

        // 2. Finalize header and TOC
        let final_header = DmaHeader {
            magic: DMA_MAGIC,
            version: DMA_VERSION,
            flags: FLAG_LITTLE_ENDIAN,
            chunk_count,
            total_file_size,
            reserved: [0; 8],
        };

        writer.seek(SeekFrom::Start(0))?;
        writer.write_all(bytemuck::bytes_of(&final_header))?;
        if chunk_count > 0 {
            writer.write_all(bytemuck::cast_slice(&final_toc))?;
        }

        writer.seek(SeekFrom::Start(total_file_size))?;
        Ok(total_file_size)
    }
}
