use std::collections::HashMap;
use std::ops::Range;

use eframe::egui;
use egui_wgpu::wgpu;
use physis::stm::StainingTemplate;

use crate::game_data::{
    shade_group_name, EquipSlot, EquipmentItem, GameData, StainEntry, RACE_CODES, SHADE_ORDER,
};
use crate::glamour::GlamourSet;
use crate::tex_loader::CachedMaterial;
use crate::{EquipmentSet, SortOrder, ALL_SLOTS};
use tomestone_render::{BoundingBox, Camera, ModelRenderer};

/// 聚合只读引用，减少参数数量
pub struct AppContext<'a> {
    pub items: &'a [EquipmentItem],
    pub item_id_map: &'a HashMap<u32, usize>,
    pub stains: &'a [StainEntry],
    pub stm: Option<&'a StainingTemplate>,
    pub game: &'a GameData,
    pub equipment_sets: &'a [EquipmentSet],
    pub set_id_to_set_idx: &'a HashMap<u16, usize>,
}

/// 每个槽位的加载状态
struct SlotState {
    loaded_item_id: Option<u32>,
    mesh_range: Range<usize>,
    cached_materials: HashMap<u16, CachedMaterial>,
    cached_meshes: Vec<crate::mdl_loader::MeshData>,
    is_dual_dye: bool,
}

impl Default for SlotState {
    fn default() -> Self {
        Self {
            loaded_item_id: None,
            mesh_range: 0..0,
            cached_materials: HashMap::new(),
            cached_meshes: Vec::new(),
            is_dual_dye: false,
        }
    }
}

/// 编辑器返回的动作
pub enum GlamourEditorAction {
    None,
    Save,
    Close,
}

/// 三面板幻化编辑器
pub struct GlamourEditor {
    pub glamour_set: GlamourSet,
    pub active_slot: EquipSlot,

    // 左侧面板状态
    search: String,
    sort_order: SortOrder,

    // 右侧面板状态 (每槽位染色)
    selected_stain_ids: HashMap<EquipSlot, [u32; 2]>,
    active_dye_channel: usize,
    selected_shade: u8,

    // 渲染 (独立 ModelRenderer)
    model_renderer: ModelRenderer,
    render_state: egui_wgpu::RenderState,
    camera: Camera,
    model_texture_id: Option<egui::TextureId>,
    slot_states: HashMap<EquipSlot, SlotState>,
    last_bbox: Option<BoundingBox>,
    needs_mesh_rebuild: bool,
    needs_rebake: bool,
    pub dirty: bool,

    // 骨骼蒙皮缓存
    skeleton_cache: crate::skeleton::SkeletonCache,
}

impl GlamourEditor {
    pub fn new(glamour_set: GlamourSet, render_state: egui_wgpu::RenderState) -> Self {
        let model_renderer = ModelRenderer::new(&render_state.device);

        // 从 glamour_set 中恢复每个槽位的染色状态
        let mut selected_stain_ids = HashMap::new();
        for slot in &ALL_SLOTS {
            let stain_ids = glamour_set
                .get_slot(*slot)
                .map(|gs| gs.stain_ids)
                .unwrap_or([0, 0]);
            selected_stain_ids.insert(*slot, stain_ids);
        }

        Self {
            glamour_set,
            active_slot: EquipSlot::Body,
            search: String::new(),
            sort_order: SortOrder::ByName,
            selected_stain_ids,
            active_dye_channel: 0,
            selected_shade: 2,
            model_renderer,
            render_state,
            camera: Camera::default(),
            model_texture_id: None,
            slot_states: HashMap::new(),
            last_bbox: None,
            needs_mesh_rebuild: true,
            needs_rebake: false,
            dirty: false,
            skeleton_cache: crate::skeleton::SkeletonCache::new(),
        }
    }

