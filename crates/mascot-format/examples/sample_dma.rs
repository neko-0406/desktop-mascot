use std::collections::HashMap;
use std::fs::File;

use mascot_format::{
    AvatarMetadata, DmaMaterial, DmaMesh, DmaTexture, DmaVertex, DmaWriter,
    LilToonMaterialParams, MorphTarget, PhysBoneChain, PhysCollider, RawBone, SubmeshInfo,
    TEXTURE_FORMAT_PNG,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut visemes = HashMap::new();
    visemes.insert("vrc.v_aa".to_string(), 1);
    visemes.insert("vrc.v_ih".to_string(), 2);
    visemes.insert("vrc.v_ou".to_string(), 3);

    let mut blinks = HashMap::new();
    blinks.insert("vrc.blink".to_string(), 0);

    let metadata = AvatarMetadata {
        avatar_name: "Kikyo_Sample".to_string(),
        author: "DesktopMascotTeam".to_string(),
        view_position: [0.0, 1.35, 0.08],
        scale: 1.0,
        viseme_blendshapes: visemes,
        blink_blendshapes: blinks,
        extra: HashMap::new(),
    };

    let mut identity = [0.0f32; 16];
    identity[0] = 1.0;
    identity[5] = 1.0;
    identity[10] = 1.0;
    identity[15] = 1.0;

    let bones = vec![
        RawBone::new("Hips", -1, [0.0, 0.8, 0.0], [0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0], identity),
        RawBone::new("Spine", 0, [0.0, 0.2, 0.0], [0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0], identity),
        RawBone::new("Chest", 1, [0.0, 0.15, 0.0], [0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0], identity),
        RawBone::new("Neck", 2, [0.0, 0.15, 0.0], [0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0], identity),
        RawBone::new("Head", 3, [0.0, 0.1, 0.0], [0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0], identity),
        RawBone::new("Hair_Back_01", 4, [0.0, 0.05, -0.08], [0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0], identity),
    ];

    let vertices = vec![
        DmaVertex {
            position: [-0.2, 1.3, 0.0],
            normal: [0.0, 0.0, 1.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            uv0: [0.0, 0.0],
            uv1: [0.0, 0.0],
            bone_indices: [4, 3, 0, 0],
            bone_weights: [0.8, 0.2, 0.0, 0.0],
        },
        DmaVertex {
            position: [0.2, 1.3, 0.0],
            normal: [0.0, 0.0, 1.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            uv0: [1.0, 0.0],
            uv1: [0.0, 0.0],
            bone_indices: [4, 3, 0, 0],
            bone_weights: [0.8, 0.2, 0.0, 0.0],
        },
        DmaVertex {
            position: [0.0, 1.5, 0.0],
            normal: [0.0, 0.0, 1.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            uv0: [0.5, 1.0],
            uv1: [0.0, 0.0],
            bone_indices: [4, 0, 0, 0],
            bone_weights: [1.0, 0.0, 0.0, 0.0],
        },
    ];

    let mesh = DmaMesh {
        submeshes: vec![SubmeshInfo::new(0, 3, 0)],
        vertices,
        indices: vec![0, 1, 2],
    };

    let morph_targets = vec![MorphTarget {
        name: "vrc.blink".to_string(),
        deltas: vec![mascot_format::DeltaVertex {
            vertex_index: 2,
            delta_position: [0.0, -0.05, 0.01],
            delta_normal: [0.0, -0.1, 0.0],
            delta_tangent: [0.0, 0.0, 0.0],
        }],
    }];

    let mut mat_params = LilToonMaterialParams::default();
    mat_params.base_color = [1.0, 0.95, 0.95, 1.0];
    mat_params.shade_color = [0.85, 0.75, 0.8, 1.0];
    mat_params.outline_enable = 1;
    mat_params.outline_width = 1.0;
    mat_params.base_texture_idx = 0;

    let materials = vec![DmaMaterial {
        name: "Face_Mat".to_string(),
        params: mat_params,
    }];

    let textures = vec![DmaTexture {
        name: "Face_Albedo.png".to_string(),
        format: TEXTURE_FORMAT_PNG,
        width: 256,
        height: 256,
        data: vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A],
    }];

    let mut chain = PhysBoneChain::default();
    chain.root_bone_index = 5; // Hair_Back_01
    chain.pull = 0.25;
    chain.spring = 0.75;
    chain.damping = 0.12;
    chain.collider_count = 1;
    chain.collider_indices[0] = 0;

    let phys_chains = vec![chain];
    let colliders = vec![PhysCollider::sphere(4, [0.0, 0.05, 0.0], 0.15)];

    let out_path = "assets/sample.dma";
    let mut file = File::create(out_path)?;
    DmaWriter::write(
        &mut file,
        Some(&metadata),
        &bones,
        Some(&mesh),
        &morph_targets,
        &materials,
        &textures,
        &phys_chains,
        &colliders,
    )?;

    println!("Sample DMA created at {}", out_path);
    Ok(())
}
