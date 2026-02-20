use std::collections::HashMap;

use physis::mtrl::{ColorDyeTable, ColorTable};
use physis::stm::StainingTemplate;

use crate::game::CachedMaterial;

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
                        if let Some(pack) = stm.get_dye_pack(dye_row.template, stain_index) {
                            return pack.diffuse;
                        }
                    }
                    row.diffuse_color
                })
                .collect()
        }
        (ColorTable::DawntrailColorTable(ct), ColorDyeTable::DawntrailColorDyeTable(dt)) => ct
            .rows
            .iter()
            .zip(dt.rows.iter())
            .map(|(row, dye_row)| {
                let ch = (dye_row.channel as usize).min(1);
                let stain_id = stain_ids[ch];
                if stain_id > 0 && dye_row.diffuse {
                    let stain_index = (stain_id - 1) as usize;
                    if let Some(pack) = stm.get_dye_pack(dye_row.template, stain_index) {
                        return pack.diffuse;
                    }
                }
                row.diffuse_color
            })
            .collect(),
        _ => match color_table {
            ColorTable::LegacyColorTable(ct) => ct.rows.iter().map(|r| r.diffuse_color).collect(),
            ColorTable::DawntrailColorTable(ct) => {
                ct.rows.iter().map(|r| r.diffuse_color).collect()
            }
            ColorTable::OpaqueColorTable(_) => Vec::new(),
        },
    }
}

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
