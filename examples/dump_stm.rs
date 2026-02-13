//! 导出并解析 stainingtemplate.stm
//!
//! 用法:
//!   cargo run --example dump_stm
//!   cargo run --example dump_stm -- --raw output.stm
//!   cargo run --example dump_stm -- --csv output.csv
//!
//! 默认行为: 导出原始二进制到 stainingtemplate.stm，并在终端打印解析摘要。
//! --raw <path>  指定原始二进制输出路径
//! --csv <path>  额外导出为 CSV 格式 (template_id, stain_index, diffuse_r, diffuse_g, diffuse_b, ...)

use std::fs;

use physis::Platform;
use physis::resource::{Resource as _, SqPackResource};
use physis::stm::StainingTemplate;
use physis::ReadableFile;

const GAME_DIR: &str = r"G:\最终幻想XIV\game";
const STM_PATH: &str = "chara/base_material/stainingtemplate.stm";

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let mut raw_output = String::from("stainingtemplate.stm");
    let mut csv_output: Option<String> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--raw" => {
                i += 1;
                raw_output = args.get(i).expect("--raw 需要文件路径参数").clone();
            }
            "--csv" => {
                i += 1;
                csv_output = Some(args.get(i).expect("--csv 需要文件路径参数").clone());
            }
            other => {
                eprintln!("未知参数: {}", other);
                std::process::exit(1);
            }
        }
        i += 1;
    }

    // 1. 从 SqPack 读取原始字节
    println!("正在从游戏目录读取 STM 文件...");
    println!("游戏目录: {}", GAME_DIR);
    println!("文件路径: {}", STM_PATH);

    let mut resource = SqPackResource::from_existing(GAME_DIR);
    let raw_bytes = resource
        .read(STM_PATH)
        .expect("无法读取 stainingtemplate.stm，请确认游戏目录正确");

    println!("原始文件大小: {} 字节 ({:.1} KB)", raw_bytes.len(), raw_bytes.len() as f64 / 1024.0);

    // 2. 导出原始二进制
    fs::write(&raw_output, &raw_bytes).expect("无法写入输出文件");
    println!("已导出原始二进制: {}", raw_output);

    // 3. 解析并打印摘要
    println!("\n====== 文件头解析 ======");
    dump_header(&raw_bytes);

    println!("\n====== 解析后数据摘要 ======");
    let stm = StainingTemplate::from_existing(Platform::Win32, &raw_bytes)
        .expect("STM 解析失败");

    println!("共 {} 个模板条目", stm.entries.len());

    let mut keys: Vec<u16> = stm.entries.keys().copied().collect();
    keys.sort();

    println!("\n模板 ID 列表 (前 30 个):");
    for (i, key) in keys.iter().take(30).enumerate() {
        if i > 0 && i % 10 == 0 {
            println!();
        }
        print!("{:5} ", key);
    }
    if keys.len() > 30 {
        print!("... (共 {})", keys.len());
    }
    println!();

    // 打印几个示例条目的详细数据
    println!("\n====== 示例条目详情 ======");
    let sample_keys: Vec<u16> = keys.iter().take(3).copied().collect();
    for key in &sample_keys {
        let entry = &stm.entries[key];
        println!("\n--- Template ID: {} ---", key);
        println!("  diffuse:        {} 条目", entry.diffuse.len());
        println!("  specular:       {} 条目", entry.specular.len());
        println!("  emissive:       {} 条目", entry.emissive.len());
        println!("  gloss:          {} 条目", entry.gloss.len());
        println!("  specular_power: {} 条目", entry.specular_power.len());

        // 打印前 5 个染料的 diffuse 颜色
        println!("  diffuse 前 5 个染料颜色:");
        for si in 0..5.min(entry.diffuse.len()) {
            let c = entry.diffuse[si];
            println!(
                "    [{:3}] R={:.4} G={:.4} B={:.4}",
                si, c[0], c[1], c[2]
            );
        }
    }

    // 4. 可选: CSV 导出
    if let Some(csv_path) = &csv_output {
        println!("\n正在导出 CSV: {}", csv_path);
        export_csv(&stm, &keys, csv_path);
        println!("CSV 导出完成");
    }

    println!("\n完成！");
}

