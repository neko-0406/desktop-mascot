use std::collections::HashSet;
use std::io::{Read, Seek, SeekFrom};

use crate::error::DmaError;
use crate::header::{
    DmaHeader, TocEntry, CHUNK_MAT, CHUNK_MESH, CHUNK_META, CHUNK_MORPH, CHUNK_PHYS, CHUNK_SKEL,
    DMA_MAGIC, DMA_VERSION,
};
use crate::material::{
    DmaMaterial, DmaTexture, MaterialChunkHeader, RawMaterial, RawTextureHeader,
};
use crate::metadata::AvatarMetadata;
use crate::mesh::{DmaMesh, DmaVertex, MeshChunkHeader, SubmeshInfo};
use crate::morph::{DeltaVertex, MorphChunkHeader, MorphTarget, MorphTargetHeader};
use crate::physics::{PhysBoneChain, PhysChunkHeader, PhysCollider};
use crate::skeleton::{RawBone, SkeletonChunkHeader};
use crate::validate::validate_avatar;

#[derive(Debug, Clone, PartialEq)]
pub struct DmaFile {
    pub header: DmaHeader,
    pub toc: Vec<TocEntry>,
    pub metadata: Option<AvatarMetadata>,
    pub skeleton: Vec<RawBone>,
    pub mesh: Option<DmaMesh>,
    pub morph_targets: Vec<MorphTarget>,
    pub materials: Vec<DmaMaterial>,
    pub textures: Vec<DmaTexture>,
    pub phys_chains: Vec<PhysBoneChain>,
    pub colliders: Vec<PhysCollider>,
}

impl DmaFile {
    /// Reads and parses a `.dma` file from an arbitrary seekable stream.
    pub fn from_reader<R: Read + Seek>(reader: &mut R) -> Result<Self, DmaError> {
        let mut bytes = Vec::new();
        reader.seek(SeekFrom::Start(0))?;
        reader.read_to_end(&mut bytes)?;
        Self::from_bytes(&bytes)
    }

