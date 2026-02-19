use std::cell::RefCell;
use std::path::{Path, PathBuf};

use physis::excel::{Field, Row};
use physis::mtrl::{ColorDyeTable, ColorTable};
use physis::resource::{Resource as _, SqPackResource};
use physis::stm::StainingTemplate;
use physis::Language;

use tomestone_render::TextureData;

/// 解析后的完整材质数据
pub struct ParsedMaterial {
    pub texture_paths: Vec<String>,
    pub color_table: Option<ColorTable>,
    pub color_dye_table: Option<ColorDyeTable>,
}

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

    /// 生成指定种族码的模型路径
    pub fn model_path_for_race(&self, race_code: &str) -> String {
        format!(
            "chara/equipment/e{:04}/model/{}e{:04}_{}.mdl",
            self.set_id,
            race_code,
            self.set_id,
            self.slot.slot_abbr()
        )
    }

    /// 返回候选模型路径列表，按种族码优先级尝试
    pub fn model_paths(&self) -> Vec<String> {
        // FF14 种族码: c{raceId:02}{bodyId:02}
        // 优先尝试通用种族，再尝试其他种族的专属模型
        RACE_CODES
            .iter()
            .map(|rc| {
                format!(
                    "chara/equipment/e{:04}/model/{}e{:04}_{}.mdl",
                    self.set_id,
                    rc,
                    self.set_id,
                    self.slot.slot_abbr()
                )
            })
            .collect()
    }
}

/// 种族码优先级列表
pub const RACE_CODES: &[&str] = &[
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

// Item 表列索引 (通过 column inspector 确定)
const COL_NAME: usize = 0;
const COL_EQUIP_SLOT_CATEGORY: usize = 17;
const COL_MODEL_MAIN: usize = 47;

/// 染料条目
#[derive(Debug, Clone)]
pub struct StainEntry {
    pub id: u32,
    pub name: String,
    pub color: [u8; 3],
    pub shade: u8,
}

/// 游戏数据访问层
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

    /// 解析 MTRL 文件，返回完整材质数据
    pub fn parsed_mtrl(&self, path: &str) -> Option<ParsedMaterial> {
        let mtrl: physis::mtrl::Material = self.physis.borrow_mut().parsed(path).ok()?;
        Some(ParsedMaterial {
            texture_paths: mtrl.texture_paths,
            color_table: mtrl.color_table,
            color_dye_table: mtrl.color_dye_table,
        })
    }

    /// 加载 STM 染色模板
    pub fn load_staining_template(&self) -> Option<StainingTemplate> {
        let stm: StainingTemplate = self
            .physis
            .borrow_mut()
            .parsed("chara/base_material/stainingtemplate.stm")
            .ok()?;
        println!("STM 加载成功: {} 个模板", stm.entries.len());
        Some(stm)
    }

    /// 加载指定种族码的骨骼
    pub fn load_skeleton(&self, race_code: &str) -> Option<physis::skeleton::Skeleton> {
        let path = format!(
            "chara/human/{}/skeleton/base/b0001/skl_{}b0001.sklb",
            race_code, race_code
        );
        self.physis.borrow_mut().parsed(&path).ok()
    }

    /// 获取所有 EXD 表名
    pub fn get_all_sheet_names(&self) -> Vec<String> {
        self.physis
            .borrow_mut()
            .get_all_sheet_names()
            .unwrap_or_default()
    }

    /// 读取表头 (EXH)
    pub fn read_excel_header(&self, name: &str) -> Option<physis::exh::EXH> {
        self.physis.borrow_mut().read_excel_sheet_header(name).ok()
    }

    /// 读取表数据 (Sheet)
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

    /// 加载染料列表
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
        // Col 0 = Color (u32, 0xRRGGBB)
        let color_val = match row.columns.get(0)? {
            Field::UInt32(v) => *v,
            _ => return None,
        };

        if color_val == 0 {
            return None; // 跳过空行
        }

        let color = [
            ((color_val >> 16) & 0xFF) as u8,
            ((color_val >> 8) & 0xFF) as u8,
            (color_val & 0xFF) as u8,
        ];

        // Col 1 = Shade (u8)
        let shade = match row.columns.get(1) {
            Some(Field::UInt8(v)) => *v,
            _ => 0,
        };

        // Name: 尝试 Col 2 和 Col 3（不同版本列序可能不同）
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

        Some(EquipmentItem {
            row_id,
            name,
            slot,
            set_id,
            variant_id,
        })
    }
}

/// Shade 组显示顺序
pub const SHADE_ORDER: &[u8] = &[2, 4, 5, 6, 7, 8, 9, 10, 1];

/// shade 值 → 中文组名
pub fn shade_group_name(shade: u8) -> &'static str {
    match shade {
        2 => "白/灰/黑",
        4 => "红/粉",
        5 => "橙/棕",
        6 => "黄",
        7 => "绿",
        8 => "蓝",
        9 => "紫",
        10 => "特殊",
        1 => "其他",
        _ => "未知",
    }
}
