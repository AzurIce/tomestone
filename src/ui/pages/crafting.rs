use std::collections::{BTreeMap, HashSet};

use eframe::egui;

use crate::app::App;
use crate::domain::{
    build_craft_tree, resolve_source, summarize_materials_with_collapsed, total_amount_in_tree,
    CraftTreeNode, ItemSource, Recipe, SourceChoice, ViewMode, CRAFT_TYPE_ABBRS, CRAFT_TYPE_NAMES,
};
use crate::loading::GameState;
use crate::ui::components::item_detail::{self, ItemDetailConfig};
use crate::ui::components::item_list::{self, DisplayItem};

/// 根据解析后的来源返回淡色背景
fn source_bg_color(source: Option<&ItemSource>, visuals: &egui::Visuals) -> Option<egui::Color32> {
    let tag = source?.color_tag();
    let alpha = if visuals.dark_mode { 25 } else { 40 };
    match tag {
        1 => Some(egui::Color32::from_rgba_unmultiplied(255, 200, 60, alpha)), // 金币商店: 淡金
        2 => Some(egui::Color32::from_rgba_unmultiplied(180, 130, 255, alpha)), // 兑换: 淡紫
        3 => Some(egui::Color32::from_rgba_unmultiplied(80, 200, 80, alpha)),  // 采集: 淡绿
        _ => None,
    }
}

/// 来源简短标签
fn source_tag_text(source: Option<&ItemSource>) -> &'static str {
    match source {
        Some(ItemSource::GilShop { .. }) => "商",
        Some(ItemSource::SpecialShop { .. }) => "换",
        Some(ItemSource::Gathering) => "采",
        None => "",
    }
}

