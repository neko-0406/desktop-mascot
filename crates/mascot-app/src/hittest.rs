use glam::{Mat4, Vec2, Vec3};
use std::sync::RwLock;

/// Classification of an interactive target under the cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitTarget {
    None,
    Head,
    Body,
    Ui,
}

/// 2D axis-aligned bounding box in window client coordinates (pixels).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiRect {
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
}

impl UiRect {
    pub fn new(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> Self {
        Self {
            min_x: min_x.min(max_x),
            min_y: min_y.min(max_y),
            max_x: min_x.max(max_x),
            max_y: min_y.max(max_y),
        }
    }

    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.min_x && x <= self.max_x && y >= self.min_y && y <= self.max_y
    }
}

/// 2D sphere / circle in window client coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HitCircle {
    pub center: Vec2,
    pub radius: f32,
}

impl HitCircle {
    pub fn new(center: Vec2, radius: f32) -> Self {
        Self { center, radius }
    }

    pub fn contains(&self, x: f32, y: f32) -> bool {
        let dx = x - self.center.x;
        let dy = y - self.center.y;
        (dx * dx + dy * dy) <= (self.radius * self.radius)
    }
}

/// Hit bounds of all interactive elements.
#[derive(Debug, Clone, Default)]
pub struct HitBounds {
    pub head_circle: Option<HitCircle>,
    pub body_rect: Option<UiRect>,
    pub ui_rects: Vec<UiRect>,
}

/// Thread-safe hit tester queried during Win32 WM_NCHITTEST and winit event handling.
pub struct HitTester {
    bounds: RwLock<HitBounds>,
}

impl Default for HitTester {
    fn default() -> Self {
        Self::new()
    }
}

impl HitTester {
    pub fn new() -> Self {
        Self {
            bounds: RwLock::new(HitBounds::default()),
        }
    }

    /// Evaluates which target is under the window client coordinate (x, y).
    /// Precedence: UI > Head > Body > None.
    pub fn hit_test(&self, x: f32, y: f32) -> HitTarget {
        let bounds = match self.bounds.read() {
            Ok(b) => b,
            Err(poisoned) => poisoned.into_inner(),
        };

        // 1. UI elements (speech bubble, context menu, buttons) have top priority
        for rect in &bounds.ui_rects {
            if rect.contains(x, y) {
                return HitTarget::Ui;
            }
        }

        // 2. Head region (for petting & head clicks)
        if let Some(head) = &bounds.head_circle
            && head.contains(x, y) {
                return HitTarget::Head;
            }

        // 3. Body region (for dragging / body clicks)
        if let Some(body) = &bounds.body_rect
            && body.contains(x, y) {
                return HitTarget::Body;
            }

        HitTarget::None
    }

    /// Returns true if cursor is over any interactive element.
    pub fn is_hit(&self, x: f32, y: f32) -> bool {
        self.hit_test(x, y) != HitTarget::None
    }

    /// Updates interactive geometry bounds for the next frame.
    pub fn update_bounds(
        &self,
        head: Option<HitCircle>,
        body: Option<UiRect>,
        ui: Vec<UiRect>,
    ) {
        if let Ok(mut bounds) = self.bounds.write() {
            bounds.head_circle = head;
            bounds.body_rect = body;
            bounds.ui_rects = ui;
        }
    }
}

/// Projects a 3D world position into 2D window client coordinates (pixels).
/// Returns None if the point is behind the camera near plane.
pub fn project_world_to_screen(
    world_pos: Vec3,
    view_proj: Mat4,
    screen_width: f32,
    screen_height: f32,
) -> Option<Vec2> {
    let clip = view_proj * world_pos.extend(1.0);
    if clip.w <= 0.001 {
        return None;
    }
    let ndc = clip.truncate() / clip.w;
    let screen_x = (ndc.x * 0.5 + 0.5) * screen_width;
    let screen_y = (1.0 - (ndc.y * 0.5 + 0.5)) * screen_height;
    Some(Vec2::new(screen_x, screen_y))
}

