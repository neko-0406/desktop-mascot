use glam::{Quat, Vec2};

/// Critically damped spring interpolation (SmoothDamp).
///
/// Smoothly changes a value toward a target value over time without overshooting.
pub fn smooth_damp(
    current: f32,
    target: f32,
    current_velocity: &mut f32,
    smooth_time: f32,
    max_speed: f32,
    dt: f32,
) -> f32 {
    let smooth_time = smooth_time.max(0.0001);
    let omega = 2.0 / smooth_time;
    let x = omega * dt;
    let exp = 1.0 / (1.0 + x + 0.48 * x * x + 0.235 * x * x * x);
    let mut change = current - target;
    let original_to = target;

    let max_change = max_speed * smooth_time;
    change = change.clamp(-max_change, max_change);
    let target = current - change;

    let temp = (*current_velocity + omega * change) * dt;
    *current_velocity = (*current_velocity - omega * temp) * exp;
    let mut output = target + (change + temp) * exp;

    if (original_to - current > 0.0) == (output > original_to) {
        output = original_to;
        *current_velocity = (output - original_to) / dt;
    }

    output
}

/// Simple fast 64-bit XorShift pseudorandom generator.
#[derive(Debug, Clone)]
pub struct FastRng {
    state: u64,
}

impl FastRng {
    pub fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 0x853c49e6748fea9b } else { seed },
        }
    }

    pub fn next_u32(&mut self) -> u32 {
        self.state ^= self.state >> 12;
        self.state ^= self.state << 25;
        self.state ^= self.state >> 27;
        (self.state.wrapping_mul(0x2545F4914F6CDD1D) >> 32) as u32
    }

    pub fn gen_range(&mut self, min: f32, max: f32) -> f32 {
        let frac = (self.next_u32() as f64) / (u32::MAX as f64);
        (min as f64 + frac * (max as f64 - min as f64)) as f32
    }
}

// ---------------------------------------------------------------------------
// 1. Breathing Motion
// ---------------------------------------------------------------------------

/// Procedural breathing animation generator using a sine-wave model.
#[derive(Debug, Clone)]
pub struct BreathingController {
    /// Breathing frequency in Hz (default: 0.25 Hz = 4.0 sec period).
    pub frequency: f32,
    /// Amplitude of pitch rotation for chest bone in radians (default: ~1.5 deg = 0.026 rad).
    pub chest_amplitude: f32,
    /// Amplitude of pitch rotation for spine bone in radians (default: ~0.75 deg = 0.013 rad).
    pub spine_amplitude: f32,
}

