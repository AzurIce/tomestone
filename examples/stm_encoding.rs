//! 深入分析 STM 子表的编码方式
//!
//! 关键问题: P >= 128 时是 indexed 还是 one-to-one?
//!
//! 用法: cargo run --example stm_encoding

use physis::resource::{Resource as _, SqPackResource};
use physis::ReadableFile;

const GAME_DIR: &str = r"G:\最终幻想XIV\game";

fn main() {
    let mut resource = SqPackResource::from_existing(GAME_DIR);
    let data = resource
        .read("chara/base_material/stainingtemplate.stm")
        .expect("无法读取 STM");

    let entry_count =
        i32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;
    let data_base = 8 + 8 * entry_count;

    println!("====== 分析 P >= 128 vs P < 128 的子表 ======\n");

    for i in 0..entry_count {
        let key = u32_le(&data, 8 + 4 * i);
        let offset = u32_le(&data, 8 + 4 * entry_count + 4 * i);
        let entry_abs = data_base + offset as usize * 2;

        // 读 5 个 ends
        let mut ends = [0u32; 5];
        for j in 0..5 {
            ends[j] = u16_le(&data, entry_abs + 2 * j) as u32 * 2;
        }

        let sub_data_start = entry_abs + 10;

        // 分析 diffuse (Half3, elem_size=6)
        let diffuse_size = ends[0] as usize;
        if diffuse_size < 128 {
            continue;
        }

        let p = (diffuse_size - 128) / 6;

        // 检查: 如果是 indexed, 读 indices 部分
        let idx_offset = sub_data_start + p * 6;
        let indices: Vec<u8> = (0..128).map(|j| data[idx_offset + j]).collect();
        let unique_indices: std::collections::HashSet<u8> = indices.iter().copied().collect();

        // 检查: 如果是 one-to-one, 读前 128 个 Half3
        let oto_values: Vec<[f32; 3]> = (0..128)
            .map(|j| {
                let off = sub_data_start + j * 6;
                [
                    half_to_f32(u16_le(&data, off)),
                    half_to_f32(u16_le(&data, off + 2)),
                    half_to_f32(u16_le(&data, off + 4)),
                ]
            })
            .collect();
        let oto_unique: std::collections::HashSet<[u32; 3]> = oto_values
            .iter()
            .map(|c| [c[0].to_bits(), c[1].to_bits(), c[2].to_bits()])
            .collect();

        if i < 5 || p >= 128 {
            println!(
                "Entry[{:2}] key={:3} | diffuse P={:3} ({}) | indexed: unique_indices={:3} min={} max={} | one-to-one: unique_colors={:3}",
                i, key, p,
                if p >= 128 { "P>=128" } else { "P<128 " },
                unique_indices.len(),
                indices.iter().min().unwrap(),
                indices.iter().max().unwrap(),
                oto_unique.len(),
            );

            // 对于 P >= 128, 额外检查 "indices" 区域的字节模式
            if p >= 128 {
                // 打印 indices 区域前 32 个字节的 hex
                print!("  indices hex (first 32): ");
                for j in 0..32 {
                    print!("{:02X} ", data[idx_offset + j]);
                }
                println!();

                // one-to-one 前 5 个颜色
                println!("  one-to-one 前 5 个 stain 颜色:");
                for j in 0..5 {
                    let [r, g, b] = oto_values[j];
                    println!("    stain[{:3}] = ({:.4}, {:.4}, {:.4})", j, r, g, b);
                }

                // indexed 方式: palette[indices[j]] 前 5 个
                println!("  indexed 前 5 个 stain 颜色:");
                for j in 0..5 {
                    let idx = indices[j] as usize;
                    if idx < p {
                        let off = sub_data_start + idx * 6;
                        let r = half_to_f32(u16_le(&data, off));
                        let g = half_to_f32(u16_le(&data, off + 2));
                        let b = half_to_f32(u16_le(&data, off + 4));
                        println!("    stain[{:3}] → idx={} → ({:.4}, {:.4}, {:.4})", j, idx, r, g, b);
                    }
                }
            }
        }
    }

    // 统计所有 P >= 128 和 P < 128 的子表
    println!("\n====== 所有子表的 P 值统计 ======");
    let sub_names = ["diffuse", "specular", "emissive", "gloss", "spec_power"];
    let elem_sizes = [6usize, 6, 6, 2, 2];

    let mut p_large_count = 0;
    let mut p_small_count = 0;
    let mut p_large_indexed_diverse = 0;
    let mut p_large_oto_diverse = 0;

    for i in 0..entry_count {
        let offset = u32_le(&data, 8 + 4 * entry_count + 4 * i);
        let entry_abs = data_base + offset as usize * 2;

        let mut ends = [0u32; 5];
        for j in 0..5 {
            ends[j] = u16_le(&data, entry_abs + 2 * j) as u32 * 2;
        }

        let sub_data_start = entry_abs + 10;
        let mut prev = 0u32;

        for (j, &elem_size) in elem_sizes.iter().enumerate() {
            let sub_size = (ends[j] - prev) as usize;
            prev = ends[j];

            if sub_size < 128 {
                continue; // empty
            }

            let p = (sub_size - 128) / elem_size;

            if p >= 128 {
                p_large_count += 1;

                // Check indexed diversity
                let idx_offset = sub_data_start + (ends[j] as usize - sub_size) + p * elem_size;
                // Actually compute absolute sub-table start
                let sub_abs_start = if j == 0 {
                    sub_data_start
                } else {
                    sub_data_start + ends[j - 1] as usize
                };
                let idx_abs = sub_abs_start + p * elem_size;
                let indices: Vec<u8> = (0..128).map(|k| data[idx_abs + k]).collect();
                let unique_idx: std::collections::HashSet<u8> = indices.iter().copied().collect();

                // Check one-to-one diversity
                let oto_unique = {
                    let mut set = std::collections::HashSet::new();
                    for k in 0..128 {
                        let off = sub_abs_start + k * elem_size;
                        let mut val = Vec::new();
                        for b in 0..elem_size {
                            val.push(data[off + b]);
                        }
                        set.insert(val);
                    }
                    set.len()
                };

                if unique_idx.len() > 3 {
                    p_large_indexed_diverse += 1;
                }
                if oto_unique > 3 {
                    p_large_oto_diverse += 1;
                }
            } else {
                p_small_count += 1;
            }
        }
    }

    println!("P < 128 子表数量: {}", p_small_count);
    println!("P >= 128 子表数量: {}", p_large_count);
    println!("P >= 128 且 indexed 模式唯一索引 > 3: {}", p_large_indexed_diverse);
    println!("P >= 128 且 one-to-one 模式唯一颜色 > 3: {}", p_large_oto_diverse);

    // 检查 P < 128 的子表是否有合理的索引变化
    println!("\n====== P < 128 子表采样 ======");
    let mut checked = 0;
    for i in 0..entry_count {
        let key = u32_le(&data, 8 + 4 * i);
        let offset = u32_le(&data, 8 + 4 * entry_count + 4 * i);
        let entry_abs = data_base + offset as usize * 2;

        let mut ends = [0u32; 5];
        for j in 0..5 {
            ends[j] = u16_le(&data, entry_abs + 2 * j) as u32 * 2;
        }

        let sub_data_start = entry_abs + 10;

        // Check specular (usually P < 128)
        let spec_size = (ends[1] - ends[0]) as usize;
        if spec_size >= 128 {
            let p = (spec_size - 128) / 6;
            if p > 1 && p < 128 {
                let spec_start = sub_data_start + ends[0] as usize;
                let idx_offset = spec_start + p * 6;
                let indices: Vec<u8> = (0..128).map(|j| data[idx_offset + j]).collect();
                let unique: std::collections::HashSet<u8> = indices.iter().copied().collect();

                if checked < 5 {
                    println!("  Entry[{}] key={} specular P={} unique_indices={}", i, key, p, unique.len());
                    print!("    前 16 个索引: ");
                    for j in 0..16 {
                        print!("{:3} ", indices[j]);
                    }
                    println!();
                    checked += 1;
                }
            }
        }
    }

    // ====== 检查 Dawntrail 模板 ID 1200 是否可能是不同编码 ======
    println!("\n====== Dawntrail 模板 ID 分析 ======");
    println!("STM 模板 ID 范围: 100-612");
    println!("Dawntrail MTRL 中的模板 ID: 1200, 1500");
    println!("可能的映射方式:");
    println!("  1200 / 10 = 120 (不在STM中)");
    println!("  1200 % 1000 = 200 (在STM中!)");
    println!("  1500 % 1000 = 500 (在STM中!)");
    println!("  1200 >> 1 = 600 (在STM中!)");
    println!("  1500 >> 1 = 750 (不在STM中)");

    // 检查更多 Dawntrail MTRL 的模板 ID
    println!("\n====== 扫描更多 Dawntrail MTRL ======");
    let test_ids = [801, 802, 803, 804, 805, 810, 820, 830, 840, 850, 860];
    for set_id in test_ids {
        let path = format!("chara/equipment/e{:04}/material/v0001/mt_c0201e{:04}_top_a.mtrl", set_id, set_id);
        if let Some(mtrl_data) = resource.read(&path) {
            if let Some(mtrl) = physis::mtrl::Material::from_existing(physis::Platform::Win32, &mtrl_data) {
                if let Some(physis::mtrl::ColorDyeTable::DawntrailColorDyeTable(dt)) = &mtrl.color_dye_table {
                    let templates: Vec<u16> = dt.rows.iter()
                        .filter(|r| r.diffuse)
                        .map(|r| r.template)
                        .collect();
                    let unique_templates: std::collections::HashSet<u16> = templates.iter().copied().collect();
                    if !unique_templates.is_empty() {
                        println!("  e{:04}: templates = {:?}", set_id, unique_templates);
                    }
                }
                if let Some(physis::mtrl::ColorDyeTable::LegacyColorDyeTable(dt)) = &mtrl.color_dye_table {
                    let templates: Vec<u16> = dt.rows.iter()
                        .filter(|r| r.diffuse)
                        .map(|r| r.template)
                        .collect();
                    let unique_templates: std::collections::HashSet<u16> = templates.iter().copied().collect();
                    if !unique_templates.is_empty() {
                        println!("  e{:04} (legacy): templates = {:?}", set_id, unique_templates);
                    }
                }
            }
        }
    }

    // 也试试经典装备
    println!("\n====== Legacy MTRL 模板 ID ======");
    let legacy_ids = [51, 60, 80, 100, 150, 200];
    for set_id in legacy_ids {
        let path = format!("chara/equipment/e{:04}/material/v0001/mt_c0201e{:04}_top_a.mtrl", set_id, set_id);
        if let Some(mtrl_data) = resource.read(&path) {
            if let Some(mtrl) = physis::mtrl::Material::from_existing(physis::Platform::Win32, &mtrl_data) {
                if let Some(physis::mtrl::ColorDyeTable::LegacyColorDyeTable(dt)) = &mtrl.color_dye_table {
                    let templates: Vec<u16> = dt.rows.iter()
                        .filter(|r| r.diffuse)
                        .map(|r| r.template)
                        .collect();
                    let unique_templates: std::collections::HashSet<u16> = templates.iter().copied().collect();
                    if !unique_templates.is_empty() {
                        println!("  e{:04} (legacy): templates = {:?}", set_id, unique_templates);
                    }
                }
                if let Some(physis::mtrl::ColorDyeTable::DawntrailColorDyeTable(dt)) = &mtrl.color_dye_table {
                    let templates: Vec<u16> = dt.rows.iter()
                        .filter(|r| r.diffuse)
                        .map(|r| r.template)
                        .collect();
                    let unique_templates: std::collections::HashSet<u16> = templates.iter().copied().collect();
                    if !unique_templates.is_empty() {
                        println!("  e{:04} (dawntrail): templates = {:?}", set_id, unique_templates);
                    }
                }
            }
        }
    }
}

fn half_to_f32(bits: u16) -> f32 {
    let sign = ((bits >> 15) & 1) as u32;
    let exp = ((bits >> 10) & 0x1F) as u32;
    let mant = (bits & 0x3FF) as u32;
    if exp == 0 {
        if mant == 0 { return f32::from_bits(sign << 31); }
        let val = (mant as f32) / 1024.0 * 2.0f32.powi(-14);
        if sign == 1 { -val } else { val }
    } else if exp == 31 {
        if mant == 0 { if sign == 1 { f32::NEG_INFINITY } else { f32::INFINITY } }
        else { f32::NAN }
    } else {
        let f_exp = (exp as i32) - 15 + 127;
        let f_bits = (sign << 31) | ((f_exp as u32) << 23) | (mant << 13);
        f32::from_bits(f_bits)
    }
}

fn u32_le(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([data[offset], data[offset+1], data[offset+2], data[offset+3]])
}

fn u16_le(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([data[offset], data[offset+1]])
}
