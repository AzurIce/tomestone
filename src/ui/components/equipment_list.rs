use std::collections::{BTreeMap, HashMap, HashSet};

use eframe::egui;

use crate::domain::{EquipSlot, EquipmentItem, EquipmentSet, SortOrder};

/// 套装分组装备列表的共享状态
pub struct EquipmentListState {
    pub search: String,
    pub sort_order: SortOrder,
    pub expanded_sets: HashSet<u16>,
}

impl EquipmentListState {
    pub fn new() -> Self {
        Self {
            search: String::new(),
            sort_order: SortOrder::ByName,
            expanded_sets: HashSet::new(),
        }
    }
}

/// 点击物品时返回的信息
pub struct ItemClicked {
    pub global_idx: usize,
    pub item_id: u32,
    pub slot: EquipSlot,
}

/// 物品高亮配置
pub struct HighlightConfig<'a> {
    /// 高亮的物品 ID 集合 (如已装备的物品、当前选中的物品)
    pub highlighted_ids: &'a HashSet<u32>,
    /// 预览中的物品 ID (用不同颜色显示)
    pub preview_id: Option<u32>,
}

impl<'a> Default for HighlightConfig<'a> {
    fn default() -> Self {
        Self {
            highlighted_ids: &EMPTY_SET,
            preview_id: None,
        }
    }
}

static EMPTY_SET: std::sync::LazyLock<HashSet<u32>> = std::sync::LazyLock::new(HashSet::new);

impl EquipmentListState {
    /// 显示套装分组的装备列表，返回被点击的物品信息
    ///
    /// - `slot_filter`: 可选槽位筛选
    /// - `highlight`: 高亮配置
    /// - `id_salt`: egui ID 盐值，避免多实例冲突
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        items: &[EquipmentItem],
        equipment_sets: &[EquipmentSet],
        set_id_to_set_idx: &HashMap<u16, usize>,
        slot_filter: Option<EquipSlot>,
        highlight: &HighlightConfig<'_>,
        id_salt: &str,
    ) -> Option<ItemClicked> {
        // 搜索
        ui.horizontal(|ui| {
            ui.label("搜索:");
            ui.text_edit_singleline(&mut self.search);
        });

        // 排序
        ui.horizontal(|ui| {
            ui.label("排序:");
            egui::ComboBox::from_id_salt(format!("{}_sort", id_salt))
                .selected_text(self.sort_order.label())
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.sort_order,
                        SortOrder::ByName,
                        SortOrder::ByName.label(),
                    );
                    ui.selectable_value(
                        &mut self.sort_order,
                        SortOrder::BySetId,
                        SortOrder::BySetId.label(),
                    );
                });
        });

        ui.separator();

        // 构建套装分组
        let search_lower = self.search.to_lowercase();
        let mut set_groups: Vec<(u16, String, bool, bool, Vec<(usize, &EquipmentItem)>)> =
            Vec::new();
        {
            let mut by_set: BTreeMap<u16, Vec<(usize, &EquipmentItem)>> = BTreeMap::new();
            for (idx, item) in items.iter().enumerate() {
                if let Some(sf) = slot_filter {
                    if item.slot != sf {
                        continue;
                    }
                }
                if !search_lower.is_empty() && !item.name.to_lowercase().contains(&search_lower) {
                    continue;
                }
                by_set.entry(item.set_id).or_default().push((idx, item));
            }

            for (set_id, items_in_set) in by_set {
                let group_name = if let Some(&set_idx) = set_id_to_set_idx.get(&set_id) {
                    equipment_sets[set_idx].display_name.clone()
                } else if let Some((_, first)) = items_in_set.first() {
                    first.name.clone()
                } else {
                    format!("set {:04}", set_id)
                };
                let has_gear = items_in_set.iter().any(|(_, item)| !item.is_accessory());
                let has_acc = items_in_set.iter().any(|(_, item)| item.is_accessory());
                set_groups.push((set_id, group_name, has_gear, has_acc, items_in_set));
            }
        }

        match self.sort_order {
            SortOrder::ByName | SortOrder::BySlot => {
                set_groups.sort_by(|a, b| a.1.cmp(&b.1));
            }
            SortOrder::BySetId => {
                set_groups.sort_by(|a, b| a.0.cmp(&b.0));
            }
        }

        let total_items: usize = set_groups
            .iter()
            .map(|(_, _, _, _, items)| items.len())
            .sum();
        ui.label(format!("{} 组, {} 件", set_groups.len(), total_items));

        // 渲染列表
        let mut clicked: Option<ItemClicked> = None;

        egui::ScrollArea::vertical()
            .id_salt(format!("{}_scroll", id_salt))
            .show(ui, |ui| {
                for (set_id, group_name, has_gear, has_acc, items_in_set) in &set_groups {
                    let expanded = self.expanded_sets.contains(set_id);
                    let prefix = match (*has_gear, *has_acc) {
                        (true, true) => "e+a",
                        (false, true) => "a",
                        _ => "e",
                    };
                    let arrow = if expanded { "▼" } else { "▶" };
                    let header_text = format!(
                        "{} {} ({}件) {}{:04}",
                        arrow,
                        group_name,
                        items_in_set.len(),
                        prefix,
                        set_id
                    );

                    let group_has_highlight = items_in_set
                        .iter()
                        .any(|(_, item)| highlight.highlighted_ids.contains(&item.row_id));

                    if ui
                        .selectable_label(
                            group_has_highlight,
                            egui::RichText::new(&header_text).strong(),
                        )
                        .clicked()
                    {
                        if self.expanded_sets.contains(set_id) {
                            self.expanded_sets.remove(set_id);
                        } else {
                            self.expanded_sets.insert(*set_id);
                        }
                    }

                    if expanded {
                        for (global_idx, item) in items_in_set {
                            let is_highlighted = highlight.highlighted_ids.contains(&item.row_id);
                            let is_preview = highlight.preview_id == Some(item.row_id);
                            let label_text = format!("  [{}] {}", item.slot.slot_abbr(), item.name);
                            let rich = if is_preview {
                                egui::RichText::new(&label_text)
                                    .color(egui::Color32::from_rgb(100, 200, 255))
                            } else {
                                egui::RichText::new(&label_text)
                            };
                            if ui
                                .selectable_label(is_highlighted || is_preview, rich)
                                .clicked()
                            {
                                clicked = Some(ItemClicked {
                                    global_idx: *global_idx,
                                    item_id: item.row_id,
                                    slot: item.slot,
                                });
                            }
                        }
                    }
                }
            });

        clicked
    }
}
