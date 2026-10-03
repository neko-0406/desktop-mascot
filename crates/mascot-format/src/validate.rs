use crate::error::ValidationError;
use crate::material::{DmaMaterial, DmaTexture};
use crate::mesh::DmaMesh;
use crate::morph::MorphTarget;
use crate::physics::{PhysBoneChain, PhysCollider};
use crate::skeleton::RawBone;

pub fn validate_avatar(
    bones: &[RawBone],
    mesh: Option<&DmaMesh>,
    morph_targets: &[MorphTarget],
    materials: &[DmaMaterial],
    textures: &[DmaTexture],
    phys_chains: &[PhysBoneChain],
    colliders: &[PhysCollider],
) -> Result<(), ValidationError> {
    let bone_count = bones.len();

    // 1. Skeleton validation
    for (idx, bone) in bones.iter().enumerate() {
        if bone.parent_index >= 0 {
            let parent = bone.parent_index as usize;
            if parent >= bone_count {
                return Err(ValidationError::InvalidBoneParent {
                    bone_index: idx,
                    bone_name: bone.name_str().to_string(),
                    parent_index: bone.parent_index,
                    bone_count,
                });
            }
        }
    }

    // 2. Mesh validation
    if let Some(m) = mesh {
        let vertex_count = m.vertices.len();
        let index_count = m.indices.len();

        for (v_idx, vert) in m.vertices.iter().enumerate() {
            for i in 0..4 {
                if vert.bone_weights[i] > 1e-4 {
                    let b_idx = vert.bone_indices[i];
                    if bone_count > 0 && (b_idx as usize) >= bone_count {
                        return Err(ValidationError::VertexBoneIndexOutOfBounds {
                            vertex_index: v_idx,
                            bone_index: b_idx,
                            bone_count,
                        });
                    }
                }
            }
        }

        for (s_idx, submesh) in m.submeshes.iter().enumerate() {
            let end = submesh.index_offset.saturating_add(submesh.index_count);
            if end as usize > index_count {
                return Err(ValidationError::SubmeshIndexOutOfBounds {
                    submesh_index: s_idx,
                    offset: submesh.index_offset,
                    end,
                    total_indices: index_count,
                });
            }

            if !materials.is_empty() && (submesh.material_index as usize) >= materials.len() {
                return Err(ValidationError::SubmeshMaterialOutOfBounds {
                    submesh_index: s_idx,
                    material_index: submesh.material_index,
                    material_count: materials.len(),
                });
            }
        }

        // 3. Morph targets validation
        for target in morph_targets {
            for delta in &target.deltas {
                if (delta.vertex_index as usize) >= vertex_count {
                    return Err(ValidationError::MorphVertexIndexOutOfBounds {
                        target_name: target.name.clone(),
                        vertex_index: delta.vertex_index,
                        vertex_count,
                    });
                }
            }
        }
    }

    // 4. Material texture reference validation
    let texture_count = textures.len();
    for (m_idx, mat) in materials.iter().enumerate() {
        for (tex_idx, _name) in [
            (mat.params.base_texture_idx, "base_texture_idx"),
            (mat.params.shade_texture_idx, "shade_texture_idx"),
            (mat.params.outline_texture_idx, "outline_texture_idx"),
            (mat.params.normal_texture_idx, "normal_texture_idx"),
        ] {
            if tex_idx >= 0 && (tex_idx as usize) >= texture_count {
                return Err(ValidationError::MaterialTextureOutOfBounds {
                    material_index: m_idx,
                    material_name: mat.name.clone(),
                    texture_index: tex_idx,
                    texture_count,
                });
            }
        }
    }

    // 5. PhysBone validation
    let collider_count = colliders.len();
    for (c_idx, chain) in phys_chains.iter().enumerate() {
        if bone_count > 0 && (chain.root_bone_index as usize) >= bone_count {
            return Err(ValidationError::PhysBoneRootOutOfBounds {
                chain_index: c_idx,
                root_bone_index: chain.root_bone_index,
                bone_count,
            });
        }

        let active_colliders = chain.collider_count.min(8) as usize;
        for i in 0..active_colliders {
            let col_idx = chain.collider_indices[i];
            if (col_idx as usize) >= collider_count {
                return Err(ValidationError::PhysBoneColliderOutOfBounds {
                    chain_index: c_idx,
                    collider_index: col_idx,
                    collider_count,
                });
            }
        }
    }

    for (col_idx, col) in colliders.iter().enumerate() {
        if bone_count > 0 && (col.root_bone_index as usize) >= bone_count {
            return Err(ValidationError::ColliderRootOutOfBounds {
                collider_index: col_idx,
                root_bone_index: col.root_bone_index,
                bone_count,
            });
        }
    }

    Ok(())
}
