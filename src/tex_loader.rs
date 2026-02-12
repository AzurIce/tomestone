use ironworks::file::tex::{Format, Texture};
use ironworks::Ironworks;
use std::io::{Cursor, Read as _, Seek, SeekFrom};

use crate::mdl_loader::MeshData;

pub struct TextureData {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// 拼接完整的材质路径
fn resolve_material_path(short_name: &str, set_id: u16, variant_id: u16) -> String {
    format!(
        "chara/equipment/e{:04}/material/v{:04}{}",
        set_id, variant_id, short_name
    )
}

/// 解码纹理数据为 RGBA8
fn decode_texture(data: &[u8], w: u32, h: u32, format: Format) -> Option<Vec<u8>> {
    match format {
        Format::Dxt1 => {
            let block_w = (w + 3) / 4;
            let block_h = (h + 3) / 4;
            let mip0_size = (block_w * block_h * 8) as usize;
            if data.len() < mip0_size { return None; }
            let mut rgba = vec![0u8; (w * h * 4) as usize];
            texpresso::Format::Bc1.decompress(&data[..mip0_size], w as usize, h as usize, &mut rgba);
            Some(rgba)
        }
        Format::Dxt3 => {
            let block_w = (w + 3) / 4;
            let block_h = (h + 3) / 4;
            let mip0_size = (block_w * block_h * 16) as usize;
            if data.len() < mip0_size { return None; }
            let mut rgba = vec![0u8; (w * h * 4) as usize];
            texpresso::Format::Bc2.decompress(&data[..mip0_size], w as usize, h as usize, &mut rgba);
            Some(rgba)
        }
        Format::Dxt5 => {
            let block_w = (w + 3) / 4;
            let block_h = (h + 3) / 4;
            let mip0_size = (block_w * block_h * 16) as usize;
            if data.len() < mip0_size { return None; }
            let mut rgba = vec![0u8; (w * h * 4) as usize];
            texpresso::Format::Bc3.decompress(&data[..mip0_size], w as usize, h as usize, &mut rgba);
            Some(rgba)
        }
        Format::Argb8 => {
            let mip0_size = (w * h * 4) as usize;
            if data.len() < mip0_size { return None; }
            // BGRA -> RGBA swizzle
            let mut rgba = vec![0u8; mip0_size];
            for i in 0..(w * h) as usize {
                let off = i * 4;
                rgba[off]     = data[off + 2]; // R <- B
                rgba[off + 1] = data[off + 1]; // G
                rgba[off + 2] = data[off];     // B <- R
                rgba[off + 3] = data[off + 3]; // A
            }
            Some(rgba)
        }
        Format::Rgba8 => {
            let mip0_size = (w * h * 4) as usize;
            if data.len() < mip0_size { return None; }
            Some(data[..mip0_size].to_vec())
        }
        Format::Rgbx8 => {
            let mip0_size = (w * h * 4) as usize;
            if data.len() < mip0_size { return None; }
            let mut rgba = data[..mip0_size].to_vec();
            for i in 0..(w * h) as usize {
                rgba[i * 4 + 3] = 255;
            }
            Some(rgba)
        }
        _ => None,
    }
}

// ---- 二进制读取工具 ----
fn read_u8(c: &mut Cursor<&[u8]>) -> Option<u8> {
    let mut b = [0u8; 1];
    c.read_exact(&mut b).ok()?;
    Some(b[0])
}
fn read_u16(c: &mut Cursor<&[u8]>) -> Option<u16> {
    let mut b = [0u8; 2];
    c.read_exact(&mut b).ok()?;
    Some(u16::from_le_bytes(b))
}
fn read_u32(c: &mut Cursor<&[u8]>) -> Option<u32> {
    let mut b = [0u8; 4];
    c.read_exact(&mut b).ok()?;
    Some(u32::from_le_bytes(b))
}
fn skip(c: &mut Cursor<&[u8]>, n: u64) -> Option<()> {
    c.seek(SeekFrom::Current(n as i64)).ok()?;
    Some(())
}

/// 从原始 MTRL 字节解析 diffuse 纹理路径
fn parse_mtrl_diffuse_path(data: &[u8]) -> Option<String> {
    let mut c = Cursor::new(data);

    // ---- Container Header (16 bytes) ----
    let _version = read_u32(&mut c)?;
    let _file_size = read_u16(&mut c)?;
    let _data_set_size = read_u16(&mut c)?;
    let string_table_size = read_u16(&mut c)?;
    let _shader_name_offset = read_u16(&mut c)?;
    let texture_count = read_u8(&mut c)?;
    let uv_set_count = read_u8(&mut c)?;
    let color_set_count = read_u8(&mut c)?;
    let _additional_data_size = read_u8(&mut c)?;

    // ---- Texture offsets ----
    let mut tex_offsets = Vec::new();
    for _ in 0..texture_count {
        let offset = read_u16(&mut c)?;
        let _flags = read_u16(&mut c)?;
        tex_offsets.push(offset);
    }

    // ---- Skip UV color sets + color set offsets ----
    skip(&mut c, uv_set_count as u64 * 4)?;
    skip(&mut c, color_set_count as u64 * 4)?;

    // ---- String data ----
    let string_start = c.position() as usize;
    let string_end = string_start + string_table_size as usize;
    if string_end > data.len() { return None; }
    let string_data = &data[string_start..string_end];

    // 从纹理路径中找 _d.tex (diffuse)
    for &off in &tex_offsets {
        let path = read_cstring(string_data, off as usize);
        if path.ends_with("_d.tex") {
            println!("    diffuse: {}", path);
            return Some(path);
        }
    }

    // 没找到 _d.tex，回退使用第一个纹理
    if let Some(&off) = tex_offsets.first() {
        let path = read_cstring(string_data, off as usize);
        if !path.is_empty() {
            println!("    无 _d.tex，回退第一个纹理: {}", path);
            return Some(path);
        }
    }

    println!("    MTRL 无纹理路径");
    None
}

fn read_cstring(data: &[u8], offset: usize) -> String {
    if offset >= data.len() { return String::new(); }
    let end = data[offset..].iter().position(|&b| b == 0).unwrap_or(data.len() - offset);
    String::from_utf8_lossy(&data[offset..offset + end]).to_string()
}

/// 加载 diffuse 纹理，尝试指定 variant 和 v0001 回退
fn load_diffuse_texture(ironworks: &Ironworks, short_name: &str, set_id: u16, variant_id: u16) -> Option<TextureData> {
    let candidates: Vec<String> = if variant_id != 1 {
        vec![
            resolve_material_path(short_name, set_id, variant_id),
            resolve_material_path(short_name, set_id, 1),
        ]
    } else {
        vec![resolve_material_path(short_name, set_id, 1)]
    };

    for material_path in &candidates {
        println!("    尝试 MTRL: {}", material_path);
        let raw: Vec<u8> = match ironworks.file(material_path) {
            Ok(d) => d,
            Err(_) => continue,
        };

        let tex_path = match parse_mtrl_diffuse_path(&raw) {
            Some(p) => p,
            None => continue,
        };

        println!("    TEX 路径: {}", tex_path);
        let tex: Texture = match ironworks.file(&tex_path) {
            Ok(t) => t,
            Err(e) => {
                println!("    TEX 加载失败: {}", e);
                continue;
            }
        };
        let w = tex.width() as u32;
        let h = tex.height() as u32;
        let fmt = tex.format();
        println!("    TEX 信息: {}x{} format={:?} data_len={}", w, h, fmt, tex.data().len());
        match decode_texture(tex.data(), w, h, fmt) {
            Some(rgba) => return Some(TextureData { rgba, width: w, height: h }),
            None => {
                println!("    TEX 解码失败: 不支持的格式或数据不足");
                continue;
            }
        }
    }
    None
}

/// 1x1 白色回退纹理
fn fallback_white() -> TextureData {
    TextureData {
        rgba: vec![255, 255, 255, 255],
        width: 1,
        height: 1,
    }
}

/// 按 material_index 去重加载纹理，返回与 meshes 一一对应的 TextureData
pub fn load_mesh_textures(
    ironworks: &Ironworks,
    material_names: &[String],
    meshes: &[MeshData],
    set_id: u16,
    variant_id: u16,
) -> Vec<TextureData> {
    // 缓存已加载的材质索引 -> TextureData
    let mut cache: std::collections::HashMap<u16, TextureData> = std::collections::HashMap::new();

    let mut result = Vec::with_capacity(meshes.len());
    for mesh in meshes {
        let mat_idx = mesh.material_index;
        if !cache.contains_key(&mat_idx) {
            let tex = if let Some(name) = material_names.get(mat_idx as usize) {
                println!("  材质 [{}]: {}", mat_idx, name);
                match load_diffuse_texture(ironworks, name, set_id, variant_id) {
                    Some(t) => {
                        println!("    纹理加载成功: {}x{}", t.width, t.height);
                        t
                    }
                    None => {
                        println!("    纹理加载失败，使用白色回退");
                        fallback_white()
                    }
                }
            } else {
                println!("  材质索引 {} 超出范围 (共 {} 个材质名)，使用白色回退", mat_idx, material_names.len());
                fallback_white()
            };
            cache.insert(mat_idx, tex);
        }
        // 从缓存复制 (因为每个 mesh 需要自己的 TextureData 用于创建 bind group)
        let cached = cache.get(&mat_idx).unwrap();
        result.push(TextureData {
            rgba: cached.rgba.clone(),
            width: cached.width,
            height: cached.height,
        });
    }
    result
}