impl Default for BreathingController {
    fn default() -> Self {
        Self {
            frequency: 0.25,
            chest_amplitude: 0.02618, // 1.5 degrees
            spine_amplitude: 0.01309, // 0.75 degrees
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BreathingPose {
    pub chest_rotation: Quat,
    pub spine_rotation: Quat,
    pub phase: f32,
}

impl BreathingController {
    /// Evaluates breathing pose at time `elapsed_seconds`.
    pub fn update(&self, elapsed_seconds: f32) -> BreathingPose {
        let phase = elapsed_seconds * self.frequency * 2.0 * std::f32::consts::PI;
        let sin_val = phase.sin();

        // Slight pitch rotation (breathing in tilts chest slightly upward)
        let pitch_chest = self.chest_amplitude * sin_val;
        let pitch_spine = self.spine_amplitude * sin_val;

        BreathingPose {
            chest_rotation: Quat::from_rotation_x(-pitch_chest),
            spine_rotation: Quat::from_rotation_x(-pitch_spine),
            phase,
        }
    }
}

// ---------------------------------------------------------------------------
// 2. Stochastic Eye Blinking
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
enum BlinkPhase {
    Idle { timer: f32 },
    Closing { elapsed: f32 },
    Holding { elapsed: f32 },
    Opening { elapsed: f32 },
}

/// Natural stochastic eye blinking generator with Poisson intervals and double-blinks.
#[derive(Debug, Clone)]
pub struct BlinkController {
    phase: BlinkPhase,
    rng: FastRng,
    pub close_duration: f32, // Default: 0.08s
    pub hold_duration: f32,  // Default: 0.04s
    pub open_duration: f32,  // Default: 0.12s
    pub min_interval: f32,   // Default: 2.0s
    pub max_interval: f32,   // Default: 6.0s
    pub double_blink_chance: f32, // Default: 0.15 (15%)
    pub current_weight: f32,
}

impl BlinkController {
    pub fn new(seed: u64) -> Self {
        let mut rng = FastRng::new(seed);
        let initial_interval = rng.gen_range(2.0, 5.0);

        Self {
            phase: BlinkPhase::Idle { timer: initial_interval },
            rng,
            close_duration: 0.08,
            hold_duration: 0.04,
            open_duration: 0.12,
            min_interval: 2.0,
            max_interval: 6.0,
            double_blink_chance: 0.15,
            current_weight: 0.0,
        }
    }

    /// Advances blink state by `dt` seconds and returns the morph weight in [0.0, 1.0].
    pub fn update(&mut self, mut dt: f32) -> f32 {
        while dt > 0.0 {
            match self.phase {
                BlinkPhase::Idle { timer } => {
                    if dt >= timer {
                        dt -= timer;
                        self.phase = BlinkPhase::Closing { elapsed: 0.0 };
                        if dt <= 0.0 {
                            self.current_weight = 0.0;
                            break;
                        }
                    } else {
                        self.phase = BlinkPhase::Idle { timer: timer - dt };
                        self.current_weight = 0.0;
                        break;
                    }
                }
                BlinkPhase::Closing { elapsed } => {
                    let remaining = self.close_duration - elapsed;
                    if dt >= remaining {
                        dt -= remaining;
                        self.phase = BlinkPhase::Holding { elapsed: 0.0 };
                        self.current_weight = 1.0;
                    } else {
                        let new_elapsed = elapsed + dt;
                        let t = (new_elapsed / self.close_duration).clamp(0.0, 1.0);
                        self.current_weight = (t * std::f32::consts::FRAC_PI_2).sin();
                        self.phase = BlinkPhase::Closing { elapsed: new_elapsed };
                        break;
                    }
                }
                BlinkPhase::Holding { elapsed } => {
                    let remaining = self.hold_duration - elapsed;
                    if dt >= remaining {
                        dt -= remaining;
                        self.phase = BlinkPhase::Opening { elapsed: 0.0 };
                        self.current_weight = 1.0;
                    } else {
                        self.phase = BlinkPhase::Holding { elapsed: elapsed + dt };
                        self.current_weight = 1.0;
                        break;
                    }
                }
                BlinkPhase::Opening { elapsed } => {
                    let remaining = self.open_duration - elapsed;
                    if dt >= remaining {
                        dt -= remaining;
                        let roll = self.rng.gen_range(0.0, 1.0);
                        let next_interval = if roll < self.double_blink_chance {
                            0.15
                        } else {
                            self.rng.gen_range(self.min_interval, self.max_interval)
                        };
                        self.phase = BlinkPhase::Idle { timer: next_interval };
                        self.current_weight = 0.0;
                    } else {
                        let new_elapsed = elapsed + dt;
                        let t = (new_elapsed / self.open_duration).clamp(0.0, 1.0);
                        self.current_weight = 1.0 - (t * std::f32::consts::FRAC_PI_2).sin();
                        self.phase = BlinkPhase::Opening { elapsed: new_elapsed };
                        break;
                    }
                }
            }
        }

        self.current_weight
    }
}

// ---------------------------------------------------------------------------
// 3. Look-At IK Controller
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LookAtPose {
    pub head_rotation: Quat,
    pub neck_rotation: Quat,
    pub current_yaw: f32,
    pub current_pitch: f32,
}

/// Look-At IK tracking system that aligns avatar gaze with cursor position.
#[derive(Debug, Clone)]
pub struct LookAtController {
    pub max_yaw: f32,   // Maximum yaw in radians (default: 45 deg = 0.785 rad)
    pub max_pitch: f32, // Maximum pitch in radians (default: 25 deg = 0.436 rad)
    pub head_weight: f32, // 65% to head
    pub neck_weight: f32, // 35% to neck
    pub smooth_time: f32, // SmoothDamp filter time (default: 0.15s)

    target_yaw: f32,
    target_pitch: f32,
    current_yaw: f32,
    current_pitch: f32,
    yaw_velocity: f32,
    pitch_velocity: f32,
}

impl Default for LookAtController {
    fn default() -> Self {
        Self {
            max_yaw: 45.0f32.to_radians(),
            max_pitch: 25.0f32.to_radians(),
            head_weight: 0.65,
            neck_weight: 0.35,
            smooth_time: 0.15,
            target_yaw: 0.0,
            target_pitch: 0.0,
            current_yaw: 0.0,
            current_pitch: 0.0,
            yaw_velocity: 0.0,
            pitch_velocity: 0.0,
        }
    }
}

impl LookAtController {
    /// Updates target angles from window cursor coordinates.
    ///
    /// Cursor (0, 0) is top-left, (width, height) is bottom-right.
    pub fn set_cursor_window_coords(
        &mut self,
        cursor_x: f32,
        cursor_y: f32,
        window_width: f32,
        window_height: f32,
    ) {
        if window_width <= 0.0 || window_height <= 0.0 {
            return;
        }

        // Convert to Normalized Device Coordinates [-1.0, 1.0]
        let ndc_x = (cursor_x / window_width) * 2.0 - 1.0;
        let ndc_y = -((cursor_y / window_height) * 2.0 - 1.0); // Up is positive

        self.set_cursor_ndc(Vec2::new(ndc_x, ndc_y));
    }

    /// Sets target angles directly from normalized coordinates [-1.0, 1.0].
    pub fn set_cursor_ndc(&mut self, ndc: Vec2) {
        self.target_yaw = (ndc.x * self.max_yaw).clamp(-self.max_yaw, self.max_yaw);
        self.target_pitch = (ndc.y * self.max_pitch).clamp(-self.max_pitch, self.max_pitch);
    }

    /// Advances smooth damp tracking and calculates head and neck rotations.
    pub fn update(&mut self, dt: f32) -> LookAtPose {
        self.current_yaw = smooth_damp(
            self.current_yaw,
            self.target_yaw,
            &mut self.yaw_velocity,
            self.smooth_time,
            15.0,
            dt,
        );

        self.current_pitch = smooth_damp(
            self.current_pitch,
            self.target_pitch,
            &mut self.pitch_velocity,
            self.smooth_time,
            15.0,
            dt,
        );

        // Distribute to Head (65%) and Neck (35%)
        let head_yaw = self.current_yaw * self.head_weight;
        let head_pitch = self.current_pitch * self.head_weight;

        let neck_yaw = self.current_yaw * self.neck_weight;
        let neck_pitch = self.current_pitch * self.neck_weight;

        // Yaw rotates around Y, pitch rotates around X
        let head_rotation = Quat::from_rotation_y(head_yaw) * Quat::from_rotation_x(-head_pitch);
        let neck_rotation = Quat::from_rotation_y(neck_yaw) * Quat::from_rotation_x(-neck_pitch);

        LookAtPose {
            head_rotation,
            neck_rotation,
            current_yaw: self.current_yaw,
            current_pitch: self.current_pitch,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_breathing_controller() {
        let breathing = BreathingController::default();
        let pose_0 = breathing.update(0.0);
        let pose_1 = breathing.update(1.0); // 1.0 sec = quarter cycle of 4 sec = peak sin
        let pose_4 = breathing.update(4.0); // full cycle

        assert!((pose_0.phase - 0.0).abs() < 1e-4);
        assert!((pose_1.phase - std::f32::consts::FRAC_PI_2).abs() < 1e-4);
        assert!(pose_1.chest_rotation.x.abs() > 0.001);
        assert!((pose_4.phase - 2.0 * std::f32::consts::PI).abs() < 1e-4);
    }

    #[test]
    fn test_blink_controller_lifecycle() {
        let mut blink = BlinkController::new(42);
        // Force immediate blink
        blink.phase = BlinkPhase::Idle { timer: 0.0 };

        let w0 = blink.update(0.01);
        assert!(w0 > 0.0); // Closing

        // Fast forward through close and into hold
        let w_hold = blink.update(0.08);
        assert_eq!(w_hold, 1.0); // Holding

        // Fast forward through opening
        blink.update(0.04); // Finish hold
        let w_open = blink.update(0.06); // Midway opening
        assert!(w_open < 1.0 && w_open > 0.0);

        // Finish opening
        let w_idle = blink.update(0.1);
        assert_eq!(w_idle, 0.0);
    }

    #[test]
    fn test_look_at_controller_clamping_and_smoothing() {
        let mut look_at = LookAtController::default();
        // Request extreme NDC (+2.0, +2.0)
        look_at.set_cursor_ndc(Vec2::new(2.0, 2.0));

        assert!((look_at.target_yaw - look_at.max_yaw).abs() < 1e-5);
        assert!((look_at.target_pitch - look_at.max_pitch).abs() < 1e-5);

        // Step 10 frames
        for _ in 0..10 {
            look_at.update(1.0 / 60.0);
        }

        // Angles should smoothly approach targets
        assert!(look_at.current_yaw > 0.0 && look_at.current_yaw <= look_at.max_yaw);
        assert!(look_at.current_pitch > 0.0 && look_at.current_pitch <= look_at.max_pitch);
    }
}
