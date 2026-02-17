use std::collections::HashMap;

use physis::mtrl::{ColorDyeTable, ColorTable};
use physis::stm::StainingTemplate;

use crate::tex_loader::CachedMaterial;

/// 对 ColorTable 的每一行应用染色，返回染色后的 diffuse 颜色数组 (linear RGB)。
///
/// - `color_table`: 材质的 ColorTable
/// - `dye_table`: 材质的 ColorDyeTable（决定哪些行可染色、使用哪个模板）
/// - `stm`: 染色模板数据
/// - `stain_ids`: 双通道染料 ID [通道0, 通道1]（1-based, 0 表示无染料）
///
/// 返回值: 每行一个 [f32; 3] diffuse 颜色，行数与 ColorTable 行数一致
pub fn apply_dye(
    color_table: &ColorTable,
    dye_table: &ColorDyeTable,
    stm: &StainingTemplate,
    stain_ids: [u32; 2],
) -> Vec<[f32; 3]> {
    match (color_table, dye_table) {
        (ColorTable::LegacyColorTable(ct), ColorDyeTable::LegacyColorDyeTable(dt)) => {
            let stain_id = stain_ids[0];
            ct.rows
                .iter()
                .zip(dt.rows.iter())
                .map(|(row, dye_row)| {
                    if stain_id > 0 && dye_row.diffuse {
                        let stain_index = (stain_id - 1) as usize;
                        if let Some(pack) =
                            stm.get_dye_pack(dye_row.template, stain_index)
                        {
                            return pack.diffuse;
                        }
                    }
                    row.diffuse_color
                })
                .collect()
        }
        (ColorTable::DawntrailColorTable(ct), ColorDyeTable::DawntrailColorDyeTable(dt)) => {
            ct.rows
                .iter()
                .zip(dt.rows.iter())
                .map(|(row, dye_row)| {
                    let ch = (dye_row.channel as usize).min(1);
                    let stain_id = stain_ids[ch];
                    if stain_id > 0 && dye_row.diffuse {
                        let stain_index = (stain_id - 1) as usize;
                        if let Some(pack) =
                            stm.get_dye_pack(dye_row.template, stain_index)
                        {
                            return pack.diffuse;
                        }
                    }
                    row.diffuse_color
                })
                .collect()
        }
        // ColorTable 和 DyeTable 类型不匹配或不支持时，返回原始颜色
        _ => match color_table {
            ColorTable::LegacyColorTable(ct) => {
                ct.rows.iter().map(|r| r.diffuse_color).collect()
            }
            ColorTable::DawntrailColorTable(ct) => {
                ct.rows.iter().map(|r| r.diffuse_color).collect()
            }
            ColorTable::OpaqueColorTable(_) => Vec::new(),
        },
    }
}

/// 检测材质集合中是否有任何 Dawntrail 染色行的 channel > 0（即支持双染色）
pub fn has_dual_dye(materials: &HashMap<u16, CachedMaterial>) -> bool {
    for mat in materials.values() {
        if let Some(ColorDyeTable::DawntrailColorDyeTable(dt)) = &mat.color_dye_table {
            if dt.rows.iter().any(|row| row.channel > 0) {
                return true;
            }
        }
    }
    false
}
