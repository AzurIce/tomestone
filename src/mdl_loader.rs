use std::io::{Cursor, Read, Seek, SeekFrom};
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
    pub color: [f32; 4],
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

/// 计算网格数据的包围盒
pub fn compute_bounding_box(meshes: &[MeshData]) -> BoundingBox {
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    for mesh in meshes {
        for v in &mesh.vertices {
            for i in 0..3 {
                if v.position[i] < min[i] { min[i] = v.position[i]; }
                if v.position[i] > max[i] { max[i] = v.position[i]; }
            }
        }
    }
    if min[0] == f32::MAX {
        return BoundingBox { min: [0.0; 3], max: [0.0; 3] };
    }
    BoundingBox { min, max }
}

/// 从 ironworks 加载 MDL 并提取网格数据 (支持 v5/v6 Dawntrail 格式)
pub fn load_mdl(ironworks: &Ironworks, path: &str) -> Result<Vec<MeshData>, String> {
    let data: Vec<u8> = ironworks.file(path)
        .map_err(|e| format!("读取文件失败: {e}"))?;
    parse_mdl(&data)
}

// ---- 二进制读取工具 ----

fn read_u8(c: &mut Cursor<&[u8]>) -> Result<u8, String> {
    let mut b = [0u8; 1];
    c.read_exact(&mut b).map_err(|e| format!("read_u8: {e}"))?;
    Ok(b[0])
}
fn read_u16(c: &mut Cursor<&[u8]>) -> Result<u16, String> {
    let mut b = [0u8; 2];
    c.read_exact(&mut b).map_err(|e| format!("read_u16: {e}"))?;
    Ok(u16::from_le_bytes(b))
}
fn read_u32(c: &mut Cursor<&[u8]>) -> Result<u32, String> {
    let mut b = [0u8; 4];
    c.read_exact(&mut b).map_err(|e| format!("read_u32: {e}"))?;
    Ok(u32::from_le_bytes(b))
}
fn read_f32(c: &mut Cursor<&[u8]>) -> Result<f32, String> {
    let mut b = [0u8; 4];
    c.read_exact(&mut b).map_err(|e| format!("read_f32: {e}"))?;
    Ok(f32::from_le_bytes(b))
}
fn skip(c: &mut Cursor<&[u8]>, n: i64) -> Result<(), String> {
    c.seek(SeekFrom::Current(n)).map_err(|e| format!("skip: {e}"))?;
    Ok(())
}

// ---- 顶点声明 ----

const VERTEX_ELEMENT_SLOTS: usize = 17;

#[derive(Clone, Copy, Debug)]
struct VertexElement {
    stream: u8,
    offset: u8,
    format: u8,  // 2=Single3, 3=Single4, 8=ByteFloat4, 13=Half2, 14=Half4
    usage: u8,   // 0=Position, 3=Normal, 4=UV
}

fn read_vertex_declarations(c: &mut Cursor<&[u8]>, count: u16) -> Result<Vec<Vec<VertexElement>>, String> {
    let mut decls = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let mut elements = Vec::new();
        for slot in 0..VERTEX_ELEMENT_SLOTS {
            let stream = read_u8(c)?;
            let offset = read_u8(c)?;
            let format = read_u8(c)?;
            let usage = read_u8(c)?;
            skip(c, 4)?; // usage_index + padding
            if stream == 0xFF {
                // 0xFF 标记后的 slot 全部跳过
                let remaining = VERTEX_ELEMENT_SLOTS - slot - 1;
                skip(c, remaining as i64 * 8)?;
                break;
            }
            elements.push(VertexElement { stream, offset, format, usage });
        }
        decls.push(elements);
    }
    Ok(decls)
}

// ---- MDL 解析 ----

struct MdlMesh {
    vertex_count: u16,
    index_count: u32,
    start_index: u32,
    vertex_buffer_offset: [u32; 3],
    vertex_buffer_stride: [u8; 3],
}

struct MdlLod {
    mesh_index: u16,
    mesh_count: u16,
    vertex_data_offset: u32,
    index_data_offset: u32,
}

