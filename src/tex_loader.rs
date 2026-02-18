use crate::game_data::GameData;
use crate::mdl_loader::MeshData;
use physis::mtrl::{ColorDyeTable, ColorTable};
use tomestone_render::{MeshTextures, TextureData};

/// 拼接完整的材质路径
fn resolve_material_path(short_name: &str, set_id: u16, variant_id: u16) -> String {
    format!(
        "chara/equipment/e{:04}/material/v{:04}{}",
        set_id, variant_id, short_name
    )
}

/// 判断纹理路径是否为非 diffuse 纹理 (法线、高光、mask 等)
fn is_non_diffuse_texture(path: &str) -> bool {
    // 旧式后缀
    path.ends_with("_n.tex")
        || path.ends_with("_s.tex")
        || path.ends_with("_m.tex")
        // Dawntrail 新式后缀
        || path.contains("_norm.")
        || path.contains("_mask.")
        || path.contains("_id.")
}

/// 占位符或无效路径 (没有目录结构，无法在 SqPack 中查找)
fn is_placeholder_path(path: &str) -> bool {
    !path.contains('/')
}

/// Linear → sRGB 单通道转换
fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// 从纹理路径列表中查找 _id.tex 路径
fn find_id_texture_path(texture_paths: &[String]) -> Option<String> {
    texture_paths.iter().find(|p| p.contains("_id.")).cloned()
}

/// 从 ColorTable 提取每行的 diffuse 颜色 (linear RGB)
fn extract_diffuse_colors(color_table: &ColorTable) -> Vec<[f32; 3]> {
    match color_table {
        ColorTable::LegacyColorTable(data) => data.rows.iter().map(|r| r.diffuse_color).collect(),
        ColorTable::DawntrailColorTable(data) => {
            data.rows.iter().map(|r| r.diffuse_color).collect()
        }
        ColorTable::OpaqueColorTable(_) => Vec::new(),
    }
}

/// 用 _id.tex + ColorTable 烘焙出伪 diffuse 纹理
/// `dyed_colors` 为可选的染色后颜色数组，如果提供则替代 ColorTable 的 diffuse 颜色
pub fn bake_color_table_texture(
    id_tex: &TextureData,
    color_table: &ColorTable,
    dyed_colors: Option<&Vec<[f32; 3]>>,
) -> TextureData {
    let row_count = match color_table {
        ColorTable::LegacyColorTable(_) => 16,
        ColorTable::DawntrailColorTable(_) => 32,
        ColorTable::OpaqueColorTable(_) => 0,
    };

    let base_colors = extract_diffuse_colors(color_table);

    let pixel_count = (id_tex.width * id_tex.height) as usize;
    let mut rgba = Vec::with_capacity(pixel_count * 4);

    for i in 0..pixel_count {
        let r = id_tex.rgba[i * 4]; // R 通道

        // R 通道映射到 ColorTable 行号
        let row_idx = if row_count == 32 {
            // Dawntrail 32 行: R * 32 / 256 (向下取整，clamp 到 0..31)
            ((r as u32 * 32) / 256).min(31) as usize
        } else if row_count == 16 {
            // Legacy 16 行: R / 17 (每 17 个值一行，0..15)
            (r as usize / 17).min(15)
        } else {
            0
        };

        // 使用染色后颜色或原始 ColorTable 颜色
        let color = if let Some(dyed) = dyed_colors {
            if row_idx < dyed.len() {
                dyed[row_idx]
            } else if row_idx < base_colors.len() {
                base_colors[row_idx]
            } else {
                [1.0, 1.0, 1.0]
            }
        } else if row_idx < base_colors.len() {
            base_colors[row_idx]
        } else {
            [1.0, 1.0, 1.0]
        };

        // linear → sRGB → u8
        rgba.push((linear_to_srgb(color[0]).clamp(0.0, 1.0) * 255.0) as u8);
        rgba.push((linear_to_srgb(color[1]).clamp(0.0, 1.0) * 255.0) as u8);
        rgba.push((linear_to_srgb(color[2]).clamp(0.0, 1.0) * 255.0) as u8);
        rgba.push(255); // Alpha
    }

    TextureData {
        rgba,
        width: id_tex.width,
        height: id_tex.height,
    }
}

