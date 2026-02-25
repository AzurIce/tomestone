use std::collections::{BTreeMap, HashMap, HashSet};

// ── 页面路由 ──

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppPage {
    Browser,
    GlamourManager,
    HousingBrowser,
    CraftingBrowser,
    ResourceBrowser,
    Test,
}

// ── 房屋外装类型 ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExteriorPartType {
    Roof,
    Wall,
    Window,
    Door,
    RoofDecoration,
    WallDecoration,
    Placard,
    Fence,
}

impl ExteriorPartType {
    /// 从 ItemUICategory 映射
    pub fn from_ui_category(cat: u8) -> Option<Self> {
        match cat {
            65 => Some(Self::Roof),
            66 => Some(Self::Wall),
            67 => Some(Self::Window),
            68 => Some(Self::Door),
            69 => Some(Self::RoofDecoration),
            70 => Some(Self::WallDecoration),
            71 => Some(Self::Placard),
            72 => Some(Self::Fence),
            _ => None,
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Roof => "屋根",
            Self::Wall => "外壁",
            Self::Window => "窓",
            Self::Door => "扉",
            Self::RoofDecoration => "屋根装飾",
            Self::WallDecoration => "外壁装飾",
            Self::Placard => "看板",
            Self::Fence => "塀",
        }
    }
}

pub const EXTERIOR_PART_TYPES: [ExteriorPartType; 8] = [
    ExteriorPartType::Roof,
    ExteriorPartType::Wall,
    ExteriorPartType::Window,
    ExteriorPartType::Door,
    ExteriorPartType::RoofDecoration,
    ExteriorPartType::WallDecoration,
    ExteriorPartType::Placard,
    ExteriorPartType::Fence,
];

// ── 视图模式 & 排序 ──

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    /// 列表视图: 文字行列表（带小图标）
    List,
    /// 图标视图: 图标网格，横向排列自动换行，可调大小
    Grid,
}

impl ViewMode {
    pub fn label(&self) -> &'static str {
        match self {
            Self::List => "列表",
            Self::Grid => "图标",
        }
    }
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
    Earrings,
    Necklace,
    Bracelet,
    Ring,
}

impl EquipSlot {
    pub fn from_category(cat: u8) -> Option<Self> {
        match cat {
            3 => Some(Self::Head),
            4 => Some(Self::Body),
            5 => Some(Self::Gloves),
            7 => Some(Self::Legs),
            8 => Some(Self::Feet),
            9 => Some(Self::Earrings),
            10 => Some(Self::Necklace),
            11 => Some(Self::Bracelet),
            12 => Some(Self::Ring),
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
            Self::Earrings => "ear",
            Self::Necklace => "nek",
            Self::Bracelet => "wrs",
            Self::Ring => "rir",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Head => "头部",
            Self::Body => "身体",
            Self::Gloves => "手部",
            Self::Legs => "腿部",
            Self::Feet => "脚部",
            Self::Earrings => "耳饰",
            Self::Necklace => "项链",
            Self::Bracelet => "手镯",
            Self::Ring => "戒指",
        }
    }

    pub fn is_accessory(&self) -> bool {
        matches!(
            self,
            Self::Earrings | Self::Necklace | Self::Bracelet | Self::Ring
        )
    }
}

pub const ALL_SLOTS: [EquipSlot; 9] = [
    EquipSlot::Head,
    EquipSlot::Body,
    EquipSlot::Gloves,
    EquipSlot::Legs,
    EquipSlot::Feet,
    EquipSlot::Earrings,
    EquipSlot::Necklace,
    EquipSlot::Bracelet,
    EquipSlot::Ring,
];

pub const GEAR_SLOTS: [EquipSlot; 5] = [
    EquipSlot::Head,
    EquipSlot::Body,
    EquipSlot::Gloves,
    EquipSlot::Legs,
    EquipSlot::Feet,
];

pub const ACCESSORY_SLOTS: [EquipSlot; 4] = [
    EquipSlot::Earrings,
    EquipSlot::Necklace,
    EquipSlot::Bracelet,
    EquipSlot::Ring,
];

// ── 统一物品 ──

