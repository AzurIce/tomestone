mod dye;
mod game_data;
mod mdl_loader;
mod renderer;
mod tex_loader;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use eframe::egui;
use egui_wgpu::wgpu;
use game_data::{EquipSlot, EquipmentItem, GameData, StainEntry};
use mdl_loader::BoundingBox;
use physis::stm::StainingTemplate;
use renderer::{Camera, ModelRenderer};
use tex_loader::CachedMaterial;

// ── 视图模式 & 排序 ──

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViewMode {
    List,
    SetGroup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SortOrder {
    ByName,
    BySetId,
    BySlot,
}

impl SortOrder {
    fn label(&self) -> &'static str {
        match self {
            Self::ByName => "按名称",
            Self::BySetId => "按套装",
            Self::BySlot => "按槽位",
        }
    }
}

// ── 套装分组 ──

struct EquipmentSet {
    set_id: u16,
    display_name: String,
    item_indices: Vec<usize>,
    slots: Vec<EquipSlot>,
}

#[derive(Clone)]
enum FlatRow {
    GroupHeader {
        set_id: u16,
        display_name: String,
        item_count: usize,
        expanded: bool,
    },
    Item {
        global_idx: usize,
        label: String,
    },
}

fn longest_common_prefix(strings: &[&str]) -> String {
    if strings.is_empty() {
        return String::new();
    }
    let first = strings[0];
    let mut len = first.len();
    for s in &strings[1..] {
        len = len.min(s.len());
        for (i, (a, b)) in first.bytes().zip(s.bytes()).enumerate() {
            if a != b {
                len = len.min(i);
                break;
            }
        }
    }
    // 回退到最后一个完整 UTF-8 字符边界
    let prefix = &first[..len];
    // 去掉尾部不完整字符（保证 UTF-8 安全）
    let trimmed = prefix.trim_end();
    trimmed.to_string()
}

fn derive_set_name(items: &[EquipmentItem], indices: &[usize]) -> String {
    let names: Vec<&str> = indices.iter().map(|&i| items[i].name.as_str()).collect();
    let prefix = longest_common_prefix(&names);
    if prefix.is_empty() {
        // 没有公共前缀，使用第一个物品名
        if let Some(&idx) = indices.first() {
            return items[idx].name.clone();
        }
        return String::new();
    }
    prefix
}

fn build_equipment_sets(items: &[EquipmentItem]) -> Vec<EquipmentSet> {
    let mut by_set: BTreeMap<u16, Vec<usize>> = BTreeMap::new();
    for (i, item) in items.iter().enumerate() {
        by_set.entry(item.set_id).or_default().push(i);
    }
    by_set
        .into_iter()
        .map(|(set_id, item_indices)| {
            let display_name = derive_set_name(items, &item_indices);
            let slots: Vec<EquipSlot> = item_indices
                .iter()
                .map(|&i| items[i].slot)
                .collect::<HashSet<_>>()
                .into_iter()
                .collect();
            EquipmentSet {
                set_id,
                display_name,
                item_indices,
                slots,
            }
        })
        .collect()
}

const ALL_SLOTS: [EquipSlot; 5] = [
    EquipSlot::Head,
    EquipSlot::Body,
    EquipSlot::Gloves,
    EquipSlot::Legs,
    EquipSlot::Feet,
];

struct App {
    items: Vec<EquipmentItem>,
    search: String,
    selected_slot: Option<EquipSlot>,
    selected_item: Option<usize>,
    // 视图模式 & 排序
    view_mode: ViewMode,
    sort_order: SortOrder,
    // 套装分组
    equipment_sets: Vec<EquipmentSet>,
    set_id_to_set_idx: HashMap<u16, usize>,
    expanded_sets: HashSet<u16>,
    cached_flat_rows: Vec<FlatRow>,
    flat_rows_dirty: bool,
    // 3D 渲染
    game: GameData,
    render_state: egui_wgpu::RenderState,
    model_renderer: ModelRenderer,
    camera: Camera,
    model_texture_id: Option<egui::TextureId>,
    loaded_model_idx: Option<usize>,
    last_bbox: Option<BoundingBox>,
    // 染色缓存
    cached_materials: HashMap<u16, CachedMaterial>,
    cached_meshes: Vec<mdl_loader::MeshData>,
    // 染色系统
    stains: Vec<StainEntry>,
    stm: Option<StainingTemplate>,
    selected_stain_id: u32,
    needs_rebake: bool,
}

