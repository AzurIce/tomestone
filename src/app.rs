use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;

use eframe::egui;

use crate::config;
use crate::domain::EquipSlot;
use crate::domain::ExteriorPartType;
use crate::domain::SourceChoice;
use crate::domain::ViewMode;
use crate::game::{CachedMaterial, GameData, MeshData};
use crate::glamour;
use crate::loading::*;
use crate::ui::components::equipment_list::EquipmentListState;
use crate::ui::components::item_list::ItemListState;
use crate::ui::components::viewport::ViewportState;
use crate::ui::components::{show_progress_bar, ProgressTracker};

pub enum AppPhase {
    Setup {
        dir_input: String,
        error: Option<String>,
    },
    Loading {
        status: String,
        receiver: Receiver<LoadProgress>,
    },
    Ready,
}

pub struct App {
    pub phase: AppPhase,
    pub config: config::AppConfig,
    pub render_state: egui_wgpu::RenderState,
    pub viewport: ViewportState,
    pub game_state: Option<GameState>,
    pub current_page: crate::domain::AppPage,
    pub equipment_list: EquipmentListState,
    pub selected_slot: Option<EquipSlot>,
    pub selected_item: Option<usize>,
    pub cached_materials: HashMap<u16, CachedMaterial>,
    pub cached_meshes: Vec<MeshData>,
    pub loaded_model_idx: Option<usize>,
    pub selected_stain_ids: [u32; 2],
    pub active_dye_channel: usize,
    pub selected_shade: u8,
    pub is_dual_dye: bool,
    pub needs_rebake: bool,
    pub new_glamour_name: String,
    pub renaming_glamour_idx: Option<usize>,
    pub rename_buffer: String,
    pub glamour_editor: Option<glamour::GlamourEditor>,
    pub editing_glamour_idx: Option<usize>,
    pub test_progress: ProgressTracker,
    pub test_total: u64,
    pub test_current: u64,
    pub icon_cache: HashMap<u32, Option<egui::TextureHandle>>,
    // 房屋外装浏览器状态
    pub housing_viewport: ViewportState,
    pub housing_selected_part_type: Option<ExteriorPartType>,
    pub housing_selected_item: Option<usize>,
    pub housing_loaded_model_idx: Option<usize>,
    pub housing_list: ItemListState,
    // 合成检索状态
    pub crafting_list: ItemListState,
    pub crafting_selected_craft_type: Option<u8>,
    pub crafting_selected_item: Option<usize>,
    pub crafting_selected_node_item: Option<u32>,
    pub crafting_selected_node_amount: u32,
    /// 用户对素材来源的手动选择 (item_id -> SourceChoice)
    pub crafting_source_overrides: HashMap<u32, SourceChoice>,
    // 工具箱: 自动制作
    pub auto_craft: crate::ui::pages::toolbox::AutoCraftUi,
    // 工具箱: 模板编辑器
    pub template_editor: crate::ui::components::template_editor::TemplateEditorState,
}

impl App {
    pub fn new(render_state: egui_wgpu::RenderState) -> Self {
        let config = config::load_config();
        let viewport = ViewportState::new(render_state.clone());
        let housing_viewport = ViewportState::new(render_state.clone());

        let phase = if let Some(dir) = &config.game_install_dir {
            let (tx, rx) = std::sync::mpsc::channel();
            let dir = dir.clone();
            std::thread::spawn(move || {
                load_game_data_thread(dir, tx);
            });
            AppPhase::Loading {
                status: "正在初始化...".to_string(),
                receiver: rx,
            }
        } else {
            AppPhase::Setup {
                dir_input: String::new(),
                error: None,
            }
        };

        Self {
            phase,
            config,
            render_state,
            viewport,
            game_state: None,
            current_page: crate::domain::AppPage::Browser,
            equipment_list: EquipmentListState::new(),
            selected_slot: None,
            selected_item: None,
            loaded_model_idx: None,
            cached_materials: HashMap::new(),
            cached_meshes: Vec::new(),
            selected_stain_ids: [0, 0],
            active_dye_channel: 0,
            selected_shade: 2,
            is_dual_dye: false,
            needs_rebake: false,
            new_glamour_name: String::new(),
            renaming_glamour_idx: None,
            rename_buffer: String::new(),
            glamour_editor: None,
            editing_glamour_idx: None,
            test_progress: ProgressTracker::new(),
            test_total: 100,
            test_current: 0,
            icon_cache: HashMap::new(),
            housing_viewport,
            housing_selected_part_type: None,
            housing_selected_item: None,
            housing_loaded_model_idx: None,
            housing_list: ItemListState::new(ViewMode::Grid),
            crafting_list: ItemListState::new(ViewMode::List),
            crafting_selected_craft_type: None,
            crafting_selected_item: None,
            crafting_selected_node_item: None,
            crafting_selected_node_amount: 0,
            crafting_source_overrides: HashMap::new(),
            auto_craft: Default::default(),
            template_editor: Default::default(),
        }
    }

