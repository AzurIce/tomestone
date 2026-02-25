use std::collections::{BTreeMap, HashMap, HashSet};

use eframe::egui;

use super::item_list;
use crate::domain::{EquipSlot, EquipmentSet, GameItem, SortOrder, ViewMode};
use crate::game::GameData;

/// 套装分组装备列表的共享状态
pub struct EquipmentListState {
    pub search: String,
    pub sort_order: SortOrder,
    pub expanded_sets: HashSet<u16>,
    pub view_mode: ViewMode,
    /// 图标视图中的图标大小 (像素)
    pub icon_size: f32,
}

impl EquipmentListState {
    pub fn new() -> Self {
        Self {
            search: String::new(),
            sort_order: SortOrder::ByName,
            expanded_sets: HashSet::new(),
            view_mode: ViewMode::List,
            icon_size: 48.0,
        }
    }
}

/// 点击物品时返回的信息
pub struct ItemClicked {
    /// 在 all_items 中的下标
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

/// 渲染带图标的物品行
fn show_item_row(
    ui: &mut egui::Ui,
    icon_cache: &mut HashMap<u32, Option<egui::TextureHandle>>,
    ctx: &egui::Context,
    game: &GameData,
    icon_id: u32,
    is_selected: bool,
    rich: egui::RichText,
) -> bool {
    let response = ui.horizontal(|ui| {
        if let Some(icon) = item_list::get_or_load_icon(icon_cache, ctx, game, icon_id) {
            ui.image(egui::load::SizedTexture::new(
                icon.id(),
                egui::vec2(20.0, 20.0),
            ));
        } else {
            ui.allocate_space(egui::vec2(20.0, 20.0));
        }
        ui.selectable_label(is_selected, rich)
    });
    response.inner.clicked()
}

impl EquipmentListState {
    /// 显示套装分组的装备列表，返回被点击的物品信息
    ///
    /// - `all_items`: 全部物品列表
    /// - `equipment_indices`: 装备物品在 all_items 中的下标
    /// - `slot_filter`: 可选槽位筛选
    /// - `highlight`: 高亮配置
    /// - `id_salt`: egui ID 盐值，避免多实例冲突
    /// - `icon_cache`: 图标缓存 (split borrow from App)
    /// - `ctx`: egui Context
    /// - `game`: 游戏数据 (用于加载图标)
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        all_items: &[GameItem],
        equipment_indices: &[usize],
        equipment_sets: &[EquipmentSet],
        set_id_to_set_idx: &HashMap<u16, usize>,
        slot_filter: Option<EquipSlot>,
        highlight: &HighlightConfig<'_>,
        id_salt: &str,
        icon_cache: &mut HashMap<u32, Option<egui::TextureHandle>>,
        ctx: &egui::Context,
        game: &GameData,
    ) -> Option<ItemClicked> {
        // 搜索
        ui.horizontal(|ui| {
            ui.label("搜索:");
            ui.text_edit_singleline(&mut self.search);
        });

        // 排序 + 视图模式
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

            ui.separator();

            if ui
                .selectable_label(self.view_mode == ViewMode::List, ViewMode::List.label())
                .clicked()
            {
                self.view_mode = ViewMode::List;
            }
            if ui
                .selectable_label(self.view_mode == ViewMode::Grid, ViewMode::Grid.label())
                .clicked()
            {
                self.view_mode = ViewMode::Grid;
            }
        });

        // 图标大小滑块 (仅图标视图)
        if self.view_mode == ViewMode::Grid {
            ui.horizontal(|ui| {
                ui.label("图标:");
                ui.add(egui::Slider::new(&mut self.icon_size, 32.0..=128.0).suffix("px"));
            });
        }

        ui.separator();

        match self.view_mode {
            ViewMode::List => self.show_list_view(
                ui,
                all_items,
                equipment_indices,
                equipment_sets,
                set_id_to_set_idx,
                slot_filter,
                highlight,
                id_salt,
                icon_cache,
                ctx,
                game,
            ),
            ViewMode::Grid => self.show_grid_view(
                ui,
                all_items,
                equipment_indices,
                slot_filter,
                highlight,
                id_salt,
                icon_cache,
                ctx,
                game,
            ),
        }
    }

    /// 列表视图: 按套装分组折叠
    fn show_list_view(
        &mut self,
        ui: &mut egui::Ui,
        all_items: &[GameItem],
        equipment_indices: &[usize],
        equipment_sets: &[EquipmentSet],
        set_id_to_set_idx: &HashMap<u16, usize>,
        slot_filter: Option<EquipSlot>,
        highlight: &HighlightConfig<'_>,
        id_salt: &str,
        icon_cache: &mut HashMap<u32, Option<egui::TextureHandle>>,
        ctx: &egui::Context,
        game: &GameData,
    ) -> Option<ItemClicked> {
        // 构建套装分组
        let search_lower = self.search.to_lowercase();
        let mut set_groups: Vec<(u16, String, bool, bool, Vec<(usize, &GameItem)>)> = Vec::new();
        {
            let mut by_set: BTreeMap<u16, Vec<(usize, &GameItem)>> = BTreeMap::new();
            for &idx in equipment_indices {
                let item = &all_items[idx];
                let slot = match item.equip_slot() {
                    Some(s) => s,
                    None => continue,
                };
                if let Some(sf) = slot_filter {
                    if slot != sf {
                        continue;
                    }
                }
                if !search_lower.is_empty() && !item.name.to_lowercase().contains(&search_lower) {
                    continue;
                }
                by_set.entry(item.set_id()).or_default().push((idx, item));
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

        let scroll_height = ui.available_height();
        egui::ScrollArea::vertical()
            .id_salt(format!("{}_scroll", id_salt))
            .max_height(scroll_height)
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
                            let slot = match item.equip_slot() {
                                Some(s) => s,
                                None => continue,
                            };
                            let is_highlighted = highlight.highlighted_ids.contains(&item.row_id);
                            let is_preview = highlight.preview_id == Some(item.row_id);
                            let label_text = format!("[{}] {}", slot.slot_abbr(), item.name);
                            let rich = if is_preview {
                                egui::RichText::new(&label_text)
                                    .color(egui::Color32::from_rgb(100, 200, 255))
                            } else {
                                egui::RichText::new(&label_text)
                            };
                            if show_item_row(
                                ui,
                                icon_cache,
                                ctx,
                                game,
                                item.icon_id,
                                is_highlighted || is_preview,
                                rich,
                            ) {
                                clicked = Some(ItemClicked {
                                    global_idx: *global_idx,
                                    item_id: item.row_id,
                                    slot,
                                });
                            }
                        }
                    }
                }
            });

        clicked
    }

    /// 图标网格视图: 图标横向排列自动换行，可调大小
    fn show_grid_view(
        &mut self,
        ui: &mut egui::Ui,
        all_items: &[GameItem],
        equipment_indices: &[usize],
        slot_filter: Option<EquipSlot>,
        highlight: &HighlightConfig<'_>,
        id_salt: &str,
        icon_cache: &mut HashMap<u32, Option<egui::TextureHandle>>,
        ctx: &egui::Context,
        game: &GameData,
    ) -> Option<ItemClicked> {
        let search_lower = self.search.to_lowercase();
        let filtered: Vec<(usize, &GameItem)> = equipment_indices
            .iter()
            .filter_map(|&idx| {
                let item = &all_items[idx];
                let slot = item.equip_slot()?;
                if let Some(sf) = slot_filter {
                    if slot != sf {
                        return None;
                    }
                }
                if !search_lower.is_empty() && !item.name.to_lowercase().contains(&search_lower) {
                    return None;
                }
                Some((idx, item))
            })
            .collect();

        ui.label(format!("{} 件", filtered.len()));

        let available_width = ui.available_width();
        let icon_size = self.icon_size;
        let cell_padding = 4.0;
        let text_height = 14.0;
        let text_lines = 2;
        let cell_width = (icon_size + cell_padding * 2.0).min(available_width);
        let cell_height = icon_size + cell_padding * 2.0 + text_height * text_lines as f32;
        let cols = ((available_width / cell_width).floor() as usize).max(1);
        // 实际每格宽度: 均分可用宽度
        let actual_cell_width = available_width / cols as f32;
        let total_rows = (filtered.len() + cols - 1) / cols;

        let mut clicked: Option<ItemClicked> = None;

        egui::ScrollArea::vertical()
            .id_salt(format!("{}_grid_scroll", id_salt))
            .show_rows(ui, cell_height, total_rows, |ui, row_range| {
                for row_idx in row_range {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        let start = row_idx * cols;
                        let end = (start + cols).min(filtered.len());
                        for i in start..end {
                            let (idx, item) = &filtered[i];
                            let is_highlighted = highlight.highlighted_ids.contains(&item.row_id);
                            let is_preview = highlight.preview_id == Some(item.row_id);
                            let selected = is_highlighted || is_preview;

                            let (rect, response) = ui.allocate_exact_size(
                                egui::vec2(actual_cell_width, cell_height),
                                egui::Sense::click(),
                            );

                            // 背景高亮
                            if selected || response.hovered() {
                                let bg_color = if selected {
                                    ui.visuals().selection.bg_fill
                                } else {
                                    ui.visuals().widgets.hovered.bg_fill
                                };
                                ui.painter().rect_filled(rect, 2.0, bg_color);
                            }

                            // 图标 (居中在上半部分)
                            let icon_top = rect.top() + cell_padding;
                            let icon_center_x = rect.center().x;
                            let icon_rect = egui::Rect::from_center_size(
                                egui::pos2(icon_center_x, icon_top + icon_size / 2.0),
                                egui::vec2(icon_size, icon_size),
                            );
                            if let Some(icon) =
                                item_list::get_or_load_icon(icon_cache, ctx, game, item.icon_id)
                            {
                                ui.painter().image(
                                    icon.id(),
                                    icon_rect,
                                    egui::Rect::from_min_max(
                                        egui::pos2(0.0, 0.0),
                                        egui::pos2(1.0, 1.0),
                                    ),
                                    egui::Color32::WHITE,
                                );
                            }

                            // 文字名称 (图标下方，居中，最多两行，裁剪)
                            let text_top = icon_top + icon_size + cell_padding;
                            let text_rect = egui::Rect::from_min_size(
                                egui::pos2(rect.left() + 2.0, text_top),
                                egui::vec2(
                                    actual_cell_width - 4.0,
                                    text_height * text_lines as f32,
                                ),
                            );
                            let text_color = if is_preview {
                                egui::Color32::from_rgb(100, 200, 255)
                            } else {
                                ui.visuals().text_color()
                            };
                            let clipped = ui.painter().with_clip_rect(rect);
                            clipped.text(
                                text_rect.center_top(),
                                egui::Align2::CENTER_TOP,
                                &item.name,
                                egui::FontId::proportional(11.0),
                                text_color,
                            );

                            // tooltip
                            response.clone().on_hover_text(&item.name);

                            if response.clicked() {
                                if let Some(slot) = item.equip_slot() {
                                    clicked = Some(ItemClicked {
                                        global_idx: *idx,
                                        item_id: item.row_id,
                                        slot,
                                    });
                                }
                            }
                        }
                    });
                }
            });

        clicked
    }
}
