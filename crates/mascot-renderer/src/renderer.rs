use std::sync::Arc;
use glam::Mat4;
use wgpu::{
    Color, CommandEncoderDescriptor, CurrentSurfaceTexture, LoadOp, Operations,
    RenderPassColorAttachment, RenderPassDepthStencilAttachment, RenderPassDescriptor,
    StoreOp, TextureViewDescriptor,
};
use winit::window::Window;

use crate::camera::{Camera, CameraBuffer};
use crate::context::RenderContext;
use crate::error::RendererError;
use crate::mesh::GpuMesh;
use crate::pipeline::{BasicPipeline, ModelBuffer, ModelUniform};

pub struct MascotRenderer<'w> {
    pub context: RenderContext<'w>,
    pub camera: Camera,
    pub camera_buffer: CameraBuffer,
    pub pipeline: BasicPipeline,
    pub model_buffer: ModelBuffer,
}

impl<'w> MascotRenderer<'w> {
    pub async fn new(
        window: Arc<Window>,
        width: u32,
        height: u32,
    ) -> Result<Self, RendererError> {
        let context = RenderContext::new(window, width, height).await?;

        let aspect = width as f32 / height.max(1) as f32;
        let camera = Camera::new(
            glam::Vec3::new(0.0, 1.0, 2.5),
            glam::Vec3::new(0.0, 0.5, 0.0),
            aspect,
        );

        let camera_uniform = camera.build_uniform();
        let camera_buffer = CameraBuffer::new(&context.device, &camera_uniform);

        let pipeline = BasicPipeline::new(
            &context.device,
            context.config.format,
            &camera_buffer.bind_group_layout,
        );

        let model_buffer = ModelBuffer::new(
            &context.device,
            &pipeline.model_layout,
            &ModelUniform::default(),
        );

        Ok(Self {
            context,
            camera,
            camera_buffer,
            pipeline,
            model_buffer,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.context.resize(width, height);
        self.camera.aspect = width as f32 / height.max(1) as f32;
        let uniform = self.camera.build_uniform();
        self.camera_buffer.update(&self.context.queue, &uniform);
    }

    /// Render a mesh with transparent clear background (0, 0, 0, 0).
    pub fn render_mesh(
        &mut self,
        mesh: &GpuMesh,
        model_matrix: Mat4,
        color: [f32; 4],
    ) -> Result<(), RendererError> {
        let output = match self.context.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(texture) | CurrentSurfaceTexture::Suboptimal(texture) => {
                texture
            }
            status => {
                return Err(RendererError::SurfaceError(format!(
                    "Failed to acquire swapchain texture: {status:?}"
                )));
            }
        };

        let view = output.texture.create_view(&TextureViewDescriptor::default());

        // Update uniforms
        let camera_uniform = self.camera.build_uniform();
        self.camera_buffer.update(&self.context.queue, &camera_uniform);

        let model_uniform = ModelUniform {
            model_matrix: model_matrix.to_cols_array_2d(),
            color,
        };
        self.model_buffer.update(&self.context.queue, &model_uniform);

        let mut encoder = self
            .context
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("Mascot Render Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("Mascot Render Pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: Operations {
                        // Completely transparent background for OS desktop blending
                        load: LoadOp::Clear(Color::TRANSPARENT),
                        store: StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                    view: &self.context.depth_view,
                    depth_ops: Some(Operations {
                        load: LoadOp::Clear(1.0),
                        store: StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            render_pass.set_pipeline(&self.pipeline.pipeline);
            render_pass.set_bind_group(0, &self.camera_buffer.bind_group, &[]);
            render_pass.set_bind_group(1, &self.model_buffer.bind_group, &[]);
            render_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
            render_pass.set_index_buffer(mesh.index_buffer.slice(..), GpuMesh::INDEX_FORMAT);

            if mesh.submeshes.is_empty() {
                render_pass.draw_indexed(0..mesh.index_count, 0, 0..1);
            } else {
                for submesh in &mesh.submeshes {
                    render_pass.draw_indexed(
                        submesh.index_offset..(submesh.index_offset + submesh.index_count),
                        0,
                        0..1,
                    );
                }
            }
        }

        self.context.queue.submit(Some(encoder.finish()));
        self.context.queue.present(output);

        Ok(())
    }
}

