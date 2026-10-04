use glam::{Quat, Vec3};
use mascot_format::{ColliderShape, PhysCollider};

/// Calculates the closest point on a line segment `AB` to `point`.
pub fn closest_point_on_segment(a: Vec3, b: Vec3, point: Vec3) -> Vec3 {
    let ab = b - a;
    let ab_len_sq = ab.length_squared();
    if ab_len_sq < 1e-6 {
        return a;
    }
    let t = ((point - a).dot(ab) / ab_len_sq).clamp(0.0, 1.0);
    a + ab * t
}

/// Resolves collision between a particle and a sphere.
///
/// If `inside_bounds` is false (default), pushes the particle outside the sphere.
/// If `inside_bounds` is true, keeps the particle contained inside the sphere.
pub fn resolve_sphere(
    pos: &mut Vec3,
    particle_radius: f32,
    center: Vec3,
    collider_radius: f32,
    inside_bounds: bool,
) -> bool {
    let diff = *pos - center;
    let dist_sq = diff.length_squared();

    if inside_bounds {
        // Containment: particle center must not be further than (collider_radius - particle_radius) from center
        let max_dist = (collider_radius - particle_radius).max(0.0);
        if dist_sq > max_dist * max_dist {
            let dist = dist_sq.sqrt();
            if dist > 1e-6 {
                *pos = center + diff * (max_dist / dist);
                return true;
            }
        }
    } else {
        // Push-out: distance must be at least (collider_radius + particle_radius)
        let min_dist = collider_radius + particle_radius;
        if dist_sq < min_dist * min_dist {
            let dist = dist_sq.sqrt();
            if dist > 1e-6 {
                *pos = center + diff * (min_dist / dist);
            } else {
                *pos = center + Vec3::Y * min_dist;
            }
            return true;
        }
    }

    false
}

/// Resolves collision between a particle and a capsule defined by endpoints `cap_a` and `cap_b`.
pub fn resolve_capsule(
    pos: &mut Vec3,
    particle_radius: f32,
    cap_a: Vec3,
    cap_b: Vec3,
    collider_radius: f32,
    inside_bounds: bool,
) -> bool {
    let closest = closest_point_on_segment(cap_a, cap_b, *pos);
    resolve_sphere(pos, particle_radius, closest, collider_radius, inside_bounds)
}

/// Resolves collision between a particle and a plane defined by `plane_point` and unit `plane_normal`.
pub fn resolve_plane(
    pos: &mut Vec3,
    particle_radius: f32,
    plane_point: Vec3,
    plane_normal: Vec3,
) -> bool {
    let diff = *pos - plane_point;
    let dist = diff.dot(plane_normal);

    // If particle penetrated into or below the plane surface:
    if dist < particle_radius {
        let penetration = particle_radius - dist;
        *pos += plane_normal * penetration;
        return true;
    }

    false
}

/// A physical collision body defined in avatar local or bone space.
#[derive(Debug, Clone)]
pub struct ColliderDef {
    pub shape: ColliderShape,
    pub root_bone_index: usize,
    pub local_position: Vec3,
    pub local_rotation: Quat,
    pub radius: f32,
    pub height: f32,
    pub inside_bounds: bool,
}

impl ColliderDef {
    pub fn from_format(c: &PhysCollider) -> Self {
        let shape = ColliderShape::try_from(c.shape).unwrap_or(ColliderShape::Sphere);
        let rot = Quat::from_array(c.rotation);
        let local_rotation = if rot.length_squared() > 1e-4 {
            rot.normalize()
        } else {
            Quat::IDENTITY
        };

        Self {
            shape,
            root_bone_index: c.root_bone_index as usize,
            local_position: Vec3::from_slice(&c.position),
            local_rotation,
            radius: c.radius,
            height: c.height,
            inside_bounds: c.inside_bounds != 0,
        }
    }
}

/// A collider evaluated in world space for the current simulation frame.
#[derive(Debug, Clone)]
pub struct WorldCollider {
    pub shape: ColliderShape,
    pub world_center: Vec3,
    pub world_rotation: Quat,
    pub radius: f32,
    pub height: f32,
    pub inside_bounds: bool,
}

