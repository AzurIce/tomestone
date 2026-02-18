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
pub struct TextureData {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// 单个 mesh 的全部纹理数据
pub struct MeshTextures {
    pub diffuse: TextureData,
    pub normal: Option<TextureData>,
    pub mask: Option<TextureData>,
    pub emissive: Option<TextureData>,
}
