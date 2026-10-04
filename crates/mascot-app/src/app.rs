use std::sync::Arc;
use std::time::Instant;
use glam::{Mat4, Vec3};
use mascot_format::DmaFile;
use mascot_renderer::{GpuAvatar, GpuMesh, MascotRenderer};
use tracing::{error, info};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId, WindowLevel};

use crate::platform::configure_transparent_window;

use mascot_physics::{
    BlinkController, BonePoseReader, BreathingController, LookAtController, PhysicsWorld,
};

/// Adapter to allow PhysicsWorld to inspect GpuSkeleton bones without coupling crates.
struct SkeletonPoseView<'a>(&'a [mascot_renderer::BoneNode]);

impl<'a> BonePoseReader for SkeletonPoseView<'a> {
    fn bone_count(&self) -> usize {
        self.0.len()
    }

    fn parent_index(&self, index: usize) -> i32 {
        self.0.get(index).map(|b| b.parent_index).unwrap_or(-1)
    }

    fn world_position(&self, index: usize) -> glam::Vec3 {
        self.0
            .get(index)
            .map(|b| b.world_matrix.w_axis.truncate())
            .unwrap_or(glam::Vec3::ZERO)
    }

    fn world_rotation(&self, index: usize) -> glam::Quat {
        self.0
            .get(index)
            .map(|b| glam::Quat::from_mat4(&b.world_matrix))
            .unwrap_or(glam::Quat::IDENTITY)
    }
}

pub struct MascotApp {
    window: Option<Arc<Window>>,
    renderer: Option<MascotRenderer<'static>>,
    mesh: Option<GpuMesh>,
    avatar: Option<GpuAvatar>,
    physics_world: Option<PhysicsWorld>,
    breathing: BreathingController,
    blinking: BlinkController,
    look_at: LookAtController,
    start_time: Instant,
    last_frame_time: Instant,
    window_size: (f32, f32),
}

impl Default for MascotApp {
    fn default() -> Self {
        Self {
            window: None,
            renderer: None,
            mesh: None,
            avatar: None,
            physics_world: None,
            breathing: BreathingController::default(),
            blinking: BlinkController::new(42),
            look_at: LookAtController::default(),
            start_time: Instant::now(),
            last_frame_time: Instant::now(),
            window_size: (500.0, 600.0),
        }
    }
}

