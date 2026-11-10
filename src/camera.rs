//! A camera circling a point over the board. Its moves are eased: dragging, zooming and the views set where it
//! heads. The overview looks at the board from White's side at an angle; the other view from straight above.

use crate::math::{Mat4, PI, Vec3, vec3};

const MIN_PITCH: f32 = 0.12;
const MAX_PITCH: f32 = 1.5703;
const OVERVIEW_PITCH: f32 = 0.9;
const FIELD_OF_VIEW: f32 = 0.6;
const MIN_ASPECT: f32 = 1.5; // of the space for the board, below which the view widens
const EASE: f32 = 7.0;
const MIDDLE: Vec3 = vec3(0.03, 0.0, 0.0); // of the board with its tray

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CameraMode {
    Overview,
    Top,
}

/// The angle nearest to `current` that points the same way as `angle`.
fn unwrap(mut angle: f32, current: f32) -> f32 {
    while angle - current > PI {
        angle -= 2.0 * PI;
    }
    while angle - current < -PI {
        angle += 2.0 * PI;
    }
    angle
}

/// Where the camera is, or where it heads.
#[derive(Clone, Copy)]
struct Pose {
    target: Vec3,
    yaw: f32,
    pitch: f32,
    distance: f32,
    shift: f32,
    space: f32,
}

pub struct BoardCamera {
    mode: CameraMode,
    pose: Pose,
    goal: Pose,
}

impl BoardCamera {
    /// A camera in the overview.
    pub fn new() -> BoardCamera {
        let pose = Pose { target: MIDDLE, yaw: 0.0, pitch: 1.0, distance: 1.0, shift: 0.0, space: 1.0 };
        let mut camera = BoardCamera { mode: CameraMode::Overview, pose, goal: pose };
        camera.reset();
        camera.pose = camera.goal;
        camera
    }

    pub fn mode(&self) -> CameraMode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: CameraMode) {
        self.mode = mode;
        self.reset();
    }

    /// Back to the view of its mode.
    pub fn reset(&mut self) {
        self.goal.yaw = unwrap(0.0, self.goal.yaw);
        match self.mode {
            // Nearly straight down, tilted a little toward White so that the lamp is not mirrored in the whole board.
            CameraMode::Top => {
                self.goal.target = MIDDLE;
                self.goal.pitch = 1.3;
                self.goal.distance = 1.0;
            }
            CameraMode::Overview => {
                self.goal.target = MIDDLE + vec3(0.0, 0.0, 0.035);
                self.goal.pitch = OVERVIEW_PITCH;
                self.goal.distance = 0.92;
            }
        }
    }

    pub fn rotate(&mut self, yaw: f32, pitch: f32) {
        self.goal.yaw += yaw;
        self.goal.pitch = (self.goal.pitch + pitch).clamp(MIN_PITCH, MAX_PITCH);
        self.mode = CameraMode::Overview;
    }

    pub fn zoom(&mut self, steps: f32) {
        self.goal.distance = (self.goal.distance * 0.88f32.powf(steps)).clamp(0.32, 2.2);
    }

    /// The share of the window's width left for the board beside the panel: the view centers on it, and widens
    /// when it is narrow, so the whole board stays in sight.
    pub fn set_space(&mut self, share: f32) {
        self.goal.shift = (share - 1.0) / 2.0;
        self.goal.space = share;
    }

    pub fn update(&mut self, seconds: f32) {
        let ease = 1.0 - (-seconds * EASE).exp();
        self.pose.target = self.pose.target.lerp(self.goal.target, ease);
        self.pose.yaw += (self.goal.yaw - self.pose.yaw) * ease;
        self.pose.pitch += (self.goal.pitch - self.pose.pitch) * ease;
        // The distance eases by ratio, so that zooming feels the same near and far.
        self.pose.distance *= (self.goal.distance / self.pose.distance).powf(ease);
        self.pose.shift += (self.goal.shift - self.pose.shift) * ease;
        self.pose.space += (self.goal.space - self.pose.space) * ease;
    }

    pub fn moving(&self) -> bool {
        (self.goal.target - self.pose.target).length() > 1e-5
            || (self.goal.yaw - self.pose.yaw).abs() > 1e-4
            || (self.goal.pitch - self.pose.pitch).abs() > 1e-4
            || (self.goal.distance / self.pose.distance - 1.0).abs() > 1e-4
            || (self.goal.shift - self.pose.shift).abs() > 1e-4
            || (self.goal.space - self.pose.space).abs() > 1e-4
    }

    pub fn eye(&self) -> Vec3 {
        let (yaw_sin, yaw_cos) = self.pose.yaw.sin_cos();
        let (pitch_sin, pitch_cos) = self.pose.pitch.sin_cos();
        self.pose.target + vec3(yaw_sin * pitch_cos, pitch_sin, yaw_cos * pitch_cos) * self.pose.distance
    }

    pub fn view(&self) -> Mat4 {
        // Looking straight down, "up" on the screen points away from the eye's side of the board.
        let (yaw_sin, yaw_cos) = self.pose.yaw.sin_cos();
        let (pitch_sin, pitch_cos) = self.pose.pitch.sin_cos();
        let up = vec3(-yaw_sin * pitch_sin, pitch_cos, -yaw_cos * pitch_sin).normalized();
        Mat4::look_at(self.eye(), self.pose.target, up)
    }

    pub fn projection(&self, aspect: f32) -> Mat4 {
        let widen = (MIN_ASPECT / (aspect * self.pose.space)).max(1.0);
        let mut projection = Mat4::perspective(2.0 * ((FIELD_OF_VIEW / 2.0).tan() * widen).atan(), aspect, 0.01, 20.0);
        projection.set(0, 2, -2.0 * self.pose.shift); // shift in normalized coordinates, which span 2
        projection
    }

    /// The ray through a point of the window, in pixels from the top left: its origin and its direction.
    pub fn ray(&self, x: f32, y: f32, width: f32, height: f32) -> (Vec3, Vec3) {
        let inverse = (self.projection(width / height.max(1.0)) * self.view()).inverse();
        let (nx, ny) = (x / width * 2.0 - 1.0, 1.0 - y / height * 2.0);
        let near = inverse.transform_point(vec3(nx, ny, -1.0));
        let far = inverse.transform_point(vec3(nx, ny, 1.0));
        (near, (far - near).normalized())
    }
}
