use eframe::egui;
use std::time::Instant;

use crate::app::App;
use crate::domain::{HousingExteriorItem, EXTERIOR_PART_TYPES};
use crate::game::{
    compute_bounding_box, extract_mdl_paths_from_sgb, load_housing_mesh_textures, load_mdl,
    MeshData,
};
use crate::loading::GameState;

impl App {
    pub fn show_housing_page(&mut self, ctx: &egui::Context, gs: &mut GameState) {
        let t_frame = Instant::now();

        let t0 = Instant::now();
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

                // 搜索框
                ui.horizontal(|ui| {
                    ui.label("搜索:");
                    ui.text_edit_singleline(&mut self.housing_search);
                });

                ui.separator();

                // 物品列表
                let t_filter = Instant::now();
                let search_lower = self.housing_search.to_lowercase();
                let filtered: Vec<(usize, &HousingExteriorItem)> = gs
                    .housing_exteriors
                    .iter()
                    .enumerate()
                    .filter(|(_, item)| {
                        if let Some(pt) = self.housing_selected_part_type {
                            if item.part_type != pt {
                                return false;
                            }
                        }
                        if !search_lower.is_empty()
                            && !item.name.to_lowercase().contains(&search_lower)
                        {
                            return false;
                        }
                        true
                    })
                    .collect();
                let filter_ms = t_filter.elapsed().as_secs_f64() * 1000.0;

                ui.label(format!("{} 件物品", filtered.len()));
                ui.separator();

                let row_height = 28.0;
                let total_rows = filtered.len();
                let t_scroll = Instant::now();
                let mut icon_load_count = 0u32;
                let mut icon_load_ms = 0.0f64;
                egui::ScrollArea::vertical().show_rows(
                    ui,
                    row_height,
                    total_rows,
                    |ui, row_range| {
                        let range_desc = format!("{}..{}", row_range.start, row_range.end);
                        eprintln!("  [timing] show_rows range: {}", range_desc);
                        for i in row_range {
                            let (idx, item) = &filtered[i];
                            let is_selected = self.housing_selected_item == Some(*idx);
                            let response = ui.horizontal(|ui| {
                                // 图标
                                let t_icon = Instant::now();
                                if let Some(icon) =
                                    self.get_or_load_icon(ctx, &gs.game, item.icon_id)
                                {
                                    ui.image(egui::load::SizedTexture::new(
                                        icon.id(),
                                        egui::vec2(24.0, 24.0),
                                    ));
                                } else {
                                    ui.allocate_space(egui::vec2(24.0, 24.0));
                                }
                                let elapsed = t_icon.elapsed().as_secs_f64() * 1000.0;
                                icon_load_ms += elapsed;
                                icon_load_count += 1;

                                let label =
                                    format!("[{}] {}", item.part_type.display_name(), item.name);
                                ui.selectable_label(is_selected, label)
                            });

                            if response.inner.clicked() {
                                self.housing_selected_item = Some(*idx);
                            }
                        }
                    },
                );
                let scroll_ms = t_scroll.elapsed().as_secs_f64() * 1000.0;
                if filter_ms + scroll_ms > 16.0 {
                    eprintln!(
                        "  [timing] 侧栏内部: filter {:.1}ms, scroll {:.1}ms (icons: {} 个 {:.1}ms)",
                        filter_ms, scroll_ms, icon_load_count, icon_load_ms
                    );
                }
            });
        let side_panel_ms = t0.elapsed().as_secs_f64() * 1000.0;

        let t1 = Instant::now();
        self.show_housing_detail_panel(ctx, gs);
        let detail_panel_ms = t1.elapsed().as_secs_f64() * 1000.0;

        let total_ms = t_frame.elapsed().as_secs_f64() * 1000.0;
        if total_ms > 16.0 {
            eprintln!(
                "[housing] 帧耗时 {:.1}ms (侧栏 {:.1}ms, 详情 {:.1}ms)",
                total_ms, side_panel_ms, detail_panel_ms
            );
        }
    }

    fn show_housing_detail_panel(&mut self, ctx: &egui::Context, gs: &mut GameState) {
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(idx) = self.housing_selected_item {
                if let Some(item) = gs.housing_exteriors.get(idx) {
                    ui.horizontal(|ui| {
                        if let Some(icon) = self.get_or_load_icon(ctx, &gs.game, item.icon_id) {
                            ui.image(&icon);
                        }
                        ui.heading(&item.name);
                    });
                    ui.separator();

                    egui::Grid::new("housing_item_info").show(ui, |ui| {
                        ui.label("类型:");
                        ui.label(item.part_type.display_name());
                        ui.end_row();
                        ui.label("模型 Key:");
                        ui.label(format!("{:04}", item.model_key));
                        ui.end_row();
                    });

                    ui.separator();

                    // 加载模型
                    if self.housing_loaded_model_idx != Some(idx) {
                        let t_load = Instant::now();
                        self.load_housing_model(idx, item, gs);
                        eprintln!(
                            "[housing] 模型加载耗时 {:.1}ms",
                            t_load.elapsed().as_secs_f64() * 1000.0
                        );
                    }
                    let t_vp = Instant::now();
                    self.housing_viewport.show(ui, ctx, "模型加载失败");
                    let vp_ms = t_vp.elapsed().as_secs_f64() * 1000.0;
                    if vp_ms > 8.0 {
                        eprintln!("[housing] viewport.show 耗时 {:.1}ms", vp_ms);
                    }
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

    fn load_housing_model(&mut self, idx: usize, item: &HousingExteriorItem, gs: &GameState) {
        let t_total = Instant::now();
        self.housing_loaded_model_idx = Some(idx);

        let model_key = item.model_key;
        let id = format!("{:04}", model_key);

        let sgb_path = format!(
            "bgcommon/hou/outdoor/general/{}/asset/gar_b0_m{}.sgb",
            id, id
        );

        // 1. 读取 SGB
        let t_sgb = Instant::now();
        let mdl_paths = if let Ok(sgb_data) = gs.game.read_file(&sgb_path) {
            let paths = extract_mdl_paths_from_sgb(&sgb_data);
            paths
        } else {
            Vec::new()
        };
        eprintln!(
            "  [timing] SGB 读取+解析: {:.1}ms",
            t_sgb.elapsed().as_secs_f64() * 1000.0
        );

        let mdl_candidates: Vec<String> = if mdl_paths.is_empty() {
            vec![
                format!(
                    "bgcommon/hou/outdoor/general/{}/bgparts/gar_b0_m{}_a.mdl",
                    id, id
                ),
                format!(
                    "bgcommon/hou/outdoor/general/{}/bgparts/gar_b0_m{}_b.mdl",
                    id, id
                ),
                format!(
                    "bgcommon/hou/outdoor/general/{}/bgparts/gar_b0_m{}.mdl",
                    id, id
                ),
            ]
        } else {
            mdl_paths
        };

        // 2. 加载所有 MDL
        let t_mdl = Instant::now();
        let mut all_meshes: Vec<MeshData> = Vec::new();
        let mut all_material_names: Vec<String> = Vec::new();
        let mut first_mdl_path: Option<String> = None;

        for mdl_path in &mdl_candidates {
            let t_one = Instant::now();
            match load_mdl(&gs.game, mdl_path) {
                Ok(result) if !result.meshes.is_empty() => {
                    eprintln!(
                        "  [timing] MDL {}: {:.1}ms ({} meshes, {} mats)",
                        mdl_path,
                        t_one.elapsed().as_secs_f64() * 1000.0,
                        result.meshes.len(),
                        result.material_names.len()
                    );
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
                Ok(_) => {
                    eprintln!(
                        "  [timing] MDL {} (空): {:.1}ms",
                        mdl_path,
                        t_one.elapsed().as_secs_f64() * 1000.0
                    );
                }
                Err(_e) => {
                    eprintln!(
                        "  [timing] MDL {} (失败): {:.1}ms",
                        mdl_path,
                        t_one.elapsed().as_secs_f64() * 1000.0
                    );
                }
            }
        }
        eprintln!(
            "  [timing] MDL 总计: {:.1}ms",
            t_mdl.elapsed().as_secs_f64() * 1000.0
        );

        if all_meshes.is_empty() {
            eprintln!("房屋外装模型加载失败: model_key={}", model_key);
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

        // 3. 加载纹理
        let t_tex = Instant::now();
        let load_result =
            load_housing_mesh_textures(&gs.game, &all_material_names, &all_meshes, mdl_path_ref);
        eprintln!(
            "  [timing] 纹理加载: {:.1}ms ({} 材质, {} 网格)",
            t_tex.elapsed().as_secs_f64() * 1000.0,
            all_material_names.len(),
            all_meshes.len()
        );

        // 4. 上传 GPU
        let t_gpu = Instant::now();
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
        eprintln!(
            "  [timing] GPU 上传: {:.1}ms",
            t_gpu.elapsed().as_secs_f64() * 1000.0
        );

        self.housing_viewport.camera.focus_on(&bbox);
        self.housing_viewport.last_bbox = Some(bbox);
        self.housing_viewport.free_texture();

        eprintln!(
            "  [timing] load_housing_model 总计: {:.1}ms",
            t_total.elapsed().as_secs_f64() * 1000.0
        );
    }
}
