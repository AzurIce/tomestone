use ironworks::file::mdl::{Lod, ModelContainer, VertexAttributeKind, VertexValues};
use ironworks::Ironworks;

/// 从 MDL 文件提取的渲染用网格数据
pub struct MeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u16>,
}

/// GPU 顶点格式
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

/// 从 ironworks 加载 MDL 并提取网格数据
pub fn load_mdl(ironworks: &Ironworks, path: &str) -> Option<Vec<MeshData>> {
    let container: ModelContainer = ironworks.file(path).ok()?;
    let model = container.model(Lod::High);
    let meshes = model.meshes();

    let mut result = Vec::new();

    for mesh in &meshes {
        let indices = mesh.indices().ok()?;
        let attributes = mesh.attributes().ok()?;

        let mut positions: Option<&Vec<[f32; 3]>> = None;
        let mut normals: Option<&Vec<[f32; 3]>> = None;
        let mut uvs_v2: Option<&Vec<[f32; 2]>> = None;
        let mut positions_v4: Option<&Vec<[f32; 4]>> = None;
        let mut normals_v4: Option<&Vec<[f32; 4]>> = None;
        let mut uvs_v4: Option<&Vec<[f32; 4]>> = None;

        for attr in &attributes {
            match (&attr.kind, &attr.values) {
                (VertexAttributeKind::Position, VertexValues::Vector3(v)) => positions = Some(v),
                (VertexAttributeKind::Position, VertexValues::Vector4(v)) => positions_v4 = Some(v),
                (VertexAttributeKind::Normal, VertexValues::Vector3(v)) => normals = Some(v),
                (VertexAttributeKind::Normal, VertexValues::Vector4(v)) => normals_v4 = Some(v),
                (VertexAttributeKind::Uv, VertexValues::Vector2(v)) => uvs_v2 = Some(v),
                (VertexAttributeKind::Uv, VertexValues::Vector4(v)) if uvs_v4.is_none() => {
                    uvs_v4 = Some(v)
                }
                _ => {}
            }
        }

        // 确定顶点数量
        let vertex_count = positions
            .map(|v| v.len())
            .or(positions_v4.map(|v| v.len()))
            .unwrap_or(0);

        if vertex_count == 0 {
            continue;
        }

        let mut vertices = Vec::with_capacity(vertex_count);
        for i in 0..vertex_count {
            let position = if let Some(p) = positions {
                p[i]
            } else if let Some(p) = positions_v4 {
                [p[i][0], p[i][1], p[i][2]]
            } else {
                [0.0; 3]
            };

            let normal = if let Some(n) = normals {
                n[i]
            } else if let Some(n) = normals_v4 {
                [n[i][0], n[i][1], n[i][2]]
            } else {
                [0.0, 1.0, 0.0]
            };

            let uv = if let Some(u) = uvs_v2 {
                u[i]
            } else if let Some(u) = uvs_v4 {
                [u[i][0], u[i][1]]
            } else {
                [0.0; 2]
            };

            vertices.push(Vertex {
                position,
                normal,
                uv,
            });
        }

        result.push(MeshData { vertices, indices });
    }

    Some(result)
}