impl WorldCollider {
    /// Updates the world position and orientation from attached bone's world transform.
    pub fn update_from_bone(
        &mut self,
        bone_world_pos: Vec3,
        bone_world_rot: Quat,
        def: &ColliderDef,
    ) {
        self.world_center = bone_world_pos + bone_world_rot * def.local_position;
        self.world_rotation = (bone_world_rot * def.local_rotation).normalize();
        self.radius = def.radius;
        self.height = def.height;
        self.inside_bounds = def.inside_bounds;
    }

    /// Resolves collision with a simulated particle position.
    pub fn resolve_collision(&self, pos: &mut Vec3, particle_radius: f32) -> bool {
        match self.shape {
            ColliderShape::Sphere => resolve_sphere(
                pos,
                particle_radius,
                self.world_center,
                self.radius,
                self.inside_bounds,
            ),
            ColliderShape::Capsule => {
                // Capsule endpoints along local Y axis (height is distance between sphere caps)
                let half_h = (self.height * 0.5).max(0.0);
                let axis = self.world_rotation * Vec3::Y;
                let cap_a = self.world_center - axis * half_h;
                let cap_b = self.world_center + axis * half_h;
                resolve_capsule(
                    pos,
                    particle_radius,
                    cap_a,
                    cap_b,
                    self.radius,
                    self.inside_bounds,
                )
            }
            ColliderShape::Plane => {
                let normal = self.world_rotation * Vec3::Y;
                resolve_plane(pos, particle_radius, self.world_center, normal)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sphere_pushout() {
        let center = Vec3::new(0.0, 1.0, 0.0);
        let collider_radius = 0.5;
        let particle_radius = 0.05;

        // Particle placed slightly inside sphere
        let mut pos = Vec3::new(0.1, 1.0, 0.0);
        let collided = resolve_sphere(&mut pos, particle_radius, center, collider_radius, false);
        assert!(collided);

        let dist = (pos - center).length();
        let expected_min = collider_radius + particle_radius;
        assert!((dist - expected_min).abs() < 1e-4);
        assert!(pos.x > 0.5);
    }

    #[test]
    fn test_sphere_containment() {
        let center = Vec3::new(0.0, 0.0, 0.0);
        let collider_radius = 1.0;
        let particle_radius = 0.1;

        // Particle outside sphere
        let mut pos = Vec3::new(2.0, 0.0, 0.0);
        let collided = resolve_sphere(&mut pos, particle_radius, center, collider_radius, true);
        assert!(collided);

        let dist = (pos - center).length();
        let expected_max = collider_radius - particle_radius;
        assert!((dist - expected_max).abs() < 1e-4);
        assert!((pos.x - 0.9).abs() < 1e-4);
    }

    #[test]
    fn test_capsule_pushout() {
        let a = Vec3::new(0.0, 0.0, 0.0);
        let b = Vec3::new(0.0, 2.0, 0.0);
        let collider_radius = 0.3;
        let particle_radius = 0.05;

        // Particle near the middle of capsule (y=1.0)
        let mut pos = Vec3::new(0.1, 1.0, 0.0);
        let collided = resolve_capsule(&mut pos, particle_radius, a, b, collider_radius, false);
        assert!(collided);

        let expected_x = collider_radius + particle_radius;
        assert!((pos.x - expected_x).abs() < 1e-4);
        assert!((pos.y - 1.0).abs() < 1e-4);
    }

    #[test]
    fn test_plane_pushout() {
        let plane_point = Vec3::new(0.0, 0.0, 0.0);
        let normal = Vec3::Y;
        let particle_radius = 0.1;

        // Particle penetrating below ground plane
        let mut pos = Vec3::new(0.0, -0.05, 0.0);
        let collided = resolve_plane(&mut pos, particle_radius, plane_point, normal);
        assert!(collided);
        assert!((pos.y - 0.1).abs() < 1e-4);
    }
}
