use crate::game_data::GameData;
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

/// 加载 diffuse 纹理，尝试指定 variant 和 v0001 回退
fn load_diffuse_texture(game: &GameData, short_name: &str, set_id: u16, variant_id: u16) -> Option<TextureData> {
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

        let texture_paths = match game.parsed_mtrl(material_path) {
            Some(paths) => paths,
            None => continue,
        };

        // 从 texture_paths 找 diffuse 纹理: _d.tex → _base.tex → 非法线回退
        let tex_path = find_diffuse_path(&texture_paths);
        let tex_path = match tex_path {
            Some(p) => p,
            None => {
                println!("    MTRL 无 diffuse 纹理");
                continue;
            }
        };

        println!("    TEX 路径: {}", tex_path);

        if let Some(tex_data) = game.parsed_tex(&tex_path) {
            println!("    纹理加载成功: {}x{}", tex_data.width, tex_data.height);
            return Some(tex_data);
        }
        println!("    TEX 解析失败");
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
    if let Some(p) = texture_paths.iter().find(|p| !is_non_diffuse_texture(p) && !p.is_empty() && !is_placeholder_path(p)) {
        println!("    回退非法线纹理: {}", p);
        return Some(p.clone());
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
    game: &GameData,
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
                match load_diffuse_texture(game, name, set_id, variant_id) {
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
