//! mascot-renderer: wgpu-based avatar renderer for Desktop Mascot.

pub mod avatar;
pub mod camera;
pub mod context;
pub mod error;
pub mod material;
pub mod mesh;
pub mod morph;
pub mod pipeline;
pub mod renderer;
pub mod skeleton;

pub use avatar::GpuAvatar;
pub use camera::{Camera, CameraBuffer, CameraUniform};
pub use context::{RenderContext, DEPTH_FORMAT};
pub use error::RendererError;
pub use material::{GpuMaterial, GpuTexture, LilToonUniform};
pub use mesh::{create_cube_mesh, GpuMesh};
pub use morph::MorphController;
pub use pipeline::{
    BasicPipeline, LightUniform, LilToonPipeline, ModelBuffer, ModelUniform, SceneBuffer,
    PREMULTIPLIED_ALPHA_BLEND,
};
pub use renderer::MascotRenderer;
pub use skeleton::{BoneNode, GpuSkeleton};

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{Mat4, Quat, Vec3};
    use mascot_format::{DeltaVertex, DmaVertex, LilToonMaterialParams, MorphTarget};

    #[test]
    fn test_camera_matrices() {
        let camera = Camera::new(Vec3::new(0.0, 1.0, 3.0), Vec3::new(0.0, 0.5, 0.0), 1.0);
        let vp = camera.build_view_projection_matrix();
        assert!(!vp.is_nan());
        assert_ne!(vp, glam::Mat4::ZERO);

        let uniform = camera.build_uniform();
        assert_eq!(uniform.view_pos, [0.0, 1.0, 3.0]);
    }

    #[test]
    fn test_cube_mesh_generation() {
        let cube = create_cube_mesh(1.0);
        assert_eq!(cube.vertices.len(), 24);
        assert_eq!(cube.indices.len(), 36);
        assert_eq!(cube.submeshes.len(), 1);
        assert_eq!(cube.submeshes[0].index_count, 36);

        for v in &cube.vertices {
            let n = glam::Vec3::from_slice(&v.normal);
            assert!((n.length() - 1.0).abs() < 1e-4);
        }
    }

    #[test]
    fn test_premultiplied_alpha_blend() {
        assert_eq!(PREMULTIPLIED_ALPHA_BLEND.color.src_factor, wgpu::BlendFactor::One);
        assert_eq!(PREMULTIPLIED_ALPHA_BLEND.color.dst_factor, wgpu::BlendFactor::OneMinusSrcAlpha);
        assert_eq!(PREMULTIPLIED_ALPHA_BLEND.alpha.src_factor, wgpu::BlendFactor::One);
        assert_eq!(PREMULTIPLIED_ALPHA_BLEND.alpha.dst_factor, wgpu::BlendFactor::OneMinusSrcAlpha);
    }

    #[test]
    fn test_liltoon_uniform_conversion() {
        let mut params = LilToonMaterialParams::default();
        params.base_color = [1.0, 0.8, 0.6, 1.0];
        params.shade_color = [0.8, 0.6, 0.4, 1.0];
        params.outline_width = 1.5;

        let uniform = LilToonUniform::from(&params);
        assert_eq!(uniform.base_color, [1.0, 0.8, 0.6, 1.0]);
        assert_eq!(uniform.shade_color, [0.8, 0.6, 0.4, 1.0]);
        assert_eq!(uniform.outline_width, 1.5);
        assert_eq!(uniform.outline_enable, 1);
    }

    #[test]
    fn test_morph_controller_blend() {
        let vertices = vec![
            DmaVertex {
                position: [0.0, 1.0, 0.0],
                normal: [0.0, 0.0, 1.0],
                tangent: [1.0, 0.0, 0.0, 1.0],
                uv0: [0.0, 0.0],
                uv1: [0.0, 0.0],
                bone_indices: [0, 0, 0, 0],
                bone_weights: [1.0, 0.0, 0.0, 0.0],
            },
        ];

        let target = MorphTarget {
            name: "vrc.blink".to_string(),
            deltas: vec![DeltaVertex {
                vertex_index: 0,
                delta_position: [0.0, -0.05, 0.0],
                delta_normal: [0.0, -0.1, 0.0],
                delta_tangent: [0.0, 0.0, 0.0],
            }],
        };

        let mut morph = MorphController::new(vec![target], vertices);
        morph.set_weight_by_name("vrc.blink", 1.0);
        assert_eq!(morph.get_weight(0), 1.0);
        assert!(morph.dirty);
    }

    #[test]
    fn test_bone_node_hierarchy() {
        let mut hips = BoneNode {
            name: "Hips".to_string(),
            parent_index: -1,
            local_position: Vec3::new(0.0, 0.8, 0.0),
            local_rotation: Quat::IDENTITY,
            local_scale: Vec3::ONE,
            inverse_bind_matrix: Mat4::from_translation(Vec3::new(0.0, -0.8, 0.0)),
            world_matrix: Mat4::IDENTITY,
            skin_matrix: Mat4::IDENTITY,
        };

        let head = BoneNode {
            name: "Head".to_string(),
            parent_index: 0,
            local_position: Vec3::new(0.0, 0.6, 0.0),
            local_rotation: Quat::IDENTITY,
            local_scale: Vec3::ONE,
            inverse_bind_matrix: Mat4::from_translation(Vec3::new(0.0, -1.4, 0.0)),
            world_matrix: Mat4::IDENTITY,
            skin_matrix: Mat4::IDENTITY,
        };

        hips.world_matrix = hips.compute_local_matrix();
        let head_world = hips.world_matrix * head.compute_local_matrix();
        assert!((head_world.w_axis.y - 1.4).abs() < 1e-5);

        let head_skin = head_world * head.inverse_bind_matrix;
        assert!(head_skin.abs_diff_eq(Mat4::IDENTITY, 1e-4));
    }
}
