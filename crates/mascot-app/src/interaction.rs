use glam::Vec3;
use std::collections::VecDeque;
use std::time::Instant;

/// Interaction state of the mascot character.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InteractionMode {
    Idle,
    Dragging,
    Falling,
}

/// Parameters configuring physics and petting behavior.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct InteractionConfig {
    pub gravity_px: f32,          // e.g. 3500.0 px/s^2
    pub bounce_restitution: f32,  // e.g. 0.35 (0.3 ~ 0.5)
    pub velocity_threshold: f32,  // stopping velocity threshold
    pub snap_distance: f32,       // screen edge snap threshold
    pub petting_reversal_threshold: usize, // required reversals to trigger petting
    pub petting_decay_rate: f32,  // happiness decay rate per second
    pub petting_gain_rate: f32,   // happiness gain rate per second
}

impl Default for InteractionConfig {
    fn default() -> Self {
        Self {
            gravity_px: 3600.0,
            bounce_restitution: 0.38,
            velocity_threshold: 120.0,
            snap_distance: 20.0,
            petting_reversal_threshold: 2,
            petting_decay_rate: 0.45,
            petting_gain_rate: 1.6,
        }
    }
}

// Renamed gain_rate internally for clarity
impl InteractionConfig {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }
}

/// Cursor history sample for detecting petting strokes.
#[derive(Debug, Clone, Copy)]
struct CursorSample {
    pos: (f32, f32),
    time: Instant,
}

/// Detector for head-stroking / petting interactions.
pub struct PettingDetector {
    samples: VecDeque<CursorSample>,
    last_direction_x: f32,
    reversal_count: usize,
    last_reversal_time: Option<Instant>,
    pub is_petting: bool,
    pub happiness: f32,
}

impl Default for PettingDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl PettingDetector {
    pub fn new() -> Self {
        Self {
            samples: VecDeque::with_capacity(32),
            last_direction_x: 0.0,
            reversal_count: 0,
            last_reversal_time: None,
            is_petting: false,
            happiness: 0.0,
        }
    }

    /// Feed a cursor position over the head region.
    pub fn record_cursor(&mut self, pos: (f32, f32), now: Instant) {
        if let Some(prev) = self.samples.back() {
            let dt = (now - prev.time).as_secs_f32();
            if dt > 0.005 {
                let dx = pos.0 - prev.pos.0;
                let speed_x = dx / dt;

                // Only consider movements with significant horizontal speed
                if speed_x.abs() > 25.0 {
                    let dir = speed_x.signum();
                    if self.last_direction_x != 0.0 && dir != self.last_direction_x {
                        // Direction reversed!
                        self.reversal_count += 1;
                        self.last_reversal_time = Some(now);
                    }
                    self.last_direction_x = dir;
                }
            }
        }

        self.samples.push_back(CursorSample { pos, time: now });

        // Retain samples within the past 0.6 seconds
        while let Some(front) = self.samples.front() {
            if (now - front.time).as_secs_f32() > 0.6 {
                self.samples.pop_front();
            } else {
                break;
            }
        }

        // Check if reversals expired
        if let Some(lrt) = self.last_reversal_time
            && (now - lrt).as_secs_f32() > 0.6 {
                self.reversal_count = 0;
            }

        self.is_petting = self.reversal_count >= 2;
    }

    /// Reset cursor tracking when cursor leaves head area.
    pub fn cursor_left(&mut self) {
        self.samples.clear();
        self.reversal_count = 0;
        self.last_direction_x = 0.0;
        self.is_petting = false;
    }

    /// Update happiness level over time.
    pub fn update(&mut self, dt: f32) {
        if self.is_petting {
            self.happiness = (self.happiness + dt * 1.6).min(1.0);
        } else {
            self.happiness = (self.happiness - dt * 0.45).max(0.0);
        }
    }
}

/// Damped spring simulator for landing squash-and-stretch.
#[derive(Debug, Clone, Default)]
pub struct LandingSquash {
    pub amplitude: f32, // compression amplitude (e.g. 0.2 means scale_y down to 0.8)
    pub time: f32,
    pub active: bool,
}

impl LandingSquash {
    pub fn trigger(&mut self, impact_speed: f32) {
        // Higher impact speed produces stronger squash
        let amp = (impact_speed / 1500.0).clamp(0.12, 0.35);
        self.amplitude = amp;
        self.time = 0.0;
        self.active = true;
    }