/// 加载材质的全部纹理 (diffuse + normal + mask + emissive)
/// 返回 (MeshTextures, CachedMaterial)
fn load_material_textures(
    game: &GameData,
    short_name: &str,
    set_id: u16,
    variant_id: u16,
) -> Option<(MeshTextures, CachedMaterial)> {
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

        let material = match game.parsed_mtrl(material_path) {
            Some(m) => m,
            None => continue,
        };

        // 加载法线贴图
        let normal_tex = find_normal_path(&material.texture_paths).and_then(|p| {
            println!("    法线贴图: {}", p);
            game.parsed_tex(&p)
        });

        // 加载遮罩贴图
        let mask_tex = find_mask_path(&material.texture_paths).and_then(|p| {
            println!("    遮罩贴图: {}", p);
            game.parsed_tex(&p)
        });

        // 从 texture_paths 找 diffuse 纹理: _d.tex → _base.tex → 非法线回退
        let tex_path = find_diffuse_path(&material.texture_paths);
        match tex_path {
            Some(p) => {
                println!("    TEX 路径: {}", p);
                if let Some(tex_data) = game.parsed_tex(&p) {
                    println!("    纹理加载成功: {}x{}", tex_data.width, tex_data.height);
                    let cached = CachedMaterial {
                        color_table: material.color_table,
                        color_dye_table: material.color_dye_table,
                        id_texture: None,
                        uses_color_table: false,
                    };
                    let mesh_tex = MeshTextures {
                        diffuse: tex_data,
                        normal: normal_tex,
                        mask: mask_tex,
                        emissive: None,
                    };
                    return Some((mesh_tex, cached));
                }
                println!("    TEX 解析失败");
            }
            None => {
                // 没有传统 diffuse → 尝试 ColorTable + _id.tex 烘焙
                if let Some(color_table) = &material.color_table {
                    if let Some(id_path) = find_id_texture_path(&material.texture_paths) {
                        println!("    ColorTable 烘焙: {}", id_path);
                        if let Some(id_tex) = game.parsed_tex(&id_path) {
                            let baked = bake_color_table_texture(&id_tex, color_table, None);
                            let emissive = bake_emissive_texture(&id_tex, color_table);
                            // 仅当 emissive 不是 1x1 黑色时才保留
                            let emissive_opt = if emissive.width > 1 {
                                Some(emissive)
                            } else {
                                None
                            };
                            println!("    烘焙成功: {}x{}", baked.width, baked.height);
                            let cached = CachedMaterial {
                                color_table: material.color_table,
                                color_dye_table: material.color_dye_table,
                                id_texture: Some(id_tex),
                                uses_color_table: true,
                            };
                            let mesh_tex = MeshTextures {
                                diffuse: baked,
                                normal: normal_tex,
                                mask: mask_tex,
                                emissive: emissive_opt,
                            };
                            return Some((mesh_tex, cached));
                        }
                        println!("    _id.tex 解析失败");
                    } else {
                        println!("    有 ColorTable 但无 _id.tex");
                    }
                } else {
                    println!("    MTRL 无 diffuse 纹理，也无 ColorTable");
                }
            }
        }
    }
    None
}

/// 从纹理路径列表中找出 diffuse 纹理路径
fn find_diffuse_path(texture_paths: &[String]) -> Option<String> {
    // 优先: _d.tex (旧式 diffuse)
    if let Some(p) = texture_paths.iter().find(|p| p.ends_with("_d.tex")) {
        println!("    diffuse: {}", p);
        return Some(p.clone());
    }
    // 其次: _base.tex (Dawntrail 新式)
    if let Some(p) = texture_paths.iter().find(|p| p.contains("_base.tex")) {
        println!("    base (Dawntrail diffuse): {}", p);
        return Some(p.clone());
    }
    // 回退: 第一个非法线非 mask 的有效纹理
    if let Some(p) = texture_paths
        .iter()
        .find(|p| !is_non_diffuse_texture(p) && !p.is_empty() && !is_placeholder_path(p))
    {
        println!("    回退非法线纹理: {}", p);
        return Some(p.clone());
    }
    None
}