impl App {
    fn new(
        game: GameData,
        items: Vec<EquipmentItem>,
        stains: Vec<StainEntry>,
        stm: Option<StainingTemplate>,
        render_state: egui_wgpu::RenderState,
    ) -> Self {
        let model_renderer = ModelRenderer::new(&render_state.device);
        let equipment_sets = build_equipment_sets(&items);
        let set_id_to_set_idx: HashMap<u16, usize> = equipment_sets
            .iter()
            .enumerate()
            .map(|(i, s)| (s.set_id, i))
            .collect();
        Self {
            items,
            search: String::new(),
            selected_slot: None,
            selected_item: None,
            view_mode: ViewMode::List,
            sort_order: SortOrder::ByName,
            equipment_sets,
            set_id_to_set_idx,
            expanded_sets: HashSet::new(),
            cached_flat_rows: Vec::new(),
            flat_rows_dirty: true,
            game,
            render_state,
            model_renderer,
            camera: Camera::default(),
            model_texture_id: None,
            loaded_model_idx: None,
            last_bbox: None,
            cached_materials: HashMap::new(),
            cached_meshes: Vec::new(),
            stains,
            stm,
            selected_stain_id: 0,
            needs_rebake: false,
        }
    }

    fn filtered_and_sorted_items(&self) -> Vec<(usize, &EquipmentItem)> {
        let mut result: Vec<(usize, &EquipmentItem)> = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                if let Some(slot) = self.selected_slot {
                    if item.slot != slot {
                        return false;
                    }
                }
                if !self.search.is_empty() {
                    let search_lower = self.search.to_lowercase();
                    if !item.name.to_lowercase().contains(&search_lower) {
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

    fn item_matches_filter(&self, item: &EquipmentItem) -> bool {
        if let Some(slot) = self.selected_slot {
            if item.slot != slot {
                return false;
            }
        }
        if !self.search.is_empty() {
            let search_lower = self.search.to_lowercase();
            if !item.name.to_lowercase().contains(&search_lower) {
                return false;
            }
        }
        true
    }

    fn build_flat_rows(&mut self) {
        self.flat_rows_dirty = false;
        self.cached_flat_rows.clear();

        // 收集并排序套装
        let mut sets_with_items: Vec<(usize, Vec<usize>)> = Vec::new();
        for (set_idx, eq_set) in self.equipment_sets.iter().enumerate() {
            let filtered: Vec<usize> = eq_set
                .item_indices
                .iter()
                .copied()
                .filter(|&i| self.item_matches_filter(&self.items[i]))
                .collect();
            if !filtered.is_empty() {
                sets_with_items.push((set_idx, filtered));
            }
        }

        // 按 sort_order 排序套装
        match self.sort_order {
            SortOrder::ByName => {
                sets_with_items.sort_by(|a, b| {
                    self.equipment_sets[a.0]
                        .display_name
                        .cmp(&self.equipment_sets[b.0].display_name)
                });
            }
            SortOrder::BySetId => {
                sets_with_items.sort_by(|a, b| {
                    self.equipment_sets[a.0]
                        .set_id
                        .cmp(&self.equipment_sets[b.0].set_id)
                });
            }
            SortOrder::BySlot => {
                sets_with_items.sort_by(|a, b| {
                    self.equipment_sets[a.0]
                        .display_name
                        .cmp(&self.equipment_sets[b.0].display_name)
                });
            }
        }

        for (set_idx, filtered_indices) in sets_with_items {
            let eq_set = &self.equipment_sets[set_idx];
            let expanded = self.expanded_sets.contains(&eq_set.set_id);
            self.cached_flat_rows.push(FlatRow::GroupHeader {
                set_id: eq_set.set_id,
                display_name: eq_set.display_name.clone(),
                item_count: filtered_indices.len(),
                expanded,
            });
            if expanded {
                for &global_idx in &filtered_indices {
                    let item = &self.items[global_idx];
                    self.cached_flat_rows.push(FlatRow::Item {
                        global_idx,
                        label: format!("[{}] {}", item.slot.slot_abbr(), item.name),
                    });
                }
            }
        }
    }

    /// 重新烘焙所有使用 ColorTable 的纹理（染色后调用）
    fn rebake_textures(&mut self) {
        let stm = match &self.stm {
            Some(s) => s,
            None => return,
        };

        // 为每个 mesh 生成新纹理 (None = 不更新)
        let mut new_textures: Vec<Option<tex_loader::TextureData>> = Vec::new();

        for mesh in &self.cached_meshes {
            let mat_idx = mesh.material_index;
            if let Some(cached) = self.cached_materials.get(&mat_idx) {
                if cached.uses_color_table {
                    if let (Some(color_table), Some(id_tex)) =
                        (&cached.color_table, &cached.id_texture)
                    {
                        let dyed_colors = if self.selected_stain_id > 0 {
                            if let Some(dye_table) = &cached.color_dye_table {
                                Some(dye::apply_dye(
                                    color_table,
                                    dye_table,
                                    stm,
                                    self.selected_stain_id,
                                ))
                            } else {
                                None
                            }
                        } else {
                            None
                        };
                        let baked = tex_loader::bake_color_table_texture(
                            id_tex,
                            color_table,
                            dyed_colors.as_ref(),
                        );
                        new_textures.push(Some(baked));
                        continue;
                    }
                }
            }
            // 非 ColorTable 材质 → 不更新
            new_textures.push(None);
        }

        self.model_renderer.update_textures(
            &self.render_state.device,
            &self.render_state.queue,
            &new_textures,
        );
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 染色重烘焙 (在 UI 借用之前执行)
        if self.needs_rebake {
            self.needs_rebake = false;
            self.rebake_textures();
        }

        // 左侧面板: 装备列表
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

                // 搜索框
                let prev_search = self.search.clone();
                ui.horizontal(|ui| {
                    ui.label("搜索:");
                    ui.text_edit_singleline(&mut self.search);
                });
                if self.search != prev_search {
                    self.flat_rows_dirty = true;
                }

                // 排序
                let prev_sort = self.sort_order;
                ui.horizontal(|ui| {
                    ui.label("排序:");
                    egui::ComboBox::from_id_salt("sort_order")
                        .selected_text(self.sort_order.label())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.sort_order, SortOrder::ByName, SortOrder::ByName.label());
                            ui.selectable_value(&mut self.sort_order, SortOrder::BySetId, SortOrder::BySetId.label());
                            ui.selectable_value(&mut self.sort_order, SortOrder::BySlot, SortOrder::BySlot.label());
                        });
                });
                if self.sort_order != prev_sort {
                    self.flat_rows_dirty = true;
                }

                // 槽位过滤
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
                        // ── 列表视图 ──
                        let filtered: Vec<(usize, String)> = self
                            .filtered_and_sorted_items()
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
                                        let selected =
                                            self.selected_item == Some(*global_idx);
                                        if ui.selectable_label(selected, label).clicked() {
                                            self.selected_item = Some(*global_idx);
                                        }
                                    }
                                }
                            },
                        );
                    }
                    ViewMode::SetGroup => {
                        // ── 套装视图 ──
                        if self.flat_rows_dirty {
                            self.build_flat_rows();
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
                        ui.label(format!("{} 组套装, {} 件装备", num_sets, num_items));

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
                                                let resp = ui.selectable_label(
                                                    false,
                                                    egui::RichText::new(&text).strong(),
                                                );
                                                if resp.clicked() {
                                                    toggle_set = Some(*set_id);
                                                }
                                            }
                                            FlatRow::Item { global_idx, label } => {
                                                ui.horizontal(|ui| {
                                                    ui.add_space(16.0);
                                                    let selected = self.selected_item
                                                        == Some(*global_idx);
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

                        // 闭包外处理状态变更
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

        // 中央面板: 装备详情 + 3D 预览
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(idx) = self.selected_item {
                if let Some(item) = self.items.get(idx) {
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

                    ui.separator();

                    // 染料选择器
                    let has_dyeable = self.cached_materials.values().any(|m| m.uses_color_table);
                    if has_dyeable {
                        ui.horizontal(|ui| {
                            ui.label("染料:");
                            let prev_stain = self.selected_stain_id;
                            let current_label = if self.selected_stain_id == 0 {
                                "无染料".to_string()
                            } else {
                                self.stains
                                    .iter()
                                    .find(|s| s.id == self.selected_stain_id)
                                    .map(|s| s.name.clone())
                                    .unwrap_or_else(|| format!("染料 #{}", self.selected_stain_id))
                            };
                            egui::ComboBox::from_id_salt("dye_selector")
                                .selected_text(&current_label)
                                .width(200.0)
                                .show_ui(ui, |ui| {
                                    // 无染料选项
                                    ui.selectable_value(&mut self.selected_stain_id, 0, "无染料");
                                    ui.separator();
                                    // 染料列表
                                    for stain in &self.stains {
                                        let color = egui::Color32::from_rgb(
                                            stain.color[0],
                                            stain.color[1],
                                            stain.color[2],
                                        );
                                        ui.horizontal(|ui| {
                                            // 色块预览
                                            let (rect, _) = ui.allocate_exact_size(
                                                egui::vec2(14.0, 14.0),
                                                egui::Sense::hover(),
                                            );
                                            ui.painter().rect_filled(rect, 2.0, color);
                                            // 染料名
                                            if ui
                                                .selectable_value(
                                                    &mut self.selected_stain_id,
                                                    stain.id,
                                                    &stain.name,
                                                )
                                                .clicked()
                                            {}
                                        });
                                    }
                                });
                            // 当前色块
                            if self.selected_stain_id > 0 {
                                if let Some(stain) = self.stains.iter().find(|s| s.id == self.selected_stain_id) {
                                    let color = egui::Color32::from_rgb(
                                        stain.color[0],
                                        stain.color[1],
                                        stain.color[2],
                                    );
                                    let (rect, _) = ui.allocate_exact_size(
                                        egui::vec2(20.0, 20.0),
                                        egui::Sense::hover(),
                                    );
                                    ui.painter().rect_filled(rect, 3.0, color);
                                }
                            }
                            if self.selected_stain_id != prev_stain {
                                self.needs_rebake = true;
                            }
                        });
                    }

                    // 染色重烘焙 (标记在下方 UI 中设置，延迟到下一帧执行)

                    // 加载模型 (选中新装备时)
                    if self.loaded_model_idx != Some(idx) {
                        self.loaded_model_idx = Some(idx);
                        self.selected_stain_id = 0; // 重置染料选择
                        let paths = item.model_paths();
                        match mdl_loader::load_mdl_with_fallback(&self.game, &paths) {
                            Ok(result) if !result.meshes.is_empty() => {
                                let bbox = mdl_loader::compute_bounding_box(&result.meshes);
                                println!("加载纹理: {} 个材质, {} 个网格", result.material_names.len(), result.meshes.len());
                                let load_result = tex_loader::load_mesh_textures(
                                    &self.game,
                                    &result.material_names,
                                    &result.meshes,
                                    item.set_id,
                                    item.variant_id,
                                );
                                self.model_renderer.set_mesh_data(
                                    &self.render_state.device,
                                    &self.render_state.queue,
                                    &result.meshes,
                                    &load_result.textures,
                                );
                                self.cached_materials = load_result.materials;
                                self.cached_meshes = result.meshes.clone();
                                self.camera.focus_on(&bbox);
                                self.last_bbox = Some(bbox);
                                if let Some(tid) = self.model_texture_id.take() {
                                    self.render_state.renderer.write().free_texture(&tid);
                                }
                            }
                            _ => {
                                eprintln!("模型加载失败 e{:04} v{:04}: {:?}",
                                    item.set_id, item.variant_id,
                                    mdl_loader::load_mdl_with_fallback(&self.game, &paths).err());
                                self.model_renderer.set_mesh_data(
                                    &self.render_state.device,
                                    &self.render_state.queue,
                                    &[],
                                    &[],
                                );
                                self.last_bbox = None;
                            }
                        }
                    }

                    // 3D 视口
                    let available = ui.available_size();
                    let vp_w = (available.x as u32).max(1);
                    let vp_h = (available.y as u32).max(1);

                    // 鼠标交互: 拖拽旋转 + 滚轮缩放
                    let (rect, response) = ui.allocate_exact_size(
                        egui::vec2(vp_w as f32, vp_h as f32),
                        egui::Sense::click_and_drag(),
                    );

                    if response.dragged_by(egui::PointerButton::Primary) {
                        let delta = response.drag_delta();
                        self.camera.yaw += delta.x * 0.01;
                        self.camera.pitch = (self.camera.pitch + delta.y * 0.01)
                            .clamp(-1.5, 1.5);
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

                    // 离屏渲染
                    if self.model_renderer.has_mesh() {
                        self.model_renderer.render_offscreen(
                            &self.render_state.device,
                            &self.render_state.queue,
                            vp_w,
                            vp_h,
                            &self.camera,
                        );

                        // 注册/更新 egui 纹理
                        if let Some(view) = self.model_renderer.color_view() {
                            let tid = match self.model_texture_id {
                                Some(tid) => {
                                    self.render_state.renderer.write().update_egui_texture_from_wgpu_texture(
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

                        // 操作提示
                        ui.painter().text(
                            egui::pos2(rect.left() + 8.0, rect.bottom() - 8.0),
                            egui::Align2::LEFT_BOTTOM,
                            "左键旋转 | 右键平移 | 滚轮缩放 | 双击重置",
                            egui::FontId::proportional(12.0),
                            egui::Color32::from_rgba_premultiplied(180, 180, 180, 160),
                        );
                    } else {
                        ui.painter().rect_filled(rect, 0.0, egui::Color32::from_rgb(30, 30, 36));
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "模型加载失败",
                            egui::FontId::default(),
                            egui::Color32::GRAY,
                        );
                    }
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
}

fn main() {
    let install_dir = Path::new(r"G:\最终幻想XIV");

    println!("正在加载游戏数据...");
    let game = GameData::new(install_dir);

    println!("正在加载装备列表...");
    let items = game.load_equipment_list();
    println!("共加载 {} 件装备", items.len());

    println!("正在加载染料列表...");
    let stains = game.load_stain_list();
    println!("共加载 {} 种染料", stains.len());

    println!("正在加载染色模板...");
    let stm = game.load_staining_template();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 700.0])
            .with_title("FF14 装备浏览器"),
        ..Default::default()
    };

    eframe::run_native(
        "ff-tools",
        options,
        Box::new(|cc| {
            setup_fonts(cc);
            let render_state = cc.wgpu_render_state.as_ref()
                .expect("需要 wgpu 后端")
                .clone();
            Ok(Box::new(App::new(game, items, stains, stm, render_state)))
        }),
    )
    .unwrap();
}

fn setup_fonts(cc: &eframe::CreationContext) {
    // Support Chinese
    let mut fonts = egui::FontDefinitions::default();

    fonts.font_data.insert(
        "Harmony OS Sans".to_string(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/HarmonyOS_Sans_SC_Regular.ttf"
        ))),
    );

    // Put my font first (highest priority):
    // fonts
    //     .families
    //     .get_mut(&FontFamily::Proportional)
    //     .unwrap()
    //     .insert(0, "Harmony OS Sans".to_owned());

    // Put my font as last fallback:
    fonts
        .families
        .get_mut(&egui::FontFamily::Proportional)
        .unwrap()
        .push("Harmony OS Sans".to_owned());
    fonts
        .families
        .get_mut(&egui::FontFamily::Monospace)
        .unwrap()
        .push("Harmony OS Sans".to_owned());
    cc.egui_ctx.set_fonts(fonts);
}