    pub fn update(&mut self, dt: f32) {
        if !self.active {
            return;
        }
        self.time += dt;
        let decay = (-14.0 * self.time).exp();
        if decay < 0.005 || self.time > 0.6 {
            self.active = false;
            self.amplitude = 0.0;
            self.time = 0.0;
        }
    }

    /// Returns the vertical scale factor (1.0 = normal, < 1.0 = compressed).
    pub fn vertical_scale(&self) -> f32 {
        if !self.active {
            return 1.0;
        }
        // Damped harmonic oscillator: 1.0 - A * exp(-lambda * t) * cos(omega * t)
        let decay = (-14.0 * self.time).exp();
        let osc = (self.time * 26.0).cos();
        (1.0 - self.amplitude * decay * osc).clamp(0.65, 1.35)
    }

    /// Returns the horizontal scale factor preserving volume: 1.0 / sqrt(vertical_scale).
    pub fn horizontal_scale(&self) -> f32 {
        let v = self.vertical_scale();
        if v > 0.0 {
            1.0 / v.sqrt()
        } else {
            1.0
        }
    }
}

/// Coordinates dragging, gravity drop, bouncing, and petting mechanics.
pub struct InteractionController {
    pub mode: InteractionMode,
    pub config: InteractionConfig,
    pub petting: PettingDetector,
    pub squash: LandingSquash,
    pub lift_weight: f32,

    // Drag tracking
    drag_cursor_start: (f64, f64),
    drag_window_start: (i32, i32),

    // Physics fall tracking
    pub velocity_y: f32,
    pub current_y: f32,
    pub ground_y: f32,
    pub gravity_enabled: bool,
    pub scale: f32,
}

impl Default for InteractionController {
    fn default() -> Self {
        Self::new()
    }
}

impl InteractionController {
    pub fn new() -> Self {
        Self {
            mode: InteractionMode::Idle,
            config: InteractionConfig::default(),
            petting: PettingDetector::new(),
            squash: LandingSquash::default(),
            lift_weight: 0.0,
            drag_cursor_start: (0.0, 0.0),
            drag_window_start: (0, 0),
            velocity_y: 0.0,
            current_y: 0.0,
            ground_y: 0.0,
            gravity_enabled: true,
            scale: 1.0,
        }
    }

    /// Begins a drag movement when the user clicks on the mascot.
    pub fn start_drag(&mut self, cursor_screen_pos: (f64, f64), window_pos: (i32, i32)) {
        self.mode = InteractionMode::Dragging;
        self.drag_cursor_start = cursor_screen_pos;
        self.drag_window_start = window_pos;
        self.velocity_y = 0.0;
    }

    /// Updates window position during drag given current global cursor screen coordinates.
    #[allow(dead_code)]
    pub fn update_drag(&mut self, cursor_screen_pos: (f64, f64)) -> (i32, i32) {
        let dx = (cursor_screen_pos.0 - self.drag_cursor_start.0) as i32;
        let dy = (cursor_screen_pos.1 - self.drag_cursor_start.1) as i32;
        (
            self.drag_window_start.0 + dx,
            self.drag_window_start.1 + dy,
        )
    }

    /// Ends drag. If above ground, begins free fall.
    pub fn end_drag(&mut self, current_window_y: i32, ground_y: i32) {
        self.current_y = current_window_y as f32;
        self.ground_y = ground_y as f32;

        if self.gravity_enabled && self.current_y < (self.ground_y - 1.0) {
            self.mode = InteractionMode::Falling;
            self.velocity_y = 0.0;
        } else {
            self.mode = InteractionMode::Idle;
            self.velocity_y = 0.0;
        }
    }

