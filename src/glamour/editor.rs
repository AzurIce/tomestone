use std::collections::{HashMap, HashSet};
use std::ops::Range;

use eframe::egui;
use physis::stm::StainingTemplate;

use super::GlamourSet;
use crate::domain::{
    EquipSlot, EquipmentItem, EquipmentSet, ACCESSORY_SLOTS, ALL_SLOTS, GEAR_SLOTS, RACE_CODES,
};
use crate::dye::{apply_dye, has_dual_dye};
use crate::game::{
    apply_skinning, bake_color_table_texture, compute_bounding_box, load_mdl, load_mesh_textures,
    CachedMaterial, GameData, MeshData, SkeletonCache,
};
use crate::ui::components::dye_palette::show_dye_palette;
use crate::ui::components::equipment_list::{EquipmentListState, HighlightConfig};
use crate::ui::components::viewport::ViewportState;

pub struct AppContext<'a> {
    pub items: &'a [EquipmentItem],
    pub item_id_map: &'a HashMap<u32, usize>,
    pub stains: &'a [crate::domain::StainEntry],
    pub stm: Option<&'a StainingTemplate>,
    pub game: &'a GameData,
    pub equipment_sets: &'a [EquipmentSet],
    pub set_id_to_set_idx: &'a HashMap<u16, usize>,
}

struct SlotState {
    loaded_item_id: Option<u32>,
    mesh_range: Range<usize>,
    cached_materials: HashMap<u16, CachedMaterial>,
    cached_meshes: Vec<MeshData>,
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

pub enum GlamourEditorAction {
    None,
    Save,
    Close,
}

pub struct GlamourEditor {
    pub glamour_set: GlamourSet,
    pub active_slot: EquipSlot,

    equipment_list: EquipmentListState,

    selected_stain_ids: HashMap<EquipSlot, [u32; 2]>,
    active_dye_channel: usize,
    selected_shade: u8,

    viewport: ViewportState,
    slot_states: HashMap<EquipSlot, SlotState>,
    needs_mesh_rebuild: bool,
    needs_rebake: bool,
    pub dirty: bool,

    skeleton_cache: SkeletonCache,

    // 右侧详情面板的单件预览视口
    detail_viewport: ViewportState,
    detail_loaded_item_id: Option<u32>,
    detail_cached_materials: HashMap<u16, CachedMaterial>,
    detail_cached_meshes: Vec<MeshData>,
    detail_needs_rebuild: bool,
    detail_needs_rebake: bool,

    // 预览状态: 点击左侧列表时设置，尚未装备
    preview_item_id: Option<u32>,
    preview_stain_ids: [u32; 2],
}

impl GlamourEditor {
    pub fn new(glamour_set: GlamourSet, render_state: egui_wgpu::RenderState) -> Self {
        let mut selected_stain_ids = HashMap::new();
        for slot in &ALL_SLOTS {
            let stain_ids = glamour_set
                .get_slot(*slot)
                .map(|gs| gs.stain_ids)
                .unwrap_or([0, 0]);
            selected_stain_ids.insert(*slot, stain_ids);
        }

        let detail_viewport = ViewportState::new(render_state.clone());

        Self {
            glamour_set,
            active_slot: EquipSlot::Body,
            equipment_list: EquipmentListState::new(),
            selected_stain_ids,
            active_dye_channel: 0,
            selected_shade: 2,
            viewport: ViewportState::new(render_state),
            slot_states: HashMap::new(),
            needs_mesh_rebuild: true,
            needs_rebake: false,
            dirty: false,
            skeleton_cache: SkeletonCache::new(),
            detail_viewport,
            detail_loaded_item_id: None,
            detail_cached_materials: HashMap::new(),
            detail_cached_meshes: Vec::new(),
            detail_needs_rebuild: false,
            detail_needs_rebake: false,
            preview_item_id: None,
            preview_stain_ids: [0, 0],
        }
    }

