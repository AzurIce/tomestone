use std::collections::HashMap;
use std::path::PathBuf;

use physis::stm::StainingTemplate;

use crate::domain::{build_equipment_sets, EquipmentSet, GameItem, StainEntry, ALL_SLOTS};
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

    // ── 其他数据 ──
    pub stains: Vec<StainEntry>,
    pub stm: Option<StainingTemplate>,
    pub glamour_sets: Vec<glamour::GlamourSet>,
    pub resource_browser: ResourceBrowserState,
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

    let _ = tx.send(LoadProgress::Done(Box::new(LoadedData {
        game,
        all_items,
        stains,
        stm,
        all_table_names,
        housing_sgb_paths,
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

        let glamour_sets = glamour::load_all_glamour_sets();
        let resource_browser = ResourceBrowserState::new(data.all_table_names);

        println!(
            "物品总数: {}, 装备: {}, 房屋外装: {}",
            data.all_items.len(),
            equipment_indices.len(),
            housing_ext_indices.len()
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
            stains: data.stains,
            stm: data.stm,
            glamour_sets,
            resource_browser,
        }
    }
}