impl ApplicationHandler for MascotApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        info!("Creating borderless, transparent, topmost mascot window...");

        let window_attributes = WindowAttributes::default()
            .with_title("Desktop Mascot Runtime")
            .with_inner_size(LogicalSize::new(500.0, 600.0))
            .with_decorations(false)
            .with_transparent(true)
            .with_window_level(WindowLevel::AlwaysOnTop);

        let window = match event_loop.create_window(window_attributes) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                error!("Failed to create window: {:?}", e);
                event_loop.exit();
                return;
            }
        };

        // Apply Win32 transparency (WS_EX_LAYERED, WS_EX_TOPMOST, DWM margins -1)
        configure_transparent_window(&window);

        let size = window.inner_size();
        info!(
            "Initializing WGPU Renderer ({}x{}, PreMultiplied Alpha)...",
            size.width, size.height
        );

        self.window_size = (size.width as f32, size.height as f32);

        let renderer = match pollster::block_on(MascotRenderer::new(
            window.clone(),
            size.width,
            size.height,
        )) {
            Ok(r) => r,
            Err(e) => {
                error!("Failed to initialize WGPU renderer: {:?}", e);
                event_loop.exit();
                return;
            }
        };

        // Attempt to load sample.dma if present
        let dma_path = "assets/sample.dma";
        let (avatar, physics_world) = match std::fs::read(dma_path) {
            Ok(bytes) => match DmaFile::from_bytes(&bytes) {
                Ok(dma) => match renderer.load_avatar(&dma) {
                    Ok(av) => {
                        info!(
                            "Loaded avatar from '{}': {} bones, {} morphs, {} submeshes, {} phys chains, {} colliders",
                            dma_path,
                            av.skeleton.bones.len(),
                            av.morph_controller.targets.len(),
                            av.mesh.submeshes.len(),
                            dma.phys_chains.len(),
                            dma.colliders.len()
                        );
                        let world = if !dma.phys_chains.is_empty() || !dma.colliders.is_empty() {
                            Some(PhysicsWorld::from_format(
                                &dma.phys_chains,
                                &dma.colliders,
                                &dma.skeleton,
                            ))
                        } else {
                            None
                        };
                        (Some(av), world)
                    }
                    Err(e) => {
                        error!("Failed to create GPU avatar from '{}': {:?}", dma_path, e);
                        (None, None)
                    }
                },
                Err(e) => {
                    error!("Failed to parse DMA file '{}': {:?}", dma_path, e);
                    (None, None)
                }
            },
            Err(_) => {
                info!("'{}' not found, falling back to basic cube.", dma_path);
                (None, None)
            }
        };

        // Create standard test mesh (transparent colored cube) as fallback
        let mesh = GpuMesh::create_cube(&renderer.context.device, 0.9);

        self.avatar = avatar;
        self.physics_world = physics_world;
        self.mesh = Some(mesh);
        self.renderer = Some(renderer);
        self.window = Some(window.clone());
        self.last_frame_time = Instant::now();

        info!("Mascot runtime window initialized successfully. Press ESC to quit.");
        window.request_redraw();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                info!("Close requested, exiting event loop.");
                event_loop.exit();
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(KeyCode::Escape),
                        state: ElementState::Pressed,
                        ..
                    },
                ..
            } => {
                info!("ESC pressed, exiting event loop.");
                event_loop.exit();
            }
            WindowEvent::Resized(new_size) => {
                self.window_size = (new_size.width as f32, new_size.height as f32);
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(new_size.width, new_size.height);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.look_at.set_cursor_window_coords(
                    position.x as f32,
                    position.y as f32,
                    self.window_size.0,
                    self.window_size.1,
                );
            }
            WindowEvent::RedrawRequested => {
                if let (Some(renderer), Some(window)) = (&mut self.renderer, &self.window) {
                    let now = Instant::now();
                    let dt = (now - self.last_frame_time).as_secs_f32().clamp(0.001, 0.05);
                    self.last_frame_time = now;
                    let elapsed = self.start_time.elapsed().as_secs_f32();

                    if let Some(avatar) = &mut self.avatar {
                        // 1. Procedural Base Motion:
                        // (a) Breathing motion on Chest and Spine
                        let breath = self.breathing.update(elapsed);
                        if let Some(chest_idx) = avatar.skeleton.find_bone_index("Chest") {
                            avatar.skeleton.set_bone_local_rotation(chest_idx, breath.chest_rotation);
                        }
                        if let Some(spine_idx) = avatar.skeleton.find_bone_index("Spine") {
                            avatar.skeleton.set_bone_local_rotation(spine_idx, breath.spine_rotation);
                        }

                        // (b) Look-At IK targeting mouse cursor (65% Head, 35% Neck)
                        let look = self.look_at.update(dt);
                        if let Some(neck_idx) = avatar.skeleton.find_bone_index("Neck") {
                            avatar.skeleton.set_bone_local_rotation(neck_idx, look.neck_rotation);
                        }
                        if let Some(head_idx) = avatar.skeleton.find_bone_index("Head") {
                            avatar.skeleton.set_bone_local_rotation(head_idx, look.head_rotation);
                        }

                        // Recompute world transforms for kinematic bones before physics step
                        avatar.skeleton.compute_world_transforms();

                        // 2. PhysBone Physics Simulation:
                        // Simulates secondary swaying bones (hair, accessories, etc.)
                        if let Some(physics_world) = &mut self.physics_world {
                            let pose_view = SkeletonPoseView(&avatar.skeleton.bones);
                            let updates = physics_world.step(dt, &pose_view, Vec3::ZERO);
                            for update in updates {
                                avatar.skeleton.set_bone_local_rotation(
                                    update.bone_index,
                                    update.local_rotation,
                                );
                            }
                        }

                        // 3. Morph Target Animation:
                        // Stochastic natural blinking
                        let blink_weight = self.blinking.update(dt);
                        avatar.morph_controller.set_weight_by_name("vrc.blink", blink_weight);

                        // 4. Model Placement & Subtle Idle Body Sway
                        let model_matrix = Mat4::from_rotation_y((elapsed * 0.4).sin() * 0.15);

                        if let Err(e) = renderer.render_avatar(avatar, model_matrix) {
                            error!("Avatar render error: {:?}", e);
                        }
                    } else if let Some(mesh) = &self.mesh {
                        // Phase 2 fallback: rotating translucent cube
                        let model_matrix = Mat4::from_rotation_y(elapsed * 1.3)
                            * Mat4::from_rotation_x(elapsed * 0.7);
                        let color = [0.35, 0.65, 0.95, 0.85];

                        if let Err(e) = renderer.render_mesh(mesh, model_matrix, color) {
                            error!("Render error: {:?}", e);
                        }
                    }

                    // Schedule next frame
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}
