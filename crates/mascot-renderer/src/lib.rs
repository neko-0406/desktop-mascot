//! mascot-renderer: wgpu-based avatar renderer for Desktop Mascot.

pub mod camera;
pub mod context;
pub mod error;
pub mod mesh;
pub mod pipeline;
pub mod renderer;

pub use camera::{Camera, CameraBuffer, CameraUniform};
pub use context::{RenderContext, DEPTH_FORMAT};
pub use error::RendererError;
pub use mesh::{create_cube_mesh, GpuMesh};
pub use pipeline::{BasicPipeline, ModelBuffer, ModelUniform, PREMULTIPLIED_ALPHA_BLEND};
pub use renderer::MascotRenderer;

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

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
        // 6 faces * 4 vertices = 24
        assert_eq!(cube.vertices.len(), 24);
        // 6 faces * 2 triangles * 3 indices = 36
        assert_eq!(cube.indices.len(), 36);
        assert_eq!(cube.submeshes.len(), 1);
        assert_eq!(cube.submeshes[0].index_count, 36);

        // Verify normal lengths
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
}

