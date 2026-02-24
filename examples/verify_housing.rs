//! 验证不同外装类型加载不同模型
//!
//! 用法: cargo run --example verify_housing

use std::path::Path;
use tomestone::game::{extract_mdl_paths_from_sgb, load_mdl, GameData};

const INSTALL_DIR: &str = r"G:\最终幻想XIV";

fn main() {
    let game = GameData::new(Path::new(INSTALL_DIR));
    let all_items = game.load_all_items();
    let sgb_map = game.load_housing_sgb_paths();

    // 筛选房屋外装物品
    let housing_items: Vec<_> = all_items
        .iter()
        .filter(|i| i.is_housing_exterior() && sgb_map.contains_key(&i.additional_data))
        .collect();

    // 每种类型取前 3 个
    let types = [
        "屋根",
        "外壁",
        "窓",
        "扉",
        "屋根装飾",
        "外壁装飾",
        "看板",
        "塀",
    ];
    for type_name in &types {
        println!("\n=== {} ===", type_name);
        let matching: Vec<_> = housing_items
            .iter()
            .filter(|i| {
                i.exterior_part_type()
                    .map(|pt| pt.display_name() == *type_name)
                    .unwrap_or(false)
            })
            .take(3)
            .collect();

        for item in &matching {
            println!("  {}", item.name);
            if let Some(sgb_paths) = sgb_map.get(&item.additional_data) {
                for sgb_path in sgb_paths {
                    print!("    SGB: {} -> ", sgb_path);
                    match game.read_file(sgb_path) {
                        Ok(data) => {
                            let mdl_paths = extract_mdl_paths_from_sgb(&data);
                            println!("MDL: {:?}", mdl_paths);
                            for mdl_path in &mdl_paths {
                                match load_mdl(&game, mdl_path) {
                                    Ok(r) => println!(
                                        "      mesh={} mat={} verts={}",
                                        r.meshes.len(),
                                        r.material_names.len(),
                                        r.meshes.iter().map(|m| m.vertices.len()).sum::<usize>()
                                    ),
                                    Err(e) => println!("      MDL 失败: {}", e),
                                }
                            }
                        }
                        Err(e) => println!("读取失败: {}", e),
                    }
                }
            }
        }
    }
}
