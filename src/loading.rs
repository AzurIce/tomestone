use std::collections::HashMap;
use std::path::PathBuf;

use physis::stm::StainingTemplate;

use crate::domain::{
    build_equipment_sets, EquipmentSet, GameItem, ItemSource, Recipe, StainEntry, ALL_SLOTS,
};
use crate::game::GameData;
use crate::glamour;
use crate::ui::pages::resource::ResourceBrowserState;

pub struct GameState {
    pub game: GameData,
    /// 全部物品 (统一模型)
    pub all_items: Vec<GameItem>,
    /// row_id -> all_items 下标
    pub item_id_map: HashMap<u32, usize>,

    // ── 装备视图索引 ──
    /// 装备类物品在 all_items 中的下标
    pub equipment_indices: Vec<usize>,
    pub equipment_sets: Vec<EquipmentSet>,
    pub set_id_to_set_idx: HashMap<u16, usize>,

    // ── 房屋外装视图索引 ──
    /// 房屋外装物品在 all_items 中的下标
    pub housing_ext_indices: Vec<usize>,
    /// HousingExterior additional_data -> SGB 路径列表
    pub housing_sgb_paths: HashMap<u32, Vec<String>>,

    // ── 房屋家具视图索引 ──
    /// 庭院家具物品在 all_items 中的下标
    pub housing_yard_indices: Vec<usize>,
    /// 室内家具物品在 all_items 中的下标
    pub housing_indoor_indices: Vec<usize>,
    /// HousingFurniture additional_data -> SGB 路径 (室内家具)
    pub housing_furniture_sgb_paths: HashMap<u32, String>,
    /// HousingYardObject additional_data -> SGB 路径 (庭院家具)
    pub housing_yard_sgb_paths: HashMap<u32, String>,

    // ── 其他数据 ──
    pub stains: Vec<StainEntry>,
    pub stm: Option<StainingTemplate>,
    pub glamour_sets: Vec<glamour::GlamourSet>,
    pub resource_browser: ResourceBrowserState,

    // ── 合成数据 ──
    pub recipes: Vec<Recipe>,
    /// item_id -> 配方索引列表 (一个物品可能有多个配方，通常取第一个)
    pub item_to_recipes: HashMap<u32, Vec<usize>>,
    /// 可制作物品在 all_items 中的下标，按 craft_type 分组
    /// craftable_by_type[craft_type] = Vec<(all_items下标, recipe下标)>
    pub craftable_by_type: [Vec<(usize, usize)>; 8],
    /// SecretRecipeBook row_id -> 秘籍名称
    pub secret_recipe_book_names: HashMap<u32, String>,
    /// RecipeLevelTable row_id -> 配方等级 (职业等级)
    pub recipe_levels: HashMap<u16, u8>,

    // ── 物品来源 ──
    /// item_id -> 获取来源列表
    pub item_sources: HashMap<u32, Vec<ItemSource>>,
    /// ItemUICategory row_id -> 分类名称
    pub ui_category_names: HashMap<u8, String>,
}

pub enum LoadProgress {
    Status(String),
    Done(Box<LoadedData>),
    Error(String),
}

pub struct LoadedData {
    pub game: GameData,
    pub all_items: Vec<GameItem>,
    pub stains: Vec<StainEntry>,
    pub stm: Option<StainingTemplate>,
    pub all_table_names: Vec<String>,
    pub housing_sgb_paths: HashMap<u32, Vec<String>>,
    pub housing_furniture_sgb_paths: HashMap<u32, String>,
    pub housing_yard_sgb_paths: HashMap<u32, String>,
    pub recipes: Vec<Recipe>,
    pub ui_category_names: HashMap<u8, String>,
    pub gil_shop_items: std::collections::HashMap<u32, Vec<ItemSource>>,
    pub special_shop_sources: HashMap<u32, Vec<ItemSource>>,
    pub gathering_items: std::collections::HashSet<u32>,
    /// SecretRecipeBook row_id -> 名称
    pub secret_recipe_book_names: HashMap<u32, String>,
    /// RecipeLevelTable row_id -> 配方等级
    pub recipe_levels: HashMap<u16, u8>,
}

