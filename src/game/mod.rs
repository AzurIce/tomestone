mod mdl;
mod skeleton;
mod tex;

pub use mdl::{compute_bounding_box, load_mdl, load_mdl_with_fallback, MdlBoneTable, MeshData};
pub use skeleton::{apply_skinning, SkeletonCache};
pub use tex::{bake_color_table_texture, load_mesh_textures, CachedMaterial};

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use physis::excel::{Field, Row};
use physis::mtrl::{ColorDyeTable, ColorTable};
use physis::resource::{Resource as _, SqPackResource};
use physis::stm::StainingTemplate;
use physis::Language;

use tomestone_render::TextureData;

use crate::domain::{EquipSlot, EquipmentItem, StainEntry};

pub struct ParsedMaterial {
    pub texture_paths: Vec<String>,
    pub color_table: Option<ColorTable>,
    pub color_dye_table: Option<ColorDyeTable>,
}

pub fn validate_install_dir(install_dir: &Path) -> Result<(), String> {
    let sqpack = install_dir.join("game").join("sqpack");
    if !sqpack.is_dir() {
        return Err(format!("未找到 sqpack 目录: {}", sqpack.display()));
    }
    Ok(())
}

pub struct GameData {
    game_dir: PathBuf,
    physis: RefCell<SqPackResource>,
}

impl GameData {
    pub fn new(install_dir: &Path) -> Self {
        let game_dir = install_dir.join("game");
        let physis = RefCell::new(SqPackResource::from_existing(game_dir.to_str().unwrap()));
        Self { game_dir, physis }
    }

    pub fn sqpack_dir(&self) -> PathBuf {
        self.game_dir.join("sqpack")
    }

    pub fn read_file(&self, path: &str) -> Result<Vec<u8>, String> {
        self.physis
            .borrow_mut()
            .read(path)
            .ok_or_else(|| format!("physis 无法读取: {}", path))
    }

    pub fn parsed_tex(&self, path: &str) -> Option<TextureData> {
        let tex: physis::tex::Texture = self.physis.borrow_mut().parsed(path).ok()?;
        Some(TextureData {
            rgba: tex.rgba,
            width: tex.width,
            height: tex.height,
        })
    }

    pub fn parsed_mtrl(&self, path: &str) -> Option<ParsedMaterial> {
        let mtrl: physis::mtrl::Material = self.physis.borrow_mut().parsed(path).ok()?;
        Some(ParsedMaterial {
            texture_paths: mtrl.texture_paths,
            color_table: mtrl.color_table,
            color_dye_table: mtrl.color_dye_table,
        })
    }

    pub fn load_staining_template(&self) -> Option<StainingTemplate> {
        let stm: StainingTemplate = self
            .physis
            .borrow_mut()
            .parsed("chara/base_material/stainingtemplate.stm")
            .ok()?;
        println!("STM 加载成功: {} 个模板", stm.entries.len());
        Some(stm)
    }

    pub fn load_skeleton(&self, race_code: &str) -> Option<physis::skeleton::Skeleton> {
        let path = format!(
            "chara/human/{}/skeleton/base/b0001/skl_{}b0001.sklb",
            race_code, race_code
        );
        self.physis.borrow_mut().parsed(&path).ok()
    }

    pub fn get_all_sheet_names(&self) -> Vec<String> {
        self.physis
            .borrow_mut()
            .get_all_sheet_names()
            .unwrap_or_default()
    }

    pub fn read_excel_header(&self, name: &str) -> Option<physis::exh::EXH> {
        self.physis.borrow_mut().read_excel_sheet_header(name).ok()
    }

    pub fn read_excel_sheet(
        &self,
        exh: &physis::exh::EXH,
        name: &str,
        language: Language,
    ) -> Option<physis::excel::Sheet> {
        self.physis
            .borrow_mut()
            .read_excel_sheet(exh, name, language)
            .ok()
    }

