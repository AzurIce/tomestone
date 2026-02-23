//! 探索 HousingExterior 表结构和路径模式
//!
//! 用法: cargo run --example explore_housing_data

use std::path::Path;
use tomestone::game::GameData;

const INSTALL_DIR: &str = r"G:\最终幻想XIV";

fn main() {
    let game = GameData::new(Path::new(INSTALL_DIR));
    let items = game.load_housing_exterior_list();

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
        let matching: Vec<_> = items
            .iter()
            .filter(|i| i.part_type.display_name() == *name)
            .collect();
        println!("\n--- {} ({} 件) ---", name, matching.len());
        for item in matching.iter().take(5) {
            println!(
                "  {} model_key={:04} row_id={}",
                item.name, item.model_key, item.row_id
            );
        }
        // 统计 model_key 分布
        let mut keys: Vec<u16> = matching.iter().map(|i| i.model_key).collect();
        keys.sort();
        keys.dedup();
        println!("  唯一 model_key: {:?}", &keys[..keys.len().min(20)]);
    }

    // 对几个不同类型的 model_key，尝试不同路径模式
    println!("\n=== 路径探测 ===");
    let test_items: Vec<_> = items
        .iter()
        .filter(|i| {
            i.part_type.display_name() == "屋根"
                || i.part_type.display_name() == "外壁"
                || i.part_type.display_name() == "窓"
                || i.part_type.display_name() == "扉"
        })
        .take(20)
        .collect();

    let path_patterns = [
        ("gar_b0_m", "sgb"),
        ("rof_b0_m", "sgb"),
        ("wal_b0_m", "sgb"),
        ("win_b0_m", "sgb"),
        ("dor_b0_m", "sgb"),
        ("opt_b0_m", "sgb"),
        ("sig_b0_m", "sgb"),
        ("fen_b0_m", "sgb"),
    ];

    for item in &test_items {
        let id = format!("{:04}", item.model_key);
        let mut found = Vec::new();
        for (prefix, ext) in &path_patterns {
            let path = format!(
                "bgcommon/hou/outdoor/general/{}/asset/{}{}.{}",
                id, prefix, id, ext
            );
            if game.read_file(&path).is_ok() {
                found.push(*prefix);
            }
        }
        println!(
            "  [{}] {} model_key={}: {:?}",
            item.part_type.display_name(),
            item.name,
            item.model_key,
            found
        );
    }
}