pub fn load_game_data_thread(install_dir: PathBuf, tx: std::sync::mpsc::Sender<LoadProgress>) {
    if let Err(e) = crate::game::validate_install_dir(&install_dir) {
        let _ = tx.send(LoadProgress::Error(e));
        return;
    }

    let _ = tx.send(LoadProgress::Status("正在初始化游戏数据...".to_string()));
    let game = GameData::new(&install_dir);

    let _ = tx.send(LoadProgress::Status("正在加载物品列表...".to_string()));
    let all_items = game.load_all_items();

    let _ = tx.send(LoadProgress::Status("正在加载染料列表...".to_string()));
    let stains = game.load_stain_list();

    let _ = tx.send(LoadProgress::Status("正在加载染色模板...".to_string()));
    let stm = game.load_staining_template();

    let _ = tx.send(LoadProgress::Status("正在加载 EXD 表名列表...".to_string()));
    let mut all_table_names = game.get_all_sheet_names();
    all_table_names.sort();

    let _ = tx.send(LoadProgress::Status("正在加载房屋外装数据...".to_string()));
    let housing_sgb_paths = game.load_housing_sgb_paths();

    let _ = tx.send(LoadProgress::Status("正在加载房屋家具数据...".to_string()));
    let housing_furniture_sgb_paths = game.load_housing_furniture_sgb_paths();
    let housing_yard_sgb_paths = game.load_housing_yard_sgb_paths();

    let _ = tx.send(LoadProgress::Status("正在加载配方数据...".to_string()));
    let recipes = game.load_recipes();
    let secret_recipe_book_names = game.load_secret_recipe_book_names();
    let recipe_levels = game.load_recipe_level_table();

    let _ = tx.send(LoadProgress::Status("正在加载物品来源数据...".to_string()));
    let ui_category_names = game.load_ui_category_names();
    let gil_shop_items = game.load_gil_shop_items();
    let special_shop_sources = game.load_special_shop_sources();
    let gathering_items = game.load_gathering_items();

    let _ = tx.send(LoadProgress::Done(Box::new(LoadedData {
        game,
        all_items,
        stains,
        stm,
        all_table_names,
        housing_sgb_paths,
        housing_furniture_sgb_paths,
        housing_yard_sgb_paths,
        recipes,
        ui_category_names,
        gil_shop_items,
        special_shop_sources,
        gathering_items,
        secret_recipe_book_names,
        recipe_levels,
    })));
}

pub fn glamour_slot_summary(
    all_items: &[GameItem],
    item_id_map: &HashMap<u32, usize>,
    gs: &glamour::GlamourSet,
) -> String {
    let mut parts = Vec::new();
    for slot in &ALL_SLOTS {
        if let Some(gslot) = gs.get_slot(*slot) {
            let name = item_id_map
                .get(&gslot.item_id)
                .and_then(|&idx| all_items.get(idx))
                .map(|item| item.name.as_str())
                .unwrap_or("???");
            parts.push(format!("[{}]{}", slot.slot_abbr(), name));
        }
    }
    parts.join(" ")
}

impl GameState {
    pub fn from_loaded_data(data: LoadedData) -> Self {
        // 构建 item_id_map
        let item_id_map: HashMap<u32, usize> = data
            .all_items
            .iter()
            .enumerate()
            .map(|(i, item)| (item.row_id, i))
            .collect();

        // 构建装备视图索引
        let equipment_indices: Vec<usize> = data
            .all_items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.is_equipment())
            .map(|(i, _)| i)
            .collect();

