use glam::{Mat4, Quat, Vec3};
use mascot_format::DmaFile;
use mascot_renderer::{BoneNode, LilToonUniform, MorphController};

#[test]
fn test_sample_dma_avatar_parsing_and_morphs() {
    let dma_bytes = std::fs::read("../../assets/sample.dma")
        .or_else(|_| std::fs::read("assets/sample.dma"))
        .expect("Failed to read assets/sample.dma");

    let dma = DmaFile::from_bytes(&dma_bytes).expect("Failed to parse sample.dma");
    dma.validate().expect("sample.dma validation failed");

    // 1. Verify skeleton hierarchy
    assert_eq!(dma.skeleton.len(), 6);
    assert_eq!(dma.skeleton[0].name_str(), "Hips");
    assert_eq!(dma.skeleton[0].parent_index, -1);
    assert_eq!(dma.skeleton[4].name_str(), "Head");
    assert_eq!(dma.skeleton[4].parent_index, 3);
    assert_eq!(dma.skeleton[5].name_str(), "Hair_Back_01");
    assert_eq!(dma.skeleton[5].parent_index, 4);

    // 2. Verify mesh & submeshes
    let mesh = dma.mesh.as_ref().expect("Expected mesh in sample.dma");
    assert_eq!(mesh.vertices.len(), 3);
    assert_eq!(mesh.indices.len(), 3);
    assert_eq!(mesh.submeshes.len(), 1);

    // 3. Verify materials and lilToon uniform conversion
    assert_eq!(dma.materials.len(), 1);
    let mat = &dma.materials[0];
    assert_eq!(mat.name, "Face_Mat");
    assert_eq!(mat.params.outline_enable, 1);
    assert_eq!(mat.params.outline_width, 1.0);

    let uniform = LilToonUniform::from(&mat.params);
    assert_eq!(uniform.base_color, [1.0, 0.95, 0.95, 1.0]);
    assert_eq!(uniform.shade_color, [0.85, 0.75, 0.8, 1.0]);
    assert_eq!(uniform.outline_enable, 1);
    assert_eq!(uniform.outline_width, 1.0);
    assert_eq!(uniform.matcap_enable, 0);
    assert_eq!(uniform.matcap_color, [1.0, 1.0, 1.0, 0.0]);

    // Verify type aliases
    let _dma_model_type_check: Option<mascot_renderer::DmaModel> = None;
    let _skinned_model_type_check: Option<mascot_renderer::SkinnedModel> = None;
    let _liltoon_type_check: Option<mascot_renderer::LilToonMaterialUniform> = None;

    // 4. Verify morph targets and blending
    assert_eq!(dma.morph_targets.len(), 1);
    assert_eq!(dma.morph_targets[0].name, "vrc.blink");

    let mut controller = MorphController::new(dma.morph_targets.clone(), mesh.vertices.clone());
    assert_eq!(controller.get_weight_by_name("vrc.blink"), Some(0.0));

    // When weight is 0.0, current vertices match base vertices
    assert_eq!(controller.current_vertices[2].position, [0.0, 1.5, 0.0]);

    // Apply blink weight = 1.0
    controller.set_weight_by_name("vrc.blink", 1.0);
    assert!(controller.dirty);
    assert_eq!(controller.get_weight_by_name("vrc.blink"), Some(1.0));

    // Manual delta calculation check
    let delta = &dma.morph_targets[0].deltas[0];
    assert_eq!(delta.vertex_index, 2);
    assert_eq!(delta.delta_position, [0.0, -0.05, 0.01]);
}

#[test]
fn test_bone_node_transform_calculation() {
    let mut root = BoneNode {
        name: "Hips".to_string(),
        parent_index: -1,
        local_position: Vec3::new(0.0, 0.8, 0.0),
        local_rotation: Quat::IDENTITY,
        local_scale: Vec3::ONE,
        inverse_bind_matrix: Mat4::from_translation(Vec3::new(0.0, -0.8, 0.0)),
        world_matrix: Mat4::IDENTITY,
        skin_matrix: Mat4::IDENTITY,
    };

    let mut child = BoneNode {
        name: "Spine".to_string(),
        parent_index: 0,
        local_position: Vec3::new(0.0, 0.2, 0.0),
        local_rotation: Quat::IDENTITY,
        local_scale: Vec3::ONE,
        inverse_bind_matrix: Mat4::from_translation(Vec3::new(0.0, -1.0, 0.0)),
        world_matrix: Mat4::IDENTITY,
        skin_matrix: Mat4::IDENTITY,
    };

    root.world_matrix = root.compute_local_matrix();
    child.world_matrix = root.world_matrix * child.compute_local_matrix();

    assert!((child.world_matrix.w_axis.y - 1.0).abs() < 1e-5);
    child.skin_matrix = child.world_matrix * child.inverse_bind_matrix;
    assert!(child.skin_matrix.abs_diff_eq(Mat4::IDENTITY, 1e-4));

    // Rotate child by 90 deg around Z
    child.local_rotation = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
    child.world_matrix = root.world_matrix * child.compute_local_matrix();
    child.skin_matrix = child.world_matrix * child.inverse_bind_matrix;
    assert!(!child.skin_matrix.abs_diff_eq(Mat4::IDENTITY, 1e-2));
}