/// Projects a 3D bounding box (min, max) to a 2D screen bounding rect.
pub fn project_aabb_to_screen(
    min: Vec3,
    max: Vec3,
    view_proj: Mat4,
    screen_width: f32,
    screen_height: f32,
) -> Option<UiRect> {
    let corners = [
        Vec3::new(min.x, min.y, min.z),
        Vec3::new(max.x, min.y, min.z),
        Vec3::new(min.x, max.y, min.z),
        Vec3::new(max.x, max.y, min.z),
        Vec3::new(min.x, min.y, max.z),
        Vec3::new(max.x, min.y, max.z),
        Vec3::new(min.x, max.y, max.z),
        Vec3::new(max.x, max.y, max.z),
    ];

    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    let mut any_visible = false;

    for corner in corners {
        if let Some(screen_pt) =
            project_world_to_screen(corner, view_proj, screen_width, screen_height)
        {
            min_x = min_x.min(screen_pt.x);
            min_y = min_y.min(screen_pt.y);
            max_x = max_x.max(screen_pt.x);
            max_y = max_y.max(screen_pt.y);
            any_visible = true;
        }
    }

    if any_visible && min_x <= max_x && min_y <= max_y {
        Some(UiRect::new(min_x, min_y, max_x, max_y))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ui_rect_contains() {
        let rect = UiRect::new(10.0, 20.0, 100.0, 120.0);
        assert!(rect.contains(50.0, 60.0));
        assert!(rect.contains(10.0, 20.0));
        assert!(rect.contains(100.0, 120.0));
        assert!(!rect.contains(5.0, 60.0));
        assert!(!rect.contains(50.0, 125.0));
    }

    #[test]
    fn test_hit_circle_contains() {
        let circle = HitCircle::new(Vec2::new(100.0, 100.0), 30.0);
        assert!(circle.contains(100.0, 100.0));
        assert!(circle.contains(120.0, 100.0));
        assert!(circle.contains(100.0, 130.0));
        assert!(!circle.contains(130.0, 130.0));
    }

    #[test]
    fn test_hit_tester_precedence() {
        let tester = HitTester::new();
        tester.update_bounds(
            Some(HitCircle::new(Vec2::new(100.0, 100.0), 40.0)),
            Some(UiRect::new(50.0, 80.0, 150.0, 300.0)),
            vec![UiRect::new(90.0, 90.0, 110.0, 110.0)],
        );

        // Point inside UI rect (which overlaps head and body) -> UI
        assert_eq!(tester.hit_test(100.0, 100.0), HitTarget::Ui);

        // Point inside head circle outside UI rect -> Head
        assert_eq!(tester.hit_test(125.0, 100.0), HitTarget::Head);

        // Point inside body outside head and UI -> Body
        assert_eq!(tester.hit_test(100.0, 250.0), HitTarget::Body);

        // Point completely outside -> None
        assert_eq!(tester.hit_test(10.0, 10.0), HitTarget::None);
        assert!(!tester.is_hit(10.0, 10.0));
    }

    #[test]
    fn test_project_world_to_screen() {
        let view = Mat4::look_at_rh(
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::ZERO,
            Vec3::Y,
        );
        let proj = Mat4::perspective_rh(std::f32::consts::FRAC_PI_2, 1.0, 0.1, 100.0);
        let vp = proj * view;

        // Origin (0, 0, 0) should project directly to screen center (250, 250) on 500x500 screen
        let pt = project_world_to_screen(Vec3::ZERO, vp, 500.0, 500.0).unwrap();
        assert!((pt.x - 250.0).abs() < 1e-3);
        assert!((pt.y - 250.0).abs() < 1e-3);

        // Point behind camera should return None
        let behind = project_world_to_screen(Vec3::new(0.0, 0.0, 10.0), vp, 500.0, 500.0);
        assert!(behind.is_none());
    }
}