/// 从纹理路径列表中找出法线贴图路径
fn find_normal_path(texture_paths: &[String]) -> Option<String> {
    // 旧式: _n.tex
    if let Some(p) = texture_paths.iter().find(|p| p.ends_with("_n.tex")) {
        return Some(p.clone());
    }
    // Dawntrail: _norm.tex
    if let Some(p) = texture_paths.iter().find(|p| p.contains("_norm.")) {
        return Some(p.clone());
    }
    None
}

/// 从纹理路径列表中找出遮罩/高光贴图路径
fn find_mask_path(texture_paths: &[String]) -> Option<String> {
    // Dawntrail: _mask.tex
    if let Some(p) = texture_paths.iter().find(|p| p.contains("_mask.")) {
        return Some(p.clone());
    }
    // 旧式: _m.tex
    if let Some(p) = texture_paths.iter().find(|p| p.ends_with("_m.tex")) {
        return Some(p.clone());
    }
    // 旧式: _s.tex (specular)
    if let Some(p) = texture_paths.iter().find(|p| p.ends_with("_s.tex")) {
        return Some(p.clone());
    }
    None
}

/// 从 ColorTable 提取每行的 emissive 颜色 (linear RGB)
fn extract_emissive_colors(color_table: &ColorTable) -> Vec<[f32; 3]> {
    match color_table {
        ColorTable::LegacyColorTable(data) => data.rows.iter().map(|r| r.emissive_color).collect(),
        ColorTable::DawntrailColorTable(data) => {
            data.rows.iter().map(|r| r.emissive_color).collect()
        }
        ColorTable::OpaqueColorTable(_) => Vec::new(),
    }
}

/// 用 _id.tex + ColorTable 烘焙出 emissive 纹理
pub fn bake_emissive_texture(id_tex: &TextureData, color_table: &ColorTable) -> TextureData {
    let row_count = match color_table {
        ColorTable::LegacyColorTable(_) => 16,
        ColorTable::DawntrailColorTable(_) => 32,
        ColorTable::OpaqueColorTable(_) => 0,
    };

    let emissive_colors = extract_emissive_colors(color_table);

    // 检查是否有非零 emissive
    let has_emissive = emissive_colors
        .iter()
        .any(|c| c[0] > 0.001 || c[1] > 0.001 || c[2] > 0.001);
    if !has_emissive {
        // 全黑 1x1 占位
        return TextureData {
            rgba: vec![0, 0, 0, 255],
            width: 1,
            height: 1,
        };
    }

    let pixel_count = (id_tex.width * id_tex.height) as usize;
    let mut rgba = Vec::with_capacity(pixel_count * 4);

    for i in 0..pixel_count {
        let r = id_tex.rgba[i * 4];
        let row_idx = if row_count == 32 {
            ((r as u32 * 32) / 256).min(31) as usize
        } else if row_count == 16 {
            (r as usize / 17).min(15)
        } else {
            0
        };

        let color = if row_idx < emissive_colors.len() {
            emissive_colors[row_idx]
        } else {
            [0.0, 0.0, 0.0]
        };

        rgba.push((linear_to_srgb(color[0]).clamp(0.0, 1.0) * 255.0) as u8);
        rgba.push((linear_to_srgb(color[1]).clamp(0.0, 1.0) * 255.0) as u8);
        rgba.push((linear_to_srgb(color[2]).clamp(0.0, 1.0) * 255.0) as u8);
        rgba.push(255);
    }

    TextureData {
        rgba,
        width: id_tex.width,
        height: id_tex.height,
    }
}

