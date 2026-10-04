use glam::{Quat, Vec3};
use mascot_format::{PhysBoneChain, PhysCollider, RawBone};
use mascot_physics::{
    closest_point_on_segment, resolve_capsule, resolve_plane,
    BlinkController, BreathingController, LookAtController,
    PhysBoneChainSim, PhysicsWorld, SimpleBone,
};

#[test]
fn test_multi_segment_physbone_chain_and_constraints() {
    let mut chain = PhysBoneChain::default();
    chain.root_bone_index = 0;
    chain.pull = 0.3;
    chain.spring = 0.8;
    chain.damping = 0.15;
    chain.gravity = [0.0, -9.81, 0.0];
    chain.max_angle = std::f32::consts::FRAC_PI_4; // 45 deg limit
    chain.radius = 0.02;

    // Multi-segment chain: Root(0) -> Child1(1) -> Child2(2)
    let child_bones = vec![
        (1usize, Vec3::new(0.0, -0.2, 0.0), Quat::IDENTITY),
        (2usize, Vec3::new(0.0, -0.2, 0.0), Quat::IDENTITY),
    ];

    let mut sim = PhysBoneChainSim::new(
        &chain,
        0,
        -1,
        Vec3::new(0.0, 1.0, 0.0),
        Quat::IDENTITY,
        &child_bones,
    );

    // Root particle (0) + 2 child particles (1, 2) + 1 tip particle (3)
    assert_eq!(sim.particles.len(), 4);
    assert_eq!(sim.segments.len(), 3);

    let dt = 1.0 / 60.0;
    for _ in 0..10 {
        sim.step(
            dt,
            Vec3::ZERO,
            Vec3::new(0.0, 1.0, 0.0),
            Quat::IDENTITY,
            Quat::IDENTITY,
            &[],
            3,
        );
    }

    // Check distance constraints on all segments
    for seg in &sim.segments {
        let p_a = sim.particles[seg.particle_a].position;
        let p_b = sim.particles[seg.particle_b].position;
        let dist = (p_b - p_a).length();
        assert!(
            (dist - seg.rest_length).abs() < 1e-4,
            "Segment distance {} deviates from rest length {}",
            dist,
            seg.rest_length
        );
    }
}

#[test]
fn test_capsule_and_sphere_collider_interaction() {
    let mut pos = Vec3::new(0.05, 0.5, 0.0);
    let particle_radius = 0.02;

    // Capsule from (0, 0, 0) to (0, 1, 0) with radius 0.2
    let cap_a = Vec3::ZERO;
    let cap_b = Vec3::Y;
    let collider_radius = 0.2;

    let collided = resolve_capsule(
        &mut pos,
        particle_radius,
        cap_a,
        cap_b,
        collider_radius,
        false,
    );
    assert!(collided);

    // Closest point on axis is (0, 0.5, 0)
    let closest = closest_point_on_segment(cap_a, cap_b, pos);
    let dist = (pos - closest).length();
    let expected_dist = collider_radius + particle_radius;
    assert!((dist - expected_dist).abs() < 1e-4);
    assert!(pos.x > 0.2);
}

#[test]
fn test_plane_collider_reflection() {
    let mut pos = Vec3::new(0.0, -0.05, 0.0);
    let particle_radius = 0.03;
    let plane_point = Vec3::ZERO;
    let plane_normal = Vec3::Y;

    let collided = resolve_plane(&mut pos, particle_radius, plane_point, plane_normal);
    assert!(collided);
    assert!((pos.y - particle_radius).abs() < 1e-5);
}

#[test]
fn test_physics_world_with_raw_bones_and_colliders() {
    let raw_bones = vec![
        RawBone::new("Hips", -1, [0.0, 0.8, 0.0], [0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0], [0.0; 16]),
        RawBone::new("Head", 0, [0.0, 0.6, 0.0], [0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0], [0.0; 16]),
        RawBone::new("Hair", 1, [0.0, 0.05, -0.08], [0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0], [0.0; 16]),
    ];

    let mut chain = PhysBoneChain::default();
    chain.root_bone_index = 2; // Hair
    chain.pull = 0.2;
    chain.spring = 0.6;
    chain.damping = 0.1;
    chain.collider_count = 1;
    chain.collider_indices[0] = 0;

    let collider = PhysCollider::sphere(1, [0.0, 0.0, 0.0], 0.15); // Sphere attached to Head

    let mut world = PhysicsWorld::from_format(&[chain], &[collider], &raw_bones);
    assert_eq!(world.chains.len(), 1);
    assert_eq!(world.collider_defs.len(), 1);

    // Create bone pose reader
    let bones = vec![
        SimpleBone::new("Hips", -1, Vec3::new(0.0, 0.8, 0.0), Quat::IDENTITY),
        SimpleBone::new("Head", 0, Vec3::new(0.0, 1.4, 0.0), Quat::IDENTITY),
        SimpleBone::new("Hair", 1, Vec3::new(0.0, 1.45, -0.08), Quat::IDENTITY),
    ];

    let updates = world.step(0.05, &bones, Vec3::new(0.0, -1.0, 0.0));
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].bone_index, 2);
}

#[test]
fn test_procedural_motion_integration() {
    // 1. Breathing
    let breathing = BreathingController::default();
    let pose_breath = breathing.update(2.0);
    // At t=2.0 (half-cycle of 4.0s), sin(pi) is 0
    assert!(pose_breath.chest_rotation.x.abs() < 1e-4);

    let pose_quarter = breathing.update(1.0); // peak
    assert!(pose_quarter.chest_rotation.x.abs() > 0.005);

    // 2. Blinking
    let mut blink = BlinkController::new(12345);
    let mut saw_blink = false;
    for _ in 0..500 {
        let w = blink.update(0.02);
        if w > 0.5 {
            saw_blink = true;
            break;
        }
    }
    assert!(saw_blink, "BlinkController should trigger a blink within 10 seconds");

    // 3. Look-At IK
    let mut look_at = LookAtController::default();
    look_at.set_cursor_window_coords(600.0, 100.0, 800.0, 600.0);
    for _ in 0..30 {
        look_at.update(1.0 / 60.0);
    }
    let pose_look = look_at.update(1.0 / 60.0);
    assert!(pose_look.current_yaw > 0.0);
    assert!(pose_look.head_rotation.y > 0.0);
    assert!(pose_look.neck_rotation.y > 0.0);
}
