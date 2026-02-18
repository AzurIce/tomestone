use crate::game_data::GameData;
use std::io::{Cursor, Read, Seek, SeekFrom};
use tomestone_render::{BoundingBox, Vertex};

/// MDL 解析结果
pub struct MdlResult {
    pub meshes: Vec<MeshData>,
    pub material_names: Vec<String>,
    pub bone_names: Vec<String>,
    pub bone_tables: Vec<MdlBoneTable>,
}

/// 从 MDL 文件提取的渲染用网格数据
#[derive(Clone)]
pub struct MeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u16>,
    pub material_index: u16,
    pub bone_table_index: u16,
    pub skin_vertices: Vec<SkinVertex>,
}

/// 每顶点蒙皮数据（不上传 GPU）
#[derive(Clone, Debug)]
pub struct SkinVertex {
    pub blend_weights: [f32; 4],
    pub blend_indices: [u8; 4],
}

/// 骨骼表：将顶点 blend_index 映射到模型骨骼名称索引
#[derive(Clone, Debug)]
pub struct MdlBoneTable {
    pub bone_indices: Vec<u16>,
}

/// 计算网格数据的包围盒
pub fn compute_bounding_box(meshes: &[MeshData]) -> BoundingBox {
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    for mesh in meshes {
        for v in &mesh.vertices {
            for i in 0..3 {
                if v.position[i] < min[i] {
                    min[i] = v.position[i];
                }
                if v.position[i] > max[i] {
                    max[i] = v.position[i];
                }
            }
        }
    }
    if min[0] == f32::MAX {
        return BoundingBox {
            min: [0.0; 3],
            max: [0.0; 3],
        };
    }
    BoundingBox { min, max }
}

/// 从 ironworks/physis 加载 MDL 并提取网格数据 (支持 v5/v6 Dawntrail 格式)
pub fn load_mdl(game: &GameData, path: &str) -> Result<MdlResult, String> {
    let data = game.read_file(path)?;
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
    c.seek(SeekFrom::Current(n))
        .map_err(|e| format!("skip: {e}"))?;
    Ok(())
}

// ---- 顶点声明 ----

const VERTEX_ELEMENT_SLOTS: usize = 17;

#[derive(Clone, Copy, Debug)]
struct VertexElement {
    stream: u8,
    offset: u8,
    format: u8, // 2=Single3, 3=Single4, 5=Byte4, 8=ByteFloat4, 13=Half2, 14=Half4
    usage: u8,  // 0=Position, 1=BlendWeight, 2=BlendIndex, 3=Normal, 4=UV, 6=BiTangent, 7=Color
}

fn read_vertex_declarations(
    c: &mut Cursor<&[u8]>,
    count: u16,
) -> Result<Vec<Vec<VertexElement>>, String> {
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
            elements.push(VertexElement {
                stream,
                offset,
                format,
                usage,
            });
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
    material_index: u16,
    bone_table_index: u16,
    vertex_buffer_offset: [u32; 3],
    vertex_buffer_stride: [u8; 3],
}

struct MdlLod {
    mesh_index: u16,
    mesh_count: u16,
    vertex_data_offset: u32,
    index_data_offset: u32,
}

/// 在字符串块中按偏移查找 null 结尾字符串
fn string_at_offset(block: &[u8], offset: u32) -> String {
    let start = offset as usize;
    if start >= block.len() {
        return String::new();
    }
    let end = block[start..]
        .iter()
        .position(|&b| b == 0)
        .map(|p| start + p)
        .unwrap_or(block.len());
    std::str::from_utf8(&block[start..end])
        .unwrap_or("")
        .to_string()
}

