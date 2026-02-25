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

use crate::domain::{GameItem, ItemSource, Recipe, StainEntry};

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
            rgba: tex.rgba.into(),
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

    /// 一次性加载 Item 表全部物品，返回统一的 GameItem 列表
    pub fn load_all_items(&self) -> Vec<GameItem> {
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
        const COL_CRAFT_TYPE: usize = 1;
        const COL_RECIPE_LEVEL: usize = 2;
        const COL_ITEM_RESULT: usize = 4;
        const COL_AMOUNT_RESULT: usize = 5;
        const COL_INGREDIENT_START: usize = 6; // 每对占 2 列, 共 8 对

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

        Some(Recipe {
            row_id,
            result_item_id,
            result_amount,
            craft_type,
            recipe_level,
            ingredients,
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
}