    /// Parses a `.dma` file directly from an in-memory buffer (e.g. mmap).
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DmaError> {
        let total_size = bytes.len() as u64;
        if total_size < 32 {
            return Err(DmaError::MalformedChunk(
                "HEADER".into(),
                "File too small for 32-byte header".into(),
            ));
        }

        // 1. Read Header
        let header: DmaHeader = *bytemuck::from_bytes(&bytes[0..32]);
        if header.magic != DMA_MAGIC {
            return Err(DmaError::InvalidMagic(header.magic));
        }
        if header.version != DMA_VERSION {
            return Err(DmaError::UnsupportedVersion(header.version));
        }

        // 2. Read TOC
        let toc_start = 32usize;
        let toc_len = (header.chunk_count as usize) * std::mem::size_of::<TocEntry>();
        let toc_end = toc_start + toc_len;

        if (total_size as usize) < toc_end {
            return Err(DmaError::MalformedChunk(
                "TOC".into(),
                "File too small for chunk table of contents".into(),
            ));
        }

        let toc_slice: &[TocEntry] = bytemuck::cast_slice(&bytes[toc_start..toc_end]);
        let toc = toc_slice.to_vec();

        let mut seen_types = HashSet::new();

        let mut metadata = None;
        let mut skeleton = Vec::new();
        let mut mesh = None;
        let mut morph_targets = Vec::new();
        let mut materials = Vec::new();
        let mut textures = Vec::new();
        let mut phys_chains = Vec::new();
        let mut colliders = Vec::new();

        // 3. Process Chunks
        for entry in &toc {
            let type_str = entry.type_name();
            if !seen_types.insert(entry.chunk_type) {
                return Err(DmaError::DuplicateChunk(type_str));
            }

            let start = entry.offset as usize;
            let len = entry.length as usize;
            let end = start.checked_add(len).ok_or_else(|| {
                DmaError::InvalidChunkBounds(type_str.clone(), entry.offset, entry.length, total_size)
            })?;

            if end > bytes.len() {
                return Err(DmaError::InvalidChunkBounds(
                    type_str.clone(),
                    entry.offset,
                    entry.length,
                    total_size,
                ));
            }

            let chunk_data = &bytes[start..end];

            match entry.chunk_type {
                CHUNK_META => {
                    let parsed: AvatarMetadata = serde_json::from_slice(chunk_data)?;
                    metadata = Some(parsed);
                }
                CHUNK_SKEL => {
                    let hdr_size = std::mem::size_of::<SkeletonChunkHeader>();
                    if chunk_data.len() < hdr_size {
                        return Err(DmaError::MalformedChunk(
                            "SKEL".into(),
                            "Chunk too small for SkeletonChunkHeader".into(),
                        ));
                    }
                    let skel_hdr: SkeletonChunkHeader = *bytemuck::from_bytes(&chunk_data[..hdr_size]);
                    let expected_bones_size =
                        (skel_hdr.bone_count as usize) * std::mem::size_of::<RawBone>();
                    if chunk_data.len() < hdr_size + expected_bones_size {
                        return Err(DmaError::MalformedChunk(
                            "SKEL".into(),
                            format!(
                                "Skeleton data truncated: expected {} bytes, got {}",
                                hdr_size + expected_bones_size,
                                chunk_data.len()
                            ),
                        ));
                    }
                    let bone_slice: &[RawBone] = bytemuck::cast_slice(
                        &chunk_data[hdr_size..hdr_size + expected_bones_size],
                    );
                    skeleton = bone_slice.to_vec();
                }
                CHUNK_MESH => {
                    let hdr_size = std::mem::size_of::<MeshChunkHeader>();
                    if chunk_data.len() < hdr_size {
                        return Err(DmaError::MalformedChunk(
                            "MESH".into(),
                            "Chunk too small for MeshChunkHeader".into(),
                        ));
                    }
                    let mesh_hdr: MeshChunkHeader = *bytemuck::from_bytes(&chunk_data[..hdr_size]);
                    let submesh_bytes =
                        (mesh_hdr.submesh_count as usize) * std::mem::size_of::<SubmeshInfo>();
                    let vertex_bytes =
                        (mesh_hdr.vertex_count as usize) * std::mem::size_of::<DmaVertex>();
                    let index_bytes = (mesh_hdr.index_count as usize) * std::mem::size_of::<u32>();

                    let expected_total = hdr_size + submesh_bytes + vertex_bytes + index_bytes;
                    if chunk_data.len() < expected_total {
                        return Err(DmaError::MalformedChunk(
                            "MESH".into(),
                            format!(
                                "Mesh data truncated: expected {} bytes, got {}",
                                expected_total,
                                chunk_data.len()
                            ),
                        ));
                    }

                    let mut cursor = hdr_size;
                    let submeshes: &[SubmeshInfo] =
                        bytemuck::cast_slice(&chunk_data[cursor..cursor + submesh_bytes]);
                    cursor += submesh_bytes;

                    let vertices: &[DmaVertex] =
                        bytemuck::cast_slice(&chunk_data[cursor..cursor + vertex_bytes]);
                    cursor += vertex_bytes;

                    let indices: &[u32] =
                        bytemuck::cast_slice(&chunk_data[cursor..cursor + index_bytes]);

                    mesh = Some(DmaMesh {
                        submeshes: submeshes.to_vec(),
                        vertices: vertices.to_vec(),
                        indices: indices.to_vec(),
                    });
                }
                CHUNK_MORPH => {
                    let hdr_size = std::mem::size_of::<MorphChunkHeader>();
                    if chunk_data.len() < hdr_size {
                        return Err(DmaError::MalformedChunk(
                            "MORP".into(),
                            "Chunk too small for MorphChunkHeader".into(),
                        ));
                    }
                    let morph_hdr: MorphChunkHeader = *bytemuck::from_bytes(&chunk_data[..hdr_size]);
                    let mut cursor = hdr_size;

                    for _ in 0..morph_hdr.morph_target_count {
                        let target_hdr_size = std::mem::size_of::<MorphTargetHeader>();
                        if cursor + target_hdr_size > chunk_data.len() {
                            return Err(DmaError::MalformedChunk(
                                "MORP".into(),
                                "Truncated MorphTargetHeader".into(),
                            ));
                        }
                        let target_hdr: MorphTargetHeader =
                            *bytemuck::from_bytes(&chunk_data[cursor..cursor + target_hdr_size]);
                        cursor += target_hdr_size;

                        let deltas_size =
                            (target_hdr.delta_count as usize) * std::mem::size_of::<DeltaVertex>();
                        if cursor + deltas_size > chunk_data.len() {
                            return Err(DmaError::MalformedChunk(
                                "MORP".into(),
                                "Truncated delta vertices".into(),
                            ));
                        }
                        let deltas_slice: &[DeltaVertex] =
                            bytemuck::cast_slice(&chunk_data[cursor..cursor + deltas_size]);
                        cursor += deltas_size;

                        morph_targets.push(MorphTarget {
                            name: target_hdr.name_str().to_string(),
                            deltas: deltas_slice.to_vec(),
                        });
                    }
                }
                CHUNK_MAT => {
                    let hdr_size = std::mem::size_of::<MaterialChunkHeader>();
                    if chunk_data.len() < hdr_size {
                        return Err(DmaError::MalformedChunk(
                            "MAT ".into(),
                            "Chunk too small for MaterialChunkHeader".into(),
                        ));
                    }
                    let mat_hdr: MaterialChunkHeader = *bytemuck::from_bytes(&chunk_data[..hdr_size]);
                    let mut cursor = hdr_size;

                    let raw_mat_size = std::mem::size_of::<RawMaterial>();
                    for _ in 0..mat_hdr.material_count {
                        if cursor + raw_mat_size > chunk_data.len() {
                            return Err(DmaError::MalformedChunk(
                                "MAT ".into(),
                                "Truncated RawMaterial".into(),
                            ));
                        }
                        let raw_mat: RawMaterial =
                            *bytemuck::from_bytes(&chunk_data[cursor..cursor + raw_mat_size]);
                        cursor += raw_mat_size;
                        materials.push(DmaMaterial {
                            name: raw_mat.name_str().to_string(),
                            params: raw_mat.params,
                        });
                    }

                    for _ in 0..mat_hdr.texture_count {
                        let tex_hdr_size = std::mem::size_of::<RawTextureHeader>();
                        if cursor + tex_hdr_size > chunk_data.len() {
                            return Err(DmaError::MalformedChunk(
                                "MAT ".into(),
                                "Truncated RawTextureHeader".into(),
                            ));
                        }
                        let tex_hdr: RawTextureHeader =
                            *bytemuck::from_bytes(&chunk_data[cursor..cursor + tex_hdr_size]);
                        cursor += tex_hdr_size;

                        let data_len = tex_hdr.data_length as usize;
                        if cursor + data_len > chunk_data.len() {
                            return Err(DmaError::MalformedChunk(
                                "MAT ".into(),
                                "Truncated texture pixel bytes".into(),
                            ));
                        }
                        let tex_data = chunk_data[cursor..cursor + data_len].to_vec();
                        cursor += data_len;

                        // Align to 4 bytes
                        let pad = (4 - (data_len % 4)) % 4;
                        cursor += pad;

                        textures.push(DmaTexture {
                            name: tex_hdr.name_str().to_string(),
                            format: tex_hdr.format,
                            width: tex_hdr.width,
                            height: tex_hdr.height,
                            data: tex_data,
                        });
                    }
                }
                CHUNK_PHYS => {
                    let hdr_size = std::mem::size_of::<PhysChunkHeader>();
                    if chunk_data.len() < hdr_size {
                        return Err(DmaError::MalformedChunk(
                            "PHYS".into(),
                            "Chunk too small for PhysChunkHeader".into(),
                        ));
                    }
                    let phys_hdr: PhysChunkHeader = *bytemuck::from_bytes(&chunk_data[..hdr_size]);
                    let chains_bytes =
                        (phys_hdr.chain_count as usize) * std::mem::size_of::<PhysBoneChain>();
                    let colliders_bytes =
                        (phys_hdr.collider_count as usize) * std::mem::size_of::<PhysCollider>();

                    if chunk_data.len() < hdr_size + chains_bytes + colliders_bytes {
                        return Err(DmaError::MalformedChunk(
                            "PHYS".into(),
                            "Truncated PhysBone chains or colliders".into(),
                        ));
                    }

                    let mut cursor = hdr_size;
                    let chain_slice: &[PhysBoneChain] =
                        bytemuck::cast_slice(&chunk_data[cursor..cursor + chains_bytes]);
                    cursor += chains_bytes;

                    let collider_slice: &[PhysCollider] =
                        bytemuck::cast_slice(&chunk_data[cursor..cursor + colliders_bytes]);

                    phys_chains = chain_slice.to_vec();
                    colliders = collider_slice.to_vec();
                }
                _ => {
                    // Unknown or unhandled chunk types are preserved in TOC but ignored
                }
            }
        }

        Ok(Self {
            header,
            toc,
            metadata,
            skeleton,
            mesh,
            morph_targets,
            materials,
            textures,
            phys_chains,
            colliders,
        })
    }

