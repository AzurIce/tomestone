use std::collections::HashSet;

use eframe::egui;
use physis::stm::StainingTemplate;

use crate::app::App;
use crate::domain::{GameItem, ACCESSORY_SLOTS, GEAR_SLOTS};
use crate::dye;
use crate::game::{
    bake_color_table_texture, compute_bounding_box, load_mdl_with_fallback, load_mesh_textures,
};
use crate::loading::GameState;
use crate::ui::components::dye_palette;
use crate::ui::components::equipment_list::HighlightConfig;
use crate::ui::components::item_detail::{self, ItemDetailConfig};

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
                ui.heading("装备浏览器");
                ui.separator();

                // 槽位筛选
                let prev_slot = self.selected_slot;
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(self.selected_slot.is_none(), "全部")
                        .clicked()
                    {
                        self.selected_slot = None;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("装备:");
                    for slot in &GEAR_SLOTS {
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
                ui.horizontal(|ui| {
                    ui.label("饰品:");
                    for slot in &ACCESSORY_SLOTS {
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
                    // 切换槽位时自动展开当前选中物品所在的套装
                    if let Some(sel_idx) = self.selected_item {
                        if let Some(item) = gs.all_items.get(sel_idx) {
                            self.equipment_list.expanded_sets.insert(item.set_id());
                        }
                    }
                }

                ui.separator();

                // 高亮当前选中的物品
                let selected_ids: HashSet<u32> = self
                    .selected_item
                    .and_then(|idx| gs.all_items.get(idx))
                    .map(|item| {
                        let mut s = HashSet::new();
                        s.insert(item.row_id);
                        s
                    })
                    .unwrap_or_default();

                let highlight = HighlightConfig {
                    highlighted_ids: &selected_ids,
                    preview_id: None,
                };

                if let Some(clicked) = self.equipment_list.show(
                    ui,
                    &gs.all_items,
                    &gs.equipment_indices,
                    &gs.equipment_sets,
                    &gs.set_id_to_set_idx,
                    self.selected_slot,
                    &highlight,
                    "browser",
                    &mut self.icon_cache,
                    ctx,
                    &gs.game,
                ) {
                    self.selected_item = Some(clicked.global_idx);
                }
            });

        self.show_browser_detail_panel(ctx, gs);
    }

    fn show_browser_detail_panel(&mut self, ctx: &egui::Context, gs: &mut GameState) {
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(idx) = self.selected_item {
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
                    let prefix = if item.is_accessory() { "a" } else { "e" };
                    egui::Grid::new("item_info").show(ui, |ui| {
                        if let Some(slot) = item.equip_slot() {
                            ui.label("槽位:");
                            ui.label(slot.display_name());
                            ui.end_row();
                        }
                        ui.label("装备 ID:");
                        ui.label(format!("{}{:04}", prefix, item.set_id()));
                        ui.end_row();
                        ui.label("变体:");
                        ui.label(format!("v{:04}", item.variant_id()));
                        ui.end_row();
                        if let Some(path) = item.model_path() {
                            ui.label("模型路径:");
                            ui.label(path);
                            ui.end_row();
                        }
                    });

                    if let Some(&set_idx) = gs.set_id_to_set_idx.get(&item.set_id()) {
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
                                    let sib = &gs.all_items[i];
                                    let slot_abbr =
                                        sib.equip_slot().map(|s| s.slot_abbr()).unwrap_or("?");
                                    (i, format!("[{}] {}", slot_abbr, sib.name), i == idx)
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

    fn load_model_for_item(&mut self, idx: usize, item: &GameItem, gs: &GameState) {
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
                    item.set_id(),
                    item.variant_id(),
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
                    item.set_id(),
                    item.variant_id(),
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
        self.viewport.mark_dirty();
    }
}