/// 1x1 白色回退纹理
fn fallback_white() -> TextureData {
    TextureData {
        rgba: vec![255, 255, 255, 255],
        width: 1,
        height: 1,
    }
}

/// 缓存的材质数据，用于染色重烘焙
pub struct CachedMaterial {
    pub color_table: Option<ColorTable>,
    pub color_dye_table: Option<ColorDyeTable>,
    pub id_texture: Option<TextureData>,
    /// 该材质是否使用了 ColorTable 烘焙 (true) 还是传统 diffuse (false)
    pub uses_color_table: bool,
}

/// 材质加载结果，包含纹理和缓存数据
pub struct MaterialLoadResult {
    pub mesh_textures: Vec<MeshTextures>,
    /// 每个材质索引对应的缓存数据 (key = material_index)
    pub materials: std::collections::HashMap<u16, CachedMaterial>,
}

/// 按 material_index 去重加载纹理，返回与 meshes 一一对应的 MeshTextures + 缓存数据
pub fn load_mesh_textures(
    game: &GameData,
    material_names: &[String],
    meshes: &[MeshData],
    set_id: u16,
    variant_id: u16,
) -> MaterialLoadResult {
    // 缓存已加载的材质索引 -> MeshTextures
    let mut tex_cache: std::collections::HashMap<u16, MeshTextures> =
        std::collections::HashMap::new();
    let mut mat_cache: std::collections::HashMap<u16, CachedMaterial> =
        std::collections::HashMap::new();

    let mut mesh_textures = Vec::with_capacity(meshes.len());
    for mesh in meshes {
        let mat_idx = mesh.material_index;
        if !tex_cache.contains_key(&mat_idx) {
            let (mtex, cached_mat) = if let Some(name) = material_names.get(mat_idx as usize) {
                println!("  材质 [{}]: {}", mat_idx, name);
                match load_material_textures(game, name, set_id, variant_id) {
                    Some((mt, cm)) => {
                        println!(
                            "    纹理加载成功: {}x{} normal={} mask={} emissive={}",
                            mt.diffuse.width,
                            mt.diffuse.height,
                            mt.normal.is_some(),
                            mt.mask.is_some(),
                            mt.emissive.is_some()
                        );
                        (mt, Some(cm))
                    }
                    None => {
                        println!("    纹理加载失败，使用白色回退");
                        (
                            MeshTextures {
                                diffuse: fallback_white(),
                                normal: None,
                                mask: None,
                                emissive: None,
                            },
                            None,
                        )
                    }
                }
            } else {
                println!(
                    "  材质索引 {} 超出范围 (共 {} 个材质名)，使用白色回退",
                    mat_idx,
                    material_names.len()
                );
                (
                    MeshTextures {
                        diffuse: fallback_white(),
                        normal: None,
                        mask: None,
                        emissive: None,
                    },
                    None,
                )
            };
            tex_cache.insert(mat_idx, mtex);
            if let Some(cm) = cached_mat {
                mat_cache.insert(mat_idx, cm);
            }
        }
        // 从缓存复制 (因为每个 mesh 需要自己的数据用于创建 bind group)
        let cached = tex_cache.get(&mat_idx).unwrap();
        mesh_textures.push(MeshTextures {
            diffuse: TextureData {
                rgba: cached.diffuse.rgba.clone(),
                width: cached.diffuse.width,
                height: cached.diffuse.height,
            },
            normal: cached.normal.as_ref().map(|t| TextureData {
                rgba: t.rgba.clone(),
                width: t.width,
                height: t.height,
            }),
            mask: cached.mask.as_ref().map(|t| TextureData {
                rgba: t.rgba.clone(),
                width: t.width,
                height: t.height,
            }),
            emissive: cached.emissive.as_ref().map(|t| TextureData {
                rgba: t.rgba.clone(),
                width: t.width,
                height: t.height,
            }),
        });
    }
    MaterialLoadResult {
        mesh_textures,
        materials: mat_cache,
    }
}
