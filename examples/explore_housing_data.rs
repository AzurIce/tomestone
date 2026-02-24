//! 探索 HousingExterior 表结构和路径模式
//!
//! 用法: cargo run --example explore_housing_data

use std::path::Path;
use tomestone::game::GameData;

const INSTALL_DIR: &str = r"G:\最终幻想XIV";

fn main() {
    let game = GameData::new(Path::new(INSTALL_DIR));
    let all_items = game.load_all_items();
    let sgb_paths = game.load_housing_sgb_paths();

    // 筛选房屋外装物品
    let housing_items: Vec<_> = all_items
        .iter()
        .filter(|item| item.is_housing_exterior() && sgb_paths.contains_key(&item.additional_data))
        .collect();

    println!("\n=== 按类型分组 ===");
    let types = [
        ("屋根", 65u8),
        ("外壁", 66),
        ("窓", 67),
        ("扉", 68),
        ("屋根装飾", 69),
        ("外壁装飾", 70),
        ("看板", 71),
        ("塀", 72),
    ];

    for (name, _cat) in &types {
        let matching: Vec<_> = housing_items
            .iter()
            .filter(|i| {
                i.exterior_part_type()
                    .map(|pt| pt.display_name() == *name)
                    .unwrap_or(false)
            })
            .collect();
        println!("\n--- {} ({} 件) ---", name, matching.len());
        for item in matching.iter().take(5) {
            println!(
                "  {} additional_data={:04} row_id={}",
                item.name, item.additional_data, item.row_id
            );
        }
    }

    // 对几个不同类型的物品，展示 SGB 路径
    println!("\n=== SGB 路径 ===");
    for item in housing_items.iter().take(20) {
        let pt_name = item
            .exterior_part_type()
            .map(|pt| pt.display_name())
            .unwrap_or("?");
        let paths = sgb_paths.get(&item.additional_data);
        println!(
            "  [{}] {} sgb={:?}",
            pt_name,
            item.name,
            paths.map(|p| p.first().map(|s| s.as_str()).unwrap_or("无"))
        );
    }
}
