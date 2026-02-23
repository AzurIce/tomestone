use crate::math::{look_at, mat4_mul, perspective};
use crate::types::BoundingBox;

/// 轨道相机
pub struct Camera {
    pub distance: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub target: [f32; 3],
    /// 远裁面距离，根据场景大小动态调整
    pub far: f32,
    /// 最大缩放距离，根据场景大小动态调整
    pub max_distance: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            distance: 3.0,
            yaw: std::f32::consts::FRAC_PI_2,
            pitch: 0.3,
            target: [0.0, 0.8, 0.0],
            far: 100.0,
            max_distance: 20.0,
        }
    }
}

impl Camera {
    pub fn eye_position(&self) -> [f32; 3] {
        [
            self.target[0] + self.distance * self.yaw.cos() * self.pitch.cos(),
            self.target[1] + self.distance * self.pitch.sin(),
            self.target[2] + self.distance * self.yaw.sin() * self.pitch.cos(),
        ]
    }

    pub fn view_proj(&self, aspect: f32) -> [[f32; 4]; 4] {
        let eye = self.eye_position();
        let view = look_at(eye, self.target, [0.0, 1.0, 0.0]);
        let proj = perspective(std::f32::consts::FRAC_PI_4, aspect, 0.1, self.far);
        mat4_mul(proj, view)
    }

    /// 根据包围盒自动对焦，同时调整远裁面和缩放范围
    pub fn focus_on(&mut self, bbox: &BoundingBox) {
        self.target = bbox.center();
        let size = bbox.size();
        self.distance = if size > 0.01 { size * 1.2 } else { 3.0 };
        self.far = (size * 10.0).max(100.0);
        self.max_distance = (size * 5.0).max(20.0);
        self.yaw = std::f32::consts::FRAC_PI_2;
        self.pitch = 0.15;
    }

    /// 右键拖拽平移
    pub fn pan(&mut self, dx: f32, dy: f32) {
        let right = [self.yaw.sin(), 0.0, -self.yaw.cos()];
        let up = [0.0, 1.0, 0.0];
        let scale = self.distance * 0.002;
        for i in 0..3 {
            self.target[i] += -right[i] * dx * scale + up[i] * dy * scale;
        }
    }
}
