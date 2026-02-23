use eframe::egui;

use crate::app::App;
use crate::domain::{HousingExteriorItem, EXTERIOR_PART_TYPES};
use crate::game::{
    compute_bounding_box, extract_mdl_paths_from_sgb, load_housing_mesh_textures, load_mdl,
    MeshData,
};
use crate::loading::GameState;

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

                // 搜索框
                ui.horizontal(|ui| {
                    ui.label("搜索:");
                    ui.text_edit_singleline(&mut self.housing_search);
                });

                ui.separator();

                // 物品列表
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

                ui.label(format!("{} 件物品", filtered.len()));
                ui.separator();

                let row_height = 28.0;
                let total_rows = filtered.len();
                egui::ScrollArea::vertical().show_rows(
                    ui,
                    row_height,
                    total_rows,
                    |ui, row_range| {
                        for i in row_range {
                            let (idx, item) = &filtered[i];
                            let is_selected = self.housing_selected_item == Some(*idx);
                            let response = ui.horizontal(|ui| {
                                // 图标
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
            });

        self.show_housing_detail_panel(ctx, gs);
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

    fn load_housing_model(&mut self, idx: usize, item: &HousingExteriorItem, gs: &GameState) {
        self.housing_loaded_model_idx = Some(idx);

        let model_key = item.model_key;
        let id = format!("{:04}", model_key);

        // 房屋外装模型路径:
        // SGB: bgcommon/hou/outdoor/general/{id}/asset/gar_b0_m{id}.sgb
        // 也可能直接有 MDL 文件
        let sgb_path = format!(
            "bgcommon/hou/outdoor/general/{}/asset/gar_b0_m{}.sgb",
            id, id
        );

        println!("尝试加载房屋外装 SGB: {}", sgb_path);

        // 尝试从 SGB 提取 MDL 路径
        let mdl_paths = if let Ok(sgb_data) = gs.game.read_file(&sgb_path) {
            let paths = extract_mdl_paths_from_sgb(&sgb_data);
            println!("SGB 中找到 {} 个 MDL 路径: {:?}", paths.len(), paths);
            paths
        } else {
            println!("SGB 加载失败，尝试直接查找 MDL");
            Vec::new()
        };

        // 如果 SGB 没有找到 MDL，尝试常见的直接 MDL 路径
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

        // 尝试加载所有 MDL 并合并
        let mut all_meshes: Vec<MeshData> = Vec::new();
        let mut all_material_names: Vec<String> = Vec::new();
        let mut first_mdl_path: Option<String> = None;

        for mdl_path in &mdl_candidates {
            println!("  尝试 MDL: {}", mdl_path);
            match load_mdl(&gs.game, mdl_path) {
                Ok(result) if !result.meshes.is_empty() => {
                    println!(
                        "  MDL 加载成功: {} 个网格, {} 个材质",
                        result.meshes.len(),
                        result.material_names.len()
                    );
                    if first_mdl_path.is_none() {
                        first_mdl_path = Some(mdl_path.clone());
                    }

                    // 调整材质索引偏移
                    let mat_offset = all_material_names.len() as u16;
                    for mut mesh in result.meshes {
                        mesh.material_index += mat_offset;
                        all_meshes.push(mesh);
                    }
                    all_material_names.extend(result.material_names);
                }
                Ok(_) => {
                    println!("  MDL 网格为空: {}", mdl_path);
                }
                Err(e) => {
                    println!("  MDL 加载失败: {} - {}", mdl_path, e);
                }
            }
        }

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

        println!(
            "加载房屋外装纹理: {} 个材质, {} 个网格",
            all_material_names.len(),
            all_meshes.len()
        );

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