    pub fn load_equipment_list(&self) -> Vec<EquipmentItem> {
        let mut physis = self.physis.borrow_mut();

        let exh = match physis.read_excel_sheet_header("Item") {
            Ok(h) => h,
            Err(e) => {
                eprintln!("无法加载 Item 表头: {}", e);
                return Vec::new();
            }
        };

        let sheet = match physis.read_excel_sheet(&exh, "Item", Language::ChineseSimplified) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("无法加载 Item 表: {}", e);
                return Vec::new();
            }
        };

        let mut items = Vec::new();
        for page in &sheet.pages {
            for (row_id, row) in page.into_iter().flatten_subrows() {
                if let Some(item) = Self::parse_equipment_row(row_id, row) {
                    items.push(item);
                }
            }
        }
        items
    }

    pub fn load_stain_list(&self) -> Vec<StainEntry> {
        let mut physis = self.physis.borrow_mut();

        let exh = match physis.read_excel_sheet_header("Stain") {
            Ok(h) => h,
            Err(e) => {
                eprintln!("无法加载 Stain 表头: {}", e);
                return Vec::new();
            }
        };

        let sheet = match physis.read_excel_sheet(&exh, "Stain", Language::ChineseSimplified) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("无法加载 Stain 表: {}", e);
                return Vec::new();
            }
        };

        let mut stains = Vec::new();
        for page in &sheet.pages {
            for (row_id, row) in page.into_iter().flatten_subrows() {
                if let Some(stain) = Self::parse_stain_row(row_id, row) {
                    stains.push(stain);
                }
            }
        }
        stains
    }

    fn parse_stain_row(row_id: u32, row: &Row) -> Option<StainEntry> {
        let color_val = match row.columns.get(0)? {
            Field::UInt32(v) => *v,
            _ => return None,
        };

        if color_val == 0 {
            return None;
        }

        let color = [
            ((color_val >> 16) & 0xFF) as u8,
            ((color_val >> 8) & 0xFF) as u8,
            (color_val & 0xFF) as u8,
        ];

        let shade = match row.columns.get(1) {
            Some(Field::UInt8(v)) => *v,
            _ => 0,
        };

        let name = row
            .columns
            .iter()
            .find_map(|col| {
                if let Field::String(s) = col {
                    if !s.is_empty() {
                        return Some(s.clone());
                    }
                }
                None
            })
            .unwrap_or_default();

        Some(StainEntry {
            id: row_id,
            name,
            color,
            shade,
        })
    }

    fn parse_equipment_row(row_id: u32, row: &Row) -> Option<EquipmentItem> {
        const COL_EQUIP_SLOT_CATEGORY: usize = 17;
        const COL_MODEL_MAIN: usize = 47;
        const COL_NAME: usize = 0;
        const COL_ICON: usize = 10;

        let equip_cat = match row.columns.get(COL_EQUIP_SLOT_CATEGORY)? {
            Field::UInt8(v) => *v,
            _ => return None,
        };

        let slot = EquipSlot::from_category(equip_cat)?;

        let model_main = match row.columns.get(COL_MODEL_MAIN)? {
            Field::UInt64(v) => *v,
            _ => return None,
        };

        if model_main == 0 {
            return None;
        }

        let set_id = (model_main & 0xFFFF) as u16;
        let variant_id = ((model_main >> 16) & 0xFFFF) as u16;

        let name = match row.columns.get(COL_NAME)? {
            Field::String(s) => {
                if s.is_empty() {
                    return None;
                }
                s.clone()
            }
            _ => return None,
        };

        let icon_id = match row.columns.get(COL_ICON) {
            Some(Field::UInt16(v)) => *v as u32,
            Some(Field::UInt32(v)) => *v,
            _ => 0,
        };

        Some(EquipmentItem {
            row_id,
            name,
            slot,
            set_id,
            variant_id,
            icon_id,
        })
    }

    pub fn load_icon(&self, icon_id: u32) -> Option<TextureData> {
        if icon_id == 0 {
            return None;
        }
        let high = icon_id / 1000;
        let path = format!("ui/icon/{:03}/{:06}_hr1.tex", high, icon_id);

        if let Some(tex) = self.parsed_tex(&path) {
            return Some(tex);
        }

        let fallback_path = format!("ui/icon/{:03}/{:06}.tex", high, icon_id);
        self.parsed_tex(&fallback_path)
    }
}
