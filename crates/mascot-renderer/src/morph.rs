use glam::Vec3;
use mascot_format::{DmaVertex, MorphTarget};
use wgpu::{Buffer, Queue};

/// Manages BlendShape / Morph Targets on CPU and uploads dirty vertex updates to the GPU vertex buffer.
pub struct MorphController {
    pub targets: Vec<MorphTarget>,
    pub weights: Vec<f32>,
    pub dirty: bool,
    pub base_vertices: Vec<DmaVertex>,
    pub current_vertices: Vec<DmaVertex>,
}

impl MorphController {
    pub fn new(targets: Vec<MorphTarget>, vertices: Vec<DmaVertex>) -> Self {
        let count = targets.len();
        Self {
            targets,
            weights: vec![0.0; count],
            dirty: false,
            current_vertices: vertices.clone(),
            base_vertices: vertices,
        }
    }

    /// Sets morph target weight (0.0 to 1.0) by index.
    pub fn set_weight(&mut self, index: usize, weight: f32) {
        if index < self.weights.len() {
            let clamped = weight.clamp(0.0, 1.0);
            if (self.weights[index] - clamped).abs() > 1e-4 {
                self.weights[index] = clamped;
                self.dirty = true;
            }
        }
    }

    /// Sets morph target weight by name. Returns true if found.
    pub fn set_weight_by_name(&mut self, name: &str, weight: f32) -> bool {
        if let Some(idx) = self.find_target_index(name) {
            self.set_weight(idx, weight);
            true
        } else {
            false
        }
    }

    /// Gets morph target weight by index.
    pub fn get_weight(&self, index: usize) -> f32 {
        self.weights.get(index).copied().unwrap_or(0.0)
    }

    /// Gets morph target weight by name.
    pub fn get_weight_by_name(&self, name: &str) -> Option<f32> {
        self.find_target_index(name).map(|idx| self.weights[idx])
    }

    /// Finds morph target index by name.
    pub fn find_target_index(&self, name: &str) -> Option<usize> {
        self.targets.iter().position(|t| t.name == name)
    }

    /// Re-evaluates all active morph deltas and writes updated vertex positions and normals
    /// to the GPU vertex buffer if weights have changed.
    pub fn apply(&mut self, vertex_buffer: &Buffer, queue: &Queue) {
        if !self.dirty {
            return;
        }

        // Reset to base vertices
        self.current_vertices.copy_from_slice(&self.base_vertices);

        // Apply active blendshape weights
        for (i, target) in self.targets.iter().enumerate() {
            let w = self.weights[i];
            if w <= 1e-4 {
                continue;
            }

            for delta in &target.deltas {
                let idx = delta.vertex_index as usize;
                if idx < self.current_vertices.len() {
                    let v = &mut self.current_vertices[idx];

                    v.position[0] += delta.delta_position[0] * w;
                    v.position[1] += delta.delta_position[1] * w;
                    v.position[2] += delta.delta_position[2] * w;

                    v.normal[0] += delta.delta_normal[0] * w;
                    v.normal[1] += delta.delta_normal[1] * w;
                    v.normal[2] += delta.delta_normal[2] * w;
                }
            }
        }

        // Re-normalize normals for modified vertices
        for v in &mut self.current_vertices {
            let n = Vec3::from_slice(&v.normal);
            let len_sq = n.length_squared();
            if len_sq > 1e-4 {
                v.normal = (n / len_sq.sqrt()).to_array();
            }
        }

        queue.write_buffer(
            vertex_buffer,
            0,
            bytemuck::cast_slice(&self.current_vertices),
        );

        self.dirty = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mascot_format::DeltaVertex;

    #[test]
    fn test_morph_controller_weights_and_deltas() {
        let base_vertices = vec![
            DmaVertex {
                position: [0.0, 0.0, 0.0],
                normal: [0.0, 1.0, 0.0],
                tangent: [1.0, 0.0, 0.0, 1.0],
                uv0: [0.0, 0.0],
                uv1: [0.0, 0.0],
                bone_indices: [0, 0, 0, 0],
                bone_weights: [1.0, 0.0, 0.0, 0.0],
            },
        ];

        let targets = vec![MorphTarget {
            name: "smile".to_string(),
            deltas: vec![DeltaVertex {
                vertex_index: 0,
                delta_position: [0.0, 0.1, 0.0],
                delta_normal: [0.0, 0.0, 0.0],
                delta_tangent: [0.0, 0.0, 0.0],
            }],
        }];

        let mut controller = MorphController::new(targets, base_vertices);
        assert_eq!(controller.get_weight_by_name("smile"), Some(0.0));

        assert!(controller.set_weight_by_name("smile", 0.8));
        assert!(controller.dirty);
        assert_eq!(controller.get_weight_by_name("smile"), Some(0.8));
    }
}
