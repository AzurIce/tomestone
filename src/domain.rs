use std::collections::BTreeMap;

// ── 页面路由 ──

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppPage {
    Browser,
    GlamourManager,
    ResourceBrowser,
    Test,
}

// ── 视图模式 & 排序 ──

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    List,
    SetGroup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    ByName,
    BySetId,
    BySlot,
}

impl SortOrder {
    pub fn label(&self) -> &'static str {
        match self {
            Self::ByName => "按名称",
            Self::BySetId => "按套装",
            Self::BySlot => "按槽位",
        }
    }
}

// ── 装备槽位 ──

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

pub const ALL_SLOTS: [EquipSlot; 5] = [
    EquipSlot::Head,
    EquipSlot::Body,
    EquipSlot::Gloves,
    EquipSlot::Legs,
    EquipSlot::Feet,
];

// ── 装备物品 ──

#[derive(Debug, Clone)]
pub struct EquipmentItem {
    pub row_id: u32,
    pub name: String,
    pub slot: EquipSlot,
    pub set_id: u16,
    pub variant_id: u16,
}

impl EquipmentItem {
    pub fn model_path(&self) -> String {
        format!(
            "chara/equipment/e{:04}/model/c0201e{:04}_{}.mdl",
            self.set_id,
            self.set_id,
            self.slot.slot_abbr()
        )
    }

    pub fn model_path_for_race(&self, race_code: &str) -> String {
        format!(
            "chara/equipment/e{:04}/model/{}e{:04}_{}.mdl",
            self.set_id,
            race_code,
            self.set_id,
            self.slot.slot_abbr()
        )
    }

    pub fn model_paths(&self) -> Vec<String> {
        RACE_CODES
            .iter()
            .map(|rc| self.model_path_for_race(rc))
            .collect()
    }
}

pub const RACE_CODES: &[&str] = &[
    "c0201", "c0101", "c0401", "c0301", "c0801", "c0701", "c0601", "c0501", "c1401", "c1301",
    "c1201", "c1101", "c1001", "c0901", "c1801", "c1701", "c1501",
];

// ── 套装分组 ──

pub struct EquipmentSet {
    pub set_id: u16,
    pub display_name: String,
    pub item_indices: Vec<usize>,
}

#[derive(Clone)]
pub enum FlatRow {
    GroupHeader {
        set_id: u16,
        display_name: String,
        item_count: usize,
        expanded: bool,
    },
    Item {
        global_idx: usize,
        label: String,
    },
}

pub fn longest_common_prefix(strings: &[&str]) -> String {
    if strings.is_empty() {
        return String::new();
    }
    let first = strings[0];
    let mut len = first.len();
    for s in &strings[1..] {
        let common: usize = first
            .chars()
            .zip(s.chars())
            .take_while(|(a, b)| a == b)
            .map(|(c, _)| c.len_utf8())
            .sum();
        len = len.min(common);
    }
    first[..len].trim_end().to_string()
}

pub fn derive_set_name(items: &[EquipmentItem], indices: &[usize]) -> String {
    let names: Vec<&str> = indices.iter().map(|&i| items[i].name.as_str()).collect();
    let prefix = longest_common_prefix(&names);
    if prefix.is_empty() {
        if let Some(&idx) = indices.first() {
            return items[idx].name.clone();
        }
        return String::new();
    }
    prefix
}

pub fn build_equipment_sets(items: &[EquipmentItem]) -> Vec<EquipmentSet> {
    let mut by_set: BTreeMap<u16, Vec<usize>> = BTreeMap::new();
    for (i, item) in items.iter().enumerate() {
        by_set.entry(item.set_id).or_default().push(i);
    }
    by_set
        .into_iter()
        .map(|(set_id, item_indices)| {
            let display_name = derive_set_name(items, &item_indices);
            EquipmentSet {
                set_id,
                display_name,
                item_indices,
            }
        })
        .collect()
}

// ── 染料 ──

#[derive(Debug, Clone)]
pub struct StainEntry {
    pub id: u32,
    pub name: String,
    pub color: [u8; 3],
    pub shade: u8,
}

pub const SHADE_ORDER: &[u8] = &[2, 4, 5, 6, 7, 8, 9, 10, 1];

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