    /// 合并所有槽位的 mesh，全量上传
    fn rebuild_merged_meshes(
        &mut self,
        items: &[EquipmentItem],
        item_id_map: &HashMap<u32, usize>,
        game: &GameData,
    ) {
        self.needs_mesh_rebuild = false;

        // 收集所有有装备的槽位对应的 EquipmentItem
        let equipped_items: Vec<(EquipSlot, &EquipmentItem)> = ALL_SLOTS
            .iter()
            .filter_map(|slot| {
                self.glamour_set
                    .get_slot(*slot)
                    .and_then(|gs| item_id_map.get(&gs.item_id))
                    .and_then(|&idx| items.get(idx))
                    .map(|item| (*slot, item))
            })
            .collect();

        // 确定统一种族码：找到对所有部件都有模型文件的最优先种族码
        let unified_race = if equipped_items.is_empty() {
            RACE_CODES[0]
        } else {
            let mut chosen = RACE_CODES[0];
            for &rc in RACE_CODES {
                let all_exist = equipped_items.iter().all(|(_, item)| {
                    let path = item.model_path_for_race(rc);
                    game.read_file(&path).is_ok()
                });
                if all_exist {
                    chosen = rc;
                    break;
                }
            }
            chosen
        };

        let mut all_meshes: Vec<crate::mdl_loader::MeshData> = Vec::new();
        let mut all_textures: Vec<tomestone_render::MeshTextures> = Vec::new();

        for slot in &ALL_SLOTS {
            let state = self.slot_states.entry(*slot).or_default();

            let slot_item_id = self.glamour_set.get_slot(*slot).map(|s| s.item_id);

            if slot_item_id.is_none() {
                state.loaded_item_id = None;
                state.mesh_range = all_meshes.len()..all_meshes.len();
                state.cached_materials.clear();
                state.cached_meshes.clear();
                state.is_dual_dye = false;
                continue;
            }

            let item_id = slot_item_id.unwrap();

            let item = match item_id_map.get(&item_id).and_then(|&idx| items.get(idx)) {
                Some(item) => item,
                None => {
                    state.loaded_item_id = None;
                    state.mesh_range = all_meshes.len()..all_meshes.len();
                    continue;
                }
            };

            // 优先用统一种族码加载，跟踪实际使用的种族码
            let unified_path = item.model_path_for_race(unified_race);
            let (load_result_mdl, actual_race) =
                match crate::mdl_loader::load_mdl(game, &unified_path) {
                    Ok(result) if !result.meshes.is_empty() => {
                        (Some(result), unified_race.to_string())
                    }
                    _ => {
                        // 回退: 逐个种族码尝试
                        let mut found = (None, String::new());
                        for &rc in RACE_CODES {
                            let path = item.model_path_for_race(rc);
                            if let Ok(result) = crate::mdl_loader::load_mdl(game, &path) {
                                if !result.meshes.is_empty() {
                                    found = (Some(result), rc.to_string());
                                    break;
                                }
                            }
                        }
                        found
                    }
                };

            match load_result_mdl {
                Some(mut result) if !result.meshes.is_empty() => {
                    // 当实际种族码与统一种族码不同时，应用 CPU 蒙皮
                    if actual_race != unified_race {
                        if let Some(target_bind) =
                            self.skeleton_cache.get_bind_pose(unified_race, game)
                        {
                            // 需要 clone 因为 get_bind_pose 借用了 self
                            let target_bind = target_bind.clone();
                            if let Some(source_bind) =
                                self.skeleton_cache.get_bind_pose(&actual_race, game)
                            {
                                let source_bind = source_bind.clone();
                                crate::skeleton::apply_skinning(
                                    &mut result.meshes,
                                    &result.bone_names,
                                    &result.bone_tables,
                                    &source_bind,
                                    &target_bind,
                                );
                            }
                        }
                    }

                    let start = all_meshes.len();
                    let load_result = crate::tex_loader::load_mesh_textures(
                        game,
                        &result.material_names,
                        &result.meshes,
                        item.set_id,
                        item.variant_id,
                    );
                    state.loaded_item_id = Some(item_id);
                    state.cached_materials = load_result.materials;
                    state.is_dual_dye = crate::dye::has_dual_dye(&state.cached_materials);
                    state.cached_meshes = result.meshes.clone();
                    all_meshes.extend(result.meshes);
                    all_textures.extend(load_result.mesh_textures);
                    state.mesh_range = start..all_meshes.len();
                }
                _ => {
                    state.loaded_item_id = None;
                    state.mesh_range = all_meshes.len()..all_meshes.len();
                    state.cached_materials.clear();
                    state.cached_meshes.clear();
                }
            }
        }

        // 全量上传
        let geometry: Vec<(&[tomestone_render::Vertex], &[u16])> = all_meshes
            .iter()
            .map(|m| (m.vertices.as_slice(), m.indices.as_slice()))
            .collect();
        self.model_renderer.set_mesh_data(
            &self.render_state.device,
            &self.render_state.queue,
            &geometry,
            &all_textures,
        );

        // 计算合并包围盒
        if !all_meshes.is_empty() {
            let bbox = crate::mdl_loader::compute_bounding_box(&all_meshes);
            self.camera.focus_on(&bbox);
            self.last_bbox = Some(bbox);
        } else {
            self.last_bbox = None;
        }

        // 释放旧纹理
        if let Some(tid) = self.model_texture_id.take() {
            self.render_state.renderer.write().free_texture(&tid);
        }
    }

