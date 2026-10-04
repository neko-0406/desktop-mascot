use std::sync::Arc;
use glam::Mat4;
use mascot_format::DmaFile;
use wgpu::{
    Color, CommandEncoder, CommandEncoderDescriptor, CurrentSurfaceTexture, LoadOp, Operations,
    RenderPassColorAttachment, RenderPassDepthStencilAttachment, RenderPassDescriptor, StoreOp,
    TextureView, TextureViewDescriptor,
};
use winit::window::Window;

use crate::avatar::GpuAvatar;
use crate::camera::{Camera, CameraBuffer};
use crate::context::RenderContext;
use crate::error::RendererError;
use crate::mesh::GpuMesh;
use crate::pipeline::{BasicPipeline, LightUniform, LilToonPipeline, ModelBuffer, ModelUniform, SceneBuffer};

pub struct MascotRenderer<'w> {
    pub context: RenderContext<'w>,
    pub camera: Camera,
    pub camera_buffer: CameraBuffer,
    pub pipeline: BasicPipeline,
    pub model_buffer: ModelBuffer,
    pub scene_buffer: SceneBuffer,
    pub light_uniform: LightUniform,
    pub liltoon_pipeline: LilToonPipeline,
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
            glam::Vec3::new(0.0, 1.35, 2.5),
            glam::Vec3::new(0.0, 1.25, 0.0),
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

        let light_uniform = LightUniform::default();
        let scene_buffer = SceneBuffer::new(&context.device, &camera_uniform, &light_uniform);

        let liltoon_pipeline = LilToonPipeline::new(&context.device, context.config.format);

        Ok(Self {
            context,
            camera,
            camera_buffer,
            pipeline,
            model_buffer,
            scene_buffer,
            light_uniform,
            liltoon_pipeline,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.context.resize(width, height);
        self.camera.aspect = width as f32 / height.max(1) as f32;
        let uniform = self.camera.build_uniform();
        self.camera_buffer.update(&self.context.queue, &uniform);
        self.scene_buffer.update_camera(&self.context.queue, &uniform);
    }

    /// Loads a `.dma` avatar into GPU buffers.
    pub fn load_avatar(&self, dma: &DmaFile) -> Result<GpuAvatar, RendererError> {
        GpuAvatar::from_dma(
            &self.context.device,
            &self.context.queue,
            dma,
            &self.liltoon_pipeline.material_layout,
            &self.liltoon_pipeline.model_skeleton_layout,
        )
    }

    /// Loads a `.dma` model into GPU buffers (alias for load_avatar).
    pub fn load_dma_model(&self, dma: &DmaFile) -> Result<crate::avatar::DmaModel, RendererError> {
        self.load_avatar(dma)
    }

    /// Renders a `.dma` model with lilToon shading and inverted hull outline (alias for render_avatar).
    pub fn render_dma_model(
        &mut self,
        model: &mut crate::avatar::DmaModel,
        model_matrix: Mat4,
    ) -> Result<(), RendererError> {
        self.render_avatar(model, model_matrix)
    }

    /// Renders an avatar with lilToon shading and inverted hull outline passes.
    /// Begins a frame by acquiring the swapchain texture and creating an encoder and view.
    pub fn begin_frame(
        &mut self,
    ) -> Result<(wgpu::SurfaceTexture, TextureView, CommandEncoder), RendererError> {
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

        let encoder = self
            .context
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("Mascot Frame Encoder"),
            });

        Ok((output, view, encoder))
    }

    /// Renders an avatar into the provided texture view without presenting.
    pub fn render_avatar_to_pass(
        &mut self,
        avatar: &mut GpuAvatar,
        model_matrix: Mat4,
        view: &TextureView,
        encoder: &mut CommandEncoder,
    ) {
        // Update camera and light globals (Group 0)
        let camera_uniform = self.camera.build_uniform();
        self.scene_buffer.update_camera(&self.context.queue, &camera_uniform);
        self.scene_buffer.update_light(&self.context.queue, &self.light_uniform);

        // Update avatar transforms, bones, and morph targets
        avatar.update(&self.context.queue, model_matrix);

        let mut render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("Mascot Avatar Render Pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: Operations {
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

        render_pass.set_bind_group(0, &self.scene_buffer.bind_group, &[]);
        render_pass.set_bind_group(2, &avatar.model_bind_group, &[]);
        render_pass.set_vertex_buffer(0, avatar.mesh.vertex_buffer.slice(..));
        render_pass.set_index_buffer(avatar.mesh.index_buffer.slice(..), GpuMesh::INDEX_FORMAT);

        // Pass 1: Main Toon Shading
        render_pass.set_pipeline(&self.liltoon_pipeline.main_pipeline);
        if avatar.mesh.submeshes.is_empty() {
            let mat = avatar.materials.first().unwrap_or(&avatar.default_material);
            render_pass.set_bind_group(1, &mat.bind_group, &[]);
            render_pass.draw_indexed(0..avatar.mesh.index_count, 0, 0..1);
        } else {
            for submesh in &avatar.mesh.submeshes {
                let mat_idx = submesh.material_index as usize;
                let mat = avatar.materials.get(mat_idx).unwrap_or(&avatar.default_material);
                render_pass.set_bind_group(1, &mat.bind_group, &[]);
                render_pass.draw_indexed(
                    submesh.index_offset..(submesh.index_offset + submesh.index_count),
                    0,
                    0..1,
                );
            }
        }

        // Pass 2: Inverted Hull Outline
        render_pass.set_pipeline(&self.liltoon_pipeline.outline_pipeline);
        if avatar.mesh.submeshes.is_empty() {
            let mat = avatar.materials.first().unwrap_or(&avatar.default_material);
            if mat.uniform.outline_enable != 0 && mat.uniform.outline_width > 0.0 {
                render_pass.set_bind_group(1, &mat.bind_group, &[]);
                render_pass.draw_indexed(0..avatar.mesh.index_count, 0, 0..1);
            }
        } else {
            for submesh in &avatar.mesh.submeshes {
                let mat_idx = submesh.material_index as usize;
                let mat = avatar.materials.get(mat_idx).unwrap_or(&avatar.default_material);
                if mat.uniform.outline_enable != 0 && mat.uniform.outline_width > 0.0 {
                    render_pass.set_bind_group(1, &mat.bind_group, &[]);
                    render_pass.draw_indexed(
                        submesh.index_offset..(submesh.index_offset + submesh.index_count),
                        0,
                        0..1,
                    );
                }
            }
        }
    }

    /// Renders a basic fallback mesh into the provided texture view without presenting.
    pub fn render_mesh_to_pass(
        &mut self,
        mesh: &GpuMesh,
        model_matrix: Mat4,
        color: [f32; 4],
        view: &TextureView,
        encoder: &mut CommandEncoder,
    ) {
        let camera_uniform = self.camera.build_uniform();
        self.camera_buffer.update(&self.context.queue, &camera_uniform);

        let model_uniform = ModelUniform {
            model_matrix: model_matrix.to_cols_array_2d(),
            color,
        };
        self.model_buffer.update(&self.context.queue, &model_uniform);

        let mut render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("Mascot Fallback Render Pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: Operations {
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

    /// Clears the texture view to transparent without drawing 3D geometry.
    pub fn clear_to_pass(&self, view: &TextureView, encoder: &mut CommandEncoder) {
        let _ = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("Mascot Clear Pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::TRANSPARENT),
                    store: StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }

    /// Submits the command encoder and presents the swapchain texture.
    pub fn finish_frame(&mut self, output: wgpu::SurfaceTexture, encoder: CommandEncoder) {
        self.context.queue.submit(Some(encoder.finish()));
        self.context.queue.present(output);
    }

    /// Renders an avatar with lilToon shading and inverted hull outline passes.
    pub fn render_avatar(
        &mut self,
        avatar: &mut GpuAvatar,
        model_matrix: Mat4,
    ) -> Result<(), RendererError> {
        let (output, view, mut encoder) = self.begin_frame()?;
        self.render_avatar_to_pass(avatar, model_matrix, &view, &mut encoder);
        self.finish_frame(output, encoder);
        Ok(())
    }

    /// Render a basic mesh with transparent clear background (0, 0, 0, 0).
    pub fn render_mesh(
        &mut self,
        mesh: &GpuMesh,
        model_matrix: Mat4,
        color: [f32; 4],
    ) -> Result<(), RendererError> {
        let (output, view, mut encoder) = self.begin_frame()?;
        self.render_mesh_to_pass(mesh, model_matrix, color, &view, &mut encoder);
        self.finish_frame(output, encoder);
        Ok(())
    }
}
