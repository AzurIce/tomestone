mod game_data;

use std::path::Path;
use game_data::GameData;

fn main() {
    let install_dir = Path::new(r"G:\最终幻想XIV");
    let game = GameData::new(install_dir);

    println!("正在加载装备列表...");
    let items = game.load_equipment_list();
    println!("共加载 {} 件装备\n", items.len());

    // 按槽位统计
    let mut by_slot = std::collections::HashMap::new();
    for item in &items {
        *by_slot.entry(item.slot).or_insert(0u32) += 1;
    }
    for (slot, count) in &by_slot {
        println!("  {} ({}): {} 件", slot.display_name(), slot.slot_abbr(), count);
    }

    // 打印前 10 件身体装备
    println!("\n前 10 件身体装备:");
    for item in items.iter().filter(|i| i.slot == game_data::EquipSlot::Body).take(10) {
        println!("  [{}] {} → e{:04}/v{:04} ({})",
            item.row_id, item.name, item.set_id, item.variant_id, item.model_path());
    }

    // 验证模型文件是否存在
    println!("\n模型文件验证:");
    for item in items.iter().filter(|i| i.slot == game_data::EquipSlot::Body).take(3) {
        let path = item.model_path();
        match game.ironworks().file::<Vec<u8>>(&path) {
            Ok(data) => println!("  ✓ {} ({} bytes)", path, data.len()),
            Err(e) => println!("  ✗ {} ({})", path, e),
        }
    }
}