        let equipment_sets = build_equipment_sets(&data.all_items, &equipment_indices);
        let set_id_to_set_idx = equipment_sets
            .iter()
            .enumerate()
            .map(|(i, s)| (s.set_id, i))
            .collect();

        // 构建房屋外装视图索引
        let housing_ext_indices: Vec<usize> = data
            .all_items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                item.is_housing_exterior()
                    && data.housing_sgb_paths.contains_key(&item.additional_data)
            })
            .map(|(i, _)| i)
            .collect();

        // 构建庭院家具视图索引 (直接用 HousingYardObject 表的 Item 列)
        let housing_yard_indices: Vec<usize> = data
            .all_items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                item.filter_group == 14 && data.housing_yard_sgb_paths.contains_key(&item.row_id)
            })
            .map(|(i, _)| i)
            .collect();

        // 构建室内家具视图索引 (直接用 HousingFurniture 表的 Item 列)
        let housing_indoor_indices: Vec<usize> = data
            .all_items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                item.filter_group == 14
                    && data.housing_furniture_sgb_paths.contains_key(&item.row_id)
            })
            .map(|(i, _)| i)
            .collect();

        let glamour_sets = glamour::load_all_glamour_sets();
        let resource_browser = ResourceBrowserState::new(data.all_table_names);

        // 构建配方索引
        let mut item_to_recipes: HashMap<u32, Vec<usize>> = HashMap::new();
        let mut craftable_by_type: [Vec<(usize, usize)>; 8] = Default::default();
        for (recipe_idx, recipe) in data.recipes.iter().enumerate() {
            item_to_recipes
                .entry(recipe.result_item_id)
                .or_default()
                .push(recipe_idx);
            if let Some(&item_idx) = item_id_map.get(&recipe.result_item_id) {
                let ct = (recipe.craft_type as usize).min(7);
                craftable_by_type[ct].push((item_idx, recipe_idx));
            }
        }

        // 构建物品来源索引
        let mut item_sources: HashMap<u32, Vec<ItemSource>> = HashMap::new();
        // 金币商店
        for (item_id, sources) in data.gil_shop_items {
            item_sources.entry(item_id).or_default().extend(sources);
        }
        // 特殊兑换
        for (item_id, sources) in data.special_shop_sources {
            item_sources.entry(item_id).or_default().extend(sources);
        }
        // 采集
        for &item_id in &data.gathering_items {
            item_sources
                .entry(item_id)
                .or_default()
                .push(ItemSource::Gathering);
        }

        // 按消耗去重: 多个商店/兑换点但消耗相同的只保留一个
        for sources in item_sources.values_mut() {
            let mut seen = std::collections::HashSet::new();
            sources.retain(|s| seen.insert(s.cost_key()));
        }

        println!(
            "物品总数: {}, 装备: {}, 房屋外装: {}, 庭院家具: {}, 室内家具: {}, 配方: {}, 有来源物品: {}",
            data.all_items.len(),
            equipment_indices.len(),
            housing_ext_indices.len(),
            housing_yard_indices.len(),
            housing_indoor_indices.len(),
            data.recipes.len(),
            item_sources.len(),
        );

        Self {
            game: data.game,
            all_items: data.all_items,
            item_id_map,
            equipment_indices,
            equipment_sets,
            set_id_to_set_idx,
            housing_ext_indices,
            housing_sgb_paths: data.housing_sgb_paths,
            housing_yard_indices,
            housing_indoor_indices,
            housing_furniture_sgb_paths: data.housing_furniture_sgb_paths,
            housing_yard_sgb_paths: data.housing_yard_sgb_paths,
            stains: data.stains,
            stm: data.stm,
            glamour_sets,
            resource_browser,
            recipes: data.recipes,
            item_to_recipes,
            craftable_by_type,
            item_sources,
            ui_category_names: data.ui_category_names,
            secret_recipe_book_names: data.secret_recipe_book_names,
            recipe_levels: data.recipe_levels,
        }
    }
}
