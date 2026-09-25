use glam::{Mat4, Vec2};

const MIN_HALF_HEIGHT: f32 = 2.0;
const MAX_HALF_HEIGHT: f32 = 30.0;
/// How quickly the camera glides to a focus point; higher is snappier.
const GLIDE_RATE: f32 = 10.0;

/// Top-down orthographic camera. World +Y is screen-up, and `half_height` is
/// the zoom level: world units from the view's center to its top edge.
pub struct Camera {
    pub center: Vec2,
    pub half_height: f32,
    /// Point the camera is gliding towards, if any.
    target: Option<Vec2>,
}

impl Camera {
    pub fn new(center: Vec2, half_height: f32) -> Self {
        Self {
            center,
            half_height,
            target: None,
        }
    }

    /// Starts gliding the view to center on `point`.
    pub fn focus_on(&mut self, point: Vec2) {
        self.target = Some(point);
    }

    /// Moves the view `dt` seconds further along any glide in progress.
    pub fn update(&mut self, dt: f32) {
        let Some(target) = self.target else { return };
        self.center = self.center.lerp(target, 1.0 - (-GLIDE_RATE * dt).exp());
        if self.center.distance(target) < 0.01 {
            self.center = target;
            self.target = None;
        }
    }

    pub fn view_proj(&self, screen_size: Vec2) -> Mat4 {
        let half_width = self.half_width(screen_size);

        // glam 0.33's `vulkan::orthographic` flips the Y scale but not the Y
        // translation, so flip the OpenGL projection into Vulkan's Y-down
        // clip space ourselves.
        let mut proj = glam::camera::rh::proj::opengl::orthographic(
            self.center.x - half_width,
            self.center.x + half_width,
            self.center.y - self.half_height,
            self.center.y + self.half_height,
            -1.0,
            1.0,
        );
        proj.y_axis.y = -proj.y_axis.y;
        proj.w_axis.y = -proj.w_axis.y;
        proj
    }

    /// Inverse of `view_proj`: a pixel position (origin top-left) to a world point.
    pub fn screen_to_world(&self, cursor: Vec2, screen_size: Vec2) -> Vec2 {
        let ndc = cursor / screen_size * 2.0 - 1.0;
        Vec2::new(
            self.center.x + ndc.x * self.half_width(screen_size),
            self.center.y - ndc.y * self.half_height,
        )
    }

    /// Positive `steps` zoom in; each step scales the view by 10%.
    pub fn zoom(&mut self, steps: f32) {
        let factor = 1.1f32.powf(-steps);
        self.half_height = (self.half_height * factor).clamp(MIN_HALF_HEIGHT, MAX_HALF_HEIGHT);
    }

    /// Drags the view so the world point under the cursor follows the cursor.
    pub fn pan(&mut self, delta_px: Vec2, screen_size: Vec2) {
        self.target = None;
        let world_per_pixel = 2.0 * self.half_height / screen_size.y;
        self.center += Vec2::new(-delta_px.x, delta_px.y) * world_per_pixel;
    }

    fn half_width(&self, screen_size: Vec2) -> f32 {
        self.half_height * screen_size.x / screen_size.y
    }
}