fn parse_mdl(data: &[u8]) -> Result<MdlResult, String> {
    let mut c = Cursor::new(data);

    // ---- File Header (68 bytes) ----
    let version = read_u32(&mut c)?;
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
    let string_start = c.position();
    let string_end = string_start + string_size as u64;
    let string_block = data[string_start as usize..string_end as usize].to_vec();
    c.seek(SeekFrom::Start(string_end))
        .map_err(|e| format!("seek past strings: {e}"))?;

    // ---- Model Header ----
    let _radius = read_f32(&mut c)?;
    let mesh_count = read_u16(&mut c)?;
    let attribute_count = read_u16(&mut c)?;
    let submesh_count = read_u16(&mut c)?;
    let material_count = read_u16(&mut c)?;
    let bone_count = read_u16(&mut c)?;
    let bone_table_count = read_u16(&mut c)?;
    let _shape_count = read_u16(&mut c)?;
    let _shape_mesh_count = read_u16(&mut c)?;
    let _shape_value_count = read_u16(&mut c)?;
    let _lod_count = read_u8(&mut c)?;
    let _flags1 = read_u8(&mut c)?;
    let element_id_count = read_u16(&mut c)?;
    let terrain_shadow_mesh_count = read_u8(&mut c)?;
    let flags2 = read_u8(&mut c)?;
    skip(&mut c, 4 + 4)?; // clip distances
    let _unknown4 = read_u16(&mut c)?;
    let terrain_shadow_submesh_count = read_u16(&mut c)?;
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
        lods.push(MdlLod {
            mesh_index,
            mesh_count: mesh_count_lod,
            vertex_data_offset,
            index_data_offset,
        });
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
        let material_index = read_u16(&mut c)?;
        let _submesh_index = read_u16(&mut c)?;
        let _submesh_count = read_u16(&mut c)?;
        let bone_table_index = read_u16(&mut c)?;
        let start_index = read_u32(&mut c)?;
        let vbo0 = read_u32(&mut c)?;
        let vbo1 = read_u32(&mut c)?;
        let vbo2 = read_u32(&mut c)?;
        let vbs0 = read_u8(&mut c)?;
        let vbs1 = read_u8(&mut c)?;
        let vbs2 = read_u8(&mut c)?;
        let _stream_count = read_u8(&mut c)?;
        meshes.push(MdlMesh {
            vertex_count,
            index_count,
            start_index,
            material_index,
            bone_table_index,
            vertex_buffer_offset: [vbo0, vbo1, vbo2],
            vertex_buffer_stride: [vbs0, vbs1, vbs2],
        });
    }

    // ---- 元数据: 骨骼名称 & 骨骼表 ----
    // 跳过 attribute_name_offsets
    skip(&mut c, attribute_count as i64 * 4)?;
    // 跳过 terrain_shadow_meshes (每个 20 字节)
    skip(&mut c, terrain_shadow_mesh_count as i64 * 20)?;
    // 跳过 submeshes (每个 16 字节)
    skip(&mut c, submesh_count as i64 * 16)?;
    // 跳过 terrain_shadow_submeshes (每个 12 字节)
    skip(&mut c, terrain_shadow_submesh_count as i64 * 12)?;

    // 读取 material_name_offsets
    let mut material_name_offsets = Vec::with_capacity(material_count as usize);
    for _ in 0..material_count {
        material_name_offsets.push(read_u32(&mut c)?);
    }

    // 读取 bone_name_offsets
    let mut bone_name_offsets = Vec::with_capacity(bone_count as usize);
    for _ in 0..bone_count {
        bone_name_offsets.push(read_u32(&mut c)?);
    }

    // 解析骨骼表
    let bone_tables = if version <= 0x1000005 {
        // V1: 固定 132 字节 = [u16; 64](128B) + u8 count + 3B padding
        let mut tables = Vec::with_capacity(bone_table_count as usize);
        for _ in 0..bone_table_count {
            let mut indices = [0u16; 64];
            for idx in &mut indices {
                *idx = read_u16(&mut c)?;
            }
            let count = read_u8(&mut c)?;
            skip(&mut c, 3)?; // padding
            tables.push(MdlBoneTable {
                bone_indices: indices[..count as usize].to_vec(),
            });
        }
        tables
    } else {
        // V2: 可变长度
        let mut offset_counts = Vec::with_capacity(bone_table_count as usize);
        for _ in 0..bone_table_count {
            let _offset = read_u16(&mut c)?;
            let count = read_u16(&mut c)?;
            offset_counts.push(count);
        }
        let mut tables = Vec::with_capacity(bone_table_count as usize);
        for &count in &offset_counts {
            let mut indices = Vec::with_capacity(count as usize);
            for _ in 0..count {
                indices.push(read_u16(&mut c)?);
            }
            // 4 字节对齐
            let pos = c.position() as i64;
            let padding = if pos % 4 == 0 { 0 } else { 4 - (pos % 4) };
            if padding > 0 {
                skip(&mut c, padding)?;
            }
            tables.push(MdlBoneTable {
                bone_indices: indices,
            });
        }
        tables
    };

    // 从偏移量解析名称
    let material_names: Vec<String> = material_name_offsets
        .iter()
        .map(|&off| string_at_offset(&string_block, off))
        .collect();

    let bone_names: Vec<String> = bone_name_offsets
        .iter()
        .map(|&off| string_at_offset(&string_block, off))
        .collect();

    // ---- 提取 LOD 0 (High) 的网格数据 ----
    let lod = &lods[0];
    let mut result = Vec::new();

    for mi in lod.mesh_index..(lod.mesh_index + lod.mesh_count) {
        let mesh = &meshes[mi as usize];
        let decl = &decls[mi as usize];
        if mesh.vertex_count == 0 {
            continue;
        }

        let mut vertices = vec![
            Vertex {
                position: [0.0; 3],
                normal: [0.0, 1.0, 0.0],
                uv: [0.0; 2],
                color: [1.0, 1.0, 1.0, 1.0],
                tangent: [1.0, 0.0, 0.0, 1.0]
            };
            mesh.vertex_count as usize
        ];
        let mut skin_vertices = vec![
            SkinVertex {
                blend_weights: [0.0; 4],
                blend_indices: [0; 4]
            };
            mesh.vertex_count as usize
        ];

        for k in 0..mesh.vertex_count as usize {
            for elem in decl {
                let abs_offset = lod.vertex_data_offset
                    + mesh.vertex_buffer_offset[elem.stream as usize]
                    + elem.offset as u32
                    + mesh.vertex_buffer_stride[elem.stream as usize] as u32 * k as u32;

                c.seek(SeekFrom::Start(abs_offset as u64))
                    .map_err(|e| format!("seek vertex: {e}"))?;

                match (elem.usage, elem.format) {
                    // Position
                    (0, 2) => {
                        // Single3
                        vertices[k].position =
                            [read_f32(&mut c)?, read_f32(&mut c)?, read_f32(&mut c)?];
                    }
                    (0, 3) => {
                        // Single4
                        vertices[k].position =
                            [read_f32(&mut c)?, read_f32(&mut c)?, read_f32(&mut c)?];
                    }
                    (0, 14) => {
                        // Half4
                        let v = read_half4(&mut c)?;
                        vertices[k].position = [v[0], v[1], v[2]];
                    }
                    // BlendWeight
                    (1, 8) | (1, 5) => {
                        // ByteFloat4 or Byte4
                        skin_vertices[k].blend_weights = read_byte_float4(&mut c)?;
                    }
                    // BlendIndex
                    (2, 5) => {
                        // Byte4 (4 raw u8)
                        skin_vertices[k].blend_indices = [
                            read_u8(&mut c)?,
                            read_u8(&mut c)?,
                            read_u8(&mut c)?,
                            read_u8(&mut c)?,
                        ];
                    }
                    // Normal
                    (3, 2) => {
                        // Single3
                        vertices[k].normal =
                            [read_f32(&mut c)?, read_f32(&mut c)?, read_f32(&mut c)?];
                    }
                    (3, 3) => {
                        // Single4
                        vertices[k].normal =
                            [read_f32(&mut c)?, read_f32(&mut c)?, read_f32(&mut c)?];
                    }
                    (3, 14) => {
                        // Half4
                        let v = read_half4(&mut c)?;
                        vertices[k].normal = [v[0], v[1], v[2]];
                    }
                    (3, 8) => {
                        // ByteFloat4 (packed normal)
                        let v = read_byte_float4(&mut c)?;
                        vertices[k].normal = [v[0] * 2.0 - 1.0, v[1] * 2.0 - 1.0, v[2] * 2.0 - 1.0];
                    }
                    // UV
                    (4, 1) => {
                        // Single2
                        vertices[k].uv = [read_f32(&mut c)?, read_f32(&mut c)?];
                    }
                    (4, 13) => {
                        // Half2
                        vertices[k].uv = read_half2(&mut c)?;
                    }
                    (4, 14) => {
                        // Half4 (take first 2)
                        let v = read_half4(&mut c)?;
                        vertices[k].uv = [v[0], v[1]];
                    }
                    // Color
                    (7, 8) => {
                        // ByteFloat4
                        vertices[k].color = read_byte_float4(&mut c)?;
                    }
                    // Tangent1 (BiTangent)
                    (6, 14) => {
                        // Half4 (already in [-1,1] range)
                        let v = read_half4(&mut c)?;
                        vertices[k].tangent = v;
                    }
                    (6, 8) => {
                        // ByteFloat4 (packed tangent, [0,1] → [-1,1])
                        let v = read_byte_float4(&mut c)?;
                        vertices[k].tangent = [
                            v[0] * 2.0 - 1.0,
                            v[1] * 2.0 - 1.0,
                            v[2] * 2.0 - 1.0,
                            v[3] * 2.0 - 1.0,
                        ];
                    }
                    _ => {} // 跳过其他属性
                }
            }
        }

        // 读取索引
        let idx_offset = lod.index_data_offset + mesh.start_index * 2;
        c.seek(SeekFrom::Start(idx_offset as u64))
            .map_err(|e| format!("seek index: {e}"))?;
        let mut indices = Vec::with_capacity(mesh.index_count as usize);
        for _ in 0..mesh.index_count {
            indices.push(read_u16(&mut c)?);
        }

        result.push(MeshData {
            vertices,
            indices,
            material_index: mesh.material_index,
            bone_table_index: mesh.bone_table_index,
            skin_vertices,
        });
    }

    Ok(MdlResult {
        meshes: result,
        material_names,
        bone_names,
        bone_tables,
    })
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
    Ok([
        half_to_f32(a),
        half_to_f32(b),
        half_to_f32(cc),
        half_to_f32(d),
    ])
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
        if mant == 0 {
            return if sign == 1 { -0.0 } else { 0.0 };
        }
        // subnormal
        let v = mant as f32 / 1024.0 * (2.0f32).powi(-14);
        return if sign == 1 { -v } else { v };
    }
    if exp == 31 {
        return if mant == 0 {
            if sign == 1 {
                f32::NEG_INFINITY
            } else {
                f32::INFINITY
            }
        } else {
            f32::NAN
        };
    }
    let bits = (sign << 31) | ((exp + 112) << 23) | (mant << 13);
    f32::from_bits(bits)
}

/// 尝试多个路径加载 MDL，返回第一个成功的结果
pub fn load_mdl_with_fallback(game: &GameData, paths: &[String]) -> Result<MdlResult, String> {
    let mut last_err = String::from("无候选路径");
    for path in paths {
        match load_mdl(game, path) {
            Ok(result) if !result.meshes.is_empty() => return Ok(result),
            Ok(_) => {
                last_err = format!("{}: 网格为空", path);
            }
            Err(e) => {
                last_err = format!("{}: {}", path, e);
            }
        }
    }
    Err(last_err)
}