    /// 重烘焙单个槽位的染色纹理
    fn rebake_slot_textures(&mut self, slot: EquipSlot, stm: &StainingTemplate) {
        let stain_ids = self
            .selected_stain_ids
            .get(&slot)
            .copied()
            .unwrap_or([0, 0]);
        let total_meshes = self.model_renderer.mesh_count();

        let state = match self.slot_states.get(&slot) {
            Some(s) => s,
            None => return,
        };

        if state.mesh_range.is_empty() {
            return;
        }

        let mut new_textures: Vec<Option<tomestone_render::TextureData>> =
            (0..total_meshes).map(|_| None).collect();

        for (local_idx, mesh) in state.cached_meshes.iter().enumerate() {
            let global_idx = state.mesh_range.start + local_idx;
            if global_idx >= total_meshes {
                break;
            }

            let mat_idx = mesh.material_index;
            if let Some(cached) = state.cached_materials.get(&mat_idx) {
                if cached.uses_color_table {
                    if let (Some(color_table), Some(id_tex)) =
                        (&cached.color_table, &cached.id_texture)
                    {
                        let dyed_colors = if stain_ids[0] > 0 || stain_ids[1] > 0 {
                            if let Some(dye_table) = &cached.color_dye_table {
                                Some(crate::dye::apply_dye(
                                    color_table,
                                    dye_table,
                                    stm,
                                    stain_ids,
                                ))
                            } else {
                                None
                            }
                        } else {
                            None
                        };
                        let baked = crate::tex_loader::bake_color_table_texture(
                            id_tex,
                            color_table,
                            dyed_colors.as_ref(),
                        );
                        new_textures[global_idx] = Some(baked);
                    }
                }
            }
        }

        self.model_renderer.update_textures(
            &self.render_state.device,
            &self.render_state.queue,
            &new_textures,
        );
    }

