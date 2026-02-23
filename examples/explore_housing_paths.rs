//! 探索房屋外装的 SGB 路径模式
//!
//! 用法: cargo run --example explore_housing_paths

use std::path::Path;
use tomestone::game::GameData;

const INSTALL_DIR: &str = r"G:\最终幻想XIV";

fn main() {
    let game = GameData::new(Path::new(INSTALL_DIR));

    // 对 model_key=1，尝试所有可能的 b{n} 前缀
    let model_key = 1u16;
    let id = format!("{:04}", model_key);

    println!("=== model_key={} ===", id);
    for b in 0..=8 {
        let sgb_path = format!(
            "bgcommon/hou/outdoor/general/{}/asset/gar_b0_m{}.sgb",
            id, id
        );
        // 实际上 b 可能在不同位置
        // 尝试 gar_b{n}_m{id}
        let sgb_path = format!(
            "bgcommon/hou/outdoor/general/{}/asset/gar_b{}_m{}.sgb",
            id, b, id
        );
        let exists = game.read_file(&sgb_path).is_ok();
        if exists {
            println!("  b{}: {} -> 存在", b, sgb_path);
        }
    }

    // 也尝试其他前缀模式
    let prefixes = ["rof", "wal", "win", "dor", "opt", "sig", "fen", "gar"];
    for prefix in &prefixes {
        for suffix in ["", "_a", "_b", "_c"] {
            let sgb_path = format!(
                "bgcommon/hou/outdoor/general/{}/asset/{}_b0_m{}{}.sgb",
                id, prefix, id, suffix
            );
            let exists = game.read_file(&sgb_path).is_ok();
            if exists {
                println!("  {}: {} -> 存在", prefix, sgb_path);
            }
        }
    }

    // 尝试更多 model_key
    println!("\n=== 扫描多个 model_key 的 b0~b7 ===");
    for model_key in [1u16, 2, 3, 10, 20, 50] {
        let id = format!("{:04}", model_key);
        let mut found = Vec::new();
        for b in 0..=8 {
            let sgb_path = format!(
                "bgcommon/hou/outdoor/general/{}/asset/gar_b{}_m{}.sgb",
                id, b, id
            );
            if game.read_file(&sgb_path).is_ok() {
                found.push(b);
            }
        }
        println!("  model_key={}: b{:?}", id, found);
    }
}