fn parse_mdl(data: &[u8]) -> Result<Vec<MeshData>, String> {
    let mut c = Cursor::new(data);

    // ---- File Header (68 bytes) ----
    let _version = read_u32(&mut c)?;
    let _stack_size = read_u32(&mut c)?;
    let _runtime_size = read_u32(&mut c)?;
    let vertex_decl_count = read_u16(&mut c)?;
    let _material_count = read_u16(&mut c)?;
    skip(&mut c, 12 + 12 + 12 + 12)?; // vertex_offsets, index_offsets, vertex/index_buffer_size
    skip(&mut c, 4)?; // lod_count + 3 bools/padding

    // ---- Vertex Declarations ----
    let decls = read_vertex_declarations(&mut c, vertex_decl_count)?;

    // ---- Strings ----
    let _string_count = read_u16(&mut c)?;
    skip(&mut c, 2)?; // padding
    let string_size = read_u32(&mut c)?;
    skip(&mut c, string_size as i64)?;

    // ---- Model Header ----
    let _radius = read_f32(&mut c)?;
    let mesh_count = read_u16(&mut c)?;
    let _attribute_count = read_u16(&mut c)?;
    let _submesh_count = read_u16(&mut c)?;
    let _material_count2 = read_u16(&mut c)?;
    let _bone_count = read_u16(&mut c)?;
    let _bone_table_count = read_u16(&mut c)?;
    let _shape_count = read_u16(&mut c)?;
    let _shape_mesh_count = read_u16(&mut c)?;
    let _shape_value_count = read_u16(&mut c)?;
    let _lod_count = read_u8(&mut c)?;
    let _flags1 = read_u8(&mut c)?;
    let element_id_count = read_u16(&mut c)?;
    let _terrain_shadow_mesh_count = read_u8(&mut c)?;
    let flags2 = read_u8(&mut c)?;
    skip(&mut c, 4 + 4)?; // clip distances
    let _unknown4 = read_u16(&mut c)?;
    let _terrain_shadow_submesh_count = read_u16(&mut c)?;
    skip(&mut c, 1 + 1 + 1 + 1 + 2 + 2 + 2 + 6)?; // unknowns + padding

    // ---- Element IDs ----
    skip(&mut c, element_id_count as i64 * 32)?;

    // ---- LODs (3) ----
    let mut lods = Vec::new();
    for _ in 0..3 {
        let mesh_index = read_u16(&mut c)?;
        let mesh_count_lod = read_u16(&mut c)?;
        skip(&mut c, 4 + 4)?; // lod ranges
        skip(&mut c, 2 * 8)?; // water/shadow/terrain/fog mesh index+count
        skip(&mut c, 4 + 4 + 4 + 4)?; // edge_geometry + polygon_count + unknown
        skip(&mut c, 4 + 4)?; // vertex/index buffer size
        let vertex_data_offset = read_u32(&mut c)?;
        let index_data_offset = read_u32(&mut c)?;
        lods.push(MdlLod { mesh_index, mesh_count: mesh_count_lod, vertex_data_offset, index_data_offset });
    }

    // ---- Extra LODs (optional) ----
    let extra_lod_enabled = (flags2 & 0x10) != 0;
    if extra_lod_enabled {
        skip(&mut c, 3 * 32)?; // 3 ExtraLod structs, 16 u16 each = 32 bytes
    }

    // ---- Meshes ----
    let mut meshes = Vec::with_capacity(mesh_count as usize);
    for _ in 0..mesh_count {
        let vertex_count = read_u16(&mut c)?;
        skip(&mut c, 2)?; // padding
        let index_count = read_u32(&mut c)?;
        let _material_index = read_u16(&mut c)?;
        let _submesh_index = read_u16(&mut c)?;
        let _submesh_count = read_u16(&mut c)?;
        let _bone_table_index = read_u16(&mut c)?;
        let start_index = read_u32(&mut c)?;
        let vbo0 = read_u32(&mut c)?;
        let vbo1 = read_u32(&mut c)?;
        let vbo2 = read_u32(&mut c)?;
        let vbs0 = read_u8(&mut c)?;
        let vbs1 = read_u8(&mut c)?;
        let vbs2 = read_u8(&mut c)?;
        let _stream_count = read_u8(&mut c)?;
        meshes.push(MdlMesh {
            vertex_count, index_count, start_index,
            vertex_buffer_offset: [vbo0, vbo1, vbo2],
            vertex_buffer_stride: [vbs0, vbs1, vbs2],
        });
    }

    // 剩余元数据 (bone tables, shapes, bounding boxes 等) 不需要解析
    // vertex_data_offset / index_data_offset 是文件内绝对偏移，可直接 seek

    // ---- 提取 LOD 0 (High) 的网格数据 ----
    let lod = &lods[0];
    let mut result = Vec::new();

    for mi in lod.mesh_index..(lod.mesh_index + lod.mesh_count) {
        let mesh = &meshes[mi as usize];
        let decl = &decls[mi as usize];
        if mesh.vertex_count == 0 { continue; }

        let mut vertices = vec![Vertex { position: [0.0; 3], normal: [0.0, 1.0, 0.0], uv: [0.0; 2], color: [1.0, 1.0, 1.0, 1.0] }; mesh.vertex_count as usize];

        for k in 0..mesh.vertex_count as usize {
            for elem in decl {
                let abs_offset = lod.vertex_data_offset
                    + mesh.vertex_buffer_offset[elem.stream as usize]
                    + elem.offset as u32
                    + mesh.vertex_buffer_stride[elem.stream as usize] as u32 * k as u32;

                c.seek(SeekFrom::Start(abs_offset as u64)).map_err(|e| format!("seek vertex: {e}"))?;

                match (elem.usage, elem.format) {
                    // Position
                    (0, 2) => { // Single3
                        vertices[k].position = [read_f32(&mut c)?, read_f32(&mut c)?, read_f32(&mut c)?];
                    }
                    (0, 3) => { // Single4
                        vertices[k].position = [read_f32(&mut c)?, read_f32(&mut c)?, read_f32(&mut c)?];
                    }
                    (0, 14) => { // Half4
                        let v = read_half4(&mut c)?;
                        vertices[k].position = [v[0], v[1], v[2]];
                    }
                    // Normal
                    (3, 2) => { // Single3
                        vertices[k].normal = [read_f32(&mut c)?, read_f32(&mut c)?, read_f32(&mut c)?];
                    }
                    (3, 3) => { // Single4
                        vertices[k].normal = [read_f32(&mut c)?, read_f32(&mut c)?, read_f32(&mut c)?];
                    }
                    (3, 14) => { // Half4
                        let v = read_half4(&mut c)?;
                        vertices[k].normal = [v[0], v[1], v[2]];
                    }
                    (3, 8) => { // ByteFloat4 (packed normal)
                        let v = read_byte_float4(&mut c)?;
                        vertices[k].normal = [v[0] * 2.0 - 1.0, v[1] * 2.0 - 1.0, v[2] * 2.0 - 1.0];
                    }
                    // UV
                    (4, 1) => { // Single2
                        vertices[k].uv = [read_f32(&mut c)?, read_f32(&mut c)?];
                    }
                    (4, 13) => { // Half2
                        vertices[k].uv = read_half2(&mut c)?;
                    }
                    (4, 14) => { // Half4 (take first 2)
                        let v = read_half4(&mut c)?;
                        vertices[k].uv = [v[0], v[1]];
                    }
                    // Color
                    (7, 8) => { // ByteFloat4
                        let v = read_byte_float4(&mut c)?;
                        vertices[k].color = v;
                    }
                    _ => {} // 跳过其他属性 (BlendWeights, Tangent 等)
                }
            }
        }

        // 读取索引
        let idx_offset = lod.index_data_offset + mesh.start_index * 2;
        c.seek(SeekFrom::Start(idx_offset as u64)).map_err(|e| format!("seek index: {e}"))?;
        let mut indices = Vec::with_capacity(mesh.index_count as usize);
        for _ in 0..mesh.index_count {
            indices.push(read_u16(&mut c)?);
        }

        result.push(MeshData { vertices, indices });
    }

    Ok(result)
}

