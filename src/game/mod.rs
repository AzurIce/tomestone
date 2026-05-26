mod mdl;
mod sgb;
mod skeleton;
mod tex;

pub use mdl::{compute_bounding_box, load_mdl, load_mdl_with_fallback, MdlBoneTable, MeshData};
pub use sgb::extract_mdl_paths_from_sgb;
pub use skeleton::{apply_skinning, SkeletonCache};
pub use tex::{
    bake_color_table_texture, load_housing_mesh_textures, load_mesh_textures, CachedMaterial,
};

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use physis::excel::{Field, Row};
use physis::mtrl::{ColorDyeTable, ColorTable};
use physis::resource::{Resource as _, SqPackResource};
use physis::stm::StainingTemplate;
use physis::Language;

use tomestone_render::TextureData;

use crate::domain::{ConsumableEffect, ConsumableInfo, GameItem, ItemSource, Recipe, StainEntry};

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
            rgba: tex.to_rgba()?.into(),
            width: tex.width as u32,
            height: tex.height as u32,
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

    /// 一次性加载 Item 表全部物品，返回统一的 GameItem 列表
    pub fn load_all_items(&self) -> Vec<GameItem> {
        // 使用正确的 ItemAction 列索引 (col[30])
        
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
                if let Some(item) = Self::parse_item_row(row_id, row) {
                    items.push(item);
                }
            }
        }
        items
    }

    fn parse_item_row(row_id: u32, row: &Row) -> Option<GameItem> {
        // Item 表列索引 (通过 debug dump 确认)
        const COL_NAME: usize = 0;
        const COL_DESCRIPTION: usize = 8;
        const COL_ICON: usize = 10;
        const COL_FILTER_GROUP: usize = 13;
        const COL_ADDITIONAL_DATA: usize = 14;
        const COL_ITEM_UI_CATEGORY: usize = 15;
        const COL_ITEM_SEARCH_CATEGORY: usize = 16;
        const COL_EQUIP_SLOT_CATEGORY: usize = 17;
        const COL_PRICE_MID: usize = 25;
        const COL_PRICE_LOW: usize = 26;
        const COL_MODEL_MAIN: usize = 47;

        let name = match row.columns.get(COL_NAME)? {
            Field::String(s) => {
                if s.is_empty() {
                    return None;
                }
                s.clone()
            }
            _ => return None,
        };

        let description = match row.columns.get(COL_DESCRIPTION) {
            Some(Field::String(s)) => s.clone(),
            _ => String::new(),
        };

        let icon_id = match row.columns.get(COL_ICON) {
            Some(Field::UInt16(v)) => *v as u32,
            Some(Field::UInt32(v)) => *v,
            _ => 0,
        };

        let filter_group = match row.columns.get(COL_FILTER_GROUP) {
            Some(Field::UInt8(v)) => *v,
            _ => 0,
        };

        let additional_data = match row.columns.get(COL_ADDITIONAL_DATA) {
            Some(Field::UInt32(v)) => *v,
            Some(Field::UInt16(v)) => *v as u32,
            _ => 0,
        };

        let item_ui_category = match row.columns.get(COL_ITEM_UI_CATEGORY) {
            Some(Field::UInt8(v)) => *v,
            _ => 0,
        };

        let item_search_category = match row.columns.get(COL_ITEM_SEARCH_CATEGORY) {
            Some(Field::UInt8(v)) => *v,
            _ => 0,
        };

        let equip_slot_category = match row.columns.get(COL_EQUIP_SLOT_CATEGORY) {
            Some(Field::UInt8(v)) => *v,
            _ => 0,
        };

        let price_mid = match row.columns.get(COL_PRICE_MID) {
            Some(Field::UInt32(v)) => *v,
            _ => 0,
        };

        let price_low = match row.columns.get(COL_PRICE_LOW) {
            Some(Field::UInt32(v)) => *v,
            _ => 0,
        };

        let model_main = match row.columns.get(COL_MODEL_MAIN) {
            Some(Field::UInt64(v)) => *v,
            _ => 0,
        };

        // ItemAction 列 (根据测试结果, col[30])
        const COL_ITEM_ACTION: usize = 30;
        let item_action = match row.columns.get(COL_ITEM_ACTION) {
            Some(Field::UInt16(v)) => *v as u32,
            Some(Field::UInt8(v)) => *v as u32,
            Some(Field::UInt32(v)) => *v,
            Some(Field::Int32(v)) if *v > 0 => *v as u32,
            _ => 0,
        };

        Some(GameItem {
            row_id,
            name,
            icon_id,
            filter_group,
            item_ui_category,
            equip_slot_category,
            model_main,
            additional_data,
            description,
            price_mid,
            price_low,
            item_search_category,
            item_action,
        })
    }

    /// 加载 HousingExterior 表的 SGB 路径映射
    /// 返回 HousingExterior row_id -> SGB 路径列表
    pub fn load_housing_sgb_paths(&self) -> std::collections::HashMap<u32, Vec<String>> {
        let mut physis = self.physis.borrow_mut();

        let ext_exh = match physis.read_excel_sheet_header("HousingExterior") {
            Ok(h) => h,
            Err(e) => {
                eprintln!("无法加载 HousingExterior 表头: {}", e);
                return std::collections::HashMap::new();
            }
        };
        let ext_sheet = match physis.read_excel_sheet(&ext_exh, "HousingExterior", Language::None) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("无法加载 HousingExterior 表: {}", e);
                return std::collections::HashMap::new();
            }
        };

        let mut sgb_paths: std::collections::HashMap<u32, Vec<String>> =
            std::collections::HashMap::new();
        for page in &ext_sheet.pages {
            for (row_id, row) in page.into_iter().flatten_subrows() {
                let mut paths = Vec::new();
                for col in &row.columns {
                    if let Field::String(s) = col {
                        if !s.is_empty() && s.ends_with(".sgb") {
                            paths.push(s.clone());
                        }
                    }
                }
                if !paths.is_empty() {
                    sgb_paths.insert(row_id, paths);
                }
            }
        }
        println!("HousingExterior 表: {} 条有效记录", sgb_paths.len());
        sgb_paths
    }

    /// 加载 HousingFurniture 表的 SGB 路径映射 (室内家具)
    /// 返回 Item.row_id -> SGB 路径 (通过表中的 Item 列反查)
    pub fn load_housing_furniture_sgb_paths(&self) -> std::collections::HashMap<u32, String> {
        let mut physis = self.physis.borrow_mut();

        let exh = match physis.read_excel_sheet_header("HousingFurniture") {
            Ok(h) => h,
            Err(e) => {
                eprintln!("无法加载 HousingFurniture 表头: {}", e);
                return std::collections::HashMap::new();
            }
        };
        let sheet = match physis.read_excel_sheet(&exh, "HousingFurniture", Language::None) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("无法加载 HousingFurniture 表: {}", e);
                return std::collections::HashMap::new();
            }
        };

        // HousingFurniture 列布局:
        // col[0] = ModelKey (UInt16)
        // col[7] = Item (UInt32, 链接到 Item 表)
        let mut sgb_paths: std::collections::HashMap<u32, String> =
            std::collections::HashMap::new();
        for page in &sheet.pages {
            for (_row_id, row) in page.into_iter().flatten_subrows() {
                let model_key = match row.columns.first() {
                    Some(Field::UInt16(v)) => *v,
                    Some(Field::UInt8(v)) => *v as u16,
                    _ => continue,
                };
                if model_key == 0 {
                    continue;
                }
                // col[7] = Item row_id
                let item_id = match row.columns.get(7) {
                    Some(Field::UInt32(v)) if *v > 0 => *v,
                    Some(Field::Int32(v)) if *v > 0 => *v as u32,
                    _ => continue,
                };
                let sgb = format!(
                    "bgcommon/hou/indoor/general/{:04}/asset/fun_b0_m{:04}.sgb",
                    model_key, model_key
                );
                sgb_paths.insert(item_id, sgb);
            }
        }
        println!("HousingFurniture 表: {} 条有效记录", sgb_paths.len());
        sgb_paths
    }

    /// 加载 HousingYardObject 表的 SGB 路径映射 (庭院家具)
    /// 返回 Item.row_id -> SGB 路径 (通过表中的 Item 列反查)
    pub fn load_housing_yard_sgb_paths(&self) -> std::collections::HashMap<u32, String> {
        let mut physis = self.physis.borrow_mut();

        let exh = match physis.read_excel_sheet_header("HousingYardObject") {
            Ok(h) => h,
            Err(e) => {
                eprintln!("无法加载 HousingYardObject 表头: {}", e);
                return std::collections::HashMap::new();
            }
        };
        let sheet = match physis.read_excel_sheet(&exh, "HousingYardObject", Language::None) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("无法加载 HousingYardObject 表: {}", e);
                return std::collections::HashMap::new();
            }
        };

        // HousingYardObject 列布局:
        // col[0] = ModelKey (UInt16)
        // col[6] = Item (UInt32, 链接到 Item 表)
        let mut sgb_paths: std::collections::HashMap<u32, String> =
            std::collections::HashMap::new();
        for page in &sheet.pages {
            for (_row_id, row) in page.into_iter().flatten_subrows() {
                let model_key = match row.columns.first() {
                    Some(Field::UInt16(v)) => *v,
                    Some(Field::UInt8(v)) => *v as u16,
                    _ => continue,
                };
                if model_key == 0 {
                    continue;
                }
                // col[6] = Item row_id
                let item_id = match row.columns.get(6) {
                    Some(Field::UInt32(v)) if *v > 0 => *v,
                    Some(Field::Int32(v)) if *v > 0 => *v as u32,
                    _ => continue,
                };
                let sgb = format!(
                    "bgcommon/hou/outdoor/general/{:04}/asset/gar_b0_m{:04}.sgb",
                    model_key, model_key
                );
                sgb_paths.insert(item_id, sgb);
            }
        }
        println!("HousingYardObject 表: {} 条有效记录", sgb_paths.len());
        sgb_paths
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

    pub fn load_icon(&self, icon_id: u32) -> Option<TextureData> {
        if icon_id == 0 {
            return None;
        }
        let high = icon_id / 1000 * 1000;
        let path = format!("ui/icon/{:06}/{:06}_hr1.tex", high, icon_id);

        if let Some(tex) = self.parsed_tex(&path) {
            return Some(tex);
        }

        let fallback_path = format!("ui/icon/{:06}/{:06}.tex", high, icon_id);
        self.parsed_tex(&fallback_path)
    }

    /// 加载 Recipe EXD 表，返回配方列表
    pub fn load_recipes(&self) -> Vec<Recipe> {
        let mut physis = self.physis.borrow_mut();

        let exh = match physis.read_excel_sheet_header("Recipe") {
            Ok(h) => h,
            Err(e) => {
                eprintln!("无法加载 Recipe 表头: {}", e);
                return Vec::new();
            }
        };

        // Recipe 表不含文本，使用 Language::None
        let sheet = match physis.read_excel_sheet(&exh, "Recipe", Language::None) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("无法加载 Recipe 表: {}", e);
                return Vec::new();
            }
        };

        let mut recipes = Vec::new();
        for page in &sheet.pages {
            for (row_id, row) in page.into_iter().flatten_subrows() {
                if let Some(recipe) = Self::parse_recipe_row(row_id, row) {
                    recipes.push(recipe);
                }
            }
        }
        println!("Recipe 表: {} 条有效配方", recipes.len());
        recipes
    }

    fn parse_recipe_row(row_id: u32, row: &Row) -> Option<Recipe> {
        // Recipe 表实际列布局 (通过 debug dump 确认):
        // col[0]: Number (Int32)
        // col[1]: CraftType (Int32)
        // col[2]: RecipeLevelTable (UInt16)
        // col[3]: UInt16 (未知)
        // col[4]: ItemResult (Int32, 产出物品 ID)
        // col[5]: AmountResult (UInt8, 产出数量)
        // col[6..21]: Ingredient[0..7] 交错排列, 每对 (Int32 item_id, UInt8 amount)
        //   col[6]=Ing0_ID, col[7]=Ing0_Amt, col[8]=Ing1_ID, col[9]=Ing1_Amt, ...
        // col[40]: SecretRecipeBook (Int32, 秘籍 ID，0 表示非秘籍)
        const COL_CRAFT_TYPE: usize = 1;
        const COL_RECIPE_LEVEL: usize = 2;
        const COL_ITEM_RESULT: usize = 4;
        const COL_AMOUNT_RESULT: usize = 5;
        const COL_INGREDIENT_START: usize = 6; // 每对占 2 列, 共 8 对
        const COL_SECRET_RECIPE_BOOK: usize = 40;

        fn read_i32_as_u32(row: &Row, col: usize) -> u32 {
            match row.columns.get(col) {
                Some(Field::Int32(v)) => {
                    if *v > 0 {
                        *v as u32
                    } else {
                        0
                    }
                }
                Some(Field::UInt32(v)) => *v,
                Some(Field::UInt16(v)) => *v as u32,
                _ => 0,
            }
        }

        // 读取产出物品 ID
        let result_item_id = read_i32_as_u32(row, COL_ITEM_RESULT);
        if result_item_id == 0 {
            return None;
        }

        let craft_type = match row.columns.get(COL_CRAFT_TYPE) {
            Some(Field::Int32(v)) => *v as u8,
            Some(Field::UInt8(v)) => *v,
            _ => 0,
        };

        let recipe_level = match row.columns.get(COL_RECIPE_LEVEL) {
            Some(Field::UInt16(v)) => *v,
            Some(Field::UInt8(v)) => *v as u16,
            _ => 0,
        };

        let result_amount = match row.columns.get(COL_AMOUNT_RESULT) {
            Some(Field::UInt8(v)) => *v,
            _ => 1,
        };

        // 读取素材 (8 对交错排列)
        let mut ingredients = Vec::new();
        for i in 0..8 {
            let id_col = COL_INGREDIENT_START + i * 2;
            let amt_col = id_col + 1;
            let ing_id = read_i32_as_u32(row, id_col);
            let ing_amount = match row.columns.get(amt_col) {
                Some(Field::UInt8(v)) => *v,
                _ => 0,
            };
            if ing_id != 0 && ing_amount > 0 {
                ingredients.push((ing_id, ing_amount));
            }
        }

        if ingredients.is_empty() {
            return None;
        }

        // 读取秘籍 ID
        let secret_recipe_book = read_i32_as_u32(row, COL_SECRET_RECIPE_BOOK);

        Some(Recipe {
            row_id,
            result_item_id,
            result_amount,
            craft_type,
            recipe_level_table_id: recipe_level,
            ingredients,
            secret_recipe_book,
        })
    }

    /// 加载 ItemUICategory 表, 返回 row_id -> 分类名称
    pub fn load_ui_category_names(&self) -> std::collections::HashMap<u8, String> {
        let mut physis = self.physis.borrow_mut();
        let exh = match physis.read_excel_sheet_header("ItemUICategory") {
            Ok(h) => h,
            Err(_) => return std::collections::HashMap::new(),
        };
        let sheet =
            match physis.read_excel_sheet(&exh, "ItemUICategory", Language::ChineseSimplified) {
                Ok(s) => s,
                Err(_) => return std::collections::HashMap::new(),
            };
        let mut map = std::collections::HashMap::new();
        for page in &sheet.pages {
            for (row_id, row) in page.into_iter().flatten_subrows() {
                if let Some(Field::String(name)) = row.columns.first() {
                    if !name.is_empty() && row_id <= 255 {
                        map.insert(row_id as u8, name.clone());
                    }
                }
            }
        }
        map
    }

    /// 加载 GilShop 相关表, 构建 NPC 关联, 返回 item_id -> Vec<ItemSource::GilShop>
    pub fn load_gil_shop_items(&self) -> std::collections::HashMap<u32, Vec<ItemSource>> {
        let mut physis = self.physis.borrow_mut();

        // 1. 加载 GilShop 表: shop_id -> 商店分类名
        let mut shop_names: std::collections::HashMap<u32, String> =
            std::collections::HashMap::new();
        if let Ok(exh) = physis.read_excel_sheet_header("GilShop") {
            if let Ok(sheet) = physis.read_excel_sheet(&exh, "GilShop", Language::ChineseSimplified)
            {
                for page in &sheet.pages {
                    for (row_id, row) in page.into_iter().flatten_subrows() {
                        let name = match row.columns.first() {
                            Some(Field::String(s)) if !s.is_empty() => s.clone(),
                            _ => String::new(),
                        };
                        shop_names.insert(row_id, name);
                    }
                }
            }
        }
        println!("GilShop: {} 个商店", shop_names.len());

        // 2. 加载 TopicSelect 表: topic_id -> Vec<shop_id>
        let mut topic_shops: std::collections::HashMap<u32, Vec<u32>> =
            std::collections::HashMap::new();
        if let Ok(exh) = physis.read_excel_sheet_header("TopicSelect") {
            if let Ok(sheet) = physis.read_excel_sheet(&exh, "TopicSelect", Language::None) {
                for page in &sheet.pages {
                    for (row_id, row) in page.into_iter().flatten_subrows() {
                        let mut shops = Vec::new();
                        // Shop[0..9] 从 col[1] 开始 (col[0] 是 Name)
                        for i in 1..=10 {
                            match row.columns.get(i) {
                                Some(Field::Int32(v)) if *v > 0 => shops.push(*v as u32),
                                Some(Field::UInt32(v)) if *v > 0 => shops.push(*v),
                                _ => {}
                            }
                        }
                        if !shops.is_empty() {
                            topic_shops.insert(row_id, shops);
                        }
                    }
                }
            }
        }
        println!("TopicSelect: {} 个话题", topic_shops.len());

        // 3. 加载 ENpcResident 表: npc_id -> npc_name
        let mut npc_names: std::collections::HashMap<u32, String> =
            std::collections::HashMap::new();
        if let Ok(exh) = physis.read_excel_sheet_header("ENpcResident") {
            if let Ok(sheet) =
                physis.read_excel_sheet(&exh, "ENpcResident", Language::ChineseSimplified)
            {
                for page in &sheet.pages {
                    for (row_id, row) in page.into_iter().flatten_subrows() {
                        let name = match row.columns.first() {
                            Some(Field::String(s)) if !s.is_empty() => s.clone(),
                            _ => continue,
                        };
                        npc_names.insert(row_id, name);
                    }
                }
            }
        }
        println!("ENpcResident: {} 个 NPC", npc_names.len());

        // 4. 加载 ENpcBase 表, 构建 shop_id -> npc_name 反向索引
        const GILSHOP_MIN: u32 = 0x40000;
        const GILSHOP_MAX: u32 = 0x160000;
        const TOPIC_MIN: u32 = 0x320000;
        const TOPIC_MAX: u32 = 0x360000;

        let mut shop_npcs: std::collections::HashMap<u32, String> =
            std::collections::HashMap::new();
        if let Ok(exh) = physis.read_excel_sheet_header("ENpcBase") {
            if let Ok(sheet) = physis.read_excel_sheet(&exh, "ENpcBase", Language::None) {
                for page in &sheet.pages {
                    for (npc_id, row) in page.into_iter().flatten_subrows() {
                        let npc_name = match npc_names.get(&npc_id) {
                            Some(n) => n.clone(),
                            None => continue,
                        };
                        // ENpcData[0..31] — 需要找到正确的列偏移
                        // ENpcBase 有很多外观字段在前面，ENpcData 通常在后半部分
                        // 遍历所有列查找 GilShop/TopicSelect 范围的值
                        for col in &row.columns {
                            let val = match col {
                                Field::Int32(v) if *v > 0 => *v as u32,
                                Field::UInt32(v) if *v > 0 => *v,
                                _ => continue,
                            };
                            if val >= GILSHOP_MIN && val < GILSHOP_MAX {
                                // 直接关联 GilShop
                                shop_npcs.entry(val).or_insert_with(|| npc_name.clone());
                            } else if val >= TOPIC_MIN && val < TOPIC_MAX {
                                // 间接关联: TopicSelect -> GilShop
                                if let Some(shops) = topic_shops.get(&val) {
                                    for &shop_id in shops {
                                        if shop_id >= GILSHOP_MIN && shop_id < GILSHOP_MAX {
                                            shop_npcs
                                                .entry(shop_id)
                                                .or_insert_with(|| npc_name.clone());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        println!("GilShop→NPC: {} 个商店有 NPC 关联", shop_npcs.len());

        // 4b. 加载 NPC 位置: npc_id -> 区域名
        // 先加载 PlaceName 表
        let mut place_names: std::collections::HashMap<u32, String> =
            std::collections::HashMap::new();
        if let Ok(exh) = physis.read_excel_sheet_header("PlaceName") {
            if let Ok(sheet) =
                physis.read_excel_sheet(&exh, "PlaceName", Language::ChineseSimplified)
            {
                for page in &sheet.pages {
                    for (row_id, row) in page.into_iter().flatten_subrows() {
                        if let Some(Field::String(s)) = row.columns.first() {
                            if !s.is_empty() {
                                place_names.insert(row_id, s.clone());
                            }
                        }
                    }
                }
            }
        }

        // 加载 TerritoryType 表: territory_id -> place_name_id
        let mut territory_place: std::collections::HashMap<u32, u32> =
            std::collections::HashMap::new();
        if let Ok(exh) = physis.read_excel_sheet_header("TerritoryType") {
            if let Ok(sheet) = physis.read_excel_sheet(&exh, "TerritoryType", Language::None) {
                for page in &sheet.pages {
                    for (row_id, row) in page.into_iter().flatten_subrows() {
                        // PlaceName 字段 — 需要找到正确的列
                        // TerritoryType 的 PlaceName 通常在前几列
                        for col in row.columns.iter().take(10) {
                            match col {
                                Field::UInt16(v) if *v > 0 => {
                                    if place_names.contains_key(&(*v as u32)) {
                                        territory_place.insert(row_id, *v as u32);
                                        break;
                                    }
                                }
                                Field::Int32(v) if *v > 0 => {
                                    if place_names.contains_key(&(*v as u32)) {
                                        territory_place.insert(row_id, *v as u32);
                                        break;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }

        // 加载 Level 表: 筛选 Type=8 (ENpc), 建立 npc_id -> 区域名
        let mut npc_locations: std::collections::HashMap<u32, String> =
            std::collections::HashMap::new();
        if let Ok(exh) = physis.read_excel_sheet_header("Level") {
            if let Ok(sheet) = physis.read_excel_sheet(&exh, "Level", Language::None) {
                for page in &sheet.pages {
                    for (_row_id, row) in page.into_iter().flatten_subrows() {
                        // Level 表列结构: X, Y, Z, Yaw, Radius, Type, Object, Territory, Map, ...
                        // 需要确认实际列偏移
                        let cols = &row.columns;
                        if cols.len() < 9 {
                            continue;
                        }
                        // Type 字段 (col[5] 或附近)
                        let obj_type = match &cols[5] {
                            Field::UInt8(v) => *v,
                            _ => continue,
                        };
                        if obj_type != 8 {
                            continue; // 只要 ENpc
                        }
                        // Object 字段 (col[6])
                        let npc_id = match &cols[6] {
                            Field::UInt32(v) => *v,
                            Field::Int32(v) if *v > 0 => *v as u32,
                            _ => continue,
                        };
                        // Territory 字段 (col[7])
                        let territory_id = match &cols[7] {
                            Field::UInt16(v) => *v as u32,
                            Field::Int32(v) if *v > 0 => *v as u32,
                            Field::UInt32(v) => *v,
                            _ => continue,
                        };
                        // 查找区域名
                        if npc_locations.contains_key(&npc_id) {
                            continue; // 只取第一个位置
                        }
                        if let Some(&place_id) = territory_place.get(&territory_id) {
                            if let Some(name) = place_names.get(&place_id) {
                                npc_locations.insert(npc_id, name.clone());
                            }
                        }
                    }
                }
            }
        }
        println!("NPC 位置: {} 个 NPC 有位置信息", npc_locations.len());

        // 构建 shop_id -> npc_location (通过 shop_npcs 中的 npc_name 反查 npc_id)
        // 需要 npc_name -> npc_id 的反向映射
        let npc_name_to_id: std::collections::HashMap<&str, u32> = npc_names
            .iter()
            .map(|(&id, name)| (name.as_str(), id))
            .collect();
        let mut shop_locations: std::collections::HashMap<u32, String> =
            std::collections::HashMap::new();
        for (&shop_id, npc_name) in &shop_npcs {
            if let Some(&npc_id) = npc_name_to_id.get(npc_name.as_str()) {
                if let Some(loc) = npc_locations.get(&npc_id) {
                    shop_locations.insert(shop_id, loc.clone());
                }
            }
        }
        println!("GilShop 位置: {} 个商店有位置信息", shop_locations.len());

        // 5. 加载 GilShopItem 表, 构建 item_id -> Vec<ItemSource::GilShop>
        let exh = match physis.read_excel_sheet_header("GilShopItem") {
            Ok(h) => h,
            Err(_) => return std::collections::HashMap::new(),
        };
        let sheet = match physis.read_excel_sheet(&exh, "GilShopItem", Language::None) {
            Ok(s) => s,
            Err(_) => return std::collections::HashMap::new(),
        };
        let mut map: std::collections::HashMap<u32, Vec<ItemSource>> =
            std::collections::HashMap::new();
        for page in &sheet.pages {
            for (row_id, row) in page.into_iter().flatten_subrows() {
                let item_id = match row.columns.first() {
                    Some(Field::Int32(v)) if *v > 0 => *v as u32,
                    _ => continue,
                };
                // 组合显示名: "NPC名 - 商店分类名" 或 "商店分类名"
                let category = shop_names.get(&row_id).filter(|s| !s.is_empty()).cloned();
                let npc = shop_npcs.get(&row_id).cloned();
                let shop_name = match (npc, category) {
                    (Some(n), Some(c)) => format!("{} - {}", n, c),
                    (Some(n), None) => n,
                    (None, Some(c)) => c,
                    (None, None) => "金币商店".to_string(),
                };
                let npc_location = shop_locations.get(&row_id).cloned();
                map.entry(item_id).or_default().push(ItemSource::GilShop {
                    shop_name,
                    npc_location,
                });
            }
        }
        println!("GilShopItem: {} 种商品", map.len());
        map
    }

    /// 加载 SpecialShop 表, 返回 item_id -> Vec<ItemSource::SpecialShop>
    pub fn load_special_shop_sources(&self) -> std::collections::HashMap<u32, Vec<ItemSource>> {
        let mut physis = self.physis.borrow_mut();
        let exh = match physis.read_excel_sheet_header("SpecialShop") {
            Ok(h) => h,
            Err(_) => return std::collections::HashMap::new(),
        };
        let sheet = match physis.read_excel_sheet(&exh, "SpecialShop", Language::ChineseSimplified)
        {
            Ok(s) => s,
            Err(_) => return std::collections::HashMap::new(),
        };

        let mut map: std::collections::HashMap<u32, Vec<ItemSource>> =
            std::collections::HashMap::new();
        for page in &sheet.pages {
            for (_row_id, row) in page.into_iter().flatten_subrows() {
                let shop_name = match row.columns.first() {
                    Some(Field::String(s)) => s.clone(),
                    _ => String::new(),
                };

                // 60 个交易槽位
                for i in 0..60usize {
                    let receive_item = match row.columns.get(1 + i) {
                        Some(Field::Int32(v)) if *v > 0 => *v as u32,
                        _ => continue,
                    };

                    // 尝试 Cost 第 1 组和第 2 组，取第一个有效的
                    let cost_groups: [(usize, usize); 2] = [
                        (241, 301), // CostItem_0, CostCount_0
                        (481, 541), // CostItem_1, CostCount_1
                    ];
                    for &(item_col_base, count_col_base) in &cost_groups {
                        let cost_item = match row.columns.get(item_col_base + i) {
                            Some(Field::Int32(v)) if *v > 0 => *v as u32,
                            _ => continue,
                        };
                        let cost_count = match row.columns.get(count_col_base + i) {
                            Some(Field::UInt32(v)) if *v > 0 => *v,
                            _ => continue,
                        };
                        let source = ItemSource::SpecialShop {
                            shop_name: shop_name.clone(),
                            cost_item_id: cost_item,
                            cost_count,
                        };
                        map.entry(receive_item).or_default().push(source);
                    }
                }
            }
        }
        println!("SpecialShop: {} 种可兑换物品", map.len());
        map
    }

    /// 加载 GatheringItem 表, 返回可采集的 item_id 集合
    pub fn load_gathering_items(&self) -> std::collections::HashSet<u32> {
        let mut physis = self.physis.borrow_mut();
        let exh = match physis.read_excel_sheet_header("GatheringItem") {
            Ok(h) => h,
            Err(_) => return std::collections::HashSet::new(),
        };
        let sheet = match physis.read_excel_sheet(&exh, "GatheringItem", Language::None) {
            Ok(s) => s,
            Err(_) => return std::collections::HashSet::new(),
        };
        let mut items = std::collections::HashSet::new();
        for page in &sheet.pages {
            for (_row_id, row) in page.into_iter().flatten_subrows() {
                let item_id = match row.columns.first() {
                    Some(Field::Int32(v)) if *v > 0 => *v as u32,
                    _ => continue,
                };
                items.insert(item_id);
            }
        }
        println!("GatheringItem: {} 种可采集物品", items.len());
        items
    }

    /// 加载 SecretRecipeBook 表, 返回多种键 -> 秘籍名称的映射
    /// 键包括:
    ///   - row_id (1-111)
    ///   - item_id (秘籍物品ID)
    ///   - recipe_col40_value (row_id + 546, 用于直接用 Recipe.col[40] 查找)
    pub fn load_secret_recipe_book_names(&self) -> std::collections::HashMap<u32, String> {
        let mut physis = self.physis.borrow_mut();
        let exh = match physis.read_excel_sheet_header("SecretRecipeBook") {
            Ok(h) => h,
            Err(_) => return std::collections::HashMap::new(),
        };
        let sheet =
            match physis.read_excel_sheet(&exh, "SecretRecipeBook", Language::ChineseSimplified) {
                Ok(s) => s,
                Err(_) => return std::collections::HashMap::new(),
            };

        let mut map = std::collections::HashMap::new();
        for page in &sheet.pages {
            for (row_id, row) in page.into_iter().flatten_subrows() {
                // SecretRecipeBook 表: col[0] = Item (Int32), col[1] = Name (String)
                if let (Some(Field::Int32(item_id)), Some(Field::String(name))) =
                    (row.columns.first(), row.columns.get(1))
                {
                    if !name.is_empty() && *item_id > 0 {
                        // 使用 row_id 作为键
                        map.insert(row_id, name.clone());
                        // 使用 item_id 作为键（反向映射）
                        map.insert(*item_id as u32, name.clone());
                        // 使用 Recipe.col[40] 值作为键 (row_id + 546)
                        // 国服只有第一卷(row_id 1-8)的配方在 Recipe.col[40] 中被标记(值 547-554)
                        map.insert(row_id + 546, name.clone());
                    }
                }
            }
        }
        println!("SecretRecipeBook: {} 条秘籍记录（含反向映射和Recipe.col40映射）", map.len());
        map
    }

    /// 加载 RecipeLevelTable 表, 返回 row_id -> 配方等级
    pub fn load_recipe_level_table(&self) -> std::collections::HashMap<u16, u8> {
        let mut physis = self.physis.borrow_mut();
        let exh = match physis.read_excel_sheet_header("RecipeLevelTable") {
            Ok(h) => h,
            Err(_) => return std::collections::HashMap::new(),
        };
        let sheet =
            match physis.read_excel_sheet(&exh, "RecipeLevelTable", Language::None) {
                Ok(s) => s,
                Err(_) => return std::collections::HashMap::new(),
            };
        let mut map = std::collections::HashMap::new();
        for page in &sheet.pages {
            for (row_id, row) in page.into_iter().flatten_subrows() {
                // RecipeLevelTable 表: col[0] = ClassJobLevel (UInt8, 配方所需职业等级)
                if let Some(Field::UInt8(level)) = row.columns.first() {
                    if *level > 0 && row_id <= u16::MAX as u32 {
                        map.insert(row_id as u16, *level);
                    }
                }
            }
        }
        println!("RecipeLevelTable: {} 条等级记录", map.len());
        map
    }

    /// 加载 BaseParam 表, 返回 row_id -> 参数名称
    /// 尝试多种语言，因为国服客户端可能不包含简体中文翻译
    pub fn load_base_param_names(&self) -> std::collections::HashMap<u32, String> {
        let mut physis = self.physis.borrow_mut();
        let exh = match physis.read_excel_sheet_header("BaseParam") {
            Ok(h) => h,
            Err(_) => return std::collections::HashMap::new(),
        };

        // 尝试多种语言加载
        let languages = [
            Language::ChineseSimplified,
            Language::English,
            Language::Japanese,
            Language::None,
        ];

        for lang in &languages {
            if let Ok(sheet) = physis.read_excel_sheet(&exh, "BaseParam", *lang) {
                let mut map = std::collections::HashMap::new();
                for page in &sheet.pages {
                    for (row_id, row) in page.into_iter().flatten_subrows() {
                        // BaseParam 表: col[0] = Name (String)
                        if let Some(Field::String(name)) = row.columns.first() {
                            if !name.is_empty() {
                                map.insert(row_id, name.clone());
                            }
                        }
                    }
                }
                if !map.is_empty() {
                    println!("BaseParam: 使用 {:?} 加载了 {} 条参数记录", lang, map.len());
                    return map;
                }
            }
        }

        println!("BaseParam: 所有语言都返回空");
        std::collections::HashMap::new()
    }

    /// 加载 ItemFood 表, 返回 row_id -> ConsumableInfo
    /// ItemFood 表结构 (根据 xivapi v2):
    ///   BaseParam[3] (引用 BaseParam 表)
    ///   EXPBonusPercent (UInt8)
    ///   IsRelative[3] (Boolean)
    ///   Max[3], MaxHQ[3] (Int16)
    ///   Value[3], ValueHQ[3] (Int16)
    pub fn load_item_food(
        &self,
        base_param_names: &std::collections::HashMap<u32, String>,
    ) -> std::collections::HashMap<u32, ConsumableInfo> {
        let mut physis = self.physis.borrow_mut();

        let exh = match physis.read_excel_sheet_header("ItemFood") {
            Ok(h) => h,
            Err(e) => {
                eprintln!("无法加载 ItemFood 表头: {}", e);
                return std::collections::HashMap::new();
            }
        };
        let sheet = match physis.read_excel_sheet(&exh, "ItemFood", Language::None) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("无法加载 ItemFood 表: {}", e);
                return std::collections::HashMap::new();
            }
        };

        let mut map = std::collections::HashMap::new();
        for page in &sheet.pages {
            for (row_id, row) in page.into_iter().flatten_subrows() {
                let mut effects = Vec::new();

                // ItemFood 表实际结构 (根据测试结果):
                // col[0] = EXPBonusPercent (UInt8)
                // 每组效果占 6 列, 共 3 组:
                //   col[1+i*6+0] = BaseParam[i] (UInt8)
                //   col[1+i*6+1] = IsRelative[i] (Bool)
                //   col[1+i*6+2] = Value[i] (Int8)
                //   col[1+i*6+3] = Max[i] (Int16)
                //   col[1+i*6+4] = ValueHQ[i] (Int8)
                //   col[1+i*6+5] = MaxHQ[i] (Int16)
                for i in 0..3 {
                    let base_col = 1 + i * 6;
                    let base_param_id = match row.columns.get(base_col) {
                        Some(Field::UInt8(v)) if *v > 0 => *v as u32,
                        Some(Field::UInt16(v)) if *v > 0 => *v as u32,
                        _ => continue,
                    };

                    let value = match row.columns.get(base_col + 2) {
                        Some(Field::Int8(v)) => *v as u16,
                        Some(Field::UInt8(v)) => *v as u16,
                        _ => 0,
                    };

                    let max_value = match row.columns.get(base_col + 3) {
                        Some(Field::Int16(v)) => *v as u16,
                        Some(Field::UInt16(v)) => *v,
                        _ => 0,
                    };

                    let hq_value = match row.columns.get(base_col + 4) {
                        Some(Field::Int8(v)) => *v as u16,
                        Some(Field::UInt8(v)) => *v as u16,
                        _ => 0,
                    };

                    let hq_max_value = match row.columns.get(base_col + 5) {
                        Some(Field::Int16(v)) => *v as u16,
                        Some(Field::UInt16(v)) => *v,
                        _ => 0,
                    };

                    let param_name = base_param_names
                        .get(&base_param_id)
                        .cloned()
                        .unwrap_or_else(|| format!("属性#{}", base_param_id));

                    effects.push(ConsumableEffect {
                        param_name,
                        percentage: value,
                        max_value,
                        hq_percentage: hq_value,
                        hq_max_value,
                    });
                }

                if !effects.is_empty() {
                    map.insert(row_id, ConsumableInfo { item_id: 0, effects });
                }
            }
        }
        println!("ItemFood: {} 条有效果记录", map.len());
        map
    }

    /// 加载 ItemAction 表, 返回 ItemAction row_id -> (ItemFood row_id, Data[0], DataHQ[0])
    pub fn load_item_actions(&self,
    ) -> std::collections::HashMap<u32, (u32, u32)> {
        let mut physis = self.physis.borrow_mut();

        let exh = match physis.read_excel_sheet_header("ItemAction") {
            Ok(h) => h,
            Err(e) => {
                eprintln!("无法加载 ItemAction 表头: {}", e);
                return std::collections::HashMap::new();
            }
        };
        let sheet = match physis.read_excel_sheet(&exh, "ItemAction", Language::None) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("无法加载 ItemAction 表: {}", e);
                return std::collections::HashMap::new();
            }
        };

        let mut map = std::collections::HashMap::new();
        for page in &sheet.pages {
            for (row_id, row) in page.into_iter().flatten_subrows() {
                // ItemAction 表结构 (根据测试结果):
                // col[0] = Action category or flags (UInt8)
                // col[1..3] = CondBattle, CondPVP, CondPVPOnly (Bool)
                // col[4] = Action ID (UInt16)
                // col[5..13] = Data[0..8] (UInt16)
                // col[14..22] = DataHQ[0..8] (UInt16)
                // Data[0] 指向 ItemFood row_id
                let data_0 = match row.columns.get(5) {
                    Some(Field::UInt16(v)) if *v > 0 => *v as u32,
                    Some(Field::UInt8(v)) if *v > 0 => *v as u32,
                    Some(Field::UInt32(v)) if *v > 0 => *v,
                    Some(Field::Int32(v)) if *v > 0 => *v as u32,
                    _ => 0,
                };

                let data_hq_0 = match row.columns.get(14) {
                    Some(Field::UInt16(v)) if *v > 0 => *v as u32,
                    Some(Field::UInt8(v)) if *v > 0 => *v as u32,
                    Some(Field::UInt32(v)) if *v > 0 => *v,
                    Some(Field::Int32(v)) if *v > 0 => *v as u32,
                    _ => 0,
                };

                if data_0 > 0 {
                    map.insert(row_id, (data_0, data_hq_0));
                }
            }
        }
        println!("ItemAction: {} 条有效果记录", map.len());
        map
    }

    /// 扫描 Item 表，自动找出 ItemAction 列的正确索引
    /// 返回: (ItemAction 列索引, 匹配到的消耗品数量)
    pub fn scan_item_action_column(&self) -> (usize, usize) {
        let mut physis = self.physis.borrow_mut();

        // 1. 加载 ItemAction 表，收集所有 row_id
        let mut action_ids: std::collections::HashSet<u32> = std::collections::HashSet::new();
        if let Ok(exh) = physis.read_excel_sheet_header("ItemAction") {
            if let Ok(sheet) = physis.read_excel_sheet(&exh, "ItemAction", Language::None) {
                for page in &sheet.pages {
                    for (row_id, _) in page.into_iter().flatten_subrows() {
                        action_ids.insert(row_id);
                    }
                }
            }
        }
        println!("ItemAction 表: {} 个 ID", action_ids.len());
        if action_ids.is_empty() {
            return (0, 0);
        }

        // 2. 加载 Item 表，统计每列匹配 ItemAction ID 的次数
        let mut column_hits: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
        let mut total_consumables = 0;

        if let Ok(exh) = physis.read_excel_sheet_header("Item") {
            if let Ok(sheet) = physis.read_excel_sheet(&exh, "Item", Language::ChineseSimplified) {
                for page in &sheet.pages {
                    for (_row_id, row) in page.into_iter().flatten_subrows() {
                        // 检查是否为消耗品 (FilterGroup=5 Meal 或 6 Medicine)
                        let filter_group = match row.columns.get(13) {
                            Some(Field::UInt8(v)) => *v,
                            _ => continue,
                        };
                        if filter_group != 5 && filter_group != 6 {
                            continue;
                        }
                        total_consumables += 1;

                        // 检查各列的值是否在 ItemAction ID 集合中
                        for (col_idx, col) in row.columns.iter().enumerate() {
                            let val = match col {
                                Field::Int32(v) if *v > 0 => *v as u32,
                                Field::UInt32(v) if *v > 0 => *v,
                                Field::UInt16(v) if *v > 0 => *v as u32,
                                Field::UInt8(v) if *v > 0 => *v as u32,
                                _ => continue,
                            };
                            if action_ids.contains(&val) {
                                *column_hits.entry(col_idx).or_insert(0) += 1;
                            }
                        }
                    }
                }
            }
        }

        // 3. 找出匹配次数最多的列
        let best = column_hits.iter().max_by_key(|(_, count)| *count);
        if let Some((col_idx, count)) = best {
            println!(
                "ItemAction 列扫描: {} 个消耗品, 列 [{}] 匹配 {} 次 (最佳)",
                total_consumables, col_idx, count
            );
            // 打印前5个候选列
            let mut sorted: Vec<_> = column_hits.iter().collect();
            sorted.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
            for (i, (col_idx, count)) in sorted.iter().take(5).enumerate() {
                println!("  候选 #{}: 列 [{}] = {} 次", i + 1, col_idx, count);
            }
            (*col_idx, *count)
        } else {
            println!("ItemAction 列扫描: 未找到匹配的列");
            (0, 0)
        }
    }

    /// 调试：输出指定 Item 行的所有列数据
    pub fn debug_item_columns(&self, target_row_id: u32, expected_name: &str) {
        let mut physis = self.physis.borrow_mut();

        let exh = match physis.read_excel_sheet_header("Item") {
            Ok(h) => h,
            Err(e) => {
                eprintln!("无法加载 Item 表头: {}", e);
                return;
            }
        };

        let sheet = match physis.read_excel_sheet(&exh, "Item", Language::ChineseSimplified) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("无法加载 Item 表: {}", e);
                return;
            }
        };

        for page in &sheet.pages {
            for (row_id, row) in page.into_iter().flatten_subrows() {
                if row_id != target_row_id {
                    continue;
                }

                println!("\n=== 调试: Item row_id={} (预期: {}) ===", row_id, expected_name);
                println!("总列数: {}", row.columns.len());

                // 只输出非零数值列（更容易定位 ItemAction）
                for (col_idx, col) in row.columns.iter().enumerate() {
                    let val_opt = match col {
                        Field::String(s) if !s.is_empty() => {
                            if col_idx <= 15 {
                                Some(format!("String(\"{}\")", s))
                            } else {
                                None
                            }
                        }
                        Field::Int8(v) if *v != 0 => Some(format!("Int8({})", v)),
                        Field::UInt8(v) if *v != 0 => Some(format!("UInt8({})", v)),
                        Field::Int16(v) if *v != 0 => Some(format!("Int16({})", v)),
                        Field::UInt16(v) if *v != 0 => Some(format!("UInt16({})", v)),
                        Field::Int32(v) if *v != 0 => Some(format!("Int32({})", v)),
                        Field::UInt32(v) if *v != 0 => Some(format!("UInt32({})", v)),
                        Field::Int64(v) if *v != 0 => Some(format!("Int64({})", v)),
                        Field::UInt64(v) if *v != 0 => Some(format!("UInt64({})", v)),
                        _ => None,
                    };
                    if let Some(val_str) = val_opt {
                        println!("  col[{:2}] = {}", col_idx, val_str);
                    }
                }

                println!("=== 调试结束 ===\n");
                return;
            }
        }

        println!("未找到 row_id={} 的 Item 记录", target_row_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use physis::excel::Field;
    use physis::Language;

    fn get_game_data() -> Option<GameData> {
        let config = crate::config::load_config();
        println!("Config game_install_dir: {:?}", config.game_install_dir);
        let install_dir = config.game_install_dir?;
        println!("Install dir: {:?}", install_dir);
        if let Err(e) = validate_install_dir(&install_dir) {
            println!("跳过测试: {}", e);
            return None;
        }
        println!("GameData created successfully");
        Some(GameData::new(&install_dir))
    }

    /// 测试 1: 直接查找 Boiled Egg 的 ItemAction 列位置
    #[test]
    fn test_find_item_action_column() {
        let game = get_game_data().expect("无法获取游戏数据");
        let mut physis = game.physis.borrow_mut();

        // 已知: Boiled Egg (4650) 的 ItemAction = 56
        const TARGET_ROW_ID: u32 = 4650;
        const EXPECTED_ACTION_ID: u32 = 56;

        // 加载 Item 表
        let exh = physis.read_excel_sheet_header("Item").expect("无法加载 Item");
        let sheet = physis.read_excel_sheet(&exh, "Item", Language::ChineseSimplified).expect("无法读取 Item");

        let mut found = false;
        for page in &sheet.pages {
            for (row_id, row) in page.into_iter().flatten_subrows() {
                if row_id != TARGET_ROW_ID {
                    continue;
                }
                found = true;
                println!("=== Boiled Egg (row_id={}) 的列数据 ===", row_id);
                println!("总列数: {}", row.columns.len());

                // 找出哪一列等于 56
                let mut action_col = None;
                for (col_idx, col) in row.columns.iter().enumerate() {
                    let val_opt = match col {
                        Field::Int32(v) if *v == EXPECTED_ACTION_ID as i32 => Some(*v as u32),
                        Field::UInt32(v) if *v == EXPECTED_ACTION_ID => Some(*v),
                        Field::UInt16(v) if *v == EXPECTED_ACTION_ID as u16 => Some(*v as u32),
                        Field::UInt8(v) if *v == EXPECTED_ACTION_ID as u8 => Some(*v as u32),
                        _ => None,
                    };
                    if val_opt.is_some() {
                        action_col = Some(col_idx);
                        println!("*** 找到! col[{}] = {} (ItemAction ID) ***", col_idx, EXPECTED_ACTION_ID);
                    }
                    // 同时打印非零列帮助理解结构
                    let display = match col {
                        Field::String(s) if !s.is_empty() => format!("String(\"{}\")", s),
                        Field::String(_) => "String(\"\")".to_string(),
                        Field::Int32(v) => format!("Int32({})", v),
                        Field::UInt32(v) => format!("UInt32({})", v),
                        Field::Int16(v) => format!("Int16({})", v),
                        Field::UInt16(v) => format!("UInt16({})", v),
                        Field::UInt8(v) => format!("UInt8({})", v),
                        Field::Int64(v) => format!("Int64({})", v),
                        Field::UInt64(v) => format!("UInt64({})", v),
                        Field::Int8(v) => format!("Int8({})", v),
                        Field::Bool(v) => format!("Bool({})", v),
                        Field::Float32(v) => format!("Float32({})", v),
                    };
                    println!("  col[{:2}] = {}", col_idx, display);
                }

                let col = action_col.expect(&format!("未找到值为 {} 的列", EXPECTED_ACTION_ID));
                println!("\n结论: ItemAction 列在 col[{}]", col);
                
                // 同时检查其他已知消耗品验证
                println!("\n验证其他消耗品:");
                break;
            }
        }
        assert!(found, "未找到 Boiled Egg (row_id={})", TARGET_ROW_ID);
    }

    /// 测试 2: 验证 ItemAction 表结构，找到 Data[0] 所在列
    #[test]
    fn test_item_action_structure() {
        let game = get_game_data().expect("无法获取游戏数据");
        let mut physis = game.physis.borrow_mut();

        let exh = physis.read_excel_sheet_header("ItemAction").expect("无法加载 ItemAction");
        let sheet = physis.read_excel_sheet(&exh, "ItemAction", Language::None).expect("无法读取 ItemAction");

        let mut found_56 = false;
        for page in &sheet.pages {
            for (row_id, row) in page.into_iter().flatten_subrows() {
                if row_id != 56 {
                    continue;
                }
                found_56 = true;
                println!("=== ItemAction row_id=56 的列数据 ===");
                println!("总列数: {}", row.columns.len());

                // 打印所有列，找到值为 48 的列（已知的 ItemFood ID）
                let mut data_0_col = None;
                const EXPECTED_FOOD_ID: u32 = 48;
                
                for (i, col) in row.columns.iter().enumerate() {
                    let val_opt = match col {
                        Field::Int32(v) if *v == EXPECTED_FOOD_ID as i32 => Some(*v as u32),
                        Field::UInt32(v) if *v == EXPECTED_FOOD_ID => Some(*v),
                        Field::UInt16(v) if *v == EXPECTED_FOOD_ID as u16 => Some(*v as u32),
                        Field::UInt8(v) if *v == EXPECTED_FOOD_ID as u8 => Some(*v as u32),
                        _ => None,
                    };
                    if val_opt.is_some() {
                        data_0_col = Some(i);
                        println!("*** 找到! col[{}] = {} (ItemFood ID) ***", i, EXPECTED_FOOD_ID);
                    }
                    println!("  col[{:2}] = {:?}", i, col);
                }

                let col = data_0_col.expect(&format!("未找到值为 {} 的列", EXPECTED_FOOD_ID));
                println!("\n结论: ItemAction.Data[0] 在 col[{}]", col);
                break;
            }
        }
        assert!(found_56, "未找到 ItemAction row_id=56");
    }

    /// 测试 3: 验证 ItemFood 表结构
    #[test]
    fn test_item_food_structure() {
        let Some(game) = get_game_data() else { return };
        let mut physis = game.physis.borrow_mut();

        let exh = physis.read_excel_sheet_header("ItemFood").expect("无法加载 ItemFood");
        let sheet = physis.read_excel_sheet(&exh, "ItemFood", Language::None).expect("无法读取 ItemFood");

        let mut found_48 = false;
        for page in &sheet.pages {
            for (row_id, row) in page.into_iter().flatten_subrows() {
                if row_id != 48 {
                    continue;
                }
                found_48 = true;
                println!("ItemFood row_id=48 的列数: {}", row.columns.len());

                // 打印所有列
                for (i, col) in row.columns.iter().enumerate() {
                    println!("  col[{}] = {:?}", i, col);
                }

                // BaseParam[0] 应该在 col[0]
                let base_param = match row.columns.get(0) {
                    Some(Field::Int32(v)) => *v as u32,
                    Some(Field::UInt32(v)) => *v,
                    Some(Field::UInt16(v)) => *v as u32,
                    Some(Field::UInt8(v)) => *v as u32,
                    other => panic!("ItemFood BaseParam[0] 类型不对: {:?}", other),
                };
                println!("ItemFood(48).BaseParam[0] = {}", base_param);
                assert_eq!(base_param, 3, "Boiled Egg 的 ItemFood BaseParam[0] 应该是 3 (Vitality)");
                break;
            }
        }
        assert!(found_48, "未找到 ItemFood row_id=48");
    }

    /// 测试 4: 验证 BaseParam 表结构
    #[test]
    fn test_base_param_names() {
        let game = get_game_data().expect("无法获取游戏数据");
        let mut physis = game.physis.borrow_mut();

        let exh = physis.read_excel_sheet_header("BaseParam").expect("无法加载 BaseParam");
        println!("BaseParam 表: {} 列, 语言: {:?}", exh.column_definitions.len(), exh.languages);

        // 打印前几行看结构
        let sheet = physis.read_excel_sheet(&exh, "BaseParam", Language::ChineseSimplified)
            .or_else(|_| physis.read_excel_sheet(&exh, "BaseParam", Language::English))
            .or_else(|_| physis.read_excel_sheet(&exh, "BaseParam", Language::None))
            .expect("无法读取 BaseParam");
        
        let mut names = std::collections::HashMap::new();
        for page in &sheet.pages {
            for (row_id, row) in page.into_iter().flatten_subrows().take(5) {
                println!("BaseParam row_id={}: {} 列", row_id, row.columns.len());
                for (i, col) in row.columns.iter().enumerate() {
                    println!("  col[{}] = {:?}", i, col);
                }
                if let Some(Field::String(name)) = row.columns.first() {
                    if !name.is_empty() {
                        names.insert(row_id, name.clone());
                    }
                }
            }
        }
        
        println!("BaseParam: 找到 {} 条有名称记录", names.len());
        // 不强制断言，因为国服可能真的没有 BaseParam 翻译
        // 但如果找到了，验证 Vitality
        if let Some(vitality) = names.get(&3) {
            println!("BaseParam row_id=3 = '{}'", vitality);
        }
    }

    /// 测试 5: 完整链路测试（直接读取验证）
    #[test]
    fn test_full_consumable_chain() {
        let game = get_game_data().expect("无法获取游戏数据");
        let mut physis = game.physis.borrow_mut();

        // 1. 读取 Item 表，找到 Boiled Egg 和 ItemAction 列
        let item_exh = physis.read_excel_sheet_header("Item").expect("无法加载 Item");
        let item_sheet = physis.read_excel_sheet(&item_exh, "Item", Language::ChineseSimplified).expect("无法读取 Item");
        
        let mut boiled_egg_action_col = None;
        for page in &item_sheet.pages {
            for (row_id, row) in page.into_iter().flatten_subrows() {
                if row_id != 4650 {
                    continue;
                }
                // 找到值为 56 的列
                for (col_idx, col) in row.columns.iter().enumerate() {
                    let is_match = match col {
                        Field::Int32(v) => *v == 56,
                        Field::UInt32(v) => *v == 56,
                        Field::UInt16(v) => *v == 56,
                        Field::UInt8(v) => *v == 56,
                        _ => false,
                    };
                    if is_match {
                        boiled_egg_action_col = Some(col_idx);
                        println!("Boiled Egg (4650): ItemAction=56 在 col[{}]", col_idx);
                        break;
                    }
                }
                break;
            }
        }
        let action_col = boiled_egg_action_col.expect("未找到 Boiled Egg 的 ItemAction 列");

        // 2. 读取 ItemAction 表，找到 row_id=56，找到 Data[0]=48 的列
        let action_exh = physis.read_excel_sheet_header("ItemAction").expect("无法加载 ItemAction");
        let action_sheet = physis.read_excel_sheet(&action_exh, "ItemAction", Language::None).expect("无法读取 ItemAction");
        
        let mut action_data_col = None;
        for page in &action_sheet.pages {
            for (row_id, row) in page.into_iter().flatten_subrows() {
                if row_id != 56 {
                    continue;
                }
                for (col_idx, col) in row.columns.iter().enumerate() {
                    let is_match = match col {
                        Field::Int32(v) => *v == 48,
                        Field::UInt32(v) => *v == 48,
                        Field::UInt16(v) => *v == 48,
                        Field::UInt8(v) => *v == 48,
                        _ => false,
                    };
                    if is_match {
                        action_data_col = Some(col_idx);
                        println!("ItemAction(56): Data[0]=48 在 col[{}]", col_idx);
                        break;
                    }
                }
                break;
            }
        }
        let data_col = action_data_col.expect("未找到 ItemAction 56 的 Data[0] 列");

        // 3. 读取 ItemFood 表，验证 row_id=48
        let food_exh = physis.read_excel_sheet_header("ItemFood").expect("无法加载 ItemFood");
        let food_sheet = physis.read_excel_sheet(&food_exh, "ItemFood", Language::None).expect("无法读取 ItemFood");
        
        let mut found_food = false;
        for page in &food_sheet.pages {
            for (row_id, row) in page.into_iter().flatten_subrows() {
                if row_id != 48 {
                    continue;
                }
                found_food = true;
                println!("ItemFood(48): {} 列", row.columns.len());
                println!("效果数据:");
                for (i, col) in row.columns.iter().enumerate() {
                    println!("  col[{}] = {:?}", i, col);
                }
                break;
            }
        }
        assert!(found_food, "未找到 ItemFood row_id=48");

        println!("\n=== 完整链路验证通过 ===");
        println!("Item(4650 Boiled Egg) -> col[{}]=56 -> ItemAction(56) -> col[{}]=48 -> ItemFood(48)", action_col, data_col);
    }
}
