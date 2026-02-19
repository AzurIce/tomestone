use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::game_data::EquipSlot;

/// 幻化槽位数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlamourSlot {
    pub item_id: u32,
    pub stain_ids: [u32; 2],
}

/// 幻化组合
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlamourSet {
    pub id: String,
    pub name: String,
    pub slots: HashMap<String, GlamourSlot>,
}

impl GlamourSet {
    pub fn new(name: impl Into<String>) -> Self {
        let id = format!(
            "{:x}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        );
        Self {
            id,
            name: name.into(),
            slots: HashMap::new(),
        }
    }

    pub fn get_slot(&self, slot: EquipSlot) -> Option<&GlamourSlot> {
        self.slots.get(slot_key(slot))
    }

    pub fn set_slot(&mut self, slot: EquipSlot, item_id: u32, stain_ids: [u32; 2]) {
        self.slots.insert(
            slot_key(slot).to_string(),
            GlamourSlot { item_id, stain_ids },
        );
    }

    pub fn remove_slot(&mut self, slot: EquipSlot) {
        self.slots.remove(slot_key(slot));
    }

    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }
}

fn slot_key(slot: EquipSlot) -> &'static str {
    match slot {
        EquipSlot::Head => "head",
        EquipSlot::Body => "body",
        EquipSlot::Gloves => "gloves",
        EquipSlot::Legs => "legs",
        EquipSlot::Feet => "feet",
    }
}

pub fn slot_key_for(slot: EquipSlot) -> &'static str {
    slot_key(slot)
}

fn glamour_dir() -> PathBuf {
    crate::data_dir::glamours_dir()
}

pub fn save_glamour_set(set: &GlamourSet) -> Result<(), String> {
    let dir = glamour_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("创建目录失败: {}", e))?;
    let path = dir.join(format!("{}.json", set.id));
    let json = serde_json::to_string_pretty(set).map_err(|e| format!("序列化失败: {}", e))?;
    fs::write(&path, json).map_err(|e| format!("写入失败: {}", e))?;
    Ok(())
}

pub fn load_all_glamour_sets() -> Vec<GlamourSet> {
    let dir = glamour_dir();
    let mut sets = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json") {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(set) = serde_json::from_str::<GlamourSet>(&content) {
                        sets.push(set);
                    }
                }
            }
        }
    }
    sets
}

pub fn delete_glamour_set(id: &str) -> Result<(), String> {
    let path = glamour_dir().join(format!("{}.json", id));
    fs::remove_file(&path).map_err(|e| format!("删除失败: {}", e))?;
    Ok(())
}
