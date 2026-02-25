use eframe::egui;

use crate::app::App;
use crate::domain::{GameItem, ViewMode, EXTERIOR_PART_TYPES};
use crate::game::{
    compute_bounding_box, extract_mdl_paths_from_sgb, load_housing_mesh_textures, load_mdl,
    MeshData,
};
use crate::loading::GameState;
use crate::ui::components::item_detail::{self, ItemDetailConfig};
use crate::ui::components::item_list::{self, DisplayItem};

impl App {
    pub fn show_housing_page(&mut self, ctx: &egui::Context, gs: &mut GameState) {
        egui::SidePanel::left("housing_list")
            .default_width(350.0)
            .show(ctx, |ui| {
                ui.heading("房屋外装浏览器");
                ui.separator();

                // 类型筛选
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

                // 搜索框 + 视图模式 + 图标大小
                self.housing_list.show_controls(ui);

                // 物品列表: 从 housing_ext_indices 获取 all_items 中的下标
                let search_lower = self.housing_list.search_lower();
                let filtered: Vec<(usize, &GameItem)> = gs
                    .housing_ext_indices
                    .iter()
                    .filter_map(|&idx| {
                        let item = &gs.all_items[idx];
                        if let Some(pt) = self.housing_selected_part_type {
                            if item.exterior_part_type() != Some(pt) {
                                return None;
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
                                    let part_name = item
                                        .exterior_part_type()
                                        .map(|pt| pt.display_name())
                                        .unwrap_or("?");
                                    let label = format!("[{}] {}", part_name, item.name);
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

                    let sgb_paths = gs.housing_sgb_paths.get(&item.additional_data);

                    egui::Grid::new("housing_item_info").show(ui, |ui| {
                        if let Some(pt) = item.exterior_part_type() {
                            ui.label("类型:");
                            ui.label(pt.display_name());
                            ui.end_row();
                        }
                        ui.label("SGB:");
                        ui.label(
                            sgb_paths
                                .and_then(|p| p.first())
                                .map(|s| s.as_str())
                                .unwrap_or("无"),
                        );
                        ui.end_row();
                    });

                    ui.separator();

                    // 加载模型
                    if self.housing_loaded_model_idx != Some(idx) {
                        self.load_housing_model(idx, item, gs);
                    }
                    self.housing_viewport.show(ui, ctx, "模型加载失败");
                } else {
                    ui.label("选择一件外装查看详情");
                }
            } else {
                ui.centered_and_justified(|ui| {
                    ui.label("← 从左侧列表选择一件外装");
                });
            }
        });
    }

    fn load_housing_model(&mut self, idx: usize, item: &GameItem, gs: &GameState) {
        self.housing_loaded_model_idx = Some(idx);

        // 从 SGB 路径提取 MDL 路径
        let sgb_paths = match gs.housing_sgb_paths.get(&item.additional_data) {
            Some(paths) => paths,
            None => {
                let vp = &mut self.housing_viewport;
                vp.model_renderer.set_mesh_data(
                    &vp.render_state.device,
                    &vp.render_state.queue,
                    &[],
                    &[],
                );
                self.housing_viewport.last_bbox = None;
                return;
            }
        };

        let mut all_mdl_paths: Vec<String> = Vec::new();
        for sgb_path in sgb_paths {
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
            let vp = &mut self.housing_viewport;
            vp.model_renderer.set_mesh_data(
                &vp.render_state.device,
                &vp.render_state.queue,
                &[],
                &[],
            );
            self.housing_viewport.last_bbox = None;
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
            let vp = &mut self.housing_viewport;
            vp.model_renderer.set_mesh_data(
                &vp.render_state.device,
                &vp.render_state.queue,
                &[],
                &[],
            );
            self.housing_viewport.last_bbox = None;
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
        self.housing_viewport.camera.focus_on(&bbox);
        self.housing_viewport.last_bbox = Some(bbox);
        self.housing_viewport.free_texture();
    }
}
