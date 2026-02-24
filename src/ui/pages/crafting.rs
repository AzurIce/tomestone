use std::collections::HashSet;

use eframe::egui;

use crate::app::App;
use crate::domain::{
    build_craft_tree, summarize_materials_with_collapsed, CraftTreeNode, ViewMode,
    CRAFT_TYPE_ABBRS, CRAFT_TYPE_NAMES,
};
use crate::loading::GameState;

impl App {
    pub fn show_crafting_page(&mut self, ctx: &egui::Context, gs: &mut GameState) {
        // ── 左侧: 可制作物品列表 ──
        egui::SidePanel::left("crafting_list")
            .default_width(300.0)
            .show(ctx, |ui| {
                ui.heading("合成检索");
                ui.separator();

                // 职业筛选按钮
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .selectable_label(self.crafting_selected_craft_type.is_none(), "全部")
                        .clicked()
                    {
                        self.crafting_selected_craft_type = None;
                    }
                    for ct in 0u8..8 {
                        if ui
                            .selectable_label(
                                self.crafting_selected_craft_type == Some(ct),
                                CRAFT_TYPE_ABBRS[ct as usize],
                            )
                            .clicked()
                        {
                            self.crafting_selected_craft_type = Some(ct);
                        }
                    }
                });

                ui.separator();

                // 搜索框
                ui.horizontal(|ui| {
                    ui.label("搜索:");
                    ui.text_edit_singleline(&mut self.crafting_search);
                });

                // 视图模式
                ui.horizontal(|ui| {
                    ui.label("视图:");
                    if ui
                        .selectable_label(
                            self.crafting_view_mode == ViewMode::List,
                            ViewMode::List.label(),
                        )
                        .clicked()
                    {
                        self.crafting_view_mode = ViewMode::List;
                    }
                    if ui
                        .selectable_label(
                            self.crafting_view_mode == ViewMode::Grid,
                            ViewMode::Grid.label(),
                        )
                        .clicked()
                    {
                        self.crafting_view_mode = ViewMode::Grid;
                    }
                });

                if self.crafting_view_mode == ViewMode::Grid {
                    ui.horizontal(|ui| {
                        ui.label("图标:");
                        ui.add(
                            egui::Slider::new(&mut self.crafting_icon_size, 32.0..=128.0)
                                .suffix("px"),
                        );
                    });
                }

                ui.separator();

                let search_lower = self.crafting_search.to_lowercase();

                // 确定要显示的职业列表
                let craft_types: Vec<u8> = if let Some(ct) = self.crafting_selected_craft_type {
                    vec![ct]
                } else {
                    (0u8..8).collect()
                };

                // 统计总数
                let total_count: usize = craft_types
                    .iter()
                    .map(|&ct| {
                        gs.craftable_by_type[ct as usize]
                            .iter()
                            .filter(|&&(item_idx, _)| {
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
                ui.label(format!("{} 件可制作物品", total_count));
                ui.separator();

                // 按职业分组显示
                egui::ScrollArea::vertical()
                    .id_salt("crafting_item_scroll")
                    .show(ui, |ui| {
                        for &ct in &craft_types {
                            let entries: Vec<(usize, usize)> = gs.craftable_by_type[ct as usize]
                                .iter()
                                .filter(|&&(item_idx, _)| {
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

                            let header =
                                format!("{} ({})", CRAFT_TYPE_NAMES[ct as usize], entries.len());
                            let default_open = self.crafting_selected_craft_type.is_some();
                            egui::CollapsingHeader::new(&header)
                                .id_salt(format!("craft_group_{}", ct))
                                .default_open(default_open)
                                .show(ui, |ui| {
                                    self.show_crafting_item_list(ui, ctx, gs, &entries);
                                });
                        }
                    });
            });

        // ── 右侧: 选中节点信息 + 素材汇总 ──
        egui::SidePanel::right("crafting_info")
            .default_width(260.0)
            .show(ctx, |ui| {
                self.show_crafting_info_panel(ui, ctx, gs);
            });

        // ── 中央: 合成树 ──
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(item_idx) = self.crafting_selected_item {
                if let Some(item) = gs.all_items.get(item_idx) {
                    ui.horizontal(|ui| {
                        if let Some(icon) = self.get_or_load_icon(ctx, &gs.game, item.icon_id) {
                            ui.image(egui::load::SizedTexture::new(
                                icon.id(),
                                egui::vec2(32.0, 32.0),
                            ));
                        }
                        ui.heading(&item.name);
                    });
                    ui.separator();

                    // 构建合成树
                    let mut visited = HashSet::new();
                    let tree = build_craft_tree(
                        item.row_id,
                        1,
                        &gs.recipes,
                        &gs.item_to_recipes,
                        &mut visited,
                    );

                    egui::ScrollArea::vertical()
                        .id_salt("craft_tree_scroll")
                        .show(ui, |ui| {
                            self.show_craft_tree_node(ui, ctx, gs, &tree, 0);
                        });
                }
            } else {
                ui.centered_and_justified(|ui| {
                    ui.label("← 从左侧列表选择一件可制作物品");
                });
            }
        });
    }

    fn show_crafting_item_list(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        gs: &GameState,
        entries: &[(usize, usize)],
    ) {
        match self.crafting_view_mode {
            ViewMode::List => {
                for &(item_idx, _recipe_idx) in entries {
                    let item = &gs.all_items[item_idx];
                    let is_selected = self.crafting_selected_item == Some(item_idx);
                    let response = ui.horizontal(|ui| {
                        if let Some(icon) = self.get_or_load_icon(ctx, &gs.game, item.icon_id) {
                            ui.image(egui::load::SizedTexture::new(
                                icon.id(),
                                egui::vec2(20.0, 20.0),
                            ));
                        } else {
                            ui.allocate_space(egui::vec2(20.0, 20.0));
                        }
                        ui.selectable_label(is_selected, &item.name)
                    });
                    if response.inner.clicked() {
                        self.crafting_selected_item = Some(item_idx);
                        self.crafting_selected_node_item = None;
                    }
                }
            }
            ViewMode::Grid => {
                let available_width = ui.available_width();
                let icon_size = self.crafting_icon_size;
                let cell_padding = 4.0;
                let text_height = 14.0;
                let cell_width = (icon_size + cell_padding * 2.0).min(available_width);
                let cell_height = icon_size + cell_padding * 2.0 + text_height * 2.0;
                let cols = ((available_width / cell_width).floor() as usize).max(1);
                let actual_cell_width = available_width / cols as f32;
                let total_rows = (entries.len() + cols - 1) / cols;

                for row_idx in 0..total_rows {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        let start = row_idx * cols;
                        let end = (start + cols).min(entries.len());
                        for i in start..end {
                            let (item_idx, _) = entries[i];
                            let item = &gs.all_items[item_idx];
                            let is_selected = self.crafting_selected_item == Some(item_idx);

                            let (rect, response) = ui.allocate_exact_size(
                                egui::vec2(actual_cell_width, cell_height),
                                egui::Sense::click(),
                            );

                            if is_selected || response.hovered() {
                                let bg = if is_selected {
                                    ui.visuals().selection.bg_fill
                                } else {
                                    ui.visuals().widgets.hovered.bg_fill
                                };
                                ui.painter().rect_filled(rect, 2.0, bg);
                            }

                            let icon_top = rect.top() + cell_padding;
                            let icon_center_x = rect.center().x;
                            let icon_rect = egui::Rect::from_center_size(
                                egui::pos2(icon_center_x, icon_top + icon_size / 2.0),
                                egui::vec2(icon_size, icon_size),
                            );
                            if let Some(icon) = self.get_or_load_icon(ctx, &gs.game, item.icon_id) {
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

                            let text_top = icon_top + icon_size + cell_padding;
                            let text_color = ui.visuals().text_color();
                            let clipped = ui.painter().with_clip_rect(rect);
                            clipped.text(
                                egui::pos2(rect.center().x, text_top),
                                egui::Align2::CENTER_TOP,
                                &item.name,
                                egui::FontId::proportional(11.0),
                                text_color,
                            );

                            response.clone().on_hover_text(&item.name);

                            if response.clicked() {
                                self.crafting_selected_item = Some(item_idx);
                                self.crafting_selected_node_item = None;
                            }
                        }
                    });
                }
            }
        }
    }

    /// 递归渲染合成树节点
    fn show_craft_tree_node(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        gs: &GameState,
        node: &CraftTreeNode,
        depth: usize,
    ) {
        let item_name = gs
            .item_id_map
            .get(&node.item_id)
            .and_then(|&idx| gs.all_items.get(idx))
            .map(|item| item.name.as_str())
            .unwrap_or("???");

        let icon_id = gs
            .item_id_map
            .get(&node.item_id)
            .and_then(|&idx| gs.all_items.get(idx))
            .map(|item| item.icon_id)
            .unwrap_or(0);

        let is_selected = self.crafting_selected_node_item == Some(node.item_id);

        if node.children.is_empty() {
            // 叶子节点: 原始素材
            let response = ui.horizontal(|ui| {
                // 缩进对齐 (三角形占位)
                ui.allocate_space(egui::vec2(14.0, 14.0));
                if let Some(icon) = self.get_or_load_icon(ctx, &gs.game, icon_id) {
                    ui.image(egui::load::SizedTexture::new(
                        icon.id(),
                        egui::vec2(18.0, 18.0),
                    ));
                } else {
                    ui.allocate_space(egui::vec2(18.0, 18.0));
                }
                let label = format!("{} x{}", item_name, node.amount_needed);
                ui.selectable_label(is_selected, label)
            });
            if response.inner.clicked() {
                self.crafting_selected_node_item = Some(node.item_id);
                self.crafting_selected_node_amount = node.amount_needed;
            }
        } else {
            // 可制作的中间素材: 手动绘制 toggle + 图标 + 可选中标签
            let craft_info = node.recipe_idx.map(|idx| &gs.recipes[idx]);
            let job_name = craft_info
                .map(|r| CRAFT_TYPE_ABBRS[r.craft_type.min(7) as usize])
                .unwrap_or("");

            let state_id = egui::Id::new(("craft_tree", node.item_id, depth));
            let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
                ui.ctx(),
                state_id,
                true,
            );

            // 绘制 header 行: 三角形 + 图标 + 可选中标签
            let header_response = ui.horizontal(|ui| {
                // 三角形 toggle
                let (triangle_rect, triangle_resp) =
                    ui.allocate_exact_size(egui::vec2(14.0, 18.0), egui::Sense::click());
                if triangle_resp.clicked() {
                    state.toggle(ui);
                }
                let openness = state.openness(ui.ctx());
                // 绘制三角形
                let center = triangle_rect.center();
                let half = 4.0;
                let color = ui.visuals().text_color();
                if openness < 0.5 {
                    // ▶ 折叠
                    let points = vec![
                        egui::pos2(center.x - 2.0, center.y - half),
                        egui::pos2(center.x - 2.0, center.y + half),
                        egui::pos2(center.x + 3.0, center.y),
                    ];
                    ui.painter().add(egui::Shape::convex_polygon(
                        points,
                        color,
                        egui::Stroke::NONE,
                    ));
                } else {
                    // ▼ 展开
                    let points = vec![
                        egui::pos2(center.x - half, center.y - 2.0),
                        egui::pos2(center.x + half, center.y - 2.0),
                        egui::pos2(center.x, center.y + 3.0),
                    ];
                    ui.painter().add(egui::Shape::convex_polygon(
                        points,
                        color,
                        egui::Stroke::NONE,
                    ));
                }

                // 图标
                if let Some(icon) = self.get_or_load_icon(ctx, &gs.game, icon_id) {
                    ui.image(egui::load::SizedTexture::new(
                        icon.id(),
                        egui::vec2(18.0, 18.0),
                    ));
                } else {
                    ui.allocate_space(egui::vec2(18.0, 18.0));
                }

                // 可选中标签
                let label_text = format!("{} x{} [{}]", item_name, node.amount_needed, job_name);
                let rt = egui::RichText::new(&label_text).strong();
                ui.selectable_label(is_selected, rt)
            });

            if header_response.inner.clicked() {
                self.crafting_selected_node_item = Some(node.item_id);
                self.crafting_selected_node_amount = node.amount_needed;
            }

            // 子节点
            state.show_body_indented(&header_response.response, ui, |ui| {
                for child in &node.children {
                    self.show_craft_tree_node(ui, ctx, gs, child, depth + 1);
                }
            });

            state.store(ui.ctx());
        }
    }

