use eframe::egui;

use crate::app::App;
use crate::domain::{ConsumableType, CONSUMABLE_TYPES};
use crate::loading::GameState;
use crate::ui::components::item_detail::{self, ItemDetailConfig};
use crate::ui::components::item_list::{self, DisplayItem, ItemListState};
use crate::domain::ViewMode;

/// 消耗品浏览器页面状态
pub struct ConsumablesState {
    pub list: ItemListState,
    pub selected_type: Option<ConsumableType>,
    pub selected_item: Option<usize>,
}

impl Default for ConsumablesState {
    fn default() -> Self {
        Self {
            list: ItemListState::new(ViewMode::List),
            selected_type: None,
            selected_item: None,
        }
    }
}

impl App {
    pub fn show_consumables_page(&mut self, ctx: &egui::Context, gs: &mut GameState) {
        // ── 左侧: 消耗品列表 ──
        egui::SidePanel::left("consumables_list")
            .default_width(320.0)
            .show(ctx, |ui| {
                ui.heading("消耗品");
                ui.separator();

                // 类型筛选按钮
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .selectable_label(
                            self.consumables.selected_type.is_none(),
                            "全部",
                        )
                        .clicked()
                    {
                        self.consumables.selected_type = None;
                    }
                    for ct in CONSUMABLE_TYPES {
                        let is_selected = self.consumables.selected_type == Some(ct);
                        if ui.selectable_label(is_selected, ct.display_name()).clicked() {
                            self.consumables.selected_type = Some(ct);
                        }
                    }
                });

                ui.separator();

                // 搜索框 + 视图模式
                self.consumables.list.show_controls(ui);

                ui.separator();

                let search_lower = self.consumables.list.search_lower();

                // 确定要显示的类型列表
                let types: Vec<ConsumableType> = if let Some(ct) = self.consumables.selected_type {
                    vec![ct]
                } else {
                    CONSUMABLE_TYPES.to_vec()
                };

                // 统计总数
                let total_count: usize = types
                    .iter()
                    .map(|ct| {
                        let type_idx = match ct {
                            ConsumableType::Food => 0,
                            ConsumableType::Medicine => 1,
                        };
                        gs.consumable_by_type[type_idx]
                            .iter()
                            .filter(|&&item_idx| {
                                if search_lower.is_empty() {
                                    return true;
                                }
                                gs.all_items[item_idx]
                                    .name
                                    .to_lowercase()
                                    .contains(&search_lower)
                            })
                            .count()
                    })
                    .sum();
                ui.label(format!("{} 件消耗品", total_count));
                ui.separator();

                // 按类型分组显示
                egui::ScrollArea::vertical()
                    .id_salt("consumables_scroll")
                    .show(ui, |ui| {
                        for ct in types {
                            let type_idx = match ct {
                                ConsumableType::Food => 0,
                                ConsumableType::Medicine => 1,
                            };
                            let entries: Vec<usize> = gs.consumable_by_type[type_idx]
                                .iter()
                                .filter(|&&item_idx| {
                                    if search_lower.is_empty() {
                                        return true;
                                    }
                                    gs.all_items[item_idx]
                                        .name
                                        .to_lowercase()
                                        .contains(&search_lower)
                                })
                                .copied()
                                .collect();

                            if entries.is_empty() {
                                continue;
                            }

                            let header = format!("{} ({})", ct.display_name(), entries.len());
                            let default_open = self.consumables.selected_type.is_some();
                            egui::CollapsingHeader::new(&header)
                                .id_salt(format!("consumable_group_{}", type_idx))
                                .default_open(default_open)
                                .show(ui, |ui| {
                                    self.show_consumable_item_list(ui, ctx, gs, &entries);
                                });
                        }
                    });
            });

        // ── 右侧: 选中物品详情 ──
        egui::SidePanel::right("consumables_detail")
            .default_width(300.0)
            .show(ctx, |ui| {
                self.show_consumable_detail_panel(ui, ctx, gs);
                // 占满面板剩余空间
                ui.allocate_space(ui.available_size());
            });

        // ── 中央: 占位（左侧列表已足够宽） ──
        egui::CentralPanel::default().show(ctx, |_ui| {
            // 中央区域留空，主要交互在左侧面板
        });
    }

    fn show_consumable_item_list(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        gs: &GameState,
        entries: &[usize],
    ) {
        match self.consumables.list.view_mode {
            ViewMode::List => {
                for &item_idx in entries {
                    let item = &gs.all_items[item_idx];
                    let is_selected = self.consumables.selected_item == Some(item_idx);
                    let di = DisplayItem {
                        id: item_idx,
                        name: &item.name,
                        icon_id: item.icon_id,
                        is_selected,
                    };
                    if item_list::show_list_row(
                        ui,
                        &di,
                        &item.name,
                        &mut self.icon_cache,
                        ctx,
                        &gs.game,
                    ) {
                        self.consumables.selected_item = Some(item_idx);
                    }
                }
            }
            ViewMode::Grid => {
                let display_items: Vec<DisplayItem<'_>> = entries
                    .iter()
                    .map(|&item_idx| {
                        let item = &gs.all_items[item_idx];
                        DisplayItem {
                            id: item_idx,
                            name: &item.name,
                            icon_id: item.icon_id,
                            is_selected: self.consumables.selected_item == Some(item_idx),
                        }
                    })
                    .collect();
                if let Some(clicked_idx) = item_list::show_grid(
                    ui,
                    &display_items,
                    self.consumables.list.icon_size,
                    &mut self.icon_cache,
                    ctx,
                    &gs.game,
                ) {
                    self.consumables.selected_item = Some(clicked_idx);
                }
            }
        }
    }

    fn show_consumable_detail_panel(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        gs: &GameState,
    ) {
        let Some(item_idx) = self.consumables.selected_item else {
            ui.centered_and_justified(|ui| {
                ui.label("← 从左侧列表选择一件消耗品");
            });
            return;
        };

        let Some(item) = gs.all_items.get(item_idx) else {
            return;
        };

        // 统一物品详情头部
        {
            let icon = self.get_or_load_icon(ctx, &gs.game, item.icon_id);
            let cat_name = gs
                .ui_category_names
                .get(&item.item_ui_category)
                .map(|s| s.as_str());
            item_detail::show_item_detail_header(
                ui,
                item,
                icon.as_ref(),
                cat_name,
                &ItemDetailConfig::compact(),
            );
        }

        ui.add_space(8.0);

        // 效果信息
        ui.label(egui::RichText::new("效果").strong());
        ui.separator();

        // 从 ItemFood 表获取效果
        if let Some(&food_id) = gs.item_to_food.get(&item.row_id) {
            if let Some(food_info) = gs.item_food.get(&food_id) {
                // 经验值加成
                if food_info.exp_bonus > 0 {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("经验值").strong());
                        ui.label(
                            egui::RichText::new(format!("+{}%", food_info.exp_bonus))
                                .small(),
                        );
                    });
                }

                for effect in &food_info.effects {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(&effect.param_name).strong());
                        let suffix = if effect.is_relative { "%" } else { "" };
                        let max_str = if effect.max_value > 0 {
                            format!(" (上限{})", effect.max_value)
                        } else {
                            String::new()
                        };
                        ui.label(
                            egui::RichText::new(format!(
                                "+{}{}{}",
                                effect.value, suffix, max_str
                            ))
                            .small(),
                        );
                    });
                    if effect.hq_value > 0 || effect.hq_max_value > 0 {
                        let suffix = if effect.is_relative { "%" } else { "" };
                        let max_str = if effect.hq_max_value > 0 {
                            format!(" (上限{})", effect.hq_max_value)
                        } else {
                            String::new()
                        };
                        ui.label(
                            egui::RichText::new(format!(
                                "  HQ: +{}{}{}",
                                effect.hq_value, suffix, max_str
                            ))
                            .small()
                            .weak(),
                        );
                    }
                }
            } else {
                ui.label(
                    egui::RichText::new(format!(
                        "ItemFood #{} 效果未找到",
                        food_id
                    ))
                    .small()
                    .weak(),
                );
            }
        } else {
            // 显示调试信息帮助诊断
            ui.label(egui::RichText::new("无效果数据").weak());
            if item.item_action > 0 {
                ui.label(
                    egui::RichText::new(format!(
                        "调试: ItemAction=#{}",
                        item.item_action
                    ))
                    .small()
                    .weak(),
                );
                if let Some(&(food_id, hq_food_id)) = gs.item_actions.get(&item.item_action) {
                    ui.label(
                        egui::RichText::new(format!(
                            "调试: ItemFood={} HQ={}",
                            food_id, hq_food_id
                        ))
                        .small()
                        .weak(),
                    );
                } else {
                    ui.label(
                        egui::RichText::new("调试: ItemAction 映射未找到")
                            .small()
                            .weak(),
                    );
                }
            } else {
                ui.label(
                    egui::RichText::new("调试: ItemAction=0 (列位置可能不对)")
                        .small()
                        .weak(),
                );
            }
        }

        ui.add_space(8.0);

        // 市场板信息
        if item.is_marketable() {
            ui.separator();
            ui.horizontal(|ui| {
                let universalis_url = format!("https://universalis.app/market/{}", item.row_id);
                if ui
                    .link(format!(
                        "{} Universalis",
                        egui_phosphor::regular::CHART_LINE_UP
                    ))
                    .clicked()
                {
                    let _ = open::that(&universalis_url);
                }
            });
        }
    }
}