    /// 主 UI，返回编辑器动作
    pub fn show(&mut self, ctx: &egui::Context, app: &AppContext<'_>) -> GlamourEditorAction {
        // 在 UI 之前执行重建
        if self.needs_mesh_rebuild {
            self.rebuild_merged_meshes(app.items, app.item_id_map, app.game);
        }

        // 染色重烘焙
        if self.needs_rebake {
            self.needs_rebake = false;
            if let Some(stm) = app.stm {
                for slot in &ALL_SLOTS {
                    if self.glamour_set.get_slot(*slot).is_some() {
                        self.rebake_slot_textures(*slot, stm);
                    }
                }
            }
        }

        let mut action = GlamourEditorAction::None;

        // 左侧面板: 装备列表
        egui::SidePanel::left("glamour_equip_list")
            .default_width(300.0)
            .show(ctx, |ui| {
                ui.heading(format!("选择装备 - {}", self.active_slot.display_name()));
                ui.separator();

                ui.horizontal(|ui| {
                    ui.label("搜索:");
                    ui.text_edit_singleline(&mut self.search);
                });

                ui.horizontal(|ui| {
                    ui.label("排序:");
                    egui::ComboBox::from_id_salt("glamour_sort_order")
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
                });

                ui.separator();

                let slot = self.active_slot;
                let search_lower = self.search.to_lowercase();
                let mut filtered: Vec<(usize, &EquipmentItem)> = app
                    .items
                    .iter()
                    .enumerate()
                    .filter(|(_, item)| {
                        item.slot == slot
                            && (search_lower.is_empty()
                                || item.name.to_lowercase().contains(&search_lower))
                    })
                    .collect();
                match self.sort_order {
                    SortOrder::ByName => filtered.sort_by(|a, b| a.1.name.cmp(&b.1.name)),
                    SortOrder::BySetId => filtered.sort_by(|a, b| a.1.set_id.cmp(&b.1.set_id)),
                    SortOrder::BySlot => filtered.sort_by(|a, b| a.1.name.cmp(&b.1.name)),
                }

                ui.label(format!("{} 件", filtered.len()));

                let current_item_id = self.glamour_set.get_slot(slot).map(|s| s.item_id);

                egui::ScrollArea::vertical().show_rows(
                    ui,
                    18.0,
                    filtered.len(),
                    |ui, row_range| {
                        for row_idx in row_range {
                            if let Some(&(_global_idx, ref item)) = filtered.get(row_idx) {
                                let selected = current_item_id == Some(item.row_id);
                                let label = format!("[e{:04}] {}", item.set_id, item.name);
                                if ui.selectable_label(selected, &label).clicked() {
                                    self.assign_item_to_slot(slot, item);
                                }
                            }
                        }
                    },
                );
            });

        // 右侧面板: 装备信息 + 同套装链接 + 染色
        egui::SidePanel::right("glamour_info_panel")
            .default_width(250.0)
            .show(ctx, |ui| {
                let slot = self.active_slot;
                if let Some(gslot) = self.glamour_set.get_slot(slot) {
                    let item_id = gslot.item_id;
                    if let Some(&idx) = app.item_id_map.get(&item_id) {
                        if let Some(item) = app.items.get(idx) {
                            ui.heading(&item.name);
                            ui.label(format!("e{:04} v{:04}", item.set_id, item.variant_id));
                            ui.separator();

                            if ui.button("移除此槽位").clicked() {
                                self.glamour_set.remove_slot(slot);
                                self.needs_mesh_rebuild = true;
                                self.dirty = true;
                            }

                            // 同套装装备链接
                            if let Some(&set_idx) = app.set_id_to_set_idx.get(&item.set_id) {
                                let eq_set = &app.equipment_sets[set_idx];
                                if eq_set.item_indices.len() > 1 {
                                    ui.separator();
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "同套装 ({})",
                                            eq_set.display_name
                                        ))
                                        .strong(),
                                    );

                                    // 预计算兄弟装备数据
                                    let siblings: Vec<(usize, String, EquipSlot, bool)> = eq_set
                                        .item_indices
                                        .iter()
                                        .filter_map(|&i| {
                                            app.items.get(i).map(|sib| {
                                                let is_current = i == idx;
                                                let label = format!(
                                                    "[{}] {}",
                                                    sib.slot.slot_abbr(),
                                                    sib.name
                                                );
                                                (i, label, sib.slot, is_current)
                                            })
                                        })
                                        .collect();

                                    let mut clicked_sibling: Option<(usize, EquipSlot)> = None;
                                    ui.horizontal_wrapped(|ui| {
                                        for (sib_idx, sib_label, sib_slot, is_current) in &siblings
                                        {
                                            if *is_current {
                                                ui.label(
                                                    egui::RichText::new(sib_label)
                                                        .strong()
                                                        .underline(),
                                                );
                                            } else if ui.link(sib_label).clicked() {
                                                clicked_sibling = Some((*sib_idx, *sib_slot));
                                            }
                                        }
                                    });

                                    if let Some((sib_idx, sib_slot)) = clicked_sibling {
                                        if let Some(sib_item) = app.items.get(sib_idx) {
                                            self.assign_item_to_slot(sib_slot, sib_item);
                                            self.active_slot = sib_slot;
                                        }
                                    }

                                    // "填充整套"按钮
                                    if ui.button("填充整套").clicked() {
                                        for &sib_idx in &eq_set.item_indices {
                                            if let Some(sib_item) = app.items.get(sib_idx) {
                                                let sib_slot = sib_item.slot;
                                                if self.glamour_set.get_slot(sib_slot).is_none() {
                                                    let stain_ids = self
                                                        .selected_stain_ids
                                                        .get(&sib_slot)
                                                        .copied()
                                                        .unwrap_or([0, 0]);
                                                    self.glamour_set.set_slot(
                                                        sib_slot,
                                                        sib_item.row_id,
                                                        stain_ids,
                                                    );
                                                }
                                            }
                                        }
                                        self.needs_mesh_rebuild = true;
                                        self.dirty = true;
                                    }
                                }
                            }

                            ui.separator();

                            // 染色 UI
                            let has_dyeable = self
                                .slot_states
                                .get(&slot)
                                .map(|s| s.cached_materials.values().any(|m| m.uses_color_table))
                                .unwrap_or(false);

                            if has_dyeable {
                                let slot_stains = self
                                    .selected_stain_ids
                                    .get(&slot)
                                    .copied()
                                    .unwrap_or([0, 0]);
                                let ch = self.active_dye_channel;

                                let is_dual = self
                                    .slot_states
                                    .get(&slot)
                                    .map(|s| s.is_dual_dye)
                                    .unwrap_or(false);

                                if is_dual {
                                    ui.horizontal(|ui| {
                                        ui.label("通道:");
                                        ui.selectable_value(
                                            &mut self.active_dye_channel,
                                            0,
                                            "通道1",
                                        );
                                        ui.selectable_value(
                                            &mut self.active_dye_channel,
                                            1,
                                            "通道2",
                                        );
                                    });
                                }

                                ui.horizontal_wrapped(|ui| {
                                    for &shade in SHADE_ORDER {
                                        let label = shade_group_name(shade);
                                        if ui
                                            .selectable_label(self.selected_shade == shade, label)
                                            .clicked()
                                        {
                                            self.selected_shade = shade;
                                        }
                                    }
                                });

                                let stain_data: Vec<(u32, String, [u8; 3])> = app
                                    .stains
                                    .iter()
                                    .filter(|s| s.shade == self.selected_shade)
                                    .map(|s| (s.id, s.name.clone(), s.color))
                                    .collect();

                                let mut new_stain_id = None;
                                ui.horizontal_wrapped(|ui| {
                                    let no_dye_selected = slot_stains[ch] == 0;
                                    let (no_rect, no_resp) = ui.allocate_exact_size(
                                        egui::vec2(20.0, 20.0),
                                        egui::Sense::click(),
                                    );
                                    let no_bg = if no_dye_selected {
                                        egui::Color32::from_gray(180)
                                    } else {
                                        egui::Color32::from_gray(60)
                                    };
                                    ui.painter().rect_filled(no_rect, 2.0, no_bg);
                                    ui.painter().text(
                                        no_rect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        "✕",
                                        egui::FontId::proportional(12.0),
                                        egui::Color32::WHITE,
                                    );
                                    if no_dye_selected {
                                        ui.painter().rect_stroke(
                                            no_rect,
                                            2.0,
                                            egui::Stroke::new(2.0, egui::Color32::WHITE),
                                            egui::StrokeKind::Outside,
                                        );
                                    }
                                    if no_resp.clicked() {
                                        new_stain_id = Some(0u32);
                                    }
                                    no_resp.on_hover_text("无染料");

                                    for (id, name, color_rgb) in &stain_data {
                                        let color = egui::Color32::from_rgb(
                                            color_rgb[0],
                                            color_rgb[1],
                                            color_rgb[2],
                                        );
                                        let selected = slot_stains[ch] == *id;
                                        let (rect, resp) = ui.allocate_exact_size(
                                            egui::vec2(20.0, 20.0),
                                            egui::Sense::click(),
                                        );
                                        ui.painter().rect_filled(rect, 2.0, color);
                                        if selected {
                                            ui.painter().rect_stroke(
                                                rect,
                                                2.0,
                                                egui::Stroke::new(2.0, egui::Color32::WHITE),
                                                egui::StrokeKind::Outside,
                                            );
                                        }
                                        if resp.clicked() {
                                            new_stain_id = Some(*id);
                                        }
                                        resp.on_hover_text(name);
                                    }
                                });

                                // 当前选择显示
                                ui.horizontal(|ui| {
                                    if slot_stains[ch] == 0 {
                                        ui.label("当前: 无染料");
                                    } else if let Some(stain) =
                                        app.stains.iter().find(|s| s.id == slot_stains[ch])
                                    {
                                        let color = egui::Color32::from_rgb(
                                            stain.color[0],
                                            stain.color[1],
                                            stain.color[2],
                                        );
                                        let (rect, _) = ui.allocate_exact_size(
                                            egui::vec2(16.0, 16.0),
                                            egui::Sense::hover(),
                                        );
                                        ui.painter().rect_filled(rect, 2.0, color);
                                        ui.label(format!("当前: {}", stain.name));
                                    }
                                });

                                // 应用染色变更
                                if let Some(new_id) = new_stain_id {
                                    let entry =
                                        self.selected_stain_ids.entry(slot).or_insert([0, 0]);
                                    entry[ch] = new_id;
                                    if let Some(gslot) = self
                                        .glamour_set
                                        .slots
                                        .get_mut(crate::glamour::slot_key_for(slot))
                                    {
                                        gslot.stain_ids = *entry;
                                    }
                                    self.dirty = true;
                                    self.needs_rebake = true;
                                }
                            } else {
                                ui.label("此装备不支持染色");
                            }
                        }
                    }
                } else {
                    ui.label(format!("{}: 空", slot.display_name()));
                    ui.label("从左侧列表选择装备");
                }
            });

        // 中央面板: 槽位选择 + 3D 视口
        egui::CentralPanel::default().show(ctx, |ui| {
            // 标题栏
            ui.horizontal(|ui| {
                let title = if self.dirty {
                    format!("编辑: {} *", self.glamour_set.name)
                } else {
                    format!("编辑: {}", self.glamour_set.name)
                };
                ui.heading(&title);

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("返回").clicked() {
                        action = GlamourEditorAction::Close;
                    }
                    if ui.button("保存").clicked() {
                        action = GlamourEditorAction::Save;
                    }
                });
            });

            ui.separator();

            // 槽位选择横排
            ui.horizontal(|ui| {
                for slot in &ALL_SLOTS {
                    let has_item = self.glamour_set.get_slot(*slot).is_some();
                    let label = if has_item {
                        format!("{} ●", slot.display_name())
                    } else {
                        slot.display_name().to_string()
                    };
                    if ui
                        .selectable_label(self.active_slot == *slot, &label)
                        .clicked()
                    {
                        self.active_slot = *slot;
                    }
                }
            });

            ui.separator();

            // 3D 视口
            let available = ui.available_size();
            let vp_w = (available.x as u32).max(1);
            let vp_h = (available.y as u32).max(1);

            let (rect, response) = ui.allocate_exact_size(
                egui::vec2(vp_w as f32, vp_h as f32),
                egui::Sense::click_and_drag(),
            );

            if response.dragged_by(egui::PointerButton::Primary) {
                let delta = response.drag_delta();
                self.camera.yaw += delta.x * 0.01;
                self.camera.pitch = (self.camera.pitch + delta.y * 0.01).clamp(-1.5, 1.5);
            }
            if response.dragged_by(egui::PointerButton::Secondary) {
                let delta = response.drag_delta();
                self.camera.pan(delta.x, delta.y);
            }
            if response.double_clicked() {
                if let Some(bbox) = &self.last_bbox {
                    self.camera.focus_on(bbox);
                } else {
                    self.camera = Camera::default();
                }
            }
            if response.hovered() {
                let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                if scroll != 0.0 {
                    self.camera.distance = (self.camera.distance - scroll * 0.005).clamp(0.5, 20.0);
                }
            }

            if self.model_renderer.has_mesh() {
                self.model_renderer.render_offscreen(
                    &self.render_state.device,
                    &self.render_state.queue,
                    vp_w,
                    vp_h,
                    &self.camera,
                );

                if let Some(view) = self.model_renderer.color_view() {
                    let tid = match self.model_texture_id {
                        Some(tid) => {
                            self.render_state
                                .renderer
                                .write()
                                .update_egui_texture_from_wgpu_texture(
                                    &self.render_state.device,
                                    view,
                                    wgpu::FilterMode::Linear,
                                    tid,
                                );
                            tid
                        }
                        None => {
                            let tid = self.render_state.renderer.write().register_native_texture(
                                &self.render_state.device,
                                view,
                                wgpu::FilterMode::Linear,
                            );
                            self.model_texture_id = Some(tid);
                            tid
                        }
                    };

                    ui.painter().image(
                        tid,
                        rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                }

                ctx.request_repaint();

                ui.painter().text(
                    egui::pos2(rect.left() + 8.0, rect.bottom() - 8.0),
                    egui::Align2::LEFT_BOTTOM,
                    "左键旋转 | 右键平移 | 滚轮缩放 | 双击重置",
                    egui::FontId::proportional(12.0),
                    egui::Color32::from_rgba_premultiplied(180, 180, 180, 160),
                );
            } else {
                ui.painter()
                    .rect_filled(rect, 0.0, egui::Color32::from_rgb(30, 30, 36));
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "选择装备以预览",
                    egui::FontId::default(),
                    egui::Color32::GRAY,
                );
            }
        });

        action
    }

    fn assign_item_to_slot(&mut self, slot: EquipSlot, item: &EquipmentItem) {
        let stain_ids = self
            .selected_stain_ids
            .get(&slot)
            .copied()
            .unwrap_or([0, 0]);
        self.glamour_set.set_slot(slot, item.row_id, stain_ids);
        self.needs_mesh_rebuild = true;
        self.dirty = true;
    }
}
