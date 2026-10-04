use std::sync::Arc;
use egui::{
    Color32, Context, FontId, Frame, Margin, Pos2, Stroke, Window as EguiWindow,
};
use egui_wgpu::{Renderer as EguiWgpuRenderer, RendererOptions, ScreenDescriptor};
use egui_winit::State as EguiWinitState;
use glam::Vec2 as GlamVec2;
use winit::event::WindowEvent;
use winit::window::Window;

use crate::hittest::UiRect;

/// Manages speech bubble dialogue and typewriter effect.
pub struct SpeechBubble {
    pub visible: bool,
    pub full_text: String,
    pub displayed_chars: usize,
    char_timer: f32,
    pub chars_per_sec: f32,
    pub current_rect: Option<UiRect>,
}

impl Default for SpeechBubble {
    fn default() -> Self {
        Self {
            visible: true,
            full_text: "こんにちは、マスター！\n今日も一緒に頑張りましょう！✨".to_string(),
            displayed_chars: 0,
            char_timer: 0.0,
            chars_per_sec: 24.0,
            current_rect: None,
        }
    }
}

impl SpeechBubble {
    #[allow(dead_code)]
    pub fn new(text: &str) -> Self {
        Self {
            full_text: text.to_string(),
            ..Default::default()
        }
    }

    pub fn set_text(&mut self, text: &str) {
        self.full_text = text.to_string();
        self.displayed_chars = 0;
        self.char_timer = 0.0;
        self.visible = true;
    }

    pub fn update(&mut self, dt: f32) {
        if !self.visible {
            return;
        }
        let total_chars = self.full_text.chars().count();
        if self.displayed_chars < total_chars {
            self.char_timer += dt * self.chars_per_sec;
            let advance = self.char_timer.floor() as usize;
            if advance > 0 {
                self.displayed_chars = (self.displayed_chars + advance).min(total_chars);
                self.char_timer -= advance as f32;
            }
        }
    }

    pub fn visible_text(&self) -> String {
        self.full_text.chars().take(self.displayed_chars).collect()
    }

    pub fn is_typing(&self) -> bool {
        self.displayed_chars < self.full_text.chars().count()
    }
}

/// Commands emitted by the egui context menu to the mascot application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiAction {
    None,
    ResetPosition,
    SayGreeting,
    ExitApplication,
}

/// Manages egui state, speech bubble, and right-click context menu.
pub struct EguiOverlay {
    pub ctx: Context,
    pub winit_state: EguiWinitState,
    pub wgpu_renderer: EguiWgpuRenderer,
    pub speech_bubble: SpeechBubble,
    pub show_context_menu: bool,
    pub context_menu_pos: GlamVec2,
    pub context_menu_rect: Option<UiRect>,
    pub ui_action: UiAction,
}

impl EguiOverlay {
    pub fn new(
        window: &Arc<Window>,
        device: &wgpu::Device,
        output_format: wgpu::TextureFormat,
    ) -> Self {
        let ctx = Context::default();

        let winit_state = EguiWinitState::new(
            ctx.clone(),
            ctx.viewport_id(),
            window.as_ref(),
            Some(window.scale_factor() as f32),
            None,
            Some(device.limits().max_texture_dimension_2d as usize),
        );

        let wgpu_renderer = EguiWgpuRenderer::new(
            device,
            output_format,
            RendererOptions {
                msaa_samples: 1,
                depth_stencil_format: None,
                dithering: false,
                predictable_texture_filtering: false,
            },
        );

        Self {
            ctx,
            winit_state,
            wgpu_renderer,
            speech_bubble: SpeechBubble::default(),
            show_context_menu: false,
            context_menu_pos: GlamVec2::ZERO,
            context_menu_rect: None,
            ui_action: UiAction::None,
        }
    }

    /// Handles winit window events. Returns true if egui consumed the event.
    pub fn handle_window_event(&mut self, window: &Window, event: &WindowEvent) -> bool {
        let response = self.winit_state.on_window_event(window, event);
        response.consumed
    }

    /// Opens context menu at the given window client coordinates.
    pub fn open_context_menu(&mut self, pos: GlamVec2) {
        self.show_context_menu = true;
        self.context_menu_pos = pos;
    }

    /// Closes context menu.
    pub fn close_context_menu(&mut self) {
        self.show_context_menu = false;
        self.context_menu_rect = None;
    }