/// 来自 Item EXD 表的统一物品结构
/// 包含所有物品类型（装备、消耗品、素材、房屋物品等）的公共字段
#[derive(Debug, Clone)]
pub struct GameItem {
    pub row_id: u32,
    pub name: String,
    pub icon_id: u32,
    /// 物品大类 (1=物理武器, 4=防具, 12=素材, 14=房屋, 15=染料, ...)
    pub filter_group: u8,
    /// UI 分类 (链接到 ItemUICategory 表)
    pub item_ui_category: u8,
    /// 装备槽位分类 (链接到 EquipSlotCategory 表, 0=非装备)
    pub equip_slot_category: u8,
    /// 主模型数据 (低16位=set_id, 次16位=variant_id)
    pub model_main: u64,
    /// 附加数据 (FilterGroup=14 时链接到 HousingExterior 等)
    pub additional_data: u32,
    /// 物品描述
    pub description: String,
    /// NPC 买入价 (收购价)
    pub price_mid: u32,
    /// NPC 卖出价
    pub price_low: u32,
    /// 市场板搜索分类 (>0 表示可在市场板交易)
    pub item_search_category: u8,
}

impl GameItem {
    /// 获取装备槽位 (仅装备类物品有效)
    pub fn equip_slot(&self) -> Option<EquipSlot> {
        EquipSlot::from_category(self.equip_slot_category)
    }

    /// 是否为装备类物品
    pub fn is_equipment(&self) -> bool {
        self.equip_slot().is_some() && self.model_main != 0
    }

    /// 装备 set_id (从 model_main 提取)
    pub fn set_id(&self) -> u16 {
        (self.model_main & 0xFFFF) as u16
    }

    /// 装备 variant_id (从 model_main 提取)
    pub fn variant_id(&self) -> u16 {
        ((self.model_main >> 16) & 0xFFFF) as u16
    }

    /// 是否为饰品
    pub fn is_accessory(&self) -> bool {
        self.equip_slot().map_or(false, |s| s.is_accessory())
    }

    /// 获取默认模型路径 (装备类物品)
    pub fn model_path(&self) -> Option<String> {
        let slot = self.equip_slot()?;
        if self.model_main == 0 {
            return None;
        }
        let set_id = self.set_id();
        Some(if slot.is_accessory() {
            format!(
                "chara/accessory/a{:04}/model/c0101a{:04}_{}.mdl",
                set_id,
                set_id,
                slot.slot_abbr()
            )
        } else {
            format!(
                "chara/equipment/e{:04}/model/c0201e{:04}_{}.mdl",
                set_id,
                set_id,
                slot.slot_abbr()
            )
        })
    }

    /// 获取指定种族的模型路径 (装备类物品)
    pub fn model_path_for_race(&self, race_code: &str) -> Option<String> {
        let slot = self.equip_slot()?;
        if self.model_main == 0 {
            return None;
        }
        let set_id = self.set_id();
        Some(if slot.is_accessory() {
            format!(
                "chara/accessory/a{:04}/model/{}a{:04}_{}.mdl",
                set_id,
                race_code,
                set_id,
                slot.slot_abbr()
            )
        } else {
            format!(
                "chara/equipment/e{:04}/model/{}e{:04}_{}.mdl",
                set_id,
                race_code,
                set_id,
                slot.slot_abbr()
            )
        })
    }

    /// 获取所有种族的模型路径列表 (装备类物品)
    pub fn model_paths(&self) -> Vec<String> {
        RACE_CODES
            .iter()
            .filter_map(|rc| self.model_path_for_race(rc))
            .collect()
    }

    /// 是否可在市场板交易 (Universalis 可查)
    pub fn is_marketable(&self) -> bool {
        self.item_search_category > 0
    }

    /// 是否为房屋外装物品
    pub fn is_housing_exterior(&self) -> bool {
        self.filter_group == 14
            && ExteriorPartType::from_ui_category(self.item_ui_category).is_some()
    }

