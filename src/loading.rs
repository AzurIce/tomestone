use std::collections::HashMap;
use std::path::PathBuf;

use physis::stm::StainingTemplate;

use crate::domain::{
    build_equipment_sets, EquipmentItem, EquipmentSet, HousingExteriorItem, StainEntry, ALL_SLOTS,
};
use crate::game::GameData;
use crate::glamour;
use crate::ui::pages::resource::ResourceBrowserState;

pub struct GameState {
    pub game: GameData,
    pub items: Vec<EquipmentItem>,
    pub stains: Vec<StainEntry>,
    pub stm: Option<StainingTemplate>,
    pub equipment_sets: Vec<EquipmentSet>,
    pub set_id_to_set_idx: HashMap<u16, usize>,
    pub item_id_map: HashMap<u32, usize>,
    pub glamour_sets: Vec<glamour::GlamourSet>,
    pub resource_browser: ResourceBrowserState,
    pub housing_exteriors: Vec<HousingExteriorItem>,
}

pub enum LoadProgress {
    Status(String),
    Done(Box<LoadedData>),
    Error(String),
}

pub struct LoadedData {
    pub game: GameData,
    pub items: Vec<EquipmentItem>,
    pub stains: Vec<StainEntry>,
    pub stm: Option<StainingTemplate>,
    pub all_table_names: Vec<String>,
    pub housing_exteriors: Vec<HousingExteriorItem>,
}

pub fn load_game_data_thread(install_dir: PathBuf, tx: std::sync::mpsc::Sender<LoadProgress>) {
    if let Err(e) = crate::game::validate_install_dir(&install_dir) {
        let _ = tx.send(LoadProgress::Error(e));
        return;
    }

    let _ = tx.send(LoadProgress::Status("正在初始化游戏数据...".to_string()));
    let game = GameData::new(&install_dir);

    let _ = tx.send(LoadProgress::Status("正在加载装备列表...".to_string()));
    let items = game.load_equipment_list();

    let _ = tx.send(LoadProgress::Status("正在加载染料列表...".to_string()));
    let stains = game.load_stain_list();

    let _ = tx.send(LoadProgress::Status("正在加载染色模板...".to_string()));
    let stm = game.load_staining_template();

    let _ = tx.send(LoadProgress::Status("正在加载 EXD 表名列表...".to_string()));
    let mut all_table_names = game.get_all_sheet_names();
    all_table_names.sort();

    let _ = tx.send(LoadProgress::Status("正在加载房屋外装列表...".to_string()));
    let housing_exteriors = game.load_housing_exterior_list();

    let _ = tx.send(LoadProgress::Done(Box::new(LoadedData {
        game,
        items,
        stains,
        stm,
        all_table_names,
        housing_exteriors,
    })));
}

pub fn glamour_slot_summary(
    items: &[EquipmentItem],
    item_id_map: &HashMap<u32, usize>,
    gs: &glamour::GlamourSet,
) -> String {
    let mut parts = Vec::new();
    for slot in &ALL_SLOTS {
        if let Some(gslot) = gs.get_slot(*slot) {
            let name = item_id_map
                .get(&gslot.item_id)
                .and_then(|&idx| items.get(idx))
                .map(|item| item.name.as_str())
                .unwrap_or("???");
            parts.push(format!("[{}]{}", slot.slot_abbr(), name));
        }
    }
    parts.join(" ")
}

impl GameState {
    pub fn from_loaded_data(data: LoadedData) -> Self {
        let equipment_sets = build_equipment_sets(&data.items);
        let set_id_to_set_idx = equipment_sets
            .iter()
            .enumerate()
            .map(|(i, s)| (s.set_id, i))
            .collect();
        let item_id_map = data
            .items
            .iter()
            .enumerate()
            .map(|(i, item)| (item.row_id, i))
            .collect();
        let glamour_sets = glamour::load_all_glamour_sets();
        let resource_browser = ResourceBrowserState::new(data.all_table_names);

        Self {
            game: data.game,
            items: data.items,
            stains: data.stains,
            stm: data.stm,
            equipment_sets,
            set_id_to_set_idx,
            item_id_map,
            glamour_sets,
            resource_browser,
            housing_exteriors: data.housing_exteriors,
        }
    }
}
