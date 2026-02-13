use std::cell::RefCell;
use std::path::Path;

use physis::excel::{Field, Row};
use physis::resource::{Resource as _, SqPackResource};
use physis::Language;

use crate::tex_loader::TextureData;

/// 装备槽位
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EquipSlot {
    Head,
    Body,
    Gloves,
    Legs,
    Feet,
}

impl EquipSlot {
    pub fn from_category(cat: u8) -> Option<Self> {
        match cat {
            3 => Some(Self::Head),
            4 => Some(Self::Body),
            5 => Some(Self::Gloves),
            7 => Some(Self::Legs),
            8 => Some(Self::Feet),
            _ => None,
        }
    }

    pub fn slot_abbr(&self) -> &'static str {
        match self {
            Self::Head => "met",
            Self::Body => "top",
            Self::Gloves => "glv",
            Self::Legs => "dwn",
            Self::Feet => "sho",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Head => "头部",
            Self::Body => "身体",
            Self::Gloves => "手部",
            Self::Legs => "腿部",
            Self::Feet => "脚部",
        }
    }
}

/// 装备物品
#[derive(Debug, Clone)]
pub struct EquipmentItem {
    pub row_id: u32,
    pub name: String,
    pub icon_id: u16,
    pub slot: EquipSlot,
    pub set_id: u16,
    pub variant_id: u16,
}

impl EquipmentItem {
    /// 生成默认种族 (Hyur Male) 的模型路径
    pub fn model_path(&self) -> String {
        format!(
            "chara/equipment/e{:04}/model/c0201e{:04}_{}.mdl",
            self.set_id,
            self.set_id,
            self.slot.slot_abbr()
        )
    }

    /// 返回候选模型路径列表，按种族码优先级尝试
    pub fn model_paths(&self) -> Vec<String> {
        // FF14 种族码: c{raceId:02}{bodyId:02}
        // 优先尝试通用种族，再尝试其他种族的专属模型
        const RACE_CODES: &[&str] = &[
            "c0201", // Hyur Midlander ♀
            "c0101", // Hyur Midlander ♂
            "c0401", // Hyur Highlander ♀
            "c0301", // Hyur Highlander ♂
            "c0801", // Miqo'te ♀
            "c0701", // Miqo'te ♂
            "c0601", // Elezen ♀
            "c0501", // Elezen ♂
            "c1401", // Au Ra ♀
            "c1301", // Au Ra ♂
            "c1201", // Lalafell ♀
            "c1101", // Lalafell ♂
            "c1001", // Roegadyn ♀
            "c0901", // Roegadyn ♂
            "c1801", // Viera ♀
            "c1701", // Viera ♂
            "c1501", // Hrothgar ♂
        ];
        RACE_CODES
            .iter()
            .map(|rc| {
                format!(
                    "chara/equipment/e{:04}/model/{}e{:04}_{}.mdl",
                    self.set_id, rc, self.set_id, self.slot.slot_abbr()
                )
            })
            .collect()
    }
}

// Item 表列索引 (通过 column inspector 确定)
const COL_NAME: usize = 0;
const COL_ICON: usize = 10;
const COL_EQUIP_SLOT_CATEGORY: usize = 17;
const COL_MODEL_MAIN: usize = 47;

/// 游戏数据访问层
pub struct GameData {
    physis: RefCell<SqPackResource>,
}

impl GameData {
    pub fn new(install_dir: &Path) -> Self {
        let game_dir = install_dir.join("game");
        let physis = RefCell::new(SqPackResource::from_existing(game_dir.to_str().unwrap()));
        Self { physis }
    }

    /// 读取原始文件字节
    pub fn read_file(&self, path: &str) -> Result<Vec<u8>, String> {
        self.physis
            .borrow_mut()
            .read(path)
            .ok_or_else(|| format!("physis 无法读取: {}", path))
    }

    /// 解析 TEX 文件，返回已解码的 RGBA 数据
    pub fn parsed_tex(&self, path: &str) -> Option<TextureData> {
        let tex: physis::tex::Texture = self.physis.borrow_mut().parsed(path).ok()?;
        Some(TextureData {
            rgba: tex.rgba,
            width: tex.width,
            height: tex.height,
        })
    }

    /// 解析 MTRL 文件，返回纹理路径列表
    pub fn parsed_mtrl(&self, path: &str) -> Option<Vec<String>> {
        let mtrl: physis::mtrl::Material = self.physis.borrow_mut().parsed(path).ok()?;
        Some(mtrl.texture_paths)
    }

    /// 加载所有可装备的防具物品
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

    fn parse_equipment_row(row_id: u32, row: &Row) -> Option<EquipmentItem> {
        // 读取 EquipSlotCategory
        let equip_cat = match row.columns.get(COL_EQUIP_SLOT_CATEGORY)? {
            Field::UInt8(v) => *v,
            _ => return None,
        };

        let slot = EquipSlot::from_category(equip_cat)?;

        // 读取 ModelMain
        let model_main = match row.columns.get(COL_MODEL_MAIN)? {
            Field::UInt64(v) => *v,
            _ => return None,
        };

        if model_main == 0 {
            return None;
        }

        let set_id = (model_main & 0xFFFF) as u16;
        let variant_id = ((model_main >> 16) & 0xFFFF) as u16;

        // 读取名称
        let name = match row.columns.get(COL_NAME)? {
            Field::String(s) => {
                if s.is_empty() {
                    return None;
                }
                s.clone()
            }
            _ => return None,
        };

        // 读取图标
        let icon_id = match row.columns.get(COL_ICON) {
            Some(Field::UInt16(v)) => *v,
            _ => 0,
        };

        Some(EquipmentItem {
            row_id,
            name,
            icon_id,
            slot,
            set_id,
            variant_id,
        })
    }
}
