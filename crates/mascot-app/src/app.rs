use std::sync::Arc;
use std::time::Instant;
use glam::{Mat4, Quat, Vec3};
use mascot_format::DmaFile;
use mascot_renderer::{GpuAvatar, GpuMesh, MascotRenderer};
use tracing::{error, info};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, KeyEvent, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId, WindowLevel};

use crate::hittest::{
    project_aabb_to_screen, project_world_to_screen, HitCircle, HitTarget, HitTester,
};
use crate::interaction::{InteractionController, InteractionMode};
use crate::platform::{
    configure_transparent_window, get_cursor_screen_pos, get_desktop_work_area,
    install_hit_test_subclass,
};
use crate::ui::{EguiOverlay, UiAction};

use mascot_physics::{
    BlinkController, BonePoseReader, BreathingController, LookAtController, PhysicsWorld,
};

const DIALOGUE_PHRASES: &[&str] = &[
    "こんにちは、マスター！\n今日も一緒に頑張りましょう！✨",
    "マスター、お疲れ様です！\n水分補給も忘れずにね🍵",
    "えへへ、いつもそばで応援していますよ♪",
    "キーボードのカタカタ音、心地いいですね💻",
    "少し肩の力を抜いて、深呼吸しましょう🌿",
    "マスターが集中している姿、とっても素敵です！",
];

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

    // Phase 5 Interaction & UI components
    hit_tester: Arc<HitTester>,
    interaction: InteractionController,
    egui_overlay: Option<EguiOverlay>,
    cursor_pos: (f32, f32),
    cursor_screen_start: (i32, i32),
    window_pos_start: (i32, i32),
    dialogue_index: usize,
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
            hit_tester: Arc::new(HitTester::new()),
            interaction: InteractionController::new(),
            egui_overlay: None,
            cursor_pos: (250.0, 300.0),
            cursor_screen_start: (0, 0),
            window_pos_start: (0, 0),
            dialogue_index: 0,
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

        // Install Win32 WM_NCHITTEST subclassing for dynamic click-through
        install_hit_test_subclass(&window, self.hit_tester.clone());

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

        // Initialize egui overlay for speech bubble & context menu
        let egui_overlay = EguiOverlay::new(
            &window,
            &renderer.context.device,
            renderer.context.config.format,
        );

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

        // Position window at bottom-right of work area initially
        let work_area = get_desktop_work_area(Some(&window));
        let initial_x = (work_area.right - (size.width as i32) - 80).max(work_area.left);
        let initial_y = (work_area.bottom - (size.height as i32)).max(work_area.top);
        window.set_outer_position(PhysicalPosition::new(initial_x, initial_y));
        self.interaction.current_y = initial_y as f32;
        self.interaction.ground_y = initial_y as f32;

        self.avatar = avatar;
        self.physics_world = physics_world;
        self.mesh = Some(mesh);
        self.renderer = Some(renderer);
        self.egui_overlay = Some(egui_overlay);
        self.window = Some(window.clone());
        self.last_frame_time = Instant::now();

        info!("Mascot runtime window initialized successfully. Right-click mascot for menu.");
        window.request_redraw();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        // Forward event to egui overlay first
        let mut egui_consumed = false;
        if let (Some(overlay), Some(window)) = (&mut self.egui_overlay, &self.window) {
            egui_consumed = overlay.handle_window_event(window, &event);
        }

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
                self.cursor_pos = (position.x as f32, position.y as f32);
                self.look_at.set_cursor_window_coords(
                    position.x as f32,
                    position.y as f32,
                    self.window_size.0,
                    self.window_size.1,
                );

                if self.interaction.mode == InteractionMode::Dragging {
                    // Update dragged window position using global cursor deltas
                    if let Some(window) = &self.window {
                        let cur_screen = get_cursor_screen_pos();
                        let dx = cur_screen.0 - self.cursor_screen_start.0;
                        let dy = cur_screen.1 - self.cursor_screen_start.1;
                        let new_x = self.window_pos_start.0 + dx;
                        let new_y = self.window_pos_start.1 + dy;
                        window.set_outer_position(PhysicalPosition::new(new_x, new_y));
                        self.interaction.current_y = new_y as f32;
                    }
                } else {
                    // Evaluate head stroking / petting detection
                    let target = self.hit_tester.hit_test(self.cursor_pos.0, self.cursor_pos.1);
                    if target == HitTarget::Head {
                        self.interaction
                            .petting
                            .record_cursor(self.cursor_pos, Instant::now());
                    } else {
                        self.interaction.petting.cursor_left();
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if egui_consumed {
                    return;
                }

                let target = self.hit_tester.hit_test(self.cursor_pos.0, self.cursor_pos.1);

                match button {
                    MouseButton::Left => {
                        match state {
                            ElementState::Pressed => {
                                if target == HitTarget::Head || target == HitTarget::Body {
                                    if let Some(overlay) = &mut self.egui_overlay {
                                        overlay.close_context_menu();
                                    }
                                    if let Some(window) = &self.window {
                                        let outer_pos =
                                            window.outer_position().unwrap_or(PhysicalPosition::new(0, 0));
                                        self.cursor_screen_start = get_cursor_screen_pos();
                                        self.window_pos_start = (outer_pos.x, outer_pos.y);
                                        self.interaction.start_drag(
                                            (self.cursor_screen_start.0 as f64, self.cursor_screen_start.1 as f64),
                                            self.window_pos_start,
                                        );
                                    }
                                }
                            }
                            ElementState::Released => {
                                if self.interaction.mode == InteractionMode::Dragging {
                                    if let Some(window) = &self.window {
                                        let work_area = get_desktop_work_area(Some(window));
                                        let ground_y = work_area.bottom - (self.window_size.1 as i32);
                                        let cur_y =
                                            window.outer_position().unwrap_or(PhysicalPosition::new(0, 0)).y;
                                        self.interaction.end_drag(cur_y, ground_y);
                                    }
                                }
                            }
                        }
                    }
                    MouseButton::Right => {
                        if state == ElementState::Pressed && target != HitTarget::None {
                            if let Some(overlay) = &mut self.egui_overlay {
                                overlay.open_context_menu(glam::Vec2::new(
                                    self.cursor_pos.0,
                                    self.cursor_pos.1,
                                ));
                            }
                        }
                    }
                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => {
                if let (Some(renderer), Some(window)) = (&mut self.renderer, &self.window) {
                    let now = Instant::now();
                    let dt = (now - self.last_frame_time).as_secs_f32().clamp(0.001, 0.05);
                    self.last_frame_time = now;
                    let elapsed = self.start_time.elapsed().as_secs_f32();

                    // Advance interaction physics & gravity fall
                    let work_area = get_desktop_work_area(Some(window));
                    let ground_y = work_area.bottom - (self.window_size.1 as i32);
                    if let Some(new_y) = self.interaction.update_physics(dt, ground_y) {
                        let cur_x =
                            window.outer_position().unwrap_or(PhysicalPosition::new(0, 0)).x;
                        window.set_outer_position(PhysicalPosition::new(cur_x, new_y));
                    }

                    // Handle UI actions triggered from context menu
                    if let Some(overlay) = &mut self.egui_overlay {
                        match overlay.ui_action {
                            UiAction::ResetPosition => {
                                let new_x =
                                    work_area.left + (work_area.width() - self.window_size.0 as i32) / 2;
                                let new_y = work_area.bottom - (self.window_size.1 as i32);
                                window.set_outer_position(PhysicalPosition::new(new_x, new_y));
                                self.interaction.mode = InteractionMode::Idle;
                                self.interaction.velocity_y = 0.0;
                                self.interaction.current_y = new_y as f32;
                            }
                            UiAction::SayGreeting => {
                                self.dialogue_index =
                                    (self.dialogue_index + 1) % DIALOGUE_PHRASES.len();
                                overlay
                                    .speech_bubble
                                    .set_text(DIALOGUE_PHRASES[self.dialogue_index]);
                            }
                            UiAction::ExitApplication => {
                                info!("Exit triggered from context menu.");
                                event_loop.exit();
                                return;
                            }
                            UiAction::None => {}
                        }
                    }

                    // Model transformations: scale, idle sway, and landing squash/stretch
                    let scale_val = self.interaction.scale;
                    let squash_y = self.interaction.squash.vertical_scale() * scale_val;
                    let squash_xz = self.interaction.squash.horizontal_scale() * scale_val;
                    let squash_mat = Mat4::from_scale(Vec3::new(squash_xz, squash_y, squash_xz));

                    let model_matrix = squash_mat * Mat4::from_rotation_y((elapsed * 0.4).sin() * 0.15);
                    let vp = renderer.camera.build_view_projection_matrix();

                    let head_screen_pos = if let Some(avatar) = &mut self.avatar {
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

                        // (c) Lift / Pinched Pose: legs dangle back, arms hang down
                        let lift = self.interaction.lift_weight;
                        if lift > 0.001 {
                            if let Some(l_leg) = avatar.skeleton.find_bone_index("LeftUpperLeg") {
                                avatar
                                    .skeleton
                                    .set_bone_local_rotation(l_leg, Quat::from_rotation_x(0.22 * lift));
                            }
                            if let Some(r_leg) = avatar.skeleton.find_bone_index("RightUpperLeg") {
                                avatar
                                    .skeleton
                                    .set_bone_local_rotation(r_leg, Quat::from_rotation_x(0.18 * lift));
                            }
                            if let Some(l_arm) = avatar.skeleton.find_bone_index("LeftUpperArm") {
                                avatar
                                    .skeleton
                                    .set_bone_local_rotation(l_arm, Quat::from_rotation_z(0.3 * lift));
                            }
                            if let Some(r_arm) = avatar.skeleton.find_bone_index("RightUpperArm") {
                                avatar
                                    .skeleton
                                    .set_bone_local_rotation(r_arm, Quat::from_rotation_z(-0.3 * lift));
                            }
                            avatar.morph_controller.set_weight_by_name("vrc.surprised", lift * 0.4);
                        }

                        // Recompute world transforms for kinematic bones before physics step
                        avatar.skeleton.compute_world_transforms();

                        // 2. PhysBone Physics Simulation with external tension from lift & petting
                        if let Some(physics_world) = &mut self.physics_world {
                            let pose_view = SkeletonPoseView(&avatar.skeleton.bones);
                            let external_accel = self.interaction.external_acceleration();
                            let updates = physics_world.step(dt, &pose_view, external_accel);
                            for update in updates {
                                avatar.skeleton.set_bone_local_rotation(
                                    update.bone_index,
                                    update.local_rotation,
                                );
                            }
                        }

                        // 3. Morph Target Animation (Blink, Petting Happiness Smile & Blush)
                        let mut blink_weight = self.blinking.update(dt);
                        let happy = self.interaction.petting.happiness;
                        if happy > 0.001 {
                            // Petting causes joyful blush, smile, and squinting eyes
                            avatar.morph_controller.set_weight_by_name("blush", happy);
                            avatar.morph_controller.set_weight_by_name("vrc.happy", happy * 0.9);
                            avatar.morph_controller.set_weight_by_name("smile", happy * 0.9);
                            blink_weight = blink_weight.max(happy * 0.55);
                        } else {
                            avatar.morph_controller.set_weight_by_name("blush", 0.0);
                            avatar.morph_controller.set_weight_by_name("vrc.happy", 0.0);
                            avatar.morph_controller.set_weight_by_name("smile", 0.0);
                        }
                        avatar.morph_controller.set_weight_by_name("vrc.blink", blink_weight);

                        // Extract 3D head position for screen projection
                        let head_local = if let Some(head_idx) = avatar.skeleton.find_bone_index("Head") {
                            avatar.skeleton.bones[head_idx].world_matrix.w_axis.truncate()
                        } else {
                            Vec3::new(0.0, 1.35, 0.0)
                        };
                        let head_world = model_matrix.transform_point3(head_local);
                        project_world_to_screen(head_world, vp, self.window_size.0, self.window_size.1)
                    } else {
                        // Fallback cube head position (top of cube)
                        let head_world = model_matrix.transform_point3(Vec3::new(0.0, 0.45, 0.0));
                        project_world_to_screen(head_world, vp, self.window_size.0, self.window_size.1)
                    };

                    // Compute 2D Screen Hit Bounds for Mascot
                    let head_circle = head_screen_pos.map(|p| HitCircle::new(p, 48.0 * scale_val));
                    let body_rect = if self.avatar.is_some() {
                        let min_pt = Vec3::new(-0.35, 0.0, -0.25);
                        let max_pt = Vec3::new(0.35, 1.55, 0.25);
                        project_aabb_to_screen(min_pt, max_pt, vp * model_matrix, self.window_size.0, self.window_size.1)
                    } else {
                        let min_pt = Vec3::splat(-0.45);
                        let max_pt = Vec3::splat(0.45);
                        project_aabb_to_screen(min_pt, max_pt, vp * model_matrix, self.window_size.0, self.window_size.1)
                    };

                    // Render 3D Model and Egui UI Overlay in a single frame
                    let mut ui_rects = Vec::new();
                    match renderer.begin_frame() {
                        Ok((output, view, mut encoder)) => {
                            if let Some(avatar) = &mut self.avatar {
                                renderer.render_avatar_to_pass(avatar, model_matrix, &view, &mut encoder);
                            } else if let Some(mesh) = &self.mesh {
                                let color = [0.35, 0.65, 0.95, 0.85];
                                renderer.render_mesh_to_pass(mesh, model_matrix, color, &view, &mut encoder);
                            } else {
                                renderer.clear_to_pass(&view, &mut encoder);
                            }

                            if let Some(overlay) = &mut self.egui_overlay {
                                ui_rects = overlay.update_and_render(
                                    window,
                                    &renderer.context.device,
                                    &renderer.context.queue,
                                    &mut encoder,
                                    &view,
                                    head_screen_pos,
                                    self.window_size,
                                    &mut self.interaction.gravity_enabled,
                                    &mut self.interaction.scale,
                                    dt,
                                );
                            }

                            renderer.finish_frame(output, encoder);
                        }
                        Err(e) => {
                            error!("Frame begin error: {:?}", e);
                        }
                    }

                    // Update hit tester with latest head, body, and UI rects
                    self.hit_tester.update_bounds(
                        head_circle,
                        body_rect,
                        ui_rects,
                    );

                    // Schedule next frame
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}