    fn rebuild_merged_meshes(
        &mut self,
        items: &[EquipmentItem],
        item_id_map: &HashMap<u32, usize>,
        game: &GameData,
    ) {
        self.needs_mesh_rebuild = false;

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

        let mut all_meshes: Vec<MeshData> = Vec::new();
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

            let unified_path = item.model_path_for_race(unified_race);
            let (load_result_mdl, actual_race) = match load_mdl(game, &unified_path) {
                Ok(result) if !result.meshes.is_empty() => (Some(result), unified_race.to_string()),
                _ => {
                    let mut found = (None, String::new());
                    for &rc in RACE_CODES {
                        let path = item.model_path_for_race(rc);
                        if let Ok(result) = load_mdl(game, &path) {
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
                    if actual_race != unified_race {
                        if let Some(target_bind) =
                            self.skeleton_cache.get_bind_pose(unified_race, game)
                        {
                            let target_bind = target_bind.clone();
                            if let Some(source_bind) =
                                self.skeleton_cache.get_bind_pose(&actual_race, game)
                            {
                                let source_bind = source_bind.clone();
                                apply_skinning(
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
                    let load_result = load_mesh_textures(
                        game,
                        &result.material_names,
                        &result.meshes,
                        item.set_id,
                        item.variant_id,
                    );
                    state.loaded_item_id = Some(item_id);
                    state.cached_materials = load_result.materials;
                    state.is_dual_dye = has_dual_dye(&state.cached_materials);
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

        let geometry: Vec<(&[tomestone_render::Vertex], &[u16])> = all_meshes
            .iter()
            .map(|m| (m.vertices.as_slice(), m.indices.as_slice()))
            .collect();
        self.viewport.model_renderer.set_mesh_data(
            &self.viewport.render_state.device,
            &self.viewport.render_state.queue,
            &geometry,
            &all_textures,
        );

        if !all_meshes.is_empty() {
            let bbox = compute_bounding_box(&all_meshes);
            self.viewport.camera.focus_on(&bbox);
            self.viewport.last_bbox = Some(bbox);
        } else {
            self.viewport.last_bbox = None;
        }

        self.viewport.free_texture();
    }

    fn rebake_slot_textures(&mut self, slot: EquipSlot, stm: &StainingTemplate) {
        let stain_ids = self
            .selected_stain_ids
            .get(&slot)
            .copied()
            .unwrap_or([0, 0]);
        let total_meshes = self.viewport.model_renderer.mesh_count();

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
                                Some(apply_dye(color_table, dye_table, stm, stain_ids))
                            } else {
                                None
                            }
                        } else {
                            None
                        };
                        let baked =
                            bake_color_table_texture(id_tex, color_table, dyed_colors.as_ref());
                        new_textures[global_idx] = Some(baked);
                    }
                }
            }
        }

        self.viewport.model_renderer.update_textures(
            &self.viewport.render_state.device,
            &self.viewport.render_state.queue,
            &new_textures,
        );
        self.viewport.mark_dirty();
    }

    fn rebuild_detail_viewport(&mut self, item: &EquipmentItem, game: &GameData) {
        self.detail_needs_rebuild = false;
        self.detail_loaded_item_id = Some(item.row_id);

        let paths = item.model_paths();
        let mut loaded = None;
        for path in &paths {
            if let Ok(result) = load_mdl(game, path) {
                if !result.meshes.is_empty() {
                    loaded = Some(result);
                    break;
                }
            }
        }

        match loaded {
            Some(result) => {
                let load_result = load_mesh_textures(
                    game,
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
                self.detail_viewport.model_renderer.set_mesh_data(
                    &self.detail_viewport.render_state.device,
                    &self.detail_viewport.render_state.queue,
                    &geometry,
                    &load_result.mesh_textures,
                );
                self.detail_cached_materials = load_result.materials;
                self.detail_cached_meshes = result.meshes;
                let bbox = compute_bounding_box(&self.detail_cached_meshes);
                self.detail_viewport.camera.focus_on(&bbox);
                self.detail_viewport.last_bbox = Some(bbox);
                self.detail_viewport.free_texture();
            }
            None => {
                self.detail_viewport.model_renderer.set_mesh_data(
                    &self.detail_viewport.render_state.device,
                    &self.detail_viewport.render_state.queue,
                    &[],
                    &[],
                );
                self.detail_cached_materials.clear();
                self.detail_cached_meshes.clear();
                self.detail_viewport.last_bbox = None;
            }
        }
    }

    fn rebake_detail_textures(&mut self, stm: &StainingTemplate) {
        self.detail_needs_rebake = false;

        // 预览模式使用 preview_stain_ids，已装备模式使用 selected_stain_ids
        let stain_ids = if self.preview_item_id.is_some() {
            self.preview_stain_ids
        } else {
            let slot = self.active_slot;
            self.selected_stain_ids
                .get(&slot)
                .copied()
                .unwrap_or([0, 0])
        };

        let mut new_textures: Vec<Option<tomestone_render::TextureData>> = Vec::new();
        for mesh in &self.detail_cached_meshes {
            let mat_idx = mesh.material_index;
            if let Some(cached) = self.detail_cached_materials.get(&mat_idx) {
                if cached.uses_color_table {
                    if let (Some(color_table), Some(id_tex)) =
                        (&cached.color_table, &cached.id_texture)
                    {
                        let dyed_colors = if stain_ids[0] > 0 || stain_ids[1] > 0 {
                            cached
                                .color_dye_table
                                .as_ref()
                                .map(|dye_table| apply_dye(color_table, dye_table, stm, stain_ids))
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
        self.detail_viewport.model_renderer.update_textures(
            &self.detail_viewport.render_state.device,
            &self.detail_viewport.render_state.queue,
            &new_textures,
        );
        self.detail_viewport.mark_dirty();
    }

    pub fn show(&mut self, ctx: &egui::Context, app: &AppContext<'_>) -> GlamourEditorAction {
        if self.needs_mesh_rebuild {
            self.rebuild_merged_meshes(app.items, app.item_id_map, app.game);
            self.detail_needs_rebuild = true;
        }

        if self.needs_rebake {
            self.needs_rebake = false;
            if let Some(stm) = app.stm {
                for slot in &ALL_SLOTS {
                    if self.glamour_set.get_slot(*slot).is_some() {
                        self.rebake_slot_textures(*slot, stm);
                    }
                }
            }
            self.detail_needs_rebake = true;
        }

        // 更新详情视口: 优先显示预览物品，否则显示当前槽位已装备物品
        let detail_target_id = self.preview_item_id.or_else(|| {
            self.glamour_set
                .get_slot(self.active_slot)
                .map(|s| s.item_id)
        });
        if self.detail_needs_rebuild || self.detail_loaded_item_id != detail_target_id {
            if let Some(item_id) = detail_target_id {
                if let Some(&idx) = app.item_id_map.get(&item_id) {
                    if let Some(item) = app.items.get(idx) {
                        self.rebuild_detail_viewport(item, app.game);
                    }
                }
            } else {
                self.detail_loaded_item_id = None;
                self.detail_viewport.model_renderer.set_mesh_data(
                    &self.detail_viewport.render_state.device,
                    &self.detail_viewport.render_state.queue,
                    &[],
                    &[],
                );
                self.detail_cached_materials.clear();
                self.detail_cached_meshes.clear();
                self.detail_viewport.last_bbox = None;
                self.detail_needs_rebuild = false;
            }
        }

        if self.detail_needs_rebake {
            self.detail_needs_rebake = false;
            if let Some(stm) = app.stm {
                self.rebake_detail_textures(stm);
            }
        }

        let mut action = GlamourEditorAction::None;

        // ── 左侧: 装备列表 (不按槽位筛选) ──
        egui::SidePanel::left("glamour_equip_list")
            .default_width(300.0)
            .show(ctx, |ui| {
                ui.heading("选择装备");
                ui.separator();

                // 收集所有已装备的 item_id 用于高亮
                let equipped_ids: HashSet<u32> = ALL_SLOTS
                    .iter()
                    .filter_map(|slot| self.glamour_set.get_slot(*slot).map(|s| s.item_id))
                    .collect();

                let highlight = HighlightConfig {
                    highlighted_ids: &equipped_ids,
                    preview_id: self.preview_item_id,
                };

                if let Some(clicked) = self.equipment_list.show(
                    ui,
                    app.items,
                    app.equipment_sets,
                    app.set_id_to_set_idx,
                    None, // 不按槽位筛选
                    &highlight,
                    "glamour",
                ) {
                    self.preview_item_id = Some(clicked.item_id);
                    self.preview_stain_ids = [0, 0];
                    self.active_slot = clicked.slot;
                }
            });

        // ── 右侧: 当前槽位装备详情 ──
        egui::SidePanel::right("glamour_info_panel")
            .default_width(280.0)
            .show(ctx, |ui| {
                let slot = self.active_slot;
                ui.heading(slot.display_name());
                ui.separator();

                if let Some(preview_id) = self.preview_item_id {
                    // ── 预览模式 ──
                    if let Some(&idx) = app.item_id_map.get(&preview_id) {
                        if let Some(item) = app.items.get(idx) {
                            ui.label(
                                egui::RichText::new("预览")
                                    .color(egui::Color32::from_rgb(100, 200, 255)),
                            );
                            ui.label(egui::RichText::new(&item.name).strong().size(14.0));
                            let prefix = if item.is_accessory() { "a" } else { "e" };
                            ui.label(format!(
                                "{}{:04} v{:04}",
                                prefix, item.set_id, item.variant_id
                            ));

                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                if ui.button("装备").clicked() {
                                    // 将预览物品装备到对应槽位
                                    let target_slot = item.slot;
                                    let stain_ids = self.preview_stain_ids;
                                    self.glamour_set
                                        .set_slot(target_slot, item.row_id, stain_ids);
                                    self.selected_stain_ids.insert(target_slot, stain_ids);
                                    self.needs_mesh_rebuild = true;
                                    self.dirty = true;
                                    self.preview_item_id = None;
                                    self.preview_stain_ids = [0, 0];
                                }
                                if ui.button("取消").clicked() {
                                    self.preview_item_id = None;
                                    self.preview_stain_ids = [0, 0];
                                }
                            });

                            // 预览染色控制 (临时，使用 preview_stain_ids)
                            ui.separator();
                            let has_dyeable = !self.detail_cached_materials.is_empty()
                                && self
                                    .detail_cached_materials
                                    .values()
                                    .any(|m| m.uses_color_table);

                            if has_dyeable {
                                let is_dual = has_dual_dye(&self.detail_cached_materials);
                                let changed = show_dye_palette(
                                    ui,
                                    app.stains,
                                    &mut self.preview_stain_ids,
                                    &mut self.active_dye_channel,
                                    &mut self.selected_shade,
                                    is_dual,
                                );
                                if changed {
                                    self.detail_needs_rebake = true;
                                }
                            } else {
                                ui.label("此装备不支持染色");
                            }

                            // 单件预览视口
                            ui.separator();
                            ui.label(egui::RichText::new("单件预览").strong());
                            let vp_size = ui.available_height().max(150.0).min(250.0);
                            ui.allocate_ui(egui::vec2(ui.available_width(), vp_size), |ui| {
                                self.detail_viewport.show(ui, ctx, "");
                            });
                        }
                    }
                } else if let Some(gslot) = self.glamour_set.get_slot(slot) {
                    // ── 已装备模式 ──
                    let item_id = gslot.item_id;
                    if let Some(&idx) = app.item_id_map.get(&item_id) {
                        if let Some(item) = app.items.get(idx) {
                            ui.label(egui::RichText::new(&item.name).strong().size(14.0));
                            let prefix = if item.is_accessory() { "a" } else { "e" };
                            ui.label(format!(
                                "{}{:04} v{:04}",
                                prefix, item.set_id, item.variant_id
                            ));

                            ui.add_space(4.0);
                            if ui.button("卸下").clicked() {
                                self.glamour_set.remove_slot(slot);
                                self.needs_mesh_rebuild = true;
                                self.detail_needs_rebuild = true;
                                self.dirty = true;
                            }

                            // 同套装快捷操作
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
                                            // 同套装兄弟件也走预览流程
                                            self.preview_item_id = Some(sib_item.row_id);
                                            self.preview_stain_ids = [0, 0];
                                            self.active_slot = sib_slot;
                                        }
                                    }

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

                            // 染色控制 (持久化到 glamour set)
                            ui.separator();

                            let has_dyeable = self
                                .slot_states
                                .get(&slot)
                                .map(|s| s.cached_materials.values().any(|m| m.uses_color_table))
                                .unwrap_or(false);

                            if has_dyeable {
                                let is_dual = self
                                    .slot_states
                                    .get(&slot)
                                    .map(|s| s.is_dual_dye)
                                    .unwrap_or(false);
                                let slot_stains =
                                    self.selected_stain_ids.entry(slot).or_insert([0, 0]);
                                let changed = show_dye_palette(
                                    ui,
                                    app.stains,
                                    slot_stains,
                                    &mut self.active_dye_channel,
                                    &mut self.selected_shade,
                                    is_dual,
                                );
                                if changed {
                                    let stain_ids = *slot_stains;
                                    if let Some(gslot) =
                                        self.glamour_set.slots.get_mut(super::slot_key_for(slot))
                                    {
                                        gslot.stain_ids = stain_ids;
                                    }
                                    self.dirty = true;
                                    self.needs_rebake = true;
                                    self.detail_needs_rebake = true;
                                }
                            } else {
                                ui.label("此装备不支持染色");
                            }

                            // 单件装备预览视口
                            ui.separator();
                            ui.label(egui::RichText::new("单件预览").strong());
                            let vp_size = ui.available_height().max(150.0).min(250.0);
                            ui.allocate_ui(egui::vec2(ui.available_width(), vp_size), |ui| {
                                self.detail_viewport.show(ui, ctx, "");
                            });
                        }
                    }
                } else {
                    // ── 空槽位模式 ──
                    ui.label(format!("{}: 空", slot.display_name()));
                    ui.add_space(8.0);
                    ui.label("从左侧列表选择装备");
                }
            });

        // ── 中央: 标题栏 + 槽位选择 + 合并预览视口 ──
        egui::CentralPanel::default().show(ctx, |ui| {
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

            ui.horizontal(|ui| {
                ui.label("装备:");
                for slot in &GEAR_SLOTS {
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
                        self.preview_item_id = None;
                        self.preview_stain_ids = [0, 0];
                    }
                }
            });

            ui.horizontal(|ui| {
                ui.label("饰品:");
                for slot in &ACCESSORY_SLOTS {
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
                        self.preview_item_id = None;
                        self.preview_stain_ids = [0, 0];
                    }
                }
            });

            ui.separator();

            self.viewport.show(ui, ctx, "选择装备以预览");
        });

        action
    }
}