    /// Validates cross-references and bounds within the parsed avatar.
    pub fn validate(&self) -> Result<(), DmaError> {
        validate_avatar(
            &self.skeleton,
            self.mesh.as_ref(),
            &self.morph_targets,
            &self.materials,
            &self.textures,
            &self.phys_chains,
            &self.colliders,
        )?;
        Ok(())
    }

    /// Generates human-readable summary diagnostics.
    pub fn dump_summary(&self) -> String {
        use std::fmt::Write;
        let mut out = String::new();

        let _ = writeln!(out, "=== DMA Avatar Summary ===");
        let _ = writeln!(
            out,
            "Magic: {:?} | Version: {} | Chunks: {} | Total File Size: {} bytes",
            std::str::from_utf8(&self.header.magic).unwrap_or("???"),
            self.header.version,
            self.header.chunk_count,
            self.header.total_file_size
        );

        let _ = writeln!(out, "\n[Table of Contents]");
        for (i, entry) in self.toc.iter().enumerate() {
            let _ = writeln!(
                out,
                "  #{}: [{}] Offset: 0x{:08X} ({} bytes)",
                i,
                entry.type_name(),
                entry.offset,
                entry.length
            );
        }

        if let Some(ref meta) = self.metadata {
            let _ = writeln!(out, "\n[Metadata (CHUNK_META)]");
            let _ = writeln!(out, "  Avatar Name: {}", meta.avatar_name);
            let _ = writeln!(out, "  Author:      {}", meta.author);
            let _ = writeln!(
                out,
                "  View Pos:    [{:.3}, {:.3}, {:.3}]",
                meta.view_position[0], meta.view_position[1], meta.view_position[2]
            );
            let _ = writeln!(out, "  Scale:       {:.3}", meta.scale);
            let _ = writeln!(out, "  Visemes:     {} mappings", meta.viseme_blendshapes.len());
            let _ = writeln!(out, "  Blinks:      {} mappings", meta.blink_blendshapes.len());
        }

        if !self.skeleton.is_empty() {
            let _ = writeln!(out, "\n[Skeleton (CHUNK_SKEL)]");
            let _ = writeln!(out, "  Bone Count:  {}", self.skeleton.len());
            for (idx, bone) in self.skeleton.iter().take(5).enumerate() {
                let _ = writeln!(
                    out,
                    "    [{:2}] '{}' (parent: {})",
                    idx,
                    bone.name_str(),
                    bone.parent_index
                );
            }
            if self.skeleton.len() > 5 {
                let _ = writeln!(out, "    ... and {} more bones", self.skeleton.len() - 5);
            }
        }

        if let Some(ref m) = self.mesh {
            let _ = writeln!(out, "\n[Mesh (CHUNK_MESH)]");
            let _ = writeln!(out, "  Vertices:    {}", m.vertices.len());
            let _ = writeln!(out, "  Indices:     {} (Triangles: {})", m.indices.len(), m.indices.len() / 3);
            let _ = writeln!(out, "  Submeshes:   {}", m.submeshes.len());
            for (s_idx, sub) in m.submeshes.iter().enumerate() {
                let _ = writeln!(
                    out,
                    "    Submesh #{}: material_idx={}, offset={}, count={}",
                    s_idx, sub.material_index, sub.index_offset, sub.index_count
                );
            }
        }

        if !self.morph_targets.is_empty() {
            let _ = writeln!(out, "\n[Morph Targets (CHUNK_MORPH)]");
            let _ = writeln!(out, "  Target Count: {}", self.morph_targets.len());
            for (idx, target) in self.morph_targets.iter().take(5).enumerate() {
                let _ = writeln!(
                    out,
                    "    [{:2}] '{}' ({} delta vertices)",
                    idx,
                    target.name,
                    target.deltas.len()
                );
            }
            if self.morph_targets.len() > 5 {
                let _ = writeln!(
                    out,
                    "    ... and {} more morph targets",
                    self.morph_targets.len() - 5
                );
            }
        }

        if !self.materials.is_empty() || !self.textures.is_empty() {
            let _ = writeln!(out, "\n[Materials & Textures (CHUNK_MAT)]");
            let _ = writeln!(
                out,
                "  Materials: {}, Textures: {}",
                self.materials.len(),
                self.textures.len()
            );
            for (idx, mat) in self.materials.iter().enumerate() {
                let _ = writeln!(
                    out,
                    "    Material #{}: '{}' (Outline: {}, BaseTex: {})",
                    idx,
                    mat.name,
                    if mat.params.outline_enable != 0 { "yes" } else { "no" },
                    mat.params.base_texture_idx
                );
            }
            for (idx, tex) in self.textures.iter().enumerate() {
                let _ = writeln!(
                    out,
                    "    Texture #{}: '{}' (Format: {}, {}x{}, {} bytes)",
                    idx,
                    tex.name,
                    tex.format,
                    tex.width,
                    tex.height,
                    tex.data.len()
                );
            }
        }

        if !self.phys_chains.is_empty() || !self.colliders.is_empty() {
            let _ = writeln!(out, "\n[Physics (CHUNK_PHYS)]");
            let _ = writeln!(
                out,
                "  PhysBone Chains: {}, Colliders: {}",
                self.phys_chains.len(),
                self.colliders.len()
            );
            for (idx, chain) in self.phys_chains.iter().take(5).enumerate() {
                let _ = writeln!(
                    out,
                    "    Chain #{}: root_bone={}, pull={:.2}, spring={:.2}, colliders={}",
                    idx, chain.root_bone_index, chain.pull, chain.spring, chain.collider_count
                );
            }
            for (idx, col) in self.colliders.iter().take(5).enumerate() {
                let shape_name = match col.shape {
                    0 => "Sphere",
                    1 => "Capsule",
                    2 => "Plane",
                    _ => "Unknown",
                };
                let _ = writeln!(
                    out,
                    "    Collider #{}: shape={}, root_bone={}, radius={:.3}",
                    idx, shape_name, col.root_bone_index, col.radius
                );
            }
        }

        out
    }
}
