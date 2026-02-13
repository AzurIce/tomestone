//! 深入分析 stainingtemplate.stm 的数据内容格式
//!
//! 基于 hex dump 发现 keys/offsets 实际为 u32，用正确的 header 解读来分析条目结构。
//!
//! 用法: cargo run --example inspect_stm

use std::fs;

use physis::resource::{Resource as _, SqPackResource};

const GAME_DIR: &str = r"G:\最终幻想XIV\game";
const STM_PATH: &str = "chara/base_material/stainingtemplate.stm";

fn main() {
    // 读取原始数据（如果本地已有导出文件则直接读取）
    let data = if let Ok(d) = fs::read("stainingtemplate.stm") {
        println!("从本地文件 stainingtemplate.stm 读取 ({} 字节)", d.len());
        d
    } else {
        println!("从游戏目录读取...");
        let mut resource = SqPackResource::from_existing(GAME_DIR);
        let d = resource.read(STM_PATH).expect("无法读取 STM");
        fs::write("stainingtemplate.stm", &d).ok();
        d
    };

    println!("文件大小: {} 字节 (0x{:X})\n", data.len(), data.len());

    // ====== Header (u32 解读) ======
    let magic = &data[0..4];
    let entry_count = u32_le(&data, 4) as usize;
    println!("Magic: {:02X} {:02X} {:02X} {:02X} (\"{}{}\")",
        magic[0], magic[1], magic[2], magic[3],
        magic[0] as char, magic[1] as char);
    println!("Entry count: {}\n", entry_count);

    // 读取 keys 和 offsets (u32)
    let keys_start = 8;
    let offsets_start = keys_start + 4 * entry_count;
    let data_base = offsets_start + 4 * entry_count;

    println!("布局:");
    println!("  keys:    0x{:04X} - 0x{:04X} ({} × u32)", keys_start, offsets_start - 1, entry_count);
    println!("  offsets: 0x{:04X} - 0x{:04X} ({} × u32)", offsets_start, data_base - 1, entry_count);
    println!("  data:    0x{:04X} - 0x{:04X}", data_base, data.len() - 1);
    println!();

    let mut keys = Vec::new();
    let mut offsets = Vec::new();
    for i in 0..entry_count {
        keys.push(u32_le(&data, keys_start + 4 * i));
        offsets.push(u32_le(&data, offsets_start + 4 * i));
    }

    // 计算每个条目的大小
    println!("====== 条目概览 ======");
    println!("{:>5}  {:>6}  {:>10}  {:>10}  {:>10}", "idx", "key", "offset", "abs_byte", "size");

    for i in 0..entry_count {
        let off = offsets[i];
        // 尝试两种解读: 直接字节偏移 vs 半字偏移(*2)
        let abs_byte = data_base + off as usize * 2; // 半字偏移
        let next_abs = if i + 1 < entry_count {
            data_base + offsets[i + 1] as usize * 2
        } else {
            data.len()
        };
        let size = next_abs - abs_byte;
        println!("{:>5}  {:>6}  {:>10}  0x{:08X}  {:>10}", i, keys[i], off, abs_byte, size);
    }

    // ====== 深入分析每个条目 ======
    println!("\n====== 条目详细分析 ======");

    for i in 0..entry_count {
        let abs_start = data_base + offsets[i] as usize * 2;
        let abs_end = if i + 1 < entry_count {
            data_base + offsets[i + 1] as usize * 2
        } else {
            data.len()
        };
        let entry_size = abs_end - abs_start;

        println!("\n--- Entry {} (key={}, offset=0x{:X}, size={} 字节) ---",
            i, keys[i], abs_start, entry_size);

        if abs_start + 10 > data.len() {
            println!("  [超出文件范围]");
            continue;
        }

        // 读取 5 个 u16 子表端点
        let mut ends_raw = [0u16; 5];
        for j in 0..5 {
            ends_raw[j] = u16_le(&data, abs_start + 2 * j);
        }
        let entry_data_start = abs_start + 10; // 5 × u16 = 10 字节 header

        println!("  子表端点 (raw u16): {:?}", ends_raw);
        println!("  子表端点 (×2 字节): [{}, {}, {}, {}, {}]",
            ends_raw[0] as u32 * 2, ends_raw[1] as u32 * 2,
            ends_raw[2] as u32 * 2, ends_raw[3] as u32 * 2,
            ends_raw[4] as u32 * 2);

        let ends_bytes: Vec<usize> = ends_raw.iter().map(|&e| e as usize * 2).collect();

        // 验证端点单调递增
        let monotonic = ends_bytes.windows(2).all(|w| w[0] <= w[1]);
        if !monotonic {
            println!("  ⚠ 端点不单调递增！");
        }

        // 分析 5 个子表
        let sub_names = ["diffuse", "specular", "emissive", "gloss", "specular_power"];
        let sub_elem_sizes = [6usize, 6, 6, 2, 2]; // Half3=6, Half1=2

        let mut prev_end = 0usize;
        for (j, name) in sub_names.iter().enumerate() {
            let sub_start = prev_end;
            let sub_end = ends_bytes[j];
            let sub_size = if sub_end >= sub_start { sub_end - sub_start } else { 0 };
            let elem_size = sub_elem_sizes[j];

            let array_size = if elem_size > 0 { sub_size / elem_size } else { 0 };

            let mode = classify_mode(array_size, elem_size, sub_size);

            println!("  [{j}] {name:15} range=[{sub_start:5}..{sub_end:5}] \
                      size={sub_size:5}B  elem={elem_size}B  array_size={array_size:4}  mode={mode}");

            // 深入分析子表内容
            let abs_sub_start = entry_data_start + sub_start;
            if abs_sub_start + sub_size <= data.len() && sub_size > 0 {
                analyze_subtable(&data, abs_sub_start, sub_size, elem_size, array_size, name);
            }

            prev_end = sub_end;
        }

        // 验证: 最后一个端点 + 10 应该等于 entry_size
        let expected_size = ends_bytes[4] + 10;
        if expected_size != entry_size {
            println!("  ⚠ 大小不匹配: 端点指示 {} 字节, 实际 {} 字节 (差 {})",
                expected_size, entry_size, entry_size as i64 - expected_size as i64);
        }
    }
}