// ---- Half-float 读取 ----

fn read_half2(c: &mut Cursor<&[u8]>) -> Result<[f32; 2], String> {
    let a = read_u16(c)?;
    let b = read_u16(c)?;
    Ok([half_to_f32(a), half_to_f32(b)])
}

fn read_half4(c: &mut Cursor<&[u8]>) -> Result<[f32; 4], String> {
    let a = read_u16(c)?;
    let b = read_u16(c)?;
    let cc = read_u16(c)?;
    let d = read_u16(c)?;
    Ok([half_to_f32(a), half_to_f32(b), half_to_f32(cc), half_to_f32(d)])
}

fn read_byte_float4(c: &mut Cursor<&[u8]>) -> Result<[f32; 4], String> {
    Ok([
        read_u8(c)? as f32 / 255.0,
        read_u8(c)? as f32 / 255.0,
        read_u8(c)? as f32 / 255.0,
        read_u8(c)? as f32 / 255.0,
    ])
}

fn half_to_f32(h: u16) -> f32 {
    let sign = ((h >> 15) & 1) as u32;
    let exp = ((h >> 10) & 0x1F) as u32;
    let mant = (h & 0x3FF) as u32;
    if exp == 0 {
        if mant == 0 { return if sign == 1 { -0.0 } else { 0.0 }; }
        // subnormal
        let v = mant as f32 / 1024.0 * (2.0f32).powi(-14);
        return if sign == 1 { -v } else { v };
    }
    if exp == 31 {
        return if mant == 0 {
            if sign == 1 { f32::NEG_INFINITY } else { f32::INFINITY }
        } else { f32::NAN };
    }
    let bits = (sign << 31) | ((exp + 112) << 23) | (mant << 13);
    f32::from_bits(bits)
}

/// 尝试多个路径加载 MDL，返回第一个成功的结果
pub fn load_mdl_with_fallback(ironworks: &Ironworks, paths: &[String]) -> Result<Vec<MeshData>, String> {
    let mut last_err = String::from("无候选路径");
    for path in paths {
        match load_mdl(ironworks, path) {
            Ok(meshes) if !meshes.is_empty() => return Ok(meshes),
            Ok(_) => { last_err = format!("{}: 网格为空", path); }
            Err(e) => { last_err = format!("{}: {}", path, e); }
        }
    }
    Err(last_err)
}