    /// Advances physics simulation per frame.
    /// Returns Some(new_window_y) if window position changed.
    pub fn update_physics(&mut self, dt: f32, ground_y: i32) -> Option<i32> {
        self.ground_y = ground_y as f32;

        // Smooth lift weight interpolation
        let target_lift = if self.mode == InteractionMode::Dragging {
            1.0
        } else {
            0.0
        };
        self.lift_weight += (target_lift - self.lift_weight) * (dt * 12.0).min(1.0);

        // Update landing squash & stretch
        self.squash.update(dt);

        // Update petting happiness
        self.petting.update(dt);

        // Handle free-fall gravity & bouncing
        if self.mode == InteractionMode::Falling {
            let g = self.config.gravity_px;
            self.velocity_y += g * dt;
            self.current_y += self.velocity_y * dt;

            // Collision with desktop work area ground / taskbar top
            if self.current_y >= self.ground_y {
                self.current_y = self.ground_y;
                let impact_speed = self.velocity_y;

                // Invert velocity with restitution coefficient
                self.velocity_y = -self.velocity_y * self.config.bounce_restitution;

                // Trigger squash animation
                self.squash.trigger(impact_speed);

                // Stop bouncing once velocity falls below threshold
                if self.velocity_y.abs() < self.config.velocity_threshold {
                    self.velocity_y = 0.0;
                    self.mode = InteractionMode::Idle;
                }
            }

            Some(self.current_y.round() as i32)
        } else {
            None
        }
    }

    /// External acceleration vector to inject into PhysBone simulation.
    /// Provides upward tension and dangling effects during lift.
    pub fn external_acceleration(&self) -> Vec3 {
        let mut accel = Vec3::ZERO;
        if self.lift_weight > 0.01 {
            // Dangling inertia pulls secondary chains downward
            accel += Vec3::new(0.0, -22.0 * self.lift_weight, 0.0);
        }
        if self.petting.is_petting {
            // Micro-vibration on ears / tail during head petting
            let wobble = (Instant::now().elapsed().as_secs_f32() * 32.0).sin();
            accel += Vec3::new(wobble * 2.5 * self.petting.happiness, 0.0, 0.0);
        }
        accel
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_petting_detector_reversals() {
        let mut detector = PettingDetector::new();
        let start = Instant::now();

        // Stroke right
        detector.record_cursor((100.0, 100.0), start);
        detector.record_cursor((150.0, 100.0), start + Duration::from_millis(50));
        assert!(!detector.is_petting);

        // Stroke left (first reversal)
        detector.record_cursor((120.0, 100.0), start + Duration::from_millis(100));
        detector.record_cursor((80.0, 100.0), start + Duration::from_millis(150));
        assert!(!detector.is_petting);

        // Stroke right again (second reversal)
        detector.record_cursor((110.0, 100.0), start + Duration::from_millis(200));
        detector.record_cursor((160.0, 100.0), start + Duration::from_millis(250));

        // Petting should now be detected!
        assert!(detector.is_petting);

        // Update petting increases happiness
        detector.update(0.1);
        assert!(detector.happiness > 0.0);
    }

    #[test]
    fn test_falling_and_bounce_physics() {
        let mut controller = InteractionController::new();
        controller.config.gravity_px = 1000.0;
        controller.config.bounce_restitution = 0.5;
        controller.config.velocity_threshold = 100.0;

        let ground_y = 500;
        controller.end_drag(0, ground_y);
        assert_eq!(controller.mode, InteractionMode::Falling);

        // Advance simulation: should fall down
        let mut _simulated_time = 0.0;
        let dt = 0.016; // 60fps
        let mut bounced = false;

        for _ in 0..200 {
            if let Some(new_y) = controller.update_physics(dt, ground_y) {
                _simulated_time += dt;
                if new_y == ground_y && controller.velocity_y < 0.0 {
                    bounced = true;
                }
            }
            if controller.mode == InteractionMode::Idle {
                break;
            }
        }

        assert!(bounced, "Mascot should have bounced on ground");
        assert_eq!(controller.mode, InteractionMode::Idle);
        assert_eq!(controller.current_y as i32, ground_y);
    }

    #[test]
    fn test_squash_and_stretch() {
        let mut squash = LandingSquash::default();
        squash.trigger(1500.0);
        assert!(squash.active);

        // Initial vertical scale should be < 1.0 (compressed)
        let v_scale = squash.vertical_scale();
        assert!(v_scale < 1.0);

        // Horizontal scale should be > 1.0 (expanded)
        let h_scale = squash.horizontal_scale();
        assert!(h_scale > 1.0);

        // After time decays, should return to 1.0
        squash.update(1.0);
        assert!(!squash.active);
        assert_eq!(squash.vertical_scale(), 1.0);
        assert_eq!(squash.horizontal_scale(), 1.0);
    }
}
