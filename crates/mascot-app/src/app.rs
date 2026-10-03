use std::sync::Arc;
use std::time::Instant;
use glam::{Mat4, Quat};
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

pub struct MascotApp {
    window: Option<Arc<Window>>,
    renderer: Option<MascotRenderer<'static>>,
    mesh: Option<GpuMesh>,
    avatar: Option<GpuAvatar>,
    start_time: Instant,
}

impl Default for MascotApp {
    fn default() -> Self {
        Self {
            window: None,
            renderer: None,
            mesh: None,
            avatar: None,
            start_time: Instant::now(),
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
        let avatar = match std::fs::read(dma_path) {
            Ok(bytes) => match DmaFile::from_bytes(&bytes) {
                Ok(dma) => match renderer.load_avatar(&dma) {
                    Ok(av) => {
                        info!(
                            "Loaded avatar from '{}': {} bones, {} morphs, {} submeshes",
                            dma_path,
                            av.skeleton.bones.len(),
                            av.morph_controller.targets.len(),
                            av.mesh.submeshes.len()
                        );
                        Some(av)
                    }
                    Err(e) => {
                        error!("Failed to create GPU avatar from '{}': {:?}", dma_path, e);
                        None
                    }
                },
                Err(e) => {
                    error!("Failed to parse DMA file '{}': {:?}", dma_path, e);
                    None
                }
            },
            Err(_) => {
                info!("'{}' not found, falling back to basic cube.", dma_path);
                None
            }
        };

        // Create standard test mesh (transparent colored cube) as fallback
        let mesh = GpuMesh::create_cube(&renderer.context.device, 0.9);

        self.avatar = avatar;
        self.mesh = Some(mesh);
        self.renderer = Some(renderer);
        self.window = Some(window.clone());

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
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(new_size.width, new_size.height);
                }
            }
            WindowEvent::RedrawRequested => {
                if let (Some(renderer), Some(window)) = (&mut self.renderer, &self.window) {
                    let elapsed = self.start_time.elapsed().as_secs_f32();

                    if let Some(avatar) = &mut self.avatar {
                        // 1. Procedural bone animation:
                        // Gentle sway of hair bone
                        if let Some(hair_idx) = avatar.skeleton.find_bone_index("Hair_Back_01") {
                            let sway_z = (elapsed * 2.5).sin() * 0.25;
                            let sway_x = (elapsed * 3.0).cos() * 0.15;
                            let rot = Quat::from_rotation_z(sway_z) * Quat::from_rotation_x(sway_x);
                            avatar.skeleton.set_bone_local_rotation(hair_idx, rot);
                        }

                        // Gentle nodding of head bone
                        if let Some(head_idx) = avatar.skeleton.find_bone_index("Head") {
                            let nod_x = (elapsed * 1.2).sin() * 0.08;
                            let rot = Quat::from_rotation_x(nod_x);
                            avatar.skeleton.set_bone_local_rotation(head_idx, rot);
                        }

                        // 2. Morph target animation:
                        // Natural periodic blinking (e.g. quick blink every 3 seconds)
                        let blink_cycle = elapsed % 3.0;
                        let blink_weight = if blink_cycle < 0.2 {
                            (blink_cycle / 0.2 * std::f32::consts::PI).sin()
                        } else {
                            0.0
                        };
                        avatar.morph_controller.set_weight_by_name("vrc.blink", blink_weight);

                        // 3. Model placement & slight idle turn
                        let model_matrix = Mat4::from_rotation_y((elapsed * 0.6).sin() * 0.2);

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
