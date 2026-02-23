/// 模型类型，影响 shader 中的光照和材质处理方式
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ModelType {
    /// 角色装备: 使用顶点颜色遮罩、法线 alpha 裁剪
    #[default]
    Equipment,
    /// 背景/房屋模型: 不使用顶点颜色遮罩，不做法线 alpha 裁剪
    Background,
}

/// GPU 顶点格式
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
    pub tangent: [f32; 4],
}

/// 模型包围盒
#[derive(Clone, Debug)]
pub struct BoundingBox {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl BoundingBox {
    pub fn center(&self) -> [f32; 3] {
        [
            (self.min[0] + self.max[0]) * 0.5,
            (self.min[1] + self.max[1]) * 0.5,
            (self.min[2] + self.max[2]) * 0.5,
        ]
    }

    pub fn size(&self) -> f32 {
        let dx = self.max[0] - self.min[0];
        let dy = self.max[1] - self.min[1];
        let dz = self.max[2] - self.min[2];
        (dx * dx + dy * dy + dz * dz).sqrt()
    }
}

/// 通用纹理数据容器（CPU 端 RGBA）
#[derive(Clone)]
pub struct TextureData {
    pub rgba: std::sync::Arc<Vec<u8>>,
    pub width: u32,
    pub height: u32,
}

/// 单个 mesh 的全部纹理数据
#[derive(Clone)]
pub struct MeshTextures {
    pub diffuse: TextureData,
    pub normal: Option<TextureData>,
    pub mask: Option<TextureData>,
    pub emissive: Option<TextureData>,
}

/// 场景设置：光照、环境光、背景色等可配置参数
#[derive(Clone, Debug)]
pub struct SceneSettings {
    /// 主光源方向（指向光源，会被归一化）
    pub light_dir: [f32; 3],
    /// 主光源颜色（含强度，可以 > 1.0 实现更亮的光照）
    pub light_color: [f32; 3],
    /// 天空方向环境光颜色
    pub ambient_sky: [f32; 3],
    /// 地面方向环境光颜色
    pub ambient_ground: [f32; 3],
    /// 背景清除色 (RGBA, 0.0~1.0)
    pub background_color: [f64; 4],
    /// 菲涅尔边缘光强度 (0.0~1.0)
    pub fresnel_intensity: f32,
}

impl Default for SceneSettings {
    fn default() -> Self {
        Self {
            light_dir: [0.3, 0.8, 0.5],
            light_color: [1.4, 1.35, 1.3],
            ambient_sky: [0.55, 0.58, 0.68],
            ambient_ground: [0.35, 0.32, 0.30],
            background_color: [0.12, 0.12, 0.14, 1.0],
            fresnel_intensity: 0.15,
        }
    }
}

impl SceneSettings {
    /// 根据相机方向计算跟随相机的光源方向（"头灯"模式）
    pub fn light_dir_from_camera(camera_to_target: [f32; 3]) -> [f32; 3] {
        [
            camera_to_target[0] + 0.3,
            camera_to_target[1] + 0.5,
            camera_to_target[2] + 0.2,
        ]
    }
}