fn classify_mode(array_size: usize, _elem_size: usize, _sub_size: usize) -> &'static str {
    if array_size == 0 {
        "Empty"
    } else if array_size == 1 {
        "Singleton"
    } else if array_size >= 128 {
        "OneToOne (≥128)"
    } else {
        "Indexed (<128)"
    }
}

fn analyze_subtable(data: &[u8], offset: usize, size: usize, elem_size: usize, array_size: usize, name: &str) {
    if array_size == 0 {
        return;
    }

    if array_size == 1 {
        // Singleton: 一个值
        print!("      Singleton 值: ");
        print_element(data, offset, elem_size);
        println!();
        return;
    }

    if array_size >= 128 {
        // OneToOne: 直接读前几个值
        print!("      前 3 个值: ");
        for k in 0..3.min(array_size) {
            if k > 0 { print!(", "); }
            print_element(data, offset + k * elem_size, elem_size);
        }
        println!(" ...");
        return;
    }

    // Indexed mode: palette + 0xFF marker + 128 indices
    // palette_count = (size - 129) / elem_size  (含隐含的 index 0 = default)
    if size < 129 + elem_size {
        println!("      ⚠ Indexed 模式但大小太小: {} < {}", size, 129 + elem_size);
        return;
    }

    let palette_count = (size - 129) / elem_size;
    let palette_bytes = palette_count * elem_size;
    let marker_offset = offset + palette_bytes;

    println!("      Palette count: {} (+ 隐含 default)", palette_count);

    // 打印 palette 值
    print!("      Palette: [default]");
    for k in 0..palette_count.min(8) {
        print!(", ");
        print_element(data, offset + k * elem_size, elem_size);
    }
    if palette_count > 8 {
        print!(", ... ({} more)", palette_count - 8);
    }
    println!();

    // 检查 0xFF marker
    if marker_offset < data.len() {
        let marker = data[marker_offset];
        println!("      Marker 字节: 0x{:02X} (期望 0xFF) {}", marker,
            if marker == 0xFF { "✓" } else { "✗ 不匹配!" });
    }

    // 读取 128 个索引
    let indices_offset = marker_offset + 1;
    if indices_offset + 128 <= data.len() {
        let indices = &data[indices_offset..indices_offset + 128];
        let max_idx = indices.iter().copied().max().unwrap_or(0);
        let min_idx = indices.iter().copied().min().unwrap_or(0);
        let unique: std::collections::HashSet<u8> = indices.iter().copied().collect();

        println!("      索引范围: [{}, {}], 唯一值: {} 种", min_idx, max_idx, unique.len());
        print!("      前 16 个索引: ");
        for k in 0..16 {
            print!("{:3} ", indices[k]);
        }
        println!("...");
    }
}

fn print_element(data: &[u8], offset: usize, elem_size: usize) {
    if offset + elem_size > data.len() {
        print!("[OOB]");
        return;
    }

    match elem_size {
        2 => {
            // Half1: 1 个 f16
            let raw = u16_le(data, offset);
            let val = half_to_f32(raw);
            print!("{:.4}", val);
        }
        6 => {
            // Half3: 3 个 f16
            let r = half_to_f32(u16_le(data, offset));
            let g = half_to_f32(u16_le(data, offset + 2));
            let b = half_to_f32(u16_le(data, offset + 4));
            print!("({:.4}, {:.4}, {:.4})", r, g, b);
        }
        _ => {
            print!("[");
            for k in 0..elem_size {
                if k > 0 { print!(" "); }
                print!("{:02X}", data[offset + k]);
            }
            print!("]");
        }
    }
}

fn half_to_f32(bits: u16) -> f32 {
    let sign = ((bits >> 15) & 1) as u32;
    let exp = ((bits >> 10) & 0x1F) as u32;
    let mant = (bits & 0x3FF) as u32;

    if exp == 0 {
        if mant == 0 {
            return f32::from_bits(sign << 31); // ±0
        }
        // subnormal
        let val = (mant as f32) / 1024.0 * 2.0f32.powi(-14);
        if sign == 1 { -val } else { val }
    } else if exp == 31 {
        if mant == 0 {
            if sign == 1 { f32::NEG_INFINITY } else { f32::INFINITY }
        } else {
            f32::NAN
        }
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
