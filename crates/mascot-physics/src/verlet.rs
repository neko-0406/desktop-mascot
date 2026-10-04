use glam::{Quat, Vec3};

/// Clamps the direction vector within a conical limit `max_angle_rad` relative to `rest_dir`.
pub fn clamp_angle(current_dir: Vec3, rest_dir: Vec3, max_angle_rad: f32) -> Vec3 {
    if max_angle_rad >= std::f32::consts::PI {
        return current_dir;
    }

    let dot = current_dir.dot(rest_dir).clamp(-1.0, 1.0);
    let angle = dot.acos();

    if angle <= max_angle_rad {
        current_dir
    } else {
        let axis = rest_dir.cross(current_dir);
        if axis.length_squared() < 1e-6 {
            // Anti-parallel or collinear; pick an orthogonal vector
            let ortho = if rest_dir.x.abs() < 0.9 {
                Vec3::X.cross(rest_dir).normalize()
            } else {
                Vec3::Y.cross(rest_dir).normalize()
            };
            Quat::from_axis_angle(ortho, max_angle_rad) * rest_dir
        } else {
            let axis = axis.normalize();
            Quat::from_axis_angle(axis, max_angle_rad) * rest_dir
        }
    }
}

/// A simulated mass particle in position-based Verlet integration.
#[derive(Debug, Clone)]
pub struct VerletParticle {
    /// Current position in world coordinates (x_t).
    pub position: Vec3,
    /// Previous position in world coordinates (x_{t - dt}).
    pub prev_position: Vec3,
    /// Radius of the particle for collision resolution.
    pub radius: f32,
    /// Whether this particle is pinned (e.g. root bone attached to avatar).
    pub is_fixed: bool,
    /// Rest offset relative to the parent particle or bone.
    pub rest_local_offset: Vec3,
    /// Distance constraint length from parent particle.
    pub rest_length: f32,
}

impl VerletParticle {
    pub fn new(
        position: Vec3,
        radius: f32,
        is_fixed: bool,
        rest_local_offset: Vec3,
        rest_length: f32,
    ) -> Self {
        Self {
            position,
            prev_position: position,
            radius,
            is_fixed,
            rest_local_offset,
            rest_length,
        }
    }

    /// Performs position-based Verlet numerical integration.
    ///
    /// x_{t+dt} = x_t + (1 - damping) * (x_t - x_{t-dt}) + a * dt^2
    pub fn integrate(&mut self, acceleration: Vec3, damping: f32, dt: f32) {
        if self.is_fixed {
            return;
        }

        let damp = (1.0 - damping).clamp(0.0, 1.0);
        let velocity = (self.position - self.prev_position) * damp;
        let next_pos = self.position + velocity + acceleration * (dt * dt);

        self.prev_position = self.position;
        self.position = next_pos;
    }

    /// Applies pull force and stiffness toward rest target position.
    pub fn apply_restoration(&mut self, target_pos: Vec3, pull: f32, stiffness: f32, dt: f32) {
        if self.is_fixed {
            self.position = target_pos;
            self.prev_position = target_pos;
            return;
        }

        // Pull: spring restitution to animated rest pose
        let pull_factor = (pull * dt * 60.0).clamp(0.0, 1.0);
        if pull_factor > 0.0 {
            self.position = self.position.lerp(target_pos, pull_factor);
        }

        // Stiffness: resistance to deformation from base pose
        let stiffness_factor = (stiffness * dt * 60.0).clamp(0.0, 1.0);
        if stiffness_factor > 0.0 {
            self.position = self.position.lerp(target_pos, stiffness_factor);
        }
    }

    /// Resets particle position and velocity (teleport / initialization).
    pub fn reset_position(&mut self, pos: Vec3) {
        self.position = pos;
        self.prev_position = pos;
    }
}

/// Enforces distance constraint: maintains fixed distance `target_length` between parent and child.
pub fn solve_distance_constraint(
    parent_pos: Vec3,
    child_pos: &mut Vec3,
    target_length: f32,
    fallback_dir: Vec3,
) {
    let delta = *child_pos - parent_pos;
    let len = delta.length();

    if len > 1e-6 {
        *child_pos = parent_pos + delta * (target_length / len);
    } else {
        *child_pos = parent_pos + fallback_dir * target_length;
    }
}

/// Enforces conical angle limit constraint relative to target direction.
pub fn solve_angle_limit(
    parent_pos: Vec3,
    child_pos: &mut Vec3,
    target_dir: Vec3,
    max_angle_rad: f32,
    length: f32,
) {
    let delta = *child_pos - parent_pos;
    let len_sq = delta.length_squared();

    if len_sq > 1e-6 && target_dir.length_squared() > 1e-6 {
        let current_dir = delta.normalize();
        let target_unit = target_dir.normalize();
        let clamped = clamp_angle(current_dir, target_unit, max_angle_rad);
        *child_pos = parent_pos + clamped * length;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verlet_integration_gravity() {
        let mut p = VerletParticle::new(Vec3::new(0.0, 1.0, 0.0), 0.02, false, Vec3::ZERO, 0.2);
        let dt = 1.0 / 60.0;
        let gravity = Vec3::new(0.0, -9.81, 0.0);

        // First step from rest: prev_pos == pos, so v = 0
        p.integrate(gravity, 0.1, dt);

        let expected_y = 1.0 + gravity.y * (dt * dt);
        assert!((p.position.y - expected_y).abs() < 1e-5);
        assert!((p.prev_position.y - 1.0).abs() < 1e-5);

        // Next step carries momentum
        p.integrate(gravity, 0.0, dt);
        assert!(p.position.y < expected_y);
    }

    #[test]
    fn test_distance_constraint() {
        let parent = Vec3::new(0.0, 1.0, 0.0);
        let mut child = Vec3::new(0.0, 1.5, 0.0); // distance 0.5
        let rest_length = 0.2;

        solve_distance_constraint(parent, &mut child, rest_length, Vec3::Y);

        let dist = (child - parent).length();
        assert!((dist - rest_length).abs() < 1e-5);
        assert!((child.y - 1.2).abs() < 1e-5);
    }

    #[test]
    fn test_angle_limit_cone() {
        let rest_dir = Vec3::Y;
        // Direction at 90 degrees (Vec3::X)
        let current_dir = Vec3::X;
        let max_angle = std::f32::consts::FRAC_PI_4; // 45 degrees

        let clamped = clamp_angle(current_dir, rest_dir, max_angle);
        let angle = clamped.dot(rest_dir).acos();
        assert!((angle - max_angle).abs() < 1e-4);
        assert!(clamped.x > 0.0);
        assert!(clamped.y > 0.0);
    }

    #[test]
    fn test_pull_restoration() {
        let mut p = VerletParticle::new(Vec3::new(1.0, 0.0, 0.0), 0.02, false, Vec3::ZERO, 0.2);
        let target = Vec3::ZERO;
        let dt = 1.0 / 60.0;

        p.apply_restoration(target, 0.5, 0.0, dt);
        assert!(p.position.x < 1.0);
        assert!(p.position.x > 0.0);
    }
}
