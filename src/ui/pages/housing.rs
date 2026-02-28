use std::collections::{BTreeMap, HashMap};

use eframe::egui;
use physis::stm::StainingTemplate;

use crate::app::App;
use crate::domain::{GameItem, HousingSubTab, ViewMode, EXTERIOR_PART_TYPES, HOUSING_SUB_TABS};
use crate::dye;
use crate::game::{
    bake_color_table_texture, compute_bounding_box, extract_mdl_paths_from_sgb,
    load_housing_mesh_textures, load_mdl, MeshData,
};
use crate::loading::GameState;
use crate::ui::components::dye_palette;
use crate::ui::components::item_detail::{self, ItemDetailConfig};
use crate::ui::components::item_list::{self, DisplayItem};

impl App {
    pub fn show_housing_page(&mut self, ctx: &egui::Context, gs: &mut GameState) {
        // 染色重烘焙
        if self.housing_needs_rebake {
            self.housing_needs_rebake = false;
            if let Some(stm) = &gs.stm {
                self.rebake_housing_textures(stm);
            }
        }

        egui::SidePanel::left("housing_list")
            .default_width(350.0)
            .show(ctx, |ui| {
                ui.heading("房屋浏览器");
                ui.separator();

                // 子标签切换
                let prev_sub_tab = self.housing_sub_tab;
                ui.horizontal(|ui| {
                    for tab in &HOUSING_SUB_TABS {
                        if ui
                            .selectable_label(self.housing_sub_tab == *tab, tab.display_name())
                            .clicked()
                        {
                            self.housing_sub_tab = *tab;
                        }
                    }
                });

                // 切换子标签时清除选中
                if self.housing_sub_tab != prev_sub_tab {
                    self.housing_selected_item = None;
                    self.housing_loaded_model_idx = None;
                    self.housing_selected_part_type = None;
                    self.housing_selected_ui_category = None;
                }

                ui.separator();

                // 根据子标签获取物品索引列表
                let indices = match self.housing_sub_tab {
                    HousingSubTab::Exterior => &gs.housing_ext_indices,
                    HousingSubTab::Yard => &gs.housing_yard_indices,
                    HousingSubTab::Indoor => &gs.housing_indoor_indices,
                };

                // 分类筛选按钮
                match self.housing_sub_tab {
                    HousingSubTab::Exterior => {
                        // 外装: 用 ExteriorPartType 筛选
                        ui.horizontal_wrapped(|ui| {
                            if ui
                                .selectable_label(self.housing_selected_part_type.is_none(), "全部")
                                .clicked()
                            {
                                self.housing_selected_part_type = None;
                            }
                            for pt in &EXTERIOR_PART_TYPES {
                                if ui
                                    .selectable_label(
                                        self.housing_selected_part_type == Some(*pt),
                                        pt.display_name(),
                                    )
                                    .clicked()
                                {
                                    self.housing_selected_part_type = Some(*pt);
                                }
                            }
                        });
                        ui.separator();
                    }
                    HousingSubTab::Yard | HousingSubTab::Indoor => {
                        // 庭院/室内: 用 ItemUICategory 动态筛选
                        let mut cat_counts: BTreeMap<u8, usize> = BTreeMap::new();
                        for &idx in indices {
                            *cat_counts
                                .entry(gs.all_items[idx].item_ui_category)
                                .or_default() += 1;
                        }
                        if cat_counts.len() > 1 {
                            ui.horizontal_wrapped(|ui| {
                                if ui
                                    .selectable_label(
                                        self.housing_selected_ui_category.is_none(),
                                        "全部",
                                    )
                                    .clicked()
                                {
                                    self.housing_selected_ui_category = None;
                                }
                                for (&cat, &count) in &cat_counts {
                                    let name = gs
                                        .ui_category_names
                                        .get(&cat)
                                        .map(|s| s.as_str())
                                        .unwrap_or("?");
                                    let label = format!("{}({})", name, count);
                                    if ui
                                        .selectable_label(
                                            self.housing_selected_ui_category == Some(cat),
                                            label,
                                        )
                                        .clicked()
                                    {
                                        self.housing_selected_ui_category = Some(cat);
                                    }
                                }
                            });
                            ui.separator();
                        }
                    }
                }

                // 搜索框 + 视图模式 + 图标大小
                self.housing_list.show_controls(ui);

                let search_lower = self.housing_list.search_lower();
                let filtered: Vec<(usize, &GameItem)> = indices
                    .iter()
                    .filter_map(|&idx| {
                        let item = &gs.all_items[idx];
                        // 外装类型筛选
                        if self.housing_sub_tab == HousingSubTab::Exterior {
                            if let Some(pt) = self.housing_selected_part_type {
                                if item.exterior_part_type() != Some(pt) {
                                    return None;
                                }
                            }
                        }
                        // 庭院/室内分类筛选
                        if matches!(
                            self.housing_sub_tab,
                            HousingSubTab::Yard | HousingSubTab::Indoor
                        ) {
                            if let Some(cat) = self.housing_selected_ui_category {
                                if item.item_ui_category != cat {
                                    return None;
                                }
                            }
                        }
                        if !search_lower.is_empty()
                            && !item.name.to_lowercase().contains(&search_lower)
                        {
                            return None;
                        }
                        Some((idx, item))
                    })
                    .collect();

                ui.label(format!("{} 件物品", filtered.len()));
                ui.separator();

                // 构建 DisplayItem 列表
                let display_items: Vec<DisplayItem<'_>> = filtered
                    .iter()
                    .map(|&(idx, item)| DisplayItem {
                        id: idx,
                        name: &item.name,
                        icon_id: item.icon_id,
                        is_selected: self.housing_selected_item == Some(idx),
                    })
                    .collect();

                match self.housing_list.view_mode {
                    ViewMode::Grid => {
                        if let Some(clicked) = item_list::show_grid_scroll(
                            ui,
                            &display_items,
                            self.housing_list.icon_size,
                            "housing",
                            &mut self.icon_cache,
                            ctx,
                            &gs.game,
                        ) {
                            self.housing_selected_item = Some(clicked);
                        }
                    }
                    ViewMode::List => {
                        let row_height = 28.0;
                        let total_rows = filtered.len();
                        egui::ScrollArea::vertical().show_rows(
                            ui,
                            row_height,
                            total_rows,
                            |ui, row_range| {
                                for i in row_range {
                                    let (idx, item) = &filtered[i];
                                    let label = self.housing_list_label(item);
                                    let di = DisplayItem {
                                        id: *idx,
                                        name: &item.name,
                                        icon_id: item.icon_id,
                                        is_selected: self.housing_selected_item == Some(*idx),
                                    };
                                    if item_list::show_list_row(
                                        ui,
                                        &di,
                                        &label,
                                        &mut self.icon_cache,
                                        ctx,
                                        &gs.game,
                                    ) {
                                        self.housing_selected_item = Some(*idx);
                                    }
                                }
                            },
                        );
                    }
                }
            });

        self.show_housing_detail_panel(ctx, gs);
    }

