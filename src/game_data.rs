use std::path::Path;

use ironworks::excel::{ExcelOptions, Field};
use ironworks::ffxiv::{FsResource, Language, Mapper};
use ironworks::sqpack::SqPack;
use ironworks::Ironworks;

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

    /// 返回候选模型路径列表（c0201 Hyur Male 优先，c0101 Hyur Female 回退）
    pub fn model_paths(&self) -> Vec<String> {
        vec![
            format!(
                "chara/equipment/e{:04}/model/c0201e{:04}_{}.mdl",
                self.set_id, self.set_id, self.slot.slot_abbr()
            ),
            format!(
                "chara/equipment/e{:04}/model/c0101e{:04}_{}.mdl",
                self.set_id, self.set_id, self.slot.slot_abbr()
            ),
        ]
    }
}

// Item 表列索引 (通过 column inspector 确定)
const COL_NAME: usize = 0;
const COL_ICON: usize = 10;
const COL_EQUIP_SLOT_CATEGORY: usize = 17;
const COL_MODEL_MAIN: usize = 47;

/// 游戏数据访问层
pub struct GameData {
    ironworks: Ironworks,
}

impl GameData {
    pub fn new(install_dir: &Path) -> Self {
        let resource = FsResource::at(install_dir);
        let sqpack = SqPack::new(resource);
        let ironworks = Ironworks::new().with_resource(sqpack);
        Self { ironworks }
    }

    pub fn ironworks(&self) -> &Ironworks {
        &self.ironworks
    }

    /// 加载所有可装备的防具物品
    pub fn load_equipment_list(&self) -> Vec<EquipmentItem> {
        let excel = ExcelOptions::default()
            .language(Language::ChineseSimplified)
            .build(&self.ironworks, Mapper::new());

        let sheet = match excel.sheet("Item") {
            Ok(s) => s,
            Err(e) => {
                eprintln!("无法加载 Item 表: {}", e);
                return Vec::new();
            }
        };

        let header = {
            // 通过 ironworks 内部 API 获取 pages
            // 我们需要遍历所有可能的行 ID
            // Item 表的行 ID 范围通常是 0 ~ 45000+
            let exh_path = "exd/Item.exh";
            match self.ironworks.file::<ironworks::file::exh::ExcelHeader>(exh_path) {
                Ok(h) => Some(h),
                Err(_) => None,
            }
        };

        let mut items = Vec::new();

        if let Some(header) = &header {
            for page in header.pages() {
                let start = page.start_id();
                let count = page.row_count();
                for row_id in start..start + count {
                    if let Ok(row) = sheet.row(row_id) {
                        if let Some(item) = Self::parse_equipment_row(row_id, &row) {
                            items.push(item);
                        }
                    }
                }
            }
        }

        items
    }

    fn parse_equipment_row(row_id: u32, row: &ironworks::excel::Row) -> Option<EquipmentItem> {
        // 读取 EquipSlotCategory
        let equip_cat = match row.field(COL_EQUIP_SLOT_CATEGORY) {
            Ok(Field::U8(v)) => v,
            _ => return None,
        };

        let slot = EquipSlot::from_category(equip_cat)?;

        // 读取 ModelMain
        let model_main = match row.field(COL_MODEL_MAIN) {
            Ok(Field::U64(v)) => v,
            _ => return None,
        };

        if model_main == 0 {
            return None;
        }

        let set_id = (model_main & 0xFFFF) as u16;
        let variant_id = ((model_main >> 16) & 0xFFFF) as u16;

        // 读取名称
        let name = match row.field(COL_NAME) {
            Ok(Field::String(s)) => {
                let s = s.to_string();
                if s.is_empty() { return None; }
                s
            }
            _ => return None,
        };

        // 读取图标
        let icon_id = match row.field(COL_ICON) {
            Ok(Field::U16(v)) => v,
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
