//! 诊断染色管线: 验证 STM 解析 + ColorDyeTable + 染色应用
//!
//! 用法: cargo run --example diagnose_dye

use physis::mtrl::{ColorDyeTable, ColorTable, Material};
use physis::resource::{Resource as _, SqPackResource};
use physis::stm::StainingTemplate;
use physis::{Platform, ReadableFile};

const GAME_DIR: &str = r"G:\最终幻想XIV\game";

fn main() {
    let mut resource = SqPackResource::from_existing(GAME_DIR);

    // ====== 1. 验证 STM 加载 ======
    println!("====== STM 加载验证 ======");
    let stm_data = resource
        .read("chara/base_material/stainingtemplate.stm")
        .expect("无法读取 STM");
    println!("STM 原始大小: {} 字节", stm_data.len());

    // 手动检查 header
    let magic = u32::from_le_bytes([stm_data[0], stm_data[1], stm_data[2], stm_data[3]]);
    let entry_count =
        i32::from_le_bytes([stm_data[4], stm_data[5], stm_data[6], stm_data[7]]) as usize;
    println!("Magic: 0x{:08X}", magic);
    println!("Entry count (raw): {}", entry_count);

    let data_base = 8 + 8 * entry_count;
    println!("Data base: 0x{:X} ({})", data_base, data_base);

    // 打印所有 template_id
    print!("Template IDs: ");
    for i in 0..entry_count {
        let pos = 8 + 4 * i;
        let id = u32::from_le_bytes([
            stm_data[pos],
            stm_data[pos + 1],
            stm_data[pos + 2],
            stm_data[pos + 3],
        ]);
        if i > 0 {
            print!(", ");
        }
        print!("{}", id);
    }
    println!();

    // 解析 STM
    let stm = StainingTemplate::from_existing(Platform::Win32, &stm_data).expect("STM 解析失败");
    println!("\nphysis 解析得到 {} 个条目", stm.entries.len());

    // 列出所有 key
    let mut keys: Vec<u16> = stm.entries.keys().copied().collect();
    keys.sort();
    print!("HashMap keys: ");
    for (i, k) in keys.iter().enumerate() {
        if i > 0 {
            print!(", ");
        }
        print!("{}", k);
    }
    println!();

    // 验证每个条目的数据长度
    println!("\n====== 条目数据验证 ======");
    for &key in &keys {
        let entry = &stm.entries[&key];
        let d_len = entry.diffuse.len();
        let s_len = entry.specular.len();
        let e_len = entry.emissive.len();
        let g_len = entry.gloss.len();
        let sp_len = entry.specular_power.len();

        let all_128 = d_len == 128 && s_len == 128 && e_len == 128 && g_len == 128 && sp_len == 128;
        if !all_128 {
            println!(
                "  ✗ key={}: d={} s={} e={} g={} sp={} (应全为 128!)",
                key, d_len, s_len, e_len, g_len, sp_len
            );
        }
    }
    println!("  所有条目数据长度检查完毕");

    // 抽样检查: 第一个 template 的前 5 个染料的 diffuse 颜色
    if let Some(&first_key) = keys.first() {
        let entry = &stm.entries[&first_key];
        println!("\n--- Template {} 的 diffuse 前 10 个染料 ---", first_key);
        for i in 0..10.min(entry.diffuse.len()) {
            let [r, g, b] = entry.diffuse[i];
            println!("  stain[{:3}]: ({:.4}, {:.4}, {:.4})", i, r, g, b);
        }

        // 检查是否全为零
        let all_zero = entry.diffuse.iter().all(|c| c[0] == 0.0 && c[1] == 0.0 && c[2] == 0.0);
        if all_zero {
            println!("  ⚠ 全部为零! 解析可能有问题");
        }

        // 检查有多少不同的颜色
        let unique: std::collections::HashSet<[u32; 3]> = entry
            .diffuse
            .iter()
            .map(|c| [c[0].to_bits(), c[1].to_bits(), c[2].to_bits()])
            .collect();
        println!("  唯一颜色数: {} / 128", unique.len());
    }

    // ====== 2. 手动验证一个条目的 raw 数据 ======
    println!("\n====== 手动验证第一个条目 raw 数据 ======");
    {
        let first_offset_pos = 8 + 4 * entry_count;
        let first_offset = u32::from_le_bytes([
            stm_data[first_offset_pos],
            stm_data[first_offset_pos + 1],
            stm_data[first_offset_pos + 2],
            stm_data[first_offset_pos + 3],
        ]);
        let entry_abs = data_base + first_offset as usize * 2;
        println!("第一个条目: offset_raw={}, abs=0x{:X}", first_offset, entry_abs);

        // 读取 5 个 u16 ends
        let mut ends = [0u16; 5];
        for j in 0..5 {
            let pos = entry_abs + 2 * j;
            ends[j] = u16::from_le_bytes([stm_data[pos], stm_data[pos + 1]]);
        }
        println!(
            "ends (raw u16): {:?} → bytes: [{}, {}, {}, {}, {}]",
            ends,
            ends[0] as u32 * 2,
            ends[1] as u32 * 2,
            ends[2] as u32 * 2,
            ends[3] as u32 * 2,
            ends[4] as u32 * 2,
        );

        let data_start = entry_abs + 10;
        let diffuse_size = ends[0] as usize * 2;
        println!("diffuse 子表: offset=0x{:X}, size={} bytes", data_start, diffuse_size);

        let palette_count = (diffuse_size - 128) / 6;
        println!("diffuse palette_count = ({} - 128) / 6 = {}", diffuse_size, palette_count);

        // 读取前 3 个 palette 值 (f16 × 3)
        println!("前 3 个 palette 值 (raw Half3):");
        for p in 0..3.min(palette_count) {
            let pos = data_start + p * 6;
            let r = u16::from_le_bytes([stm_data[pos], stm_data[pos + 1]]);
            let g = u16::from_le_bytes([stm_data[pos + 2], stm_data[pos + 3]]);
            let b = u16::from_le_bytes([stm_data[pos + 4], stm_data[pos + 5]]);
            println!(
                "  palette[{}]: raw=({:04X}, {:04X}, {:04X}) → ({:.4}, {:.4}, {:.4})",
                p,
                r,
                g,
                b,
                half_to_f32(r),
                half_to_f32(g),
                half_to_f32(b)
            );
        }

        // 读取前 16 个 indices
        let idx_offset = data_start + palette_count * 6;
        print!("前 16 个 indices: ");
        for j in 0..16 {
            print!("{:3} ", stm_data[idx_offset + j]);
        }
        println!();

        // 解引用: stain[0] = palette[indices[0]]
        for si in 0..5 {
            let idx = stm_data[idx_offset + si] as usize;
            let pos = data_start + idx * 6;
            let r = half_to_f32(u16::from_le_bytes([stm_data[pos], stm_data[pos + 1]]));
            let g = half_to_f32(u16::from_le_bytes([stm_data[pos + 2], stm_data[pos + 3]]));
            let b = half_to_f32(u16::from_le_bytes([stm_data[pos + 4], stm_data[pos + 5]]));
            println!(
                "  stain[{}] → idx={} → palette[{}] = ({:.4}, {:.4}, {:.4})",
                si, idx, idx, r, g, b
            );
        }

        // 与 physis 解析结果对比
        let first_key = u32::from_le_bytes([stm_data[8], stm_data[9], stm_data[10], stm_data[11]])
            as u16;
        if let Some(entry) = stm.entries.get(&first_key) {
            println!("\nphysis 解析结果对比:");
            for si in 0..5 {
                let [r, g, b] = entry.diffuse[si];
                println!("  stain[{}] = ({:.4}, {:.4}, {:.4})", si, r, g, b);
            }
        }
    }

    // ====== 3. 加载一个 Dawntrail 装备的 MTRL 检查 ColorDyeTable ======
    println!("\n====== Dawntrail 装备 MTRL 检查 ======");
    // 尝试 e0800 (典型 Dawntrail 装备)
    let test_mtrls = [
        "chara/equipment/e0800/material/v0001/mt_c0201e0800_top_a.mtrl",
        "chara/equipment/e0801/material/v0001/mt_c0201e0801_top_a.mtrl",
        "chara/equipment/e0060/material/v0001/mt_c0201e0060_top_a.mtrl",
    ];

    for mtrl_path in &test_mtrls {
        println!("\n--- {} ---", mtrl_path);
        let mtrl_data = match resource.read(mtrl_path) {
            Some(d) => d,
            None => {
                println!("  无法读取");
                continue;
            }
        };
        let mtrl = match Material::from_existing(Platform::Win32, &mtrl_data) {
            Some(m) => m,
            None => {
                println!("  解析失败");
                continue;
            }
        };

        println!("  纹理: {:?}", mtrl.texture_paths);
        println!("  Shader: {}", mtrl.shader_package_name);

        match &mtrl.color_table {
            Some(ColorTable::LegacyColorTable(ct)) => {
                println!("  ColorTable: Legacy, {} 行", ct.rows.len());
                for (i, row) in ct.rows.iter().enumerate().take(4) {
                    println!(
                        "    行[{}] diffuse=({:.3},{:.3},{:.3})",
                        i, row.diffuse_color[0], row.diffuse_color[1], row.diffuse_color[2]
                    );
                }
            }
            Some(ColorTable::DawntrailColorTable(ct)) => {
                println!("  ColorTable: Dawntrail, {} 行", ct.rows.len());
                for (i, row) in ct.rows.iter().enumerate().take(4) {
                    println!(
                        "    行[{}] diffuse=({:.3},{:.3},{:.3})",
                        i, row.diffuse_color[0], row.diffuse_color[1], row.diffuse_color[2]
                    );
                }
            }
            Some(ColorTable::OpaqueColorTable(_)) => println!("  ColorTable: Opaque"),
            None => println!("  无 ColorTable"),
        }

        match &mtrl.color_dye_table {
            Some(ColorDyeTable::LegacyColorDyeTable(dt)) => {
                println!("  ColorDyeTable: Legacy, {} 行", dt.rows.len());
                for (i, row) in dt.rows.iter().enumerate().take(4) {
                    println!(
                        "    行[{}] template={} diffuse={} specular={} emissive={} gloss={}",
                        i, row.template, row.diffuse, row.specular, row.emissive, row.gloss
                    );
                    // 检查 template 是否在 STM 中
                    if row.diffuse {
                        match stm.entries.get(&row.template) {
                            Some(entry) => {
                                // 打印 stain=0 的染色
                                let [r, g, b] = entry.diffuse[0];
                                println!(
                                    "      → STM[{}].diffuse[0] = ({:.4}, {:.4}, {:.4})",
                                    row.template, r, g, b
                                );
                            }
                            None => {
                                println!("      ⚠ template {} 在 STM 中不存在!", row.template);
                            }
                        }
                    }
                }
            }
            Some(ColorDyeTable::DawntrailColorDyeTable(dt)) => {
                println!("  ColorDyeTable: Dawntrail, {} 行", dt.rows.len());
                for (i, row) in dt.rows.iter().enumerate().take(8) {
                    println!(
                        "    行[{}] template={} ch={} diffuse={} specular={} emissive={}",
                        i, row.template, row.channel, row.diffuse, row.specular, row.emissive
                    );
                    if row.diffuse {
                        // 使用 get_dye_pack 来测试 Dawntrail >= 1000 映射
                        match stm.get_dye_pack(row.template, 0) {
                            Some(pack) => {
                                let [r, g, b] = pack.diffuse;
                                println!(
                                    "      → get_dye_pack({}, 0).diffuse = ({:.4}, {:.4}, {:.4})",
                                    row.template, r, g, b
                                );
                            }
                            None => {
                                println!("      ⚠ template {} 映射后仍不存在!", row.template);
                            }
                        }
                    }
                }
            }
            None => println!("  无 ColorDyeTable"),
            _ => println!("  ColorDyeTable: 其他类型"),
        }
    }

    // ====== 4. 模拟染色: 选择几种知名染料 ======
    println!("\n====== 模拟染色 ======");
    // Stain ID 1 = 雪白 (Snow White), 2 = 灰白 (Ash Grey), 等
    // stain_index = stain_id - 1
    let test_stains = [1u32, 2, 3, 4, 5, 36]; // 1=Snow White, 36=Soot Black
    if let Some(&first_key) = keys.first() {
        let entry = &stm.entries[&first_key];
        println!("Template {} 的各染料 diffuse 颜色:", first_key);
        for &stain_id in &test_stains {
            let si = (stain_id - 1) as usize;
            if si < entry.diffuse.len() {
                let [r, g, b] = entry.diffuse[si];
                // 转 sRGB 并映射到 u8
                let sr = linear_to_srgb(r);
                let sg = linear_to_srgb(g);
                let sb = linear_to_srgb(b);
                println!(
                    "  stain_id={:3} → linear({:.4},{:.4},{:.4}) → sRGB({:.4},{:.4},{:.4}) → u8({},{},{})",
                    stain_id, r, g, b, sr, sg, sb,
                    (sr.clamp(0.0, 1.0) * 255.0) as u8,
                    (sg.clamp(0.0, 1.0) * 255.0) as u8,
                    (sb.clamp(0.0, 1.0) * 255.0) as u8,
                );
            }
        }
    }
}

fn half_to_f32(bits: u16) -> f32 {
    let sign = ((bits >> 15) & 1) as u32;
    let exp = ((bits >> 10) & 0x1F) as u32;
    let mant = (bits & 0x3FF) as u32;

    if exp == 0 {
        if mant == 0 {
            return f32::from_bits(sign << 31);
        }
        let val = (mant as f32) / 1024.0 * 2.0f32.powi(-14);
        if sign == 1 {
            -val
        } else {
            val
        }
    } else if exp == 31 {
        if mant == 0 {
            if sign == 1 {
                f32::NEG_INFINITY
            } else {
                f32::INFINITY
            }
        } else {
            f32::NAN
        }
    } else {
        let f_exp = (exp as i32) - 15 + 127;
        let f_bits = (sign << 31) | ((f_exp as u32) << 23) | (mant << 13);
        f32::from_bits(f_bits)
    }
}

fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}