/// 手动解析文件头，打印原始偏移信息
fn dump_header(data: &[u8]) {
    if data.len() < 8 {
        println!("文件太小，无法解析头部");
        return;
    }

    let unknown = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    let entry_count = i32::from_le_bytes([data[4], data[5], data[6], data[7]]);

    println!("偏移 0x00: 未知字段  = 0x{:08X} ({})", unknown, unknown);
    println!("偏移 0x04: entry_count = {}", entry_count);

    if entry_count <= 0 || entry_count > 10000 {
        println!("entry_count 异常，跳过后续解析");
        return;
    }

    let n = entry_count as usize;
    let keys_start = 8;
    let offsets_start = keys_start + 2 * n;
    let data_base = offsets_start + 2 * n; // = 8 + 4 * n

    println!("keys 区域:    0x{:04X} - 0x{:04X} ({} 字节)", keys_start, offsets_start - 1, 2 * n);
    println!("offsets 区域: 0x{:04X} - 0x{:04X} ({} 字节)", offsets_start, data_base - 1, 2 * n);
    println!("数据区起始:   0x{:04X}", data_base);

    // 打印所有 key/offset 对
    println!("\n全部 {} 个 key/offset 对:", n);
    println!("  {:>5}  {:>6}  {:>10}  {:>10}", "idx", "key", "offset_raw", "abs_offset");
    for i in 0..n {
        let key_pos = keys_start + 2 * i;
        let off_pos = offsets_start + 2 * i;
        if off_pos + 1 >= data.len() {
            break;
        }
        let key = u16::from_le_bytes([data[key_pos], data[key_pos + 1]]);
        let offset_raw = u16::from_le_bytes([data[off_pos], data[off_pos + 1]]);
        let abs_offset = offset_raw as usize * 2 + data_base;
        println!(
            "  {:>5}  {:>6}  {:>10}  0x{:08X}",
            i, key, offset_raw, abs_offset
        );
    }

    // 解析第一个条目的子表端点
    if n > 0 {
        let first_off_pos = offsets_start;
        let first_offset_raw = u16::from_le_bytes([data[first_off_pos], data[first_off_pos + 1]]);
        let entry_start = first_offset_raw as usize * 2 + data_base;

        if entry_start + 10 <= data.len() {
            println!("\n第一个条目 (偏移 0x{:04X}) 的子表端点:", entry_start);
            for j in 0..5 {
                let pos = entry_start + 2 * j;
                let end_raw = u16::from_le_bytes([data[pos], data[pos + 1]]);
                let end_bytes = end_raw as usize * 2;
                let label = match j {
                    0 => "diffuse",
                    1 => "specular",
                    2 => "emissive",
                    3 => "gloss",
                    4 => "specular_power",
                    _ => "?",
                };
                println!(
                    "  ends[{}] ({:14}) = {:5} (raw) → {:5} 字节",
                    j, label, end_raw, end_bytes
                );
            }

            // 计算各子表大小 (用 i64 避免下溢)
            let mut ends = [0i64; 5];
            for j in 0..5 {
                let pos = entry_start + 2 * j;
                ends[j] = u16::from_le_bytes([data[pos], data[pos + 1]]) as i64 * 2;
            }
            println!("\n  子表字节大小:");
            let starts = [0i64, ends[0], ends[1], ends[2], ends[3]];
            let labels = [
                ("diffuse (Half3, 6B/elem)", 6),
                ("specular (Half3, 6B/elem)", 6),
                ("emissive (Half3, 6B/elem)", 6),
                ("gloss (Half1, 2B/elem)", 2),
                ("specular_power (Half1, 2B/elem)", 2),
            ];
            for (k, ((name, elem_size), (&start, &end))) in labels
                .iter()
                .zip(starts.iter().zip(ends.iter()))
                .enumerate()
            {
                let size = end - start;
                let (array_size, mode) = if size <= 0 {
                    (0, "Empty/Invalid")
                } else {
                    let as_ = size / elem_size;
                    let m = if as_ == 1 {
                        "Singleton"
                    } else if as_ >= 128 {
                        "OneToOne"
                    } else {
                        "Indexed"
                    };
                    (as_, m)
                };
                let _ = k;
                println!(
                    "    {:35} {:6} 字节, array_size={:4}, 模式={}",
                    name, size, array_size, mode
                );
            }
        }
    }

    // Hex dump 前 256 字节
    let dump_len = 256.min(data.len());
    println!("\n====== Hex Dump (前 {} 字节) ======", dump_len);
    for row in 0..(dump_len + 15) / 16 {
        let offset = row * 16;
        print!("{:08X}  ", offset);
        for col in 0..16 {
            let pos = offset + col;
            if pos < dump_len {
                print!("{:02X} ", data[pos]);
            } else {
                print!("   ");
            }
            if col == 7 {
                print!(" ");
            }
        }
        print!(" |");
        for col in 0..16 {
            let pos = offset + col;
            if pos < dump_len {
                let b = data[pos];
                if b.is_ascii_graphic() || b == b' ' {
                    print!("{}", b as char);
                } else {
                    print!(".");
                }
            }
        }
        println!("|");
    }

    // 替代解读: keys 为 u32 (hex dump 显示 4 字节对齐)
    println!("\n====== 替代解读: keys 作为 u32 ======");
    println!("注意: hex dump 显示 keys 实际占 4 字节 (64 00 00 00 = 100),");
    println!("而 physis 按 u16 读取，导致交替出现 (value, 0) 模式。");
    println!("以下按 u32 重新解读 keys 数组:\n");

    let keys_u32_end = 8 + 4 * n;
    if keys_u32_end + 4 * n <= data.len() {
        // 如果 keys 是 u32，offsets 也应该在 keys 之后
        let offsets_u32_start = keys_u32_end;
        println!("  keys (u32):    0x{:04X} - 0x{:04X}", 8, keys_u32_end - 1);
        println!("  offsets (u32): 0x{:04X} - 0x{:04X}", offsets_u32_start, offsets_u32_start + 4 * n - 1);
        println!();
        println!("  {:>5}  {:>6}  {:>10}", "idx", "key", "offset_u32");
        for i in 0..n {
            let kp = 8 + 4 * i;
            let op = offsets_u32_start + 4 * i;
            if op + 3 >= data.len() {
                break;
            }
            let key = u32::from_le_bytes([data[kp], data[kp + 1], data[kp + 2], data[kp + 3]]);
            let off = u32::from_le_bytes([data[op], data[op + 1], data[op + 2], data[op + 3]]);
            println!("  {:>5}  {:>6}  {:>10}", i, key, off);
        }
    }

    // 也试试 keys/offsets 交替作为 (u16 key, u16 offset) 对
    println!("\n====== 替代解读: (u16 key, u16 offset) 交替对 ======");
    println!("  {:>5}  {:>6}  {:>10}  {:>10}", "idx", "key", "offset", "abs_off");
    let pair_start = 8;
    for i in 0..n {
        let pos = pair_start + 4 * i;
        if pos + 3 >= data.len() {
            break;
        }
        let key = u16::from_le_bytes([data[pos], data[pos + 1]]);
        let off = u16::from_le_bytes([data[pos + 2], data[pos + 3]]);
        let abs = off as usize * 2 + data_base;
        println!("  {:>5}  {:>6}  {:>10}  0x{:08X}", i, key, off, abs);
    }
}

/// 导出完整 CSV
fn export_csv(stm: &StainingTemplate, keys: &[u16], path: &str) {
    let mut lines = Vec::new();
    lines.push(
        "template_id,stain_index,\
         diffuse_r,diffuse_g,diffuse_b,\
         specular_r,specular_g,specular_b,\
         emissive_r,emissive_g,emissive_b,\
         gloss,specular_power"
            .to_string(),
    );

    for &key in keys {
        let entry = &stm.entries[&key];
        for si in 0..128 {
            let d = entry.diffuse.get(si).copied().unwrap_or([0.0; 3]);
            let s = entry.specular.get(si).copied().unwrap_or([0.0; 3]);
            let e = entry.emissive.get(si).copied().unwrap_or([0.0; 3]);
            let g = entry.gloss.get(si).copied().unwrap_or(0.0);
            let sp = entry.specular_power.get(si).copied().unwrap_or(0.0);
            lines.push(format!(
                "{},{},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6}",
                key, si, d[0], d[1], d[2], s[0], s[1], s[2], e[0], e[1], e[2], g, sp
            ));
        }
    }

    fs::write(path, lines.join("\n")).expect("无法写入 CSV 文件");
}