    /// 收集合成树中被折叠的节点 (item_id, depth)
    fn collect_collapsed_nodes(
        &self,
        ctx: &egui::Context,
        node: &CraftTreeNode,
        depth: usize,
        collapsed: &mut HashSet<(u32, usize)>,
    ) {
        if node.children.is_empty() {
            return;
        }
        let state_id = egui::Id::new(("craft_tree", node.item_id, depth));
        let state =
            egui::collapsing_header::CollapsingState::load_with_default_open(ctx, state_id, true);
        if !state.is_open() {
            collapsed.insert((node.item_id, depth));
        } else {
            for child in &node.children {
                self.collect_collapsed_nodes(ctx, child, depth + 1, collapsed);
            }
        }
    }

    /// 右侧信息面板: 选中节点信息 + 素材汇总
    fn show_crafting_info_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, gs: &GameState) {
        // 上半: 选中节点物品信息
        if let Some(node_item_id) = self.crafting_selected_node_item {
            if let Some(&idx) = gs.item_id_map.get(&node_item_id) {
                if let Some(item) = gs.all_items.get(idx) {
                    ui.horizontal(|ui| {
                        if let Some(icon) = self.get_or_load_icon(ctx, &gs.game, item.icon_id) {
                            ui.image(egui::load::SizedTexture::new(
                                icon.id(),
                                egui::vec2(32.0, 32.0),
                            ));
                        }
                        ui.label(egui::RichText::new(&item.name).strong().size(14.0));
                    });

                    egui::Grid::new("node_item_info").show(ui, |ui| {
                        // 需求数量
                        ui.label("需求数量:");
                        ui.label(format!("{}", self.crafting_selected_node_amount));
                        ui.end_row();

                        // 如果此物品有配方，显示配方信息
                        if let Some(recipe_indices) = gs.item_to_recipes.get(&node_item_id) {
                            if let Some(&recipe_idx) = recipe_indices.first() {
                                let recipe = &gs.recipes[recipe_idx];
                                ui.label("制作职业:");
                                ui.label(CRAFT_TYPE_NAMES[recipe.craft_type.min(7) as usize]);
                                ui.end_row();
                                ui.label("配方等级:");
                                ui.label(format!("{}", recipe.recipe_level));
                                ui.end_row();
                                ui.label("单次产出:");
                                ui.label(format!("{}", recipe.result_amount));
                                ui.end_row();
                                // 制作次数
                                let craft_count = (self.crafting_selected_node_amount as f64
                                    / recipe.result_amount.max(1) as f64)
                                    .ceil()
                                    as u32;
                                ui.label("制作次数:");
                                ui.label(format!("{}", craft_count));
                                ui.end_row();
                            }
                        }
                    });
                    ui.separator();
                }
            }
        } else {
            ui.label("点击合成树中的节点查看详情");
            ui.separator();
        }

        // 下半: 原始素材汇总 (感知折叠状态)
        ui.label(egui::RichText::new("原始素材汇总").strong());
        ui.add_space(2.0);
        ui.label(egui::RichText::new("折叠节点视为原始素材").small().weak());
        ui.separator();

        if let Some(item_idx) = self.crafting_selected_item {
            if let Some(item) = gs.all_items.get(item_idx) {
                let mut visited = HashSet::new();
                let tree = build_craft_tree(
                    item.row_id,
                    1,
                    &gs.recipes,
                    &gs.item_to_recipes,
                    &mut visited,
                );

                // 收集折叠状态
                let mut collapsed = HashSet::new();
                self.collect_collapsed_nodes(ctx, &tree, 0, &mut collapsed);

                let materials = summarize_materials_with_collapsed(&tree, &collapsed);

                if materials.is_empty() {
                    ui.label("无原始素材");
                } else {
                    egui::ScrollArea::vertical()
                        .id_salt("material_summary_scroll")
                        .show(ui, |ui| {
                            for &(mat_id, amount) in &materials {
                                let (mat_name, mat_icon) = gs
                                    .item_id_map
                                    .get(&mat_id)
                                    .and_then(|&idx| gs.all_items.get(idx))
                                    .map(|i| (i.name.as_str(), i.icon_id))
                                    .unwrap_or(("???", 0));

                                ui.horizontal(|ui| {
                                    if let Some(icon) =
                                        self.get_or_load_icon(ctx, &gs.game, mat_icon)
                                    {
                                        ui.image(egui::load::SizedTexture::new(
                                            icon.id(),
                                            egui::vec2(18.0, 18.0),
                                        ));
                                    } else {
                                        ui.allocate_space(egui::vec2(18.0, 18.0));
                                    }
                                    ui.label(format!("{} x{}", mat_name, amount));
                                });
                            }
                        });
                }
            }
        } else {
            ui.label("选择物品后显示素材汇总");
        }
    }
}
