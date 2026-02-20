use eframe::egui;
use physis::stm::StainingTemplate;

use crate::app::App;
use crate::domain::{EquipmentItem, FlatRow, SortOrder, ViewMode, ALL_SLOTS};
use crate::dye;
use crate::game::{
    bake_color_table_texture, compute_bounding_box, load_mdl_with_fallback, load_mesh_textures,
};
use crate::loading::GameState;
use crate::ui::components::dye_palette;

impl App {
    pub fn show_browser_page(&mut self, ctx: &egui::Context, gs: &mut GameState) {
        if self.needs_rebake {
            self.needs_rebake = false;
            if let Some(stm) = &gs.stm {
                self.rebake_textures(stm);
            }
        }

        egui::SidePanel::left("equipment_list")
            .default_width(350.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("装备浏览器");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .selectable_label(self.view_mode == ViewMode::SetGroup, "套装")
                            .clicked()
                        {
                            self.view_mode = ViewMode::SetGroup;
                            self.flat_rows_dirty = true;
                            if let Some(sel_idx) = self.selected_item {
                                if let Some(item) = gs.items.get(sel_idx) {
                                    self.expanded_sets.insert(item.set_id);
                                }
                            }
                        }
                        if ui
                            .selectable_label(self.view_mode == ViewMode::List, "列表")
                            .clicked()
                        {
                            self.view_mode = ViewMode::List;
                        }
                    });
                });
                ui.separator();

                let prev_search = self.search.clone();
                ui.horizontal(|ui| {
                    ui.label("搜索:");
                    ui.text_edit_singleline(&mut self.search);
                });
                if self.search != prev_search {
                    self.flat_rows_dirty = true;
                }

                let prev_sort = self.sort_order;
                ui.horizontal(|ui| {
                    ui.label("排序:");
                    egui::ComboBox::from_id_salt("sort_order")
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
                            ui.selectable_value(
                                &mut self.sort_order,
                                SortOrder::BySlot,
                                SortOrder::BySlot.label(),
                            );
                        });
                });
                if self.sort_order != prev_sort {
                    self.flat_rows_dirty = true;
                }

                let prev_slot = self.selected_slot;
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(self.selected_slot.is_none(), "全部")
                        .clicked()
                    {
                        self.selected_slot = None;
                    }
                    for slot in &ALL_SLOTS {
                        if ui
                            .selectable_label(
                                self.selected_slot == Some(*slot),
                                slot.display_name(),
                            )
                            .clicked()
                        {
                            self.selected_slot = Some(*slot);
                        }
                    }
                });
                if self.selected_slot != prev_slot {
                    self.flat_rows_dirty = true;
                }

                ui.separator();

                match self.view_mode {
                    ViewMode::List => {
                        let filtered: Vec<(usize, String)> = self
                            .filtered_and_sorted_items(&gs.items)
                            .into_iter()
                            .map(|(idx, item)| {
                                (idx, format!("[{}] {}", item.slot.slot_abbr(), item.name))
                            })
                            .collect();
                        ui.label(format!("{} 件", filtered.len()));
                        egui::ScrollArea::vertical().show_rows(
                            ui,
                            18.0,
                            filtered.len(),
                            |ui, row_range| {
                                for row_idx in row_range {
                                    if let Some((global_idx, label)) = filtered.get(row_idx) {
                                        let selected = self.selected_item == Some(*global_idx);
                                        if ui.selectable_label(selected, label).clicked() {
                                            self.selected_item = Some(*global_idx);
                                        }
                                    }
                                }
                            },
                        );
                    }
                    ViewMode::SetGroup => {
                        if self.flat_rows_dirty {
                            self.build_flat_rows(gs);
                        }
                        let rows = self.cached_flat_rows.clone();
                        let num_sets = rows
                            .iter()
                            .filter(|r| matches!(r, FlatRow::GroupHeader { .. }))
                            .count();
                        let num_items = rows
                            .iter()
                            .filter(|r| matches!(r, FlatRow::Item { .. }))
                            .count();
                        ui.horizontal(|ui| {
                            ui.label(format!("{} 组套装, {} 件装备", num_sets, num_items));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.small_button("全部折叠").clicked() {
                                        self.expanded_sets.clear();
                                        self.flat_rows_dirty = true;
                                    }
                                    if ui.small_button("全部展开").clicked() {
                                        for eq_set in &gs.equipment_sets {
                                            self.expanded_sets.insert(eq_set.set_id);
                                        }
                                        self.flat_rows_dirty = true;
                                    }
                                },
                            );
                        });

                        let mut toggle_set: Option<u16> = None;
                        let mut select_item: Option<usize> = None;
                        egui::ScrollArea::vertical().show_rows(
                            ui,
                            18.0,
                            rows.len(),
                            |ui, row_range| {
                                for row_idx in row_range {
                                    if let Some(row) = rows.get(row_idx) {
                                        match row {
                                            FlatRow::GroupHeader {
                                                set_id,
                                                display_name,
                                                item_count,
                                                expanded,
                                            } => {
                                                let arrow = if *expanded { "▼" } else { "▶" };
                                                let text = format!(
                                                    "{} {} ({}件) e{:04}",
                                                    arrow, display_name, item_count, set_id
                                                );
                                                if ui
                                                    .selectable_label(
                                                        false,
                                                        egui::RichText::new(&text).strong(),
                                                    )
                                                    .clicked()
                                                {
                                                    toggle_set = Some(*set_id);
                                                }
                                            }
                                            FlatRow::Item { global_idx, label } => {
                                                ui.horizontal(|ui| {
                                                    ui.add_space(16.0);
                                                    let selected =
                                                        self.selected_item == Some(*global_idx);
                                                    if ui
                                                        .selectable_label(selected, label)
                                                        .clicked()
                                                    {
                                                        select_item = Some(*global_idx);
                                                    }
                                                });
                                            }
                                        }
                                    }
                                }
                            },
                        );

                        if let Some(sid) = toggle_set {
                            if self.expanded_sets.contains(&sid) {
                                self.expanded_sets.remove(&sid);
                            } else {
                                self.expanded_sets.insert(sid);
                            }
                            self.flat_rows_dirty = true;
                        }
                        if let Some(idx) = select_item {
                            self.selected_item = Some(idx);
                        }
                    }
                }
            });

        self.show_browser_detail_panel(ctx, gs);
    }

    fn show_browser_detail_panel(&mut self, ctx: &egui::Context, gs: &mut GameState) {
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(idx) = self.selected_item {
                if let Some(item) = gs.items.get(idx) {
                    ui.heading(&item.name);
                    ui.separator();
                    egui::Grid::new("item_info").show(ui, |ui| {
                        ui.label("槽位:");
                        ui.label(item.slot.display_name());
                        ui.end_row();
                        ui.label("装备 ID:");
                        ui.label(format!("e{:04}", item.set_id));
                        ui.end_row();
                        ui.label("变体:");
                        ui.label(format!("v{:04}", item.variant_id));
                        ui.end_row();
                        ui.label("模型路径:");
                        ui.label(item.model_path());
                        ui.end_row();
                    });

                    if let Some(&set_idx) = gs.set_id_to_set_idx.get(&item.set_id) {
                        let eq_set = &gs.equipment_sets[set_idx];
                        if eq_set.item_indices.len() > 1 {
                            ui.separator();
                            ui.label(
                                egui::RichText::new(format!(
                                    "同套装装备 ({})",
                                    eq_set.display_name
                                ))
                                .strong(),
                            );
                            let sibling_indices: Vec<(usize, String, bool)> = eq_set
                                .item_indices
                                .iter()
                                .map(|&i| {
                                    let sib = &gs.items[i];
                                    (
                                        i,
                                        format!("[{}] {}", sib.slot.slot_abbr(), sib.name),
                                        i == idx,
                                    )
                                })
                                .collect();
                            let mut clicked_sibling: Option<usize> = None;
                            ui.horizontal_wrapped(|ui| {
                                for (sib_idx, sib_label, is_current) in &sibling_indices {
                                    if *is_current {
                                        ui.label(
                                            egui::RichText::new(sib_label).strong().underline(),
                                        );
                                    } else if ui.link(sib_label).clicked() {
                                        clicked_sibling = Some(*sib_idx);
                                    }
                                }
                            });
                            if let Some(sib) = clicked_sibling {
                                self.selected_item = Some(sib);
                            }
                        }
                    }

                    ui.separator();

                    let has_dyeable = self.cached_materials.values().any(|m| m.uses_color_table);
                    if has_dyeable {
                        let changed = dye_palette::show_dye_palette(
                            ui,
                            &gs.stains,
                            &mut self.selected_stain_ids,
                            &mut self.active_dye_channel,
                            &mut self.selected_shade,
                            self.is_dual_dye,
                        );
                        if changed {
                            self.needs_rebake = true;
                        }
                    }

                    if self.loaded_model_idx != Some(idx) {
                        self.load_model_for_item(idx, item, gs);
                    }
                    self.viewport.show(ui, ctx, "模型加载失败");
                } else {
                    ui.label("选择一件装备查看详情");
                }
            } else {
                ui.centered_and_justified(|ui| {
                    ui.label("← 从左侧列表选择一件装备");
                });
            }
        });
    }

    fn load_model_for_item(&mut self, idx: usize, item: &EquipmentItem, gs: &GameState) {
        self.loaded_model_idx = Some(idx);
        self.selected_stain_ids = [0, 0];
        self.active_dye_channel = 0;
        let paths = item.model_paths();
        match load_mdl_with_fallback(&gs.game, &paths) {
            Ok(result) if !result.meshes.is_empty() => {
                let bbox = compute_bounding_box(&result.meshes);
                println!(
                    "加载纹理: {} 个材质, {} 个网格",
                    result.material_names.len(),
                    result.meshes.len()
                );
                let load_result = load_mesh_textures(
                    &gs.game,
                    &result.material_names,
                    &result.meshes,
                    item.set_id,
                    item.variant_id,
                );
                let geometry: Vec<(&[tomestone_render::Vertex], &[u16])> = result
                    .meshes
                    .iter()
                    .map(|m| (m.vertices.as_slice(), m.indices.as_slice()))
                    .collect();
                let vp = &mut self.viewport;
                vp.model_renderer.set_mesh_data(
                    &vp.render_state.device,
                    &vp.render_state.queue,
                    &geometry,
                    &load_result.mesh_textures,
                );
                self.cached_materials = load_result.materials;
                self.is_dual_dye = dye::has_dual_dye(&self.cached_materials);
                self.cached_meshes = result.meshes;
                self.viewport.camera.focus_on(&bbox);
                self.viewport.last_bbox = Some(bbox);
                self.viewport.free_texture();
            }
            _ => {
                eprintln!(
                    "模型加载失败 e{:04} v{:04}: {:?}",
                    item.set_id,
                    item.variant_id,
                    load_mdl_with_fallback(&gs.game, &paths).err()
                );
                let vp = &mut self.viewport;
                vp.model_renderer.set_mesh_data(
                    &vp.render_state.device,
                    &vp.render_state.queue,
                    &[],
                    &[],
                );
                self.viewport.last_bbox = None;
            }
        }
    }

    pub fn filtered_and_sorted_items<'a>(
        &self,
        items: &'a [EquipmentItem],
    ) -> Vec<(usize, &'a EquipmentItem)> {
        let mut result: Vec<(usize, &EquipmentItem)> = items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                if let Some(slot) = self.selected_slot {
                    if item.slot != slot {
                        return false;
                    }
                }
                if !self.search.is_empty() {
                    if !item
                        .name
                        .to_lowercase()
                        .contains(&self.search.to_lowercase())
                    {
                        return false;
                    }
                }
                true
            })
            .collect();
        match self.sort_order {
            SortOrder::ByName => result.sort_by(|a, b| a.1.name.cmp(&b.1.name)),
            SortOrder::BySetId => result.sort_by(|a, b| {
                a.1.set_id
                    .cmp(&b.1.set_id)
                    .then_with(|| a.1.slot.slot_abbr().cmp(b.1.slot.slot_abbr()))
            }),
            SortOrder::BySlot => result.sort_by(|a, b| {
                a.1.slot
                    .slot_abbr()
                    .cmp(b.1.slot.slot_abbr())
                    .then_with(|| a.1.name.cmp(&b.1.name))
            }),
        }
        result
    }

    pub fn item_matches_filter(&self, item: &EquipmentItem) -> bool {
        if let Some(slot) = self.selected_slot {
            if item.slot != slot {
                return false;
            }
        }
        if !self.search.is_empty() {
            if !item
                .name
                .to_lowercase()
                .contains(&self.search.to_lowercase())
            {
                return false;
            }
        }
        true
    }

    pub fn build_flat_rows(&mut self, gs: &GameState) {
        self.flat_rows_dirty = false;
        self.cached_flat_rows.clear();

        let mut sets_with_items: Vec<(usize, Vec<usize>)> = Vec::new();
        for (set_idx, eq_set) in gs.equipment_sets.iter().enumerate() {
            let filtered: Vec<usize> = eq_set
                .item_indices
                .iter()
                .copied()
                .filter(|&i| self.item_matches_filter(&gs.items[i]))
                .collect();
            if !filtered.is_empty() {
                sets_with_items.push((set_idx, filtered));
            }
        }

        match self.sort_order {
            SortOrder::ByName | SortOrder::BySlot => {
                sets_with_items.sort_by(|a, b| {
                    gs.equipment_sets[a.0]
                        .display_name
                        .cmp(&gs.equipment_sets[b.0].display_name)
                });
            }
            SortOrder::BySetId => {
                sets_with_items.sort_by(|a, b| {
                    gs.equipment_sets[a.0]
                        .set_id
                        .cmp(&gs.equipment_sets[b.0].set_id)
                });
            }
        }

        for (set_idx, filtered_indices) in sets_with_items {
            let eq_set = &gs.equipment_sets[set_idx];
            let expanded = self.expanded_sets.contains(&eq_set.set_id);
            self.cached_flat_rows.push(FlatRow::GroupHeader {
                set_id: eq_set.set_id,
                display_name: eq_set.display_name.clone(),
                item_count: filtered_indices.len(),
                expanded,
            });
            if expanded {
                for &global_idx in &filtered_indices {
                    let item = &gs.items[global_idx];
                    self.cached_flat_rows.push(FlatRow::Item {
                        global_idx,
                        label: format!("[{}] {}", item.slot.slot_abbr(), item.name),
                    });
                }
            }
        }
    }

    pub fn rebake_textures(&mut self, stm: &StainingTemplate) {
        let mut new_textures: Vec<Option<tomestone_render::TextureData>> = Vec::new();
        for mesh in &self.cached_meshes {
            let mat_idx = mesh.material_index;
            if let Some(cached) = self.cached_materials.get(&mat_idx) {
                if cached.uses_color_table {
                    if let (Some(color_table), Some(id_tex)) =
                        (&cached.color_table, &cached.id_texture)
                    {
                        let dyed_colors = if self.selected_stain_ids[0] > 0
                            || self.selected_stain_ids[1] > 0
                        {
                            cached.color_dye_table.as_ref().map(|dye_table| {
                                dye::apply_dye(color_table, dye_table, stm, self.selected_stain_ids)
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
        let vp = &mut self.viewport;
        vp.model_renderer.update_textures(
            &vp.render_state.device,
            &vp.render_state.queue,
            &new_textures,
        );
    }
}
