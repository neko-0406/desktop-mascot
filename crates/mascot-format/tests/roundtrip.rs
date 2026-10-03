use std::collections::HashMap;
use std::io::Cursor;

use mascot_format::{
    AvatarMetadata, ColliderShape, DmaError, DmaFile, DmaMaterial, DmaMesh, DmaTexture, DmaVertex,
    DmaWriter, LilToonMaterialParams, MorphTarget, PhysBoneChain, PhysCollider, RawBone,
    SubmeshInfo, ValidationError, TEXTURE_FORMAT_PNG,
};

fn create_sample_avatar() -> (
    AvatarMetadata,
    Vec<RawBone>,
    DmaMesh,
    Vec<MorphTarget>,
    Vec<DmaMaterial>,
    Vec<DmaTexture>,
    Vec<PhysBoneChain>,
    Vec<PhysCollider>,
) {
    let mut visemes = HashMap::new();
    visemes.insert("vrc.v_aa".to_string(), 1);
    visemes.insert("vrc.v_ih".to_string(), 2);

    let mut blinks = HashMap::new();
    blinks.insert("vrc.blink".to_string(), 0);

    let metadata = AvatarMetadata {
        avatar_name: "TestAvatar".to_string(),
        author: "Developer".to_string(),
        view_position: [0.0, 1.45, 0.05],
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
        RawBone::new(
            "Hips",
            -1,
            [0.0, 0.8, 0.0],
            [0.0, 0.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            identity,
        ),
        RawBone::new(
            "Spine",
            0,
            [0.0, 0.2, 0.0],
            [0.0, 0.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            identity,
        ),
        RawBone::new(
            "Head",
            1,
            [0.0, 0.4, 0.0],
            [0.0, 0.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            identity,
        ),
        RawBone::new(
            "Hair_Root",
            2,
            [0.0, 0.1, -0.1],
            [0.0, 0.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            identity,
        ),
    ];

    let vertices = vec![
        DmaVertex {
            position: [0.0, 0.0, 0.0],
            normal: [0.0, 1.0, 0.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            uv0: [0.0, 0.0],
            uv1: [0.0, 0.0],
            bone_indices: [0, 1, 0, 0],
            bone_weights: [0.7, 0.3, 0.0, 0.0],
        },
        DmaVertex {
            position: [1.0, 0.0, 0.0],
            normal: [0.0, 1.0, 0.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            uv0: [1.0, 0.0],
            uv1: [0.5, 0.0],
            bone_indices: [1, 2, 0, 0],
            bone_weights: [0.5, 0.5, 0.0, 0.0],
        },
        DmaVertex {
            position: [0.0, 1.0, 0.0],
            normal: [0.0, 1.0, 0.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            uv0: [0.0, 1.0],
            uv1: [0.0, 0.5],
            bone_indices: [2, 0, 0, 0],
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
            delta_position: [0.0, -0.05, 0.0],
            delta_normal: [0.0, -0.1, 0.0],
            delta_tangent: [0.0, 0.0, 0.0],
        }],
    }];

    let mut mat_params = LilToonMaterialParams::default();
    mat_params.base_color = [1.0, 0.9, 0.9, 1.0];
    mat_params.shade_color = [0.8, 0.7, 0.75, 1.0];
    mat_params.outline_enable = 1;
    mat_params.outline_width = 1.2;
    mat_params.base_texture_idx = 0;

    let materials = vec![DmaMaterial {
        name: "BodyMat".to_string(),
        params: mat_params,
    }];

    let textures = vec![DmaTexture {
        name: "Body_Albedo.png".to_string(),
        format: TEXTURE_FORMAT_PNG,
        width: 128,
        height: 128,
        data: vec![0x89, b'P', b'N', b'G', 1, 2, 3, 4],
    }];

    let mut chain = PhysBoneChain::default();
    chain.root_bone_index = 3; // Hair_Root
    chain.pull = 0.3;
    chain.spring = 0.7;
    chain.damping = 0.15;
    chain.collider_count = 1;
    chain.collider_indices[0] = 0;

    let phys_chains = vec![chain];

    let colliders = vec![PhysCollider::sphere(2, [0.0, 0.05, 0.0], 0.12)];

    (
        metadata,
        bones,
        mesh,
        morph_targets,
        materials,
        textures,
        phys_chains,
        colliders,
    )
}

#[test]
fn test_dma_roundtrip_full() {
    let (metadata, bones, mesh, morph_targets, materials, textures, phys_chains, colliders) =
        create_sample_avatar();

    let mut buffer = Cursor::new(Vec::new());
    let write_res = DmaWriter::write(
        &mut buffer,
        Some(&metadata),
        &bones,
        Some(&mesh),
        &morph_targets,
        &materials,
        &textures,
        &phys_chains,
        &colliders,
    );

    assert!(write_res.is_ok(), "Writing DMA must succeed");
    let file_bytes = buffer.into_inner();

    // 1. Read from bytes
    let dma = DmaFile::from_bytes(&file_bytes).expect("Parsing DMA from bytes must succeed");

    assert_eq!(dma.header.magic, *b"DMA1");
    assert_eq!(dma.header.version, 1);
    assert_eq!(dma.header.total_file_size, file_bytes.len() as u64);

    // Verify metadata
    let read_meta = dma.metadata.as_ref().expect("Metadata must be present");
    assert_eq!(read_meta.avatar_name, "TestAvatar");
    assert_eq!(read_meta.author, "Developer");
    assert_eq!(read_meta.view_position, [0.0, 1.45, 0.05]);

    // Verify skeleton
    assert_eq!(dma.skeleton.len(), 4);
    assert_eq!(dma.skeleton[0].name_str(), "Hips");
    assert_eq!(dma.skeleton[0].parent_index, -1);
    assert_eq!(dma.skeleton[1].name_str(), "Spine");
    assert_eq!(dma.skeleton[1].parent_index, 0);

    // Verify mesh
    let read_mesh = dma.mesh.as_ref().expect("Mesh must be present");
    assert_eq!(read_mesh.vertices.len(), 3);
    assert_eq!(read_mesh.indices, vec![0, 1, 2]);
    assert_eq!(read_mesh.submeshes.len(), 1);
    assert_eq!(read_mesh.submeshes[0].material_index, 0);
    assert_eq!(read_mesh.vertices[0].bone_weights[0], 0.7);

    // Verify morphs
    assert_eq!(dma.morph_targets.len(), 1);
    assert_eq!(dma.morph_targets[0].name, "vrc.blink");
    assert_eq!(dma.morph_targets[0].deltas.len(), 1);
    assert_eq!(dma.morph_targets[0].deltas[0].vertex_index, 2);

    // Verify materials and textures
    assert_eq!(dma.materials.len(), 1);
    assert_eq!(dma.materials[0].name, "BodyMat");
    assert_eq!(dma.materials[0].params.outline_enable, 1);
    assert_eq!(dma.materials[0].params.base_texture_idx, 0);
    assert_eq!(dma.textures.len(), 1);
    assert_eq!(dma.textures[0].name, "Body_Albedo.png");
    assert_eq!(dma.textures[0].data, vec![0x89, b'P', b'N', b'G', 1, 2, 3, 4]);

    // Verify physics
    assert_eq!(dma.phys_chains.len(), 1);
    assert_eq!(dma.phys_chains[0].root_bone_index, 3);
    assert_eq!(dma.phys_chains[0].collider_count, 1);
    assert_eq!(dma.colliders.len(), 1);
    assert_eq!(dma.colliders[0].shape, ColliderShape::Sphere as u32);
    assert_eq!(dma.colliders[0].root_bone_index, 2);

    // Validate cross references
    assert!(dma.validate().is_ok(), "Avatar validation must pass");

    // Check summary string generation
    let summary = dma.dump_summary();
    assert!(summary.contains("TestAvatar"));
    assert!(summary.contains("Hair_Root"));
    assert!(summary.contains("BodyMat"));
}

#[test]
fn test_validation_errors() {
    let (_, bones, mut mesh, _, materials, textures, mut phys_chains, colliders) =
        create_sample_avatar();

    // 1. Invalid bone parent
    let mut invalid_bones = bones.clone();
    invalid_bones[0].parent_index = 99; // out of range
    let res = mascot_format::validate_avatar(
        &invalid_bones,
        Some(&mesh),
        &[],
        &materials,
        &textures,
        &phys_chains,
        &colliders,
    );
    assert!(matches!(res, Err(ValidationError::InvalidBoneParent { .. })));

    // 2. Vertex bone index out of bounds
    mesh.vertices[0].bone_indices[0] = 100;
    let res = mascot_format::validate_avatar(
        &bones,
        Some(&mesh),
        &[],
        &materials,
        &textures,
        &phys_chains,
        &colliders,
    );
    assert!(matches!(
        res,
        Err(ValidationError::VertexBoneIndexOutOfBounds { .. })
    ));

    // 3. PhysBone root bone out of bounds
    phys_chains[0].root_bone_index = 50;
    let res = mascot_format::validate_avatar(
        &bones,
        None,
        &[],
        &materials,
        &textures,
        &phys_chains,
        &colliders,
    );
    assert!(matches!(
        res,
        Err(ValidationError::PhysBoneRootOutOfBounds { .. })
    ));
}

#[test]
fn test_corrupt_magic_and_truncated() {
    let mut corrupt_bytes = vec![0u8; 64];
    corrupt_bytes[0..4].copy_from_slice(b"BAD!");
    let res = DmaFile::from_bytes(&corrupt_bytes);
    assert!(matches!(res, Err(DmaError::InvalidMagic(_))));

    let truncated_bytes = vec![0u8; 16];
    let res = DmaFile::from_bytes(&truncated_bytes);
    assert!(matches!(res, Err(DmaError::MalformedChunk(_, _))));
}
