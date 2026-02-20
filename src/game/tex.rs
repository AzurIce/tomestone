use std::collections::HashMap;

use physis::mtrl::{ColorDyeTable, ColorTable};
use tomestone_render::{MeshTextures, TextureData};

use super::{GameData, MeshData};

fn resolve_material_path(short_name: &str, set_id: u16, variant_id: u16) -> String {
    format!(
        "chara/equipment/e{:04}/material/v{:04}{}",
        set_id, variant_id, short_name
    )
}

fn is_non_diffuse_texture(path: &str) -> bool {
    path.ends_with("_n.tex")
        || path.ends_with("_s.tex")
        || path.ends_with("_m.tex")
        || path.contains("_norm.")
        || path.contains("_mask.")
        || path.contains("_id.")
}

fn is_placeholder_path(path: &str) -> bool {
    !path.contains('/')
}

fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

fn find_id_texture_path(texture_paths: &[String]) -> Option<String> {
    texture_paths.iter().find(|p| p.contains("_id.")).cloned()
}

fn extract_diffuse_colors(color_table: &ColorTable) -> Vec<[f32; 3]> {
    match color_table {
        ColorTable::LegacyColorTable(data) => data.rows.iter().map(|r| r.diffuse_color).collect(),
        ColorTable::DawntrailColorTable(data) => {
            data.rows.iter().map(|r| r.diffuse_color).collect()
        }
        ColorTable::OpaqueColorTable(_) => Vec::new(),
    }
}

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
        let r = id_tex.rgba[i * 4];

        let row_idx = if row_count == 32 {
            ((r as u32 * 32) / 256).min(31) as usize
        } else if row_count == 16 {
            (r as usize / 17).min(15)
        } else {
            0
        };

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

        let normal_tex = find_normal_path(&material.texture_paths).and_then(|p| {
            println!("    法线贴图: {}", p);
            game.parsed_tex(&p)
        });

        let mask_tex = find_mask_path(&material.texture_paths).and_then(|p| {
            println!("    遮罩贴图: {}", p);
            game.parsed_tex(&p)
        });

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
                if let Some(color_table) = &material.color_table {
                    if let Some(id_path) = find_id_texture_path(&material.texture_paths) {
                        println!("    ColorTable 烘焙: {}", id_path);
                        if let Some(id_tex) = game.parsed_tex(&id_path) {
                            let baked = bake_color_table_texture(&id_tex, color_table, None);
                            let emissive = bake_emissive_texture(&id_tex, color_table);
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

fn find_diffuse_path(texture_paths: &[String]) -> Option<String> {
    if let Some(p) = texture_paths.iter().find(|p| p.ends_with("_d.tex")) {
        println!("    diffuse: {}", p);
        return Some(p.clone());
    }
    if let Some(p) = texture_paths.iter().find(|p| p.contains("_base.tex")) {
        println!("    base (Dawntrail diffuse): {}", p);
        return Some(p.clone());
    }
    if let Some(p) = texture_paths
        .iter()
        .find(|p| !is_non_diffuse_texture(p) && !p.is_empty() && !is_placeholder_path(p))
    {
        println!("    回退非法线纹理: {}", p);
        return Some(p.clone());
    }
    None
}

fn find_normal_path(texture_paths: &[String]) -> Option<String> {
    if let Some(p) = texture_paths.iter().find(|p| p.ends_with("_n.tex")) {
        return Some(p.clone());
    }
    if let Some(p) = texture_paths.iter().find(|p| p.contains("_norm.")) {
        return Some(p.clone());
    }
    None
}

fn find_mask_path(texture_paths: &[String]) -> Option<String> {
    if let Some(p) = texture_paths.iter().find(|p| p.contains("_mask.")) {
        return Some(p.clone());
    }
    if let Some(p) = texture_paths.iter().find(|p| p.ends_with("_m.tex")) {
        return Some(p.clone());
    }
    if let Some(p) = texture_paths.iter().find(|p| p.ends_with("_s.tex")) {
        return Some(p.clone());
    }
    None
}

fn extract_emissive_colors(color_table: &ColorTable) -> Vec<[f32; 3]> {
    match color_table {
        ColorTable::LegacyColorTable(data) => data.rows.iter().map(|r| r.emissive_color).collect(),
        ColorTable::DawntrailColorTable(data) => {
            data.rows.iter().map(|r| r.emissive_color).collect()
        }
        ColorTable::OpaqueColorTable(_) => Vec::new(),
    }
}

fn bake_emissive_texture(id_tex: &TextureData, color_table: &ColorTable) -> TextureData {
    let row_count = match color_table {
        ColorTable::LegacyColorTable(_) => 16,
        ColorTable::DawntrailColorTable(_) => 32,
        ColorTable::OpaqueColorTable(_) => 0,
    };

    let emissive_colors = extract_emissive_colors(color_table);

    let has_emissive = emissive_colors
        .iter()
        .any(|c| c[0] > 0.001 || c[1] > 0.001 || c[2] > 0.001);
    if !has_emissive {
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

fn fallback_white() -> TextureData {
    TextureData {
        rgba: vec![255, 255, 255, 255],
        width: 1,
        height: 1,
    }
}

pub struct CachedMaterial {
    pub color_table: Option<ColorTable>,
    pub color_dye_table: Option<ColorDyeTable>,
    pub id_texture: Option<TextureData>,
    pub uses_color_table: bool,
}

pub struct MaterialLoadResult {
    pub mesh_textures: Vec<MeshTextures>,
    pub materials: HashMap<u16, CachedMaterial>,
}

pub fn load_mesh_textures(
    game: &GameData,
    material_names: &[String],
    meshes: &[MeshData],
    set_id: u16,
    variant_id: u16,
) -> MaterialLoadResult {
    let mut tex_cache: HashMap<u16, MeshTextures> = HashMap::new();
    let mut mat_cache: HashMap<u16, CachedMaterial> = HashMap::new();

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