    /// 获取房屋外装类型
    pub fn exterior_part_type(&self) -> Option<ExteriorPartType> {
        if self.filter_group == 14 {
            ExteriorPartType::from_ui_category(self.item_ui_category)
        } else {
            None
        }
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
    /// 在 all_items 中的下标
    pub item_indices: Vec<usize>,
    pub has_gear: bool,
    pub has_accessory: bool,
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

pub fn derive_set_name(items: &[GameItem], indices: &[usize]) -> String {
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

/// 从装备物品索引列表构建套装分组
pub fn build_equipment_sets(
    all_items: &[GameItem],
    equipment_indices: &[usize],
) -> Vec<EquipmentSet> {
    let mut by_set: BTreeMap<u16, Vec<usize>> = BTreeMap::new();
    for &idx in equipment_indices {
        let item = &all_items[idx];
        by_set.entry(item.set_id()).or_default().push(idx);
    }
    by_set
        .into_iter()
        .map(|(set_id, item_indices)| {
            let display_name = derive_set_name(all_items, &item_indices);
            let has_gear = item_indices.iter().any(|&i| !all_items[i].is_accessory());
            let has_accessory = item_indices.iter().any(|&i| all_items[i].is_accessory());
            EquipmentSet {
                set_id,
                display_name,
                item_indices,
                has_gear,
                has_accessory,
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

// ── 合成系统 ──

/// 制作职业名称 (CraftType 0-7)
pub const CRAFT_TYPE_NAMES: [&str; 8] = [
    "刻木匠",
    "锻铁匠",
    "铸甲匠",
    "雕金匠",
    "制革匠",
    "裁衣匠",
    "炼金术士",
    "烹调师",
];

/// 制作职业缩写
pub const CRAFT_TYPE_ABBRS: [&str; 8] = [
    "木工", "锻冶", "甲胄", "雕金", "皮革", "裁缝", "炼金", "烹调",
];

/// 配方数据 (来自 Recipe EXD 表)
#[derive(Debug, Clone)]
pub struct Recipe {
    pub row_id: u32,
    /// 产出物品 ID (链接到 Item 表)
    pub result_item_id: u32,
    /// 产出数量
    pub result_amount: u8,
    /// 制作职业 (0=CRP .. 7=CUL)
    pub craft_type: u8,
    /// 配方等级 (链接到 RecipeLevelTable)
    pub recipe_level: u16,
    /// 素材列表: (item_id, amount)，已过滤掉 item_id==0 的空槽
    pub ingredients: Vec<(u32, u8)>,
}

/// 合成树节点
#[derive(Debug, Clone)]
pub struct CraftTreeNode {
    pub item_id: u32,
    /// 需要的数量
    pub amount_needed: u32,
    /// 如果此素材本身可制作，对应的配方索引
    pub recipe_idx: Option<usize>,
    /// 子节点 (仅当 recipe_idx.is_some() 时有子节点)
    pub children: Vec<CraftTreeNode>,
}

/// 递归构建合成树
/// recipes: 全部配方列表
/// item_to_recipes: item_id -> 配方索引列表 (取第一个)
/// visited: 防止循环引用
pub fn build_craft_tree(
    item_id: u32,
    amount: u32,
    recipes: &[Recipe],
    item_to_recipes: &HashMap<u32, Vec<usize>>,
    visited: &mut HashSet<u32>,
) -> CraftTreeNode {
    // 查找此物品是否有配方
    let recipe_idx = if !visited.contains(&item_id) {
        item_to_recipes
            .get(&item_id)
            .and_then(|indices| indices.first().copied())
    } else {
        None
    };

    let children = if let Some(idx) = recipe_idx {
        visited.insert(item_id);
        let recipe = &recipes[idx];
        // 计算需要制作几次 (向上取整)
        let craft_count = (amount as f64 / recipe.result_amount.max(1) as f64).ceil() as u32;
        let children = recipe
            .ingredients
            .iter()
            .map(|&(ing_id, ing_amount)| {
                let total = ing_amount as u32 * craft_count;
                build_craft_tree(ing_id, total, recipes, item_to_recipes, visited)
            })
            .collect();
        visited.remove(&item_id);
        children
    } else {
        Vec::new()
    };

    CraftTreeNode {
        item_id,
        amount_needed: amount,
        recipe_idx,
        children,
    }
}

/// 感知折叠状态的素材汇总
/// collapsed 中的 (item_id, depth) 对应的节点视为叶子 (不展开子配方)
pub fn summarize_materials_with_collapsed(
    node: &CraftTreeNode,
    collapsed: &HashSet<(u32, usize)>,
) -> Vec<(u32, u32)> {
    let mut map: HashMap<u32, u32> = HashMap::new();
    collect_leaves_collapsed(node, 0, collapsed, &mut map);
    let mut result: Vec<(u32, u32)> = map.into_iter().collect();
    result.sort_by_key(|&(id, _)| id);
    result
}

/// 计算整棵合成树中某个物品的总需求量 (感知折叠状态)
pub fn total_amount_in_tree(
    node: &CraftTreeNode,
    target_id: u32,
    depth: usize,
    collapsed: &HashSet<(u32, usize)>,
) -> u32 {
    if node.item_id == target_id {
        // 如果是叶子或被折叠，直接返回需求量
        if node.children.is_empty() || collapsed.contains(&(node.item_id, depth)) {
            return node.amount_needed;
        }
    }
    // 如果被折叠，不递归子节点
    if collapsed.contains(&(node.item_id, depth)) {
        return if node.item_id == target_id {
            node.amount_needed
        } else {
            0
        };
    }
    let mut total = 0;
    for child in &node.children {
        total += total_amount_in_tree(child, target_id, depth + 1, collapsed);
    }
    total
}

fn collect_leaves_collapsed(
    node: &CraftTreeNode,
    depth: usize,
    collapsed: &HashSet<(u32, usize)>,
    map: &mut HashMap<u32, u32>,
) {
    // 叶子节点，或者被折叠的非叶子节点 → 视为原始素材
    if node.children.is_empty() || collapsed.contains(&(node.item_id, depth)) {
        *map.entry(node.item_id).or_insert(0) += node.amount_needed;
    } else {
        for child in &node.children {
            collect_leaves_collapsed(child, depth + 1, collapsed, map);
        }
    }
}

// ── 物品来源 ──

/// 物品获取来源
#[derive(Debug, Clone)]
pub enum ItemSource {
    /// 金币商店可购买 (价格从 Item.price_mid 获取)
    GilShop {
        shop_name: String,
        npc_location: Option<String>,
    },
    /// 特殊兑换 (诗学/军票/代币等)
    SpecialShop {
        shop_name: String,
        cost_item_id: u32,
        cost_count: u32,
    },
    /// 采集 (采矿/园艺)
    Gathering,
}

impl ItemSource {
    pub fn label(&self) -> &str {
        match self {
            Self::GilShop { .. } => "金币商店",
            Self::SpecialShop { .. } => "兑换",
            Self::Gathering => "采集",
        }
    }

    /// 用于 UI 着色的来源类型标识
    pub fn color_tag(&self) -> u8 {
        match self {
            Self::GilShop { .. } => 1,
            Self::SpecialShop { .. } => 2,
            Self::Gathering => 3,
        }
    }

    /// 用于去重的消耗指纹: 相同消耗的来源视为重复
    /// GilShop 价格来自 Item.price_mid，所有金币商店消耗相同 → 统一 key
    /// SpecialShop 按 (cost_item_id, cost_count) 区分
    /// Gathering 只有一种
    pub fn cost_key(&self) -> (u8, u32, u32) {
        match self {
            Self::GilShop { .. } => (1, 0, 0),
            Self::SpecialShop {
                cost_item_id,
                cost_count,
                ..
            } => (2, *cost_item_id, *cost_count),
            Self::Gathering => (3, 0, 0),
        }
    }

    /// 默认优先级 (越小越优先): 金币商店 > 采集 > 兑换
    pub fn priority(&self) -> u8 {
        match self {
            Self::GilShop { .. } => 1,
            Self::Gathering => 2,
            Self::SpecialShop { .. } => 3,
        }
    }
}

/// 用户对某个素材的来源选择
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceChoice {
    /// 使用 item_sources 列表中的第 N 个来源
    Index(usize),
    /// 忽略 (已持有/不统计成本)
    Ignore,
}

/// 根据来源列表选择默认最优来源的索引
pub fn default_source_index(sources: &[ItemSource]) -> Option<usize> {
    if sources.is_empty() {
        return None;
    }
    sources
        .iter()
        .enumerate()
        .min_by_key(|(_, s)| s.priority())
        .map(|(i, _)| i)
}

/// 获取用户选择的来源 (考虑 override)
pub fn resolve_source<'a>(
    item_id: u32,
    sources: &'a [ItemSource],
    overrides: &std::collections::HashMap<u32, SourceChoice>,
) -> Option<&'a ItemSource> {
    match overrides.get(&item_id) {
        Some(SourceChoice::Ignore) => None,
        Some(SourceChoice::Index(i)) => sources.get(*i),
        None => default_source_index(sources).and_then(|i| sources.get(i)),
    }
}