    pub fn get_or_load_icon(
        &mut self,
        ctx: &egui::Context,
        gs: &GameData,
        icon_id: u32,
    ) -> Option<egui::TextureHandle> {
        if icon_id == 0 {
            return None;
        }

        if let Some(cached) = self.icon_cache.get(&icon_id) {
            return cached.clone();
        }

        let result = gs.load_icon(icon_id).map(|tex_data| {
            let size = [tex_data.width as _, tex_data.height as _];
            let pixels: Vec<egui::Color32> = tex_data
                .rgba
                .chunks_exact(4)
                .map(|p| egui::Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3]))
                .collect();

            let color_image = egui::ColorImage {
                size,
                pixels,
                source_size: egui::Vec2::new(40.0, 40.0),
            };
            ctx.load_texture(
                format!("icon_{}", icon_id),
                color_image,
                egui::TextureOptions::default(),
            )
        });

        self.icon_cache.insert(icon_id, result.clone());
        result
    }

    pub fn start_loading(&mut self, install_dir: PathBuf) {
        self.game_state = None;
        self.loaded_model_idx = None;
        self.viewport.free_texture();
        self.housing_loaded_model_idx = None;
        self.housing_viewport.free_texture();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            load_game_data_thread(install_dir, tx);
        });
        self.phase = AppPhase::Loading {
            status: "正在初始化...".to_string(),
            receiver: rx,
        };
    }

    pub fn show_loading_ui(&mut self, ctx: &egui::Context) {
        let status_text = match &self.phase {
            AppPhase::Loading { status, .. } => status.clone(),
            _ => return,
        };

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space((ui.available_height() / 2.0 - 30.0).max(0.0));
                ui.spinner();
                ui.add_space(8.0);
                ui.label(&status_text);
            });
        });

        let mut transition: Option<Result<Box<LoadedData>, String>> = None;
        if let AppPhase::Loading { status, receiver } = &mut self.phase {
            while let Ok(msg) = receiver.try_recv() {
                match msg {
                    LoadProgress::Status(s) => *status = s,
                    LoadProgress::Done(data) => {
                        transition = Some(Ok(data));
                        break;
                    }
                    LoadProgress::Error(e) => {
                        transition = Some(Err(e));
                        break;
                    }
                }
            }
        }

        match transition {
            Some(Ok(data)) => {
                self.game_state = Some(GameState::from_loaded_data(*data));
                self.phase = AppPhase::Ready;
            }
            Some(Err(e)) => {
                self.phase = AppPhase::Setup {
                    dir_input: self
                        .config
                        .game_install_dir
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default(),
                    error: Some(e),
                };
            }
            None => {}
        }

        ctx.request_repaint();
    }

    pub fn show_ready_ui(&mut self, ctx: &egui::Context, gs: &mut GameState) {
        let mut goto_setup = false;
        egui::TopBottomPanel::top("top_tab_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(
                    &mut self.current_page,
                    crate::domain::AppPage::Browser,
                    "装备浏览器",
                );
                ui.selectable_value(
                    &mut self.current_page,
                    crate::domain::AppPage::GlamourManager,
                    "幻化管理",
                );
                ui.selectable_value(
                    &mut self.current_page,
                    crate::domain::AppPage::HousingBrowser,
                    "房屋外装",
                );
                ui.selectable_value(
                    &mut self.current_page,
                    crate::domain::AppPage::CraftingBrowser,
                    "合成检索",
                );
                ui.selectable_value(
                    &mut self.current_page,
                    crate::domain::AppPage::Toolbox,
                    "工具箱",
                );
                ui.selectable_value(
                    &mut self.current_page,
                    crate::domain::AppPage::ResourceBrowser,
                    "EXD 浏览器",
                );
                ui.selectable_value(&mut self.current_page, crate::domain::AppPage::Test, "测试");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("设置").clicked() {
                        goto_setup = true;
                    }
                });
            });
        });

        if goto_setup {
            self.phase = AppPhase::Setup {
                dir_input: self
                    .config
                    .game_install_dir
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default(),
                error: None,
            };
            return;
        }

        match self.current_page {
            crate::domain::AppPage::Browser => self.show_browser_page(ctx, gs),
            crate::domain::AppPage::GlamourManager => self.show_glamour_manager_page(ctx, gs),
            crate::domain::AppPage::HousingBrowser => self.show_housing_page(ctx, gs),
            crate::domain::AppPage::CraftingBrowser => self.show_crafting_page(ctx, gs),
            crate::domain::AppPage::Toolbox => self.show_toolbox_page(ctx),
            crate::domain::AppPage::ResourceBrowser => gs.resource_browser.show(ctx, &gs.game),
            crate::domain::AppPage::Test => self.show_test_page(ctx),
        }
    }

    fn show_test_page(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("进度条测试");
            ui.separator();

            ui.group(|ui| {
                ui.label("进度条设置:");
                ui.horizontal(|ui| {
                    ui.label("总大小:");
                    ui.add(egui::DragValue::new(&mut self.test_total).range(1..=10000000));
                });
                ui.horizontal(|ui| {
                    ui.label("当前位置:");
                    ui.add(egui::DragValue::new(&mut self.test_current).range(0..=self.test_total));
                });
            });

            ui.add_space(16.0);

            ui.group(|ui| {
                ui.label("进度条控制:");
                ui.horizontal_wrapped(|ui| {
                    if ui.button("开始模拟下载").clicked() {
                        let tracker = self.test_progress.clone();
                        let total = self.test_total;
                        std::thread::spawn(move || {
                            tracker.clear();
                            tracker.set_message("正在下载测试文件...");
                            tracker.set_length(total);
                            for i in 0..=total {
                                tracker.set_position(i);
                                std::thread::sleep(std::time::Duration::from_micros(100));
                            }
                            tracker.set_message("下载完成!");
                        });
                    }
                    if ui.button("不确定模式").clicked() {
                        self.test_progress.clear();
                        self.test_progress.set_message("正在处理...");
                        self.test_progress.set_indeterminate();
                    }
                    if ui.button("设置位置").clicked() {
                        self.test_progress.set_length(self.test_total);
                        self.test_progress.set_position(self.test_current);
                        self.test_progress.set_message("手动设置进度");
                    }
                    if ui.button("清除").clicked() {
                        self.test_progress.clear();
                    }
                });
            });

            ui.add_space(16.0);

            ui.group(|ui| {
                ui.label("进度条显示:");
                ui.add_space(8.0);
                show_progress_bar(ui, &self.test_progress);
            });

            ui.add_space(16.0);

            ui.group(|ui| {
                ui.label("示例进度条:");
                ui.add_space(8.0);

                let tracker1 = ProgressTracker::new();
                tracker1.set_message("下载中 (35%)...");
                tracker1.set_length(1024 * 100);
                tracker1.set_position(1024 * 35);
                show_progress_bar(ui, &tracker1);

                ui.add_space(6.0);

                let tracker2 = ProgressTracker::new();
                tracker2.set_message("已完成");
                tracker2.set_length(2048 * 1024);
                tracker2.set_position(2048 * 1024);
                show_progress_bar(ui, &tracker2);

                ui.add_space(6.0);

                let tracker3 = ProgressTracker::new();
                tracker3.set_message("不确定模式...");
                tracker3.set_indeterminate();
                show_progress_bar(ui, &tracker3);
            });
        });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if matches!(self.phase, AppPhase::Setup { .. }) {
            self.show_setup_ui(ctx);
        } else if matches!(self.phase, AppPhase::Loading { .. }) {
            self.show_loading_ui(ctx);
        } else if let Some(mut gs) = self.game_state.take() {
            self.show_ready_ui(ctx, &mut gs);
            self.game_state = Some(gs);
        } else {
            self.phase = AppPhase::Setup {
                dir_input: self
                    .config
                    .game_install_dir
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default(),
                error: Some("内部错误: 游戏数据丢失".to_string()),
            };
            self.show_setup_ui(ctx);
        }
    }
}
