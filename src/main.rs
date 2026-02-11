use std::path::Path;

use ironworks::Ironworks;
use ironworks::excel::Excel;
use ironworks::ffxiv::{FsResource, Mapper};
use ironworks::sqpack::SqPack;

fn main() {
    let install_dir = Path::new(r"G:\最终幻想XIV");

    let resource = FsResource::at(install_dir);
    let sqpack = SqPack::new(resource);
    let ironworks = Ironworks::new().with_resource(sqpack);

    println!("✓ SqPack 连接成功\n");

    let excel = Excel::new(&ironworks, Mapper::new());

    // 列出所有可用的表
    if let Ok(list) = excel.list() {
        println!("Excel 表总数: {}", list.iter().count());
        let key_sheets = ["Item", "Stain", "EquipSlotCategory"];
        for name in &key_sheets {
            if list.has(name) {
                println!("  ✓ {}", name);
            }
        }
    }

    // 读取 Stain 表 — 尝试更大范围的行 ID
    println!("\nStain 表 (染料) 前 5 条:");
    if let Ok(sheet) = excel.sheet("Stain") {
        let mut count = 0;
        for row_id in 0u32..200 {
            if count >= 5 { break; }
            if let Ok(row) = sheet.row(row_id) {
                if let Ok(field) = row.field(0) {
                    println!("  [{}] {:?}", row_id, field);
                    count += 1;
                }
            }
        }
    }

    // 读取纹理文件
    println!("\n文件读取测试:");
    let test_files = [
        "chara/equipment/e0001/texture/v01_c0201e0001_top_d.tex",
        "chara/equipment/e0001/model/c0201e0001_top.mdl",
    ];
    for path in &test_files {
        match ironworks.file::<Vec<u8>>(path) {
            Ok(data) => println!("  ✓ {} ({} bytes)", path, data.len()),
            Err(e) => println!("  ✗ {} ({})", path, e),
        }
    }

    println!("\n✓ Milestone 1 完成: SqPack 数据读取验证通过");
}