    fn housing_list_label(&self, item: &GameItem) -> String {
        match self.housing_sub_tab {
            HousingSubTab::Exterior => {
                let part_name = item
                    .exterior_part_type()
                    .map(|pt| pt.display_name())
                    .unwrap_or("?");
                format!("[{}] {}", part_name, item.name)
            }
            _ => item.name.clone(),
        }
    }

    fn show_housing_detail_panel(&mut self, ctx: &egui::Context, gs: &mut GameState) {
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(idx) = self.housing_selected_item {
                if let Some(item) = gs.all_items.get(idx) {
                    // 统一物品详情头部
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
                        &ItemDetailConfig::default(),
                    );
                    ui.separator();

                    // 显示 SGB 路径信息
                    let sgb_display = self.get_housing_sgb_display(item, gs);
                    egui::Grid::new("housing_item_info").show(ui, |ui| {
                        if let Some(pt) = item.exterior_part_type() {
                            ui.label("类型:");
                            ui.label(pt.display_name());
                            ui.end_row();
                        }
                        ui.label("SGB:");
                        ui.label(&sgb_display);
                        ui.end_row();
                    });

                    ui.separator();

                    // 染色面板
                    let has_dyeable = self
                        .housing_cached_materials
                        .values()
                        .any(|m| m.uses_color_table);
                    if has_dyeable {
                        let changed = dye_palette::show_dye_palette(
                            ui,
                            &gs.stains,
                            &mut self.housing_stain_ids,
                            &mut self.housing_active_dye_channel,
                            &mut self.housing_selected_shade,
                            self.housing_is_dual_dye,
                        );
                        if changed {
                            self.housing_needs_rebake = true;
                        }
                    }

                    // 加载模型
                    if self.housing_loaded_model_idx != Some(idx) {
                        self.load_housing_model(idx, item, gs);
                    }
                    self.housing_viewport.show(ui, ctx, "模型加载失败");
                } else {
                    ui.label("选择一件物品查看详情");
                }
            } else {
                ui.centered_and_justified(|ui| {
                    ui.label("← 从左侧列表选择一件物品");
                });
            }
        });
    }

    fn get_housing_sgb_display(&self, item: &GameItem, gs: &GameState) -> String {
        match self.housing_sub_tab {
            HousingSubTab::Exterior => gs
                .housing_sgb_paths
                .get(&item.additional_data)
                .and_then(|p| p.first())
                .map(|s| s.as_str())
                .unwrap_or("无")
                .to_string(),
            HousingSubTab::Yard => gs
                .housing_yard_sgb_paths
                .get(&item.row_id)
                .map(|s| s.as_str())
                .unwrap_or("无")
                .to_string(),
            HousingSubTab::Indoor => gs
                .housing_furniture_sgb_paths
                .get(&item.row_id)
                .map(|s| s.as_str())
                .unwrap_or("无")
                .to_string(),
        }
    }

    fn load_housing_model(&mut self, idx: usize, item: &GameItem, gs: &GameState) {
        self.housing_loaded_model_idx = Some(idx);
        self.housing_stain_ids = [0, 0];
        self.housing_active_dye_channel = 0;

        // 根据子标签获取 SGB 路径
        let sgb_list: Vec<String> = match self.housing_sub_tab {
            HousingSubTab::Exterior => gs
                .housing_sgb_paths
                .get(&item.additional_data)
                .cloned()
                .unwrap_or_default(),
            HousingSubTab::Yard => gs
                .housing_yard_sgb_paths
                .get(&item.row_id)
                .map(|s| vec![s.clone()])
                .unwrap_or_default(),
            HousingSubTab::Indoor => gs
                .housing_furniture_sgb_paths
                .get(&item.row_id)
                .map(|s| vec![s.clone()])
                .unwrap_or_default(),
        };

        if sgb_list.is_empty() {
            self.clear_housing_model();
            return;
        }

        let mut all_mdl_paths: Vec<String> = Vec::new();
        for sgb_path in &sgb_list {
            if let Ok(sgb_data) = gs.game.read_file(sgb_path) {
                let paths = extract_mdl_paths_from_sgb(&sgb_data);
                for p in paths {
                    if !all_mdl_paths.contains(&p) {
                        all_mdl_paths.push(p);
                    }
                }
            }
        }

        if all_mdl_paths.is_empty() {
            self.clear_housing_model();
            return;
        }

        let mut all_meshes: Vec<MeshData> = Vec::new();
        let mut all_material_names: Vec<String> = Vec::new();
        let mut first_mdl_path: Option<String> = None;

        for mdl_path in &all_mdl_paths {
            match load_mdl(&gs.game, mdl_path) {
                Ok(result) if !result.meshes.is_empty() => {
                    if first_mdl_path.is_none() {
                        first_mdl_path = Some(mdl_path.clone());
                    }
                    let mat_offset = all_material_names.len() as u16;
                    for mut mesh in result.meshes {
                        mesh.material_index += mat_offset;
                        all_meshes.push(mesh);
                    }
                    all_material_names.extend(result.material_names);
                }
                _ => {}
            }
        }

        if all_meshes.is_empty() {
            self.clear_housing_model();
            return;
        }

        let bbox = compute_bounding_box(&all_meshes);
        let mdl_path_ref = first_mdl_path.as_deref().unwrap_or("");

        let load_result =
            load_housing_mesh_textures(&gs.game, &all_material_names, &all_meshes, mdl_path_ref);

        let geometry: Vec<(&[tomestone_render::Vertex], &[u16])> = all_meshes
            .iter()
            .map(|m| (m.vertices.as_slice(), m.indices.as_slice()))
            .collect();

        let vp = &mut self.housing_viewport;
        vp.model_renderer
            .set_model_type(tomestone_render::ModelType::Background);
        vp.model_renderer.set_mesh_data(
            &vp.render_state.device,
            &vp.render_state.queue,
            &geometry,
            &load_result.mesh_textures,
        );

        // 缓存材质用于染色
        self.housing_cached_materials = load_result.materials;
        self.housing_is_dual_dye = dye::has_dual_dye(&self.housing_cached_materials);
        self.housing_cached_meshes = all_meshes;

        self.housing_viewport.camera.focus_on(&bbox);
        self.housing_viewport.last_bbox = Some(bbox);
        self.housing_viewport.free_texture();
    }

    fn clear_housing_model(&mut self) {
        let vp = &mut self.housing_viewport;
        vp.model_renderer
            .set_mesh_data(&vp.render_state.device, &vp.render_state.queue, &[], &[]);
        self.housing_viewport.last_bbox = None;
        self.housing_cached_materials = HashMap::new();
        self.housing_cached_meshes = Vec::new();
        self.housing_is_dual_dye = false;
    }

    pub fn rebake_housing_textures(&mut self, stm: &StainingTemplate) {
        let mut new_textures: Vec<Option<tomestone_render::TextureData>> = Vec::new();
        for mesh in &self.housing_cached_meshes {
            let mat_idx = mesh.material_index;
            if let Some(cached) = self.housing_cached_materials.get(&mat_idx) {
                if cached.uses_color_table {
                    if let (Some(color_table), Some(id_tex)) =
                        (&cached.color_table, &cached.id_texture)
                    {
                        let dyed_colors = if self.housing_stain_ids[0] > 0
                            || self.housing_stain_ids[1] > 0
                        {
                            cached.color_dye_table.as_ref().map(|dye_table| {
                                dye::apply_dye(color_table, dye_table, stm, self.housing_stain_ids)
                            })
                        } else {
                            None
                        };
                        let baked =
                            bake_color_table_texture(id_tex, color_table, dyed_colors.as_ref());
                        new_textures.push(Some(baked));
                        continue;
                    }
                }
            }
            new_textures.push(None);
        }
        let vp = &mut self.housing_viewport;
        vp.model_renderer.update_textures(
            &vp.render_state.device,
            &vp.render_state.queue,
            &new_textures,
        );
        self.housing_viewport.mark_dirty();
    }
}