    /// Renders egui UI pass and returns bounding boxes of interactive UI elements.
    #[allow(clippy::too_many_arguments)]
    pub fn update_and_render(
        &mut self,
        window: &Window,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        head_screen_pos: Option<GlamVec2>,
        window_size: (f32, f32),
        gravity_enabled: &mut bool,
        scale: &mut f32,
        dt: f32,
    ) -> Vec<UiRect> {
        self.speech_bubble.update(dt);
        self.ui_action = UiAction::None;

        let raw_input = self.winit_state.take_egui_input(window);
        let mut bubble_rect_out = None;
        let mut menu_rect_out = None;

        let show_menu = self.show_context_menu;
        let menu_pos = self.context_menu_pos;

        let bubble_visible = self.speech_bubble.visible;
        let bubble_text = self.speech_bubble.visible_text();
        let bubble_typing = self.speech_bubble.is_typing();

        let mut next_ui_action = UiAction::None;

        let full_output = self.ctx.run_ui(raw_input, |ui| {
            let ctx = ui.ctx().clone();

            // 1. Speech Bubble (Floating above Head)
            if bubble_visible {
                let default_pos = if let Some(head) = head_screen_pos {
                    // Position 75px above head, with automatic flip if near top of window
                    let mut y = head.y - 75.0;
                    if y < 15.0 {
                        y = head.y + 45.0; // Flip below head
                    }
                    let x = (head.x - 120.0).clamp(10.0, (window_size.0 - 250.0).max(10.0));
                    Pos2::new(x, y)
                } else {
                    Pos2::new(window_size.0 * 0.5 - 110.0, 30.0)
                };

                let window_resp = EguiWindow::new("speech_bubble")
                    .fixed_pos(default_pos)
                    .title_bar(false)
                    .resizable(false)
                    .frame(
                        Frame::NONE
                            .corner_radius(12.0)
                            .inner_margin(Margin::same(10))
                            .fill(Color32::from_rgba_premultiplied(15, 20, 30, 235))
                            .stroke(Stroke::new(1.0, Color32::from_rgb(130, 180, 255))),
                    )
                    .show(&ctx, |ui| {
                        ui.set_max_width(220.0);
                        let typing_cursor = if bubble_typing {
                            " ▍"
                        } else {
                            ""
                        };
                        ui.label(
                            egui::RichText::new(format!("{}{}", bubble_text, typing_cursor))
                                .font(FontId::proportional(13.5))
                                .color(Color32::from_rgb(240, 245, 255)),
                        );
                    });

                if let Some(resp) = window_resp {
                    let r = resp.response.rect;
                    bubble_rect_out = Some(UiRect::new(r.min.x, r.min.y, r.max.x, r.max.y));
                }
            }

            // 2. Right-Click Context Menu
            if show_menu {
                let menu_resp = EguiWindow::new("context_menu")
                    .fixed_pos(Pos2::new(
                        menu_pos.x.clamp(10.0, (window_size.0 - 180.0).max(10.0)),
                        menu_pos.y.clamp(10.0, (window_size.1 - 220.0).max(10.0)),
                    ))
                    .title_bar(false)
                    .resizable(false)
                    .frame(
                        Frame::NONE
                            .corner_radius(10.0)
                            .inner_margin(Margin::same(12))
                            .fill(Color32::from_rgba_premultiplied(22, 26, 36, 245))
                            .stroke(Stroke::new(1.2, Color32::from_rgb(90, 150, 250))),
                    )
                    .show(&ctx, |ui| {
                        ui.set_min_width(160.0);
                        ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new("⚙ マスコット設定")
                                    .font(FontId::proportional(14.0))
                                    .strong()
                                    .color(Color32::from_rgb(150, 200, 255)),
                            );
                            ui.separator();

                            // Gravity toggle
                            ui.checkbox(gravity_enabled, "落下・重力物理");

                            // Scale slider
                            ui.add(
                                egui::Slider::new(scale, 0.5..=2.0)
                                    .text("表示サイズ")
                                    .step_by(0.1),
                            );

                            ui.separator();

                            if ui.button("💬 お話しする").clicked() {
                                next_ui_action = UiAction::SayGreeting;
                            }

                            if ui.button("📍 位置リセット").clicked() {
                                next_ui_action = UiAction::ResetPosition;
                            }

                            ui.separator();

                            if ui
                                .button(
                                    egui::RichText::new("❌ アプリ終了")
                                        .color(Color32::from_rgb(255, 110, 110)),
                                )
                                .clicked()
                            {
                                next_ui_action = UiAction::ExitApplication;
                            }
                        });
                    });

                if let Some(resp) = menu_resp {
                    let r = resp.response.rect;
                    menu_rect_out = Some(UiRect::new(r.min.x, r.min.y, r.max.x, r.max.y));
                }
            }
        });

        self.ui_action = next_ui_action;
        self.speech_bubble.current_rect = bubble_rect_out;
        self.context_menu_rect = menu_rect_out;

        self.winit_state
            .handle_platform_output(window, full_output.platform_output);

        // Upload and render textures
        for (id, deltas) in &full_output.textures_delta.set {
            for delta in deltas {
                self.wgpu_renderer
                    .update_texture(device, queue, *id, delta);
            }
        }

        let clipped_primitives = self
            .ctx
            .tessellate(full_output.shapes, full_output.pixels_per_point);

        let screen_descriptor = ScreenDescriptor {
            size_in_pixels: [window_size.0 as u32, window_size.1 as u32],
            pixels_per_point: full_output.pixels_per_point,
        };

        self.wgpu_renderer.update_buffers(
            device,
            queue,
            encoder,
            &clipped_primitives,
            &screen_descriptor,
        );

        {
            let rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Egui Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            let mut rpass = rpass.forget_lifetime();

            self.wgpu_renderer
                .render(&mut rpass, &clipped_primitives, &screen_descriptor);
        }

        for id in &full_output.textures_delta.free {
            self.wgpu_renderer.free_texture(id);
        }

        // Collect all active UI rectangles for HitTester
        let mut ui_rects = Vec::new();
        if let Some(br) = bubble_rect_out {
            ui_rects.push(br);
        }
        if let Some(mr) = menu_rect_out {
            ui_rects.push(mr);
        }

        ui_rects
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_speech_bubble_typewriter() {
        let mut bubble = SpeechBubble::new("Hello");
        bubble.chars_per_sec = 10.0;
        assert_eq!(bubble.visible_text(), "");
        assert!(bubble.is_typing());

        // Advance 0.25s -> ~2 chars
        bubble.update(0.25);
        assert_eq!(bubble.visible_text(), "He");

        // Advance 0.5s -> completes
        bubble.update(0.5);
        assert_eq!(bubble.visible_text(), "Hello");
        assert!(!bubble.is_typing());
    }

    #[test]
    fn test_speech_bubble_set_text() {
        let mut bubble = SpeechBubble::new("First");
        bubble.update(1.0);
        assert_eq!(bubble.visible_text(), "First");

        bubble.set_text("Second text");
        assert_eq!(bubble.visible_text(), "");
        assert!(bubble.is_typing());
    }
}