/// 获取配方的实际等级 (从 RecipeLevelTable 查询)
fn get_recipe_level(recipe: &Recipe, gs: &GameState) -> u8 {
    gs.recipe_levels
        .get(&recipe.recipe_level_table_id)
        .copied()
        .unwrap_or(1)
}

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

                // 搜索框 + 视图模式 + 图标大小
                self.crafting_list.show_controls(ui);

                ui.separator();

                let search_lower = self.crafting_list.search_lower();

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

        // ── 右侧: 选中节点详情 ──
        egui::SidePanel::right("crafting_info")
            .default_width(260.0)
            .show(ctx, |ui| {
                self.show_crafting_detail_panel(ui, ctx, gs);
                // 占满面板剩余空间，防止面板根据内容收缩
                ui.allocate_space(ui.available_size());
            });

        // ── 中央: 图标+名称 + 两列(合成树 | 材料统计) ──
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(item_idx) = self.crafting_selected_item {
                if let Some(item) = gs.all_items.get(item_idx) {
                    // 顶部: 图标 + 名称 + 配方来源
                    ui.horizontal(|ui| {
                        if let Some(icon) = self.get_or_load_icon(ctx, &gs.game, item.icon_id) {
                            ui.image(egui::load::SizedTexture::new(
                                icon.id(),
                                egui::vec2(32.0, 32.0),
                            ));
                        }
                        ui.heading(&item.name);

                        // 显示配方来源 (如果有)
                        if let Some(recipe_indices) = gs.item_to_recipes.get(&item.row_id) {
                            if let Some(&recipe_idx) = recipe_indices.first() {
                                let recipe = &gs.recipes[recipe_idx];
                                let job_abbr = CRAFT_TYPE_ABBRS[recipe.craft_type.min(7) as usize];
                                let level = get_recipe_level(recipe, gs);

                                // 只有当 SecretRecipeBook > 0 且能找到名称时才显示秘籍
                                if recipe.secret_recipe_book > 0 {
                                    if let Some(name) = gs.secret_recipe_book_names.get(&recipe.secret_recipe_book) {
                                        ui.label(
                                            egui::RichText::new(format!("[{}] <{}>", job_abbr, name))
                                                .small()
                                                .color(egui::Color32::from_rgb(200, 150, 255)),
                                        );
                                    } else {
                                        // 有 SecretRecipeBook 值但找不到名称
                                        ui.label(
                                            egui::RichText::new(format!("[{}] <秘籍>", job_abbr))
                                                .small()
                                                .color(egui::Color32::from_rgb(200, 150, 255)),
                                        );
                                    }
                                } else {
                                    // 普通配方，显示等级
                                    ui.label(
                                        egui::RichText::new(format!("[{}] Lv.{}", job_abbr, level))
                                            .small()
                                            .weak(),
                                    );
                                }
                            }
                        }
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

                    // 收集折叠状态
                    let mut collapsed = HashSet::new();
                    self.collect_collapsed_nodes(ctx, &tree, 0, &mut collapsed);

                    // 两列布局 (可拖拽调整宽度)
                    // 右子面板: 材料统计
                    egui::SidePanel::right("crafting_material_panel")
                        .default_width(280.0)
                        .resizable(true)
                        .show_inside(ui, |ui| {
                            ui.label(egui::RichText::new("原始素材汇总").strong());
                            ui.add_space(2.0);
                            ui.label(egui::RichText::new("折叠节点视为原始素材").small().weak());
                            ui.separator();
                            self.show_material_summary(ui, ctx, gs, &tree, &collapsed);
                            // 占满面板剩余空间，防止面板根据内容收缩
                            ui.allocate_space(ui.available_size());
                        });

                    // 左侧剩余: 合成树
                    egui::CentralPanel::default().show_inside(ui, |ui| {
                        ui.label(egui::RichText::new("合成树").strong());
                        ui.separator();
                        egui::ScrollArea::vertical()
                            .id_salt("craft_tree_scroll")
                            .show(ui, |ui| {
                                self.show_craft_tree_node(ui, ctx, gs, &tree, 0);
                            });
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
        match self.crafting_list.view_mode {
            ViewMode::List => {
                for &(item_idx, _recipe_idx) in entries {
                    let item = &gs.all_items[item_idx];
                    let is_selected = self.crafting_selected_item == Some(item_idx);
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
                        self.crafting_selected_item = Some(item_idx);
                        self.crafting_selected_node_item = None;
                        self.crafting_source_overrides.clear();
                    }
                }
            }
            ViewMode::Grid => {
                let display_items: Vec<DisplayItem<'_>> = entries
                    .iter()
                    .map(|&(item_idx, _)| {
                        let item = &gs.all_items[item_idx];
                        DisplayItem {
                            id: item_idx,
                            name: &item.name,
                            icon_id: item.icon_id,
                            is_selected: self.crafting_selected_item == Some(item_idx),
                        }
                    })
                    .collect();
                if let Some(clicked_idx) = item_list::show_grid(
                    ui,
                    &display_items,
                    self.crafting_list.icon_size,
                    &mut self.icon_cache,
                    ctx,
                    &gs.game,
                ) {
                    self.crafting_selected_item = Some(clicked_idx);
                    self.crafting_selected_node_item = None;
                    self.crafting_source_overrides.clear();
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
            let sources = gs
                .item_sources
                .get(&node.item_id)
                .map(|v| v.as_slice())
                .unwrap_or(&[]);
            let resolved = resolve_source(node.item_id, sources, &self.crafting_source_overrides);
            let bg = source_bg_color(resolved, ui.visuals());
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
                // 来源标签
                let tag = source_tag_text(resolved);
                let label = if tag.is_empty() {
                    format!("{} x{}", item_name, node.amount_needed)
                } else {
                    format!("{} x{} [{}]", item_name, node.amount_needed, tag)
                };
                ui.selectable_label(is_selected, label)
            });
            // 绘制来源背景色
            if let Some(color) = bg {
                ui.painter().rect_filled(response.response.rect, 2.0, color);
            }
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

            // 构建配方来源文本
            let source_text = if let Some(recipe) = craft_info {
                let level = get_recipe_level(recipe, gs);

                // 只有当 secret_recipe_book > 0 且在表中找到名称时才显示秘籍名
                if recipe.secret_recipe_book > 0 {
                    if let Some(name) = gs.secret_recipe_book_names.get(&recipe.secret_recipe_book) {
                        name.clone()
                    } else {
                        // 表中没有对应名称，只显示等级
                        format!("Lv.{}", level)
                    }
                } else {
                    format!("Lv.{}", level)
                }
            } else {
                String::new()
            };

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

                // 可选中标签: 物品名 x数量 [职业] <来源>
                let label_text = if source_text.is_empty() {
                    format!("{} x{} [{}]", item_name, node.amount_needed, job_name)
                } else {
                    format!(
                        "{} x{} [{}] <{}>",
                        item_name, node.amount_needed, job_name, source_text
                    )
                };
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

    /// 右侧详情面板: 选中节点物品信息
    fn show_crafting_detail_panel(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        gs: &GameState,
    ) {
        let Some(node_item_id) = self.crafting_selected_node_item else {
            ui.label("点击合成树中的节点查看详情");
            return;
        };
        let Some(&idx) = gs.item_id_map.get(&node_item_id) else {
            return;
        };
        let Some(item) = gs.all_items.get(idx) else {
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

        ui.add_space(4.0);

        // 计算整棵树中该物品的总需求量
        let total_need = self
            .crafting_selected_item
            .and_then(|item_idx| {
                let root_item = gs.all_items.get(item_idx)?;
                let mut visited = HashSet::new();
                let tree = build_craft_tree(
                    root_item.row_id,
                    1,
                    &gs.recipes,
                    &gs.item_to_recipes,
                    &mut visited,
                );
                let mut collapsed = HashSet::new();
                self.collect_collapsed_nodes(ctx, &tree, 0, &mut collapsed);
                Some(total_amount_in_tree(&tree, node_item_id, 0, &collapsed))
            })
            .unwrap_or(self.crafting_selected_node_amount);

        egui::Grid::new("node_item_info").show(ui, |ui| {
            ui.label("需求数量:");
            ui.label(format!("{}", total_need));
            ui.end_row();

            // 收购价格 (NPC 回收价)
            if item.price_low > 0 {
                ui.label("收购价格:");
                ui.label(format!("{} Gil", item.price_low));
                ui.end_row();
            }

            if let Some(recipe_indices) = gs.item_to_recipes.get(&node_item_id) {
                if let Some(&recipe_idx) = recipe_indices.first() {
                    let recipe = &gs.recipes[recipe_idx];
                    ui.label("制作职业:");
                    ui.label(CRAFT_TYPE_NAMES[recipe.craft_type.min(7) as usize]);
                    ui.end_row();
                    ui.label("配方等级:");
                    let level = get_recipe_level(recipe, gs);
                    ui.label(format!("{}", level));
                    ui.end_row();
                    // 显示配方来源
                    if recipe.secret_recipe_book > 0 {
                        let book_name = gs
                            .secret_recipe_book_names
                            .get(&recipe.secret_recipe_book)
                            .map(|s| s.as_str())
                            .unwrap_or("秘籍");
                        ui.label("配方来源:");
                        ui.label(egui::RichText::new(book_name).color(egui::Color32::from_rgb(200, 150, 255)));
                        ui.end_row();
                    }
                    ui.label("单次产出:");
                    ui.label(format!("{}", recipe.result_amount));
                    ui.end_row();
                    let craft_count =
                        (total_need as f64 / recipe.result_amount.max(1) as f64).ceil() as u32;
                    ui.label("制作次数:");
                    ui.label(format!("{}", craft_count));
                    ui.end_row();
                }
            }
        });

        // 来源信息
        if let Some(sources) = gs.item_sources.get(&node_item_id) {
            if !sources.is_empty() {
                ui.add_space(4.0);
                // 当前选中的来源索引
                let resolved =
                    resolve_source(node_item_id, sources, &self.crafting_source_overrides);
                let is_ignored = matches!(
                    self.crafting_source_overrides.get(&node_item_id),
                    Some(SourceChoice::Ignore)
                );

                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("获取来源").strong());
                    if is_ignored {
                        ui.label(egui::RichText::new("(已忽略)").small().weak());
                    }
                });

                for (_i, source) in sources.iter().enumerate() {
                    // 判断是否为当前选中的来源
                    let is_active =
                        !is_ignored && resolved.map(|r| std::ptr::eq(r, source)).unwrap_or(false);
                    let alpha = if is_active { 255 } else { 120 };

                    match source {
                        ItemSource::GilShop {
                            shop_name,
                            npc_location,
                        } => {
                            ui.horizontal(|ui| {
                                let color =
                                    egui::Color32::from_rgba_unmultiplied(220, 180, 40, alpha);
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{} 金币",
                                        egui_phosphor::regular::COINS
                                    ))
                                    .color(color)
                                    .strong(),
                                );
                                let text = format!("{} ({}G)", shop_name, item.price_mid);
                                if is_active {
                                    ui.label(text);
                                } else {
                                    ui.label(egui::RichText::new(text).weak());
                                }
                            });
                            if let Some(loc) = npc_location {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "    {} {}",
                                        egui_phosphor::regular::MAP_PIN,
                                        loc
                                    ))
                                    .small()
                                    .weak(),
                                );
                            }
                        }
                        ItemSource::SpecialShop {
                            shop_name,
                            cost_item_id,
                            cost_count,
                        } => {
                            let cost_name = gs
                                .item_id_map
                                .get(cost_item_id)
                                .and_then(|&i| gs.all_items.get(i))
                                .map(|i| i.name.as_str())
                                .unwrap_or("???");
                            ui.horizontal(|ui| {
                                let color =
                                    egui::Color32::from_rgba_unmultiplied(160, 120, 230, alpha);
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{} 兑换",
                                        egui_phosphor::regular::SWAP
                                    ))
                                    .color(color)
                                    .strong(),
                                );
                                let text = format!("{} ({} x{})", shop_name, cost_name, cost_count);
                                if is_active {
                                    ui.label(text);
                                } else {
                                    ui.label(egui::RichText::new(text).weak());
                                }
                            });
                        }
                        ItemSource::Gathering => {
                            ui.horizontal(|ui| {
                                let color =
                                    egui::Color32::from_rgba_unmultiplied(80, 180, 80, alpha);
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{} 采集",
                                        egui_phosphor::regular::LEAF
                                    ))
                                    .color(color)
                                    .strong(),
                                );
                                if is_active {
                                    ui.label("采矿/园艺");
                                } else {
                                    ui.label(egui::RichText::new("采矿/园艺").weak());
                                }
                            });
                        }
                    }
                }
            }
        }
    }

    /// 材料统计面板 (中间右列)
    fn show_material_summary(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        gs: &GameState,
        tree: &CraftTreeNode,
        collapsed: &HashSet<(u32, usize)>,
    ) {
        let materials = summarize_materials_with_collapsed(tree, collapsed);

        if materials.is_empty() {
            ui.label("无原始素材");
            return;
        }

        // 计算汇总费用 (基于用户选择的来源)
        let mut total_gil: u64 = 0;
        let mut token_costs: BTreeMap<u32, u64> = BTreeMap::new();
        let mut gathering_count = 0u32;
        let mut other_count = 0u32;
        let mut ignored_count = 0u32;

        for &(mat_id, amount) in &materials {
            let sources = gs
                .item_sources
                .get(&mat_id)
                .map(|v| v.as_slice())
                .unwrap_or(&[]);
            let item = gs
                .item_id_map
                .get(&mat_id)
                .and_then(|&i| gs.all_items.get(i));

            if matches!(
                self.crafting_source_overrides.get(&mat_id),
                Some(SourceChoice::Ignore)
            ) {
                ignored_count += 1;
                continue;
            }

            let resolved = resolve_source(mat_id, sources, &self.crafting_source_overrides);
            match resolved {
                Some(ItemSource::GilShop { .. }) => {
                    let price = item.map(|i| i.price_mid).unwrap_or(0);
                    total_gil += price as u64 * amount as u64;
                }
                Some(ItemSource::SpecialShop {
                    cost_item_id,
                    cost_count,
                    ..
                }) => {
                    *token_costs.entry(*cost_item_id).or_insert(0) +=
                        *cost_count as u64 * amount as u64;
                }
                Some(ItemSource::Gathering) => {
                    gathering_count += 1;
                }
                None => {
                    other_count += 1;
                }
            }
        }

        // ── 总计区 ──
        if total_gil > 0 {
            ui.label(
                egui::RichText::new(format!("{} {}G", egui_phosphor::regular::COINS, total_gil))
                    .strong(),
            );
        }
        for (&token_id, &count) in &token_costs {
            let token_name = gs
                .item_id_map
                .get(&token_id)
                .and_then(|&i| gs.all_items.get(i))
                .map(|i| i.name.as_str())
                .unwrap_or("???");
            ui.label(
                egui::RichText::new(format!(
                    "{} {} x{}",
                    egui_phosphor::regular::SWAP,
                    token_name,
                    count
                ))
                .strong(),
            );
        }
        if gathering_count > 0 {
            ui.label(
                egui::RichText::new(format!(
                    "{} 采集 {}种",
                    egui_phosphor::regular::LEAF,
                    gathering_count
                ))
                .small(),
            );
        }
        if other_count > 0 {
            ui.label(egui::RichText::new(format!("其他 {}种", other_count)).small());
        }
        if ignored_count > 0 {
            ui.label(
                egui::RichText::new(format!(
                    "{} 已持有 {}种",
                    egui_phosphor::regular::CHECK_CIRCLE,
                    ignored_count
                ))
                .small()
                .weak(),
            );
        }
        ui.separator();

        // ── 素材列表 ──
        egui::ScrollArea::vertical()
            .id_salt("material_summary_scroll")
            .show(ui, |ui| {
                for &(mat_id, amount) in &materials {
                    let (mat_name, mat_icon, mat_price) = gs
                        .item_id_map
                        .get(&mat_id)
                        .and_then(|&idx| gs.all_items.get(idx))
                        .map(|i| (i.name.as_str(), i.icon_id, i.price_mid))
                        .unwrap_or(("???", 0, 0));

                    let sources = gs
                        .item_sources
                        .get(&mat_id)
                        .map(|v| v.as_slice())
                        .unwrap_or(&[]);
                    let is_ignored = matches!(
                        self.crafting_source_overrides.get(&mat_id),
                        Some(SourceChoice::Ignore)
                    );
                    let is_selected = self.crafting_selected_node_item == Some(mat_id);

                    // 当前选中的来源索引
                    let current_choice = self.crafting_source_overrides.get(&mat_id).copied();
                    let active_idx = match current_choice {
                        Some(SourceChoice::Index(i)) => Some(i),
                        Some(SourceChoice::Ignore) => None,
                        None => crate::domain::default_source_index(sources),
                    };

                    // 背景色
                    let resolved = active_idx.and_then(|i| sources.get(i));
                    let bg = if is_ignored {
                        None
                    } else {
                        source_bg_color(resolved, ui.visuals())
                    };

                    let resp = ui.horizontal(|ui| {
                        // 图标
                        if let Some(icon) = self.get_or_load_icon(ctx, &gs.game, mat_icon) {
                            ui.image(egui::load::SizedTexture::new(
                                icon.id(),
                                egui::vec2(18.0, 18.0),
                            ));
                        } else {
                            ui.allocate_space(egui::vec2(18.0, 18.0));
                        }

                        // 名称 + 数量 (可点击选中)
                        let name_text = format!("{} x{}", mat_name, amount);
                        let rt = if is_ignored {
                            egui::RichText::new(&name_text).strikethrough().weak()
                        } else {
                            egui::RichText::new(&name_text)
                        };
                        if ui.selectable_label(is_selected, rt).clicked() {
                            self.crafting_selected_node_item = Some(mat_id);
                            self.crafting_selected_node_amount = amount;
                        }

                        // 来源选择按钮 (右对齐)
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            // "已持有" 按钮
                            let own_label = if is_ignored {
                                egui::RichText::new(egui_phosphor::regular::CHECK_CIRCLE).strong()
                            } else {
                                egui::RichText::new(egui_phosphor::regular::CHECK_CIRCLE).weak()
                            };
                            if ui
                                .selectable_label(is_ignored, own_label)
                                .on_hover_text("已持有/忽略")
                                .clicked()
                            {
                                if is_ignored {
                                    // 取消忽略 → 恢复默认
                                    self.crafting_source_overrides.remove(&mat_id);
                                } else {
                                    self.crafting_source_overrides
                                        .insert(mat_id, SourceChoice::Ignore);
                                }
                            }

                            // 各来源按钮 (反向遍历因为 right_to_left)
                            for (i, source) in sources.iter().enumerate().rev() {
                                let is_active = !is_ignored && active_idx == Some(i);
                                let btn_text =
                                    self.source_button_label(source, mat_price, amount, gs);
                                let rt = if is_active {
                                    egui::RichText::new(&btn_text).small().strong()
                                } else {
                                    egui::RichText::new(&btn_text).small().weak()
                                };
                                if ui.selectable_label(is_active, rt).clicked() {
                                    self.crafting_source_overrides
                                        .insert(mat_id, SourceChoice::Index(i));
                                }
                            }
                        });
                    });

                    if let Some(color) = bg {
                        ui.painter().rect_filled(resp.response.rect, 2.0, color);
                    }
                }
            });
    }

    /// 来源按钮的显示文本
    fn source_button_label(
        &self,
        source: &ItemSource,
        unit_price: u32,
        amount: u32,
        gs: &GameState,
    ) -> String {
        match source {
            ItemSource::GilShop { .. } => {
                let total = unit_price as u64 * amount as u64;
                format!("{} {}G", egui_phosphor::regular::COINS, total)
            }
            ItemSource::SpecialShop {
                cost_item_id,
                cost_count,
                ..
            } => {
                let cost_name = gs
                    .item_id_map
                    .get(cost_item_id)
                    .and_then(|&i| gs.all_items.get(i))
                    .map(|i| i.name.as_str())
                    .unwrap_or("???");
                let total = *cost_count as u64 * amount as u64;
                // 截短代币名 (最多6字符)
                let char_count = cost_name.chars().count();
                let short_name: String = if char_count > 6 {
                    let mut s: String = cost_name.chars().take(5).collect();
                    s.push('…');
                    s
                } else {
                    cost_name.to_string()
                };
                format!("{} {} x{}", egui_phosphor::regular::SWAP, short_name, total)
            }
            ItemSource::Gathering => {
                format!("{} 采集", egui_phosphor::regular::LEAF)
            }
        }
    }
}
