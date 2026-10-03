use std::sync::Arc;
use std::time::Instant;
use glam::Mat4;
use mascot_renderer::{GpuMesh, MascotRenderer};
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
    start_time: Instant,
}

impl Default for MascotApp {
    fn default() -> Self {
        Self {
            window: None,
            renderer: None,
            mesh: None,
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

        // Create standard test mesh (transparent colored cube)
        let mesh = GpuMesh::create_cube(&renderer.context.device, 0.9);

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
                if let (Some(renderer), Some(mesh), Some(window)) =
                    (&mut self.renderer, &self.mesh, &self.window)
                {
                    let elapsed = self.start_time.elapsed().as_secs_f32();

                    // Smooth rotation over time
                    let model_matrix = Mat4::from_rotation_y(elapsed * 1.3)
                        * Mat4::from_rotation_x(elapsed * 0.7);

                    // Translucent light-blue color (RGBA = 0.35, 0.65, 0.95, 0.85)
                    let color = [0.35, 0.65, 0.95, 0.85];

                    if let Err(e) = renderer.render_mesh(mesh, model_matrix, color) {
                        error!("Render error: {:?}", e);
                    }

                    // Schedule next frame
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}
