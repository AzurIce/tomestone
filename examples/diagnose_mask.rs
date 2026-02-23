//! 诊断遮罩贴图通道值
//!
//! 用法: cargo run --example diagnose_mask

use std::path::Path;
use tomestone::game::GameData;
use tomestone_render::TextureData;

const INSTALL_DIR: &str = r"G:\最终幻想XIV";

fn main() {
    let game = GameData::new(Path::new(INSTALL_DIR));

    let mask_paths = [
        "bgcommon/hou/outdoor/general/0001/texture/gar_b0_m0001_0a_s.tex",
        "bgcommon/hou/outdoor/general/0010/texture/gar_b0_m0010_1a_s.tex",
        "bgcommon/hou/outdoor/general/0020/texture/gar_b0_m0020_0a_s.tex",
        "bgcommon/hou/outdoor/general/0050/texture/gar_b0_m0050_0a_s.tex",
        "bgcommon/texture/dummy_s.tex",
    ];

    for path in &mask_paths {
        println!("\n=== {} ===", path);
        match game.parsed_tex(path) {
            Some(tex) => analyze_channels(&tex),
            None => println!("  加载失败"),
        }
    }
}

fn analyze_channels(tex: &TextureData) {
    let pixel_count = (tex.width * tex.height) as usize;
    println!("  {}x{}, {} pixels", tex.width, tex.height, pixel_count);

    let mut r_sum = 0u64;
    let mut g_sum = 0u64;
    let mut b_sum = 0u64;
    let mut a_sum = 0u64;
    let (mut r_min, mut r_max) = (255u8, 0u8);
    let (mut g_min, mut g_max) = (255u8, 0u8);
    let (mut b_min, mut b_max) = (255u8, 0u8);
    let (mut a_min, mut a_max) = (255u8, 0u8);

    for i in 0..pixel_count {
        let r = tex.rgba[i * 4];
        let g = tex.rgba[i * 4 + 1];
        let b = tex.rgba[i * 4 + 2];
        let a = tex.rgba[i * 4 + 3];
        r_sum += r as u64;
        g_sum += g as u64;
        b_sum += b as u64;
        a_sum += a as u64;
        r_min = r_min.min(r);
        r_max = r_max.max(r);
        g_min = g_min.min(g);
        g_max = g_max.max(g);
        b_min = b_min.min(b);
        b_max = b_max.max(b);
        a_min = a_min.min(a);
        a_max = a_max.max(a);
    }

    let n = pixel_count as f64;
    println!(
        "  R: avg={:.1} min={} max={}",
        r_sum as f64 / n,
        r_min,
        r_max
    );
    println!(
        "  G: avg={:.1} min={} max={}",
        g_sum as f64 / n,
        g_min,
        g_max
    );
    println!(
        "  B: avg={:.1} min={} max={}",
        b_sum as f64 / n,
        b_min,
        b_max
    );
    println!(
        "  A: avg={:.1} min={} max={}",
        a_sum as f64 / n,
        a_min,
        a_max
    );
}
