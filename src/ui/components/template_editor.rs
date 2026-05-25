use std::thread;
use std::time::Duration;

use auto_play::cv::matcher::SingleMatcher;
use auto_play::cv::utils::{luma32f_to_luma8, normalize_luma32f};
use auto_play::{ControllerTrait, MatchDefinition, WindowsController};
use eframe::egui;
use image::DynamicImage;

use crate::template::{TemplateDef, TemplateSet};
use crate::template_images::TemplateImages;

const WINDOW_TITLE: &str = "最终幻想XIV";
const HANDLE_SIZE: f32 = 8.0;
const EDGE_THRESHOLD: f32 = 10.0;

// ═══════════════════════════════════════════════════════════
// 数据类型
// ═══════════════════════════════════════════════════════════

/// 测试结果
pub struct TestResult {
    pub screenshot_tex: egui::TextureHandle,
    pub heatmap_tex: egui::TextureHandle,
    pub matched: bool,
    pub match_value: Option<f32>,
    pub active_tab: usize,
}

/// 调整大小的边缘
#[derive(Debug, Clone, Copy, PartialEq)]
enum ResizeEdge {
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// 画布交互模式
#[derive(Debug, Clone, Copy, PartialEq)]
enum CanvasMode {
    Idle,
    CreatingSelection,
    MovingSelection,
    ResizingSelection(ResizeEdge),
    Panning,
}

/// 编辑器 Tab
#[derive(Debug, Clone, Copy, PartialEq)]
enum EditorTab {
    MatchList,
    TemplateList,
}

pub struct TemplateEditorState {
    editor_tab: EditorTab,
    template_set: Option<TemplateSet>,
    template_defs: Option<&'static [TemplateDef]>,
    selected_index: usize,
    template_id_cache: Vec<String>,
    tpl_list_selected_id: Option<String>,
    tpl_list_rename_buffer: String,
    tpl_list_creating: bool,
    screenshot_image: Option<DynamicImage>,
    screenshot_texture: Option<egui::TextureHandle>,
    image_size: (u32, u32),
    zoom: f32,
    pan_offset: egui::Vec2,
    selection: Option<[f32; 4]>,
    drag_start_world: Option<egui::Pos2>,
    drag_current_world: Option<egui::Pos2>,
    canvas_mode: CanvasMode,
    new_template_id: String,
    template_texture: Option<egui::TextureHandle>,
    builtin_template_texture: Option<egui::TextureHandle>,
    test_result: Option<TestResult>,
    status: String,
}

impl Default for TemplateEditorState {
    fn default() -> Self {
        Self {
            editor_tab: EditorTab::MatchList,
            template_set: None,
            template_defs: None,
            selected_index: 0,
            template_id_cache: Vec::new(),
            tpl_list_selected_id: None,
            tpl_list_rename_buffer: String::new(),
            tpl_list_creating: false,
            screenshot_image: None,
            screenshot_texture: None,
            image_size: (0, 0),
            zoom: 1.0,
            pan_offset: egui::Vec2::ZERO,
            selection: None,
            drag_start_world: None,
            drag_current_world: None,
            canvas_mode: CanvasMode::Idle,
            new_template_id: String::new(),
            template_texture: None,
            builtin_template_texture: None,
            test_result: None,
            status: String::new(),
        }
    }
}

impl TemplateEditorState {
    pub fn ensure_loaded(&mut self, defs: &'static [TemplateDef]) {
        if self.template_defs.is_some_and(|d| std::ptr::eq(d, defs)) && self.template_set.is_some() {
            return;
        }
        self.template_defs = Some(defs);
        self.template_set = Some(TemplateSet::load(TemplateImages::new(), defs));
        self.selected_index = 0;
        self.editor_tab = EditorTab::MatchList;
        self.clear_screenshot();
        self.status = String::new();
        self.template_texture = None;
        self.builtin_template_texture = None;
        self.test_result = None;
        self.new_template_id.clear();
        self.tpl_list_selected_id = None;
        self.tpl_list_creating = false;
        self.tpl_list_rename_buffer.clear();
        self.refresh_template_list();
    }

    pub fn template_set(&self) -> Option<&TemplateSet> {
        self.template_set.as_ref()
    }

    fn clear_screenshot(&mut self) {
        self.screenshot_image = None;
        self.screenshot_texture = None;
        self.image_size = (0, 0);
        self.zoom = 1.0;
        self.pan_offset = egui::Vec2::ZERO;
        self.selection = None;
        self.drag_start_world = None;
        self.drag_current_world = None;
        self.canvas_mode = CanvasMode::Idle;
    }

    fn format_file_size(bytes: u64) -> String {
        if bytes < 1024 {
            format!("{} B", bytes)
        } else if bytes < 1024 * 1024 {
            format!("{:.1} KB", bytes as f64 / 1024.0)
        } else {
            format!("{:.2} MB", bytes as f64 / (1024.0 * 1024.0))
        }
    }

    fn refresh_template_list(&mut self) {
        self.template_id_cache = TemplateImages::new().list_ids();
    }

    fn reload_template_set(&mut self) {
        let Some(defs) = self.template_defs else { return; };
        let prev_index = self.selected_index;
        self.template_set = Some(TemplateSet::load(TemplateImages::new(), defs));
        if let Some(ref set) = self.template_set {
            if !set.templates.is_empty() {
                self.selected_index = prev_index.min(set.templates.len() - 1);
            } else {
                self.selected_index = 0;
            }
        }
    }

    fn switch_tab(&mut self, tab: EditorTab) {
        self.editor_tab = tab;
        self.clear_screenshot();
        self.template_texture = None;
        self.builtin_template_texture = None;
        self.test_result = None;
        self.status = String::new();
    }

    fn select_match(&mut self, index: usize) {
        self.selected_index = index;
        self.clear_screenshot();
        self.template_texture = None;
        self.test_result = None;
        self.status = String::new();
    }

    fn select_template(&mut self, id: String) {
        self.tpl_list_selected_id = Some(id.clone());
        self.tpl_list_creating = false;
        self.tpl_list_rename_buffer = id;
        self.clear_screenshot();
        self.template_texture = None;
        self.test_result = None;
        self.status = String::new();
    }

    fn switch_to_new_template(&mut self) {
        self.tpl_list_selected_id = None;
        self.tpl_list_creating = true;
        self.tpl_list_rename_buffer.clear();
        self.clear_screenshot();
        self.template_texture = None;
        self.builtin_template_texture = None;
        self.test_result = None;
        self.status = "创建新模板: 截图后框选区域".to_string();
        self.new_template_id.clear();
    }

    // ═══════════════════════════════════════════════════════════
    // 主入口
    // ═══════════════════════════════════════════════════════════

    pub fn show_inline(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.show_inner(ui, ctx);
    }

    fn show_inner(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let prev_tab = self.editor_tab;
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.editor_tab, EditorTab::MatchList, "匹配列表");
            ui.selectable_value(&mut self.editor_tab, EditorTab::TemplateList, "模板列表");
        });
        ui.separator();
        if self.editor_tab != prev_tab {
            self.switch_tab(self.editor_tab);
        }

        match self.editor_tab {
            EditorTab::MatchList => self.show_match_list_tab(ui, ctx),
            EditorTab::TemplateList => self.show_template_list_tab(ui, ctx),
        }
    }

    // ═══════════════════════════════════════════════════════════
    // 匹配列表 Tab
    // ═══════════════════════════════════════════════════════════

    fn show_match_list_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let template_labels: Vec<(usize, String)> = {
            let Some(ref template_set) = self.template_set else {
                ui.label("未加载模板集");
                return;
            };
            if template_set.templates.is_empty() {
                ui.label("模板集为空");
                return;
            }
            template_set
                .templates
                .iter()
                .enumerate()
                .map(|(i, tpl)| {
                    let label = if tpl.is_custom {
                        format!("{} *", tpl.def.name)
                    } else {
                        tpl.def.name.to_string()
                    };
                    (i, label)
                })
                .collect()
        };

        ui.horizontal_top(|ui| {
            // 左侧匹配列表
            ui.vertical(|ui| {
                ui.set_max_width(220.0);
                ui.set_min_width(220.0);
                ui.label(egui::RichText::new("匹配列表").strong().size(14.0));
                ui.add_space(2.0);
                ui.label(egui::RichText::new("(当前工具使用的模板)").small().weak());
                ui.add_space(4.0);
                egui::ScrollArea::vertical()
                    .max_height(ui.available_height())
                    .show(ui, |ui| {
                        for (i, label) in &template_labels {
                            let is_selected = self.selected_index == *i;
                            if ui.selectable_label(is_selected, label).clicked() {
                                self.select_match(*i);
                            }
                        }
                    });
            });

            ui.separator();

            // 右侧编辑面板
            ui.vertical(|ui| {
                ui.set_max_width(ui.available_width());
                self.show_match_edit_panel(ui, ctx);
            });
        });
    }

    fn show_match_edit_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let Some(ref template_set) = self.template_set else {
            return;
        };
        if template_set.templates.is_empty() {
            return;
        }
        let tpl = &template_set.templates[self.selected_index];

        // 标题
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!("当前: {}", tpl.def.name))
                    .strong()
                    .size(14.0),
            );
            ui.separator();
            if tpl.is_custom {
                ui.label(
                    egui::RichText::new("自定义覆盖")
                        .color(ui.visuals().warn_fg_color)
                        .strong(),
                );
            } else {
                ui.label(egui::RichText::new("默认").weak());
            }
            ui.separator();
            ui.label(format!("id: {}", tpl.def.id));
            ui.separator();
            ui.label(format!("阈值: {}", tpl.def.threshold));
        });

        self.show_match_template_preview(ui, ctx);
        ui.add_space(4.0);

        // 操作按钮
        ui.horizontal(|ui| {
            if ui
                .button(format!("{} 截图", egui_phosphor::regular::CAMERA))
                .clicked()
            {
                self.take_screenshot(ctx);
            }
            let has_selection = self.selection.is_some();
            if ui
                .add_enabled(
                    has_selection && self.screenshot_image.is_some(),
                    egui::Button::new(format!("{} 确认裁剪", egui_phosphor::regular::CROP)),
                )
                .clicked()
            {
                self.save_selection_as_existing(ctx);
            }
            if ui
                .button(format!(
                    "{} 重置默认",
                    egui_phosphor::regular::ARROW_COUNTER_CLOCKWISE
                ))
                .clicked()
            {
                self.reset_current(ctx);
            }
            if ui
                .button(format!(
                    "{} 测试匹配",
                    egui_phosphor::regular::MAGNIFYING_GLASS
                ))
                .clicked()
            {
                self.test_match(ctx);
            }
        });

        if !self.status.is_empty() {
            ui.label(&self.status);
        }
        ui.add_space(4.0);
        self.show_screenshot_canvas(ui, ctx);

        if self.test_result.is_some() {
            ui.add_space(8.0);
            ui.separator();
            self.show_test_result(ui);
        }
    }

    fn show_match_template_preview(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let Some(ref template_set) = self.template_set else {
            return;
        };
        if template_set.templates.is_empty() {
            return;
        }
        let tpl = &template_set.templates[self.selected_index];

        if self.template_texture.is_none() {
            let rgba = tpl.image.to_rgba8();
            let (w, h) = (rgba.width(), rgba.height());
            let color_image =
                egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], rgba.as_raw());
            self.template_texture =
                Some(ctx.load_texture("tpl_preview", color_image, egui::TextureOptions::LINEAR));
        }

        if tpl.is_custom {
            // 加载默认原图
            if self.builtin_template_texture.is_none() {
                let images = TemplateImages::new();
                if let Some(builtin_img) = images.get_builtin(tpl.def.id) {
                    let rgba = builtin_img.to_rgba8();
                    let (w, h) = (rgba.width(), rgba.height());
                    let color_image = egui::ColorImage::from_rgba_unmultiplied(
                        [w as usize, h as usize],
                        rgba.as_raw(),
                    );
                    self.builtin_template_texture = Some(ctx.load_texture(
                        "tpl_builtin_preview",
                        color_image,
                        egui::TextureOptions::LINEAR,
                    ));
                }
            }

            ui.horizontal_top(|ui| {
                // 左侧：默认原图
                if let Some(ref tex) = self.builtin_template_texture {
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("默认原图").small().weak());
                        let size = tex.size_vec2();
                        let max_h_scale = (120.0 / size.y.max(1.0)).min(2.0);
                        let max_w_scale = (ui.available_width() / 2.0) / size.x.max(1.0);
                        let scale = max_h_scale.min(max_w_scale);
                        ui.image(egui::load::SizedTexture::new(tex.id(), size * scale));
                    });
                } else {
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("默认原图").small().weak());
                        ui.label("无内置版本");
                    });
                }

                ui.separator();

                // 右侧：当前生效的自定义图
                if let Some(ref tex) = self.template_texture {
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new("自定义（当前生效）")
                                .small()
                                .color(ui.visuals().warn_fg_color),
                        );
                        let size = tex.size_vec2();
                        let max_h_scale = (120.0 / size.y.max(1.0)).min(2.0);
                        let max_w_scale = (ui.available_width() / 2.0) / size.x.max(1.0);
                        let scale = max_h_scale.min(max_w_scale);
                        ui.image(egui::load::SizedTexture::new(tex.id(), size * scale));
                    });
                }
            });
        } else {
            // 非自定义：保持原有单图显示
            if let Some(ref tex) = self.template_texture {
                let size = tex.size_vec2();
                let max_h_scale = (120.0 / size.y.max(1.0)).min(2.0);
                let max_w_scale = ui.available_width() / size.x.max(1.0);
                let scale = max_h_scale.min(max_w_scale);
                ui.image(egui::load::SizedTexture::new(tex.id(), size * scale));
            }
        }
    }

    fn save_selection_as_existing(&mut self, ctx: &egui::Context) {
        let Some(ref img) = self.screenshot_image else {
            self.status = "没有截图可裁剪".into();
            return;
        };
        let Some([x, y, w, h]) = self.selection else {
            self.status = "请先框选区域".into();
            return;
        };
        let (x, y, w, h) = (x as u32, y as u32, w as u32, h as u32);
        if w == 0 || h == 0 {
            self.status = "选区太小".into();
            return;
        }
        let cropped = img.crop_imm(x, y, w, h);
        let images = self
            .template_set
            .as_ref()
            .map(|s| s.images.clone())
            .unwrap_or_default();
        let Some(ref mut template_set) = self.template_set else {
            return;
        };
        let tpl_id = template_set.templates[self.selected_index].def.id.to_string();
        let tpl = &mut template_set.templates[self.selected_index];
        match tpl.save_custom(&images, cropped) {
            Ok(()) => {
                self.status = format!("已保存 {}x{} 为自定义模板", w, h);
                self.template_texture = None;
                self.refresh_template_list();
                if let Some(pos) = self.template_id_cache.iter().position(|s| s == &tpl_id) {
                    let item = self.template_id_cache.remove(pos);
                    self.template_id_cache.insert(0, item);
                }
                self.tpl_list_selected_id = Some(tpl_id);
            }
            Err(e) => self.status = format!("保存失败: {e}"),
        }
        self.refresh_match_template_texture(ctx);
    }

    fn reset_current(&mut self, ctx: &egui::Context) {
        let images = self
            .template_set
            .as_ref()
            .map(|s| s.images.clone())
            .unwrap_or_default();
        let Some(ref mut template_set) = self.template_set else {
            return;
        };
        template_set.templates[self.selected_index].reset_to_default(&images);
        self.template_texture = None;
        self.status = "已重置为默认模板".into();
        self.refresh_match_template_texture(ctx);
    }

    fn refresh_match_template_texture(&mut self, ctx: &egui::Context) {
        let Some(ref template_set) = self.template_set else {
            return;
        };
        if template_set.templates.is_empty() {
            return;
        }
        let tpl = &template_set.templates[self.selected_index];
        let rgba = tpl.image.to_rgba8();
        let (w, h) = (rgba.width(), rgba.height());
        let color_image =
            egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], rgba.as_raw());
        self.template_texture =
            Some(ctx.load_texture("tpl_preview", color_image, egui::TextureOptions::LINEAR));

        // 若当前是自定义覆盖状态，同步加载默认原图
        if tpl.is_custom {
            let images = TemplateImages::new();
            if let Some(builtin_img) = images.get_builtin(tpl.def.id) {
                let rgba = builtin_img.to_rgba8();
                let (w, h) = (rgba.width(), rgba.height());
                let color_image = egui::ColorImage::from_rgba_unmultiplied(
                    [w as usize, h as usize],
                    rgba.as_raw(),
                );
                self.builtin_template_texture = Some(ctx.load_texture(
                    "tpl_builtin_preview",
                    color_image,
                    egui::TextureOptions::LINEAR,
                ));
            } else {
                self.builtin_template_texture = None;
            }
        } else {
            self.builtin_template_texture = None;
        }
    }

    // ═══════════════════════════════════════════════════════════
    // 模板列表 Tab
    // ═══════════════════════════════════════════════════════════

    fn show_template_list_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let left_width = 220.0;
        ui.horizontal_top(|ui| {
            // 左侧模板列表
            ui.vertical(|ui| {
                ui.set_max_width(left_width);
                ui.set_min_width(left_width);

                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("模板列表").strong().size(14.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button(format!("{}", egui_phosphor::regular::ARROWS_CLOCKWISE))
                            .on_hover_text("刷新模板列表")
                            .clicked()
                        {
                            self.refresh_template_list();
                        }
                    });
                });
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new("(扫描 assets/templates/ + .tomestone/templates/)")
                        .small()
                        .weak(),
                );
                ui.add_space(4.0);

                egui::ScrollArea::vertical()
                    .max_height(ui.available_height() - 60.0)
                    .show(ui, |ui| {
                        if self.template_id_cache.is_empty() {
                            ui.label(egui::RichText::new("暂无模板").small().weak());
                        } else {
                            for id in &self.template_id_cache.clone() {
                                let is_selected =
                                    self.tpl_list_selected_id.as_ref() == Some(id)
                                        && !self.tpl_list_creating;
                                let images = TemplateImages::new();
                                let is_custom = images.is_custom(id);
                                let label = if is_custom {
                                    format!("{} *", id)
                                } else {
                                    id.clone()
                                };
                                if ui.selectable_label(is_selected, label).clicked() {
                                    self.select_template(id.clone());
                                }
                            }
                        }
                    });

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                let is_creating = self.tpl_list_creating;
                if ui
                    .selectable_label(
                        is_creating,
                        format!("{} 新建模板", egui_phosphor::regular::PLUS),
                    )
                    .clicked()
                {
                    self.switch_to_new_template();
                }
            });

            ui.separator();

            // 右侧面板
            ui.vertical(|ui| {
                ui.set_max_width(ui.available_width());
                if self.tpl_list_creating {
                    self.show_create_panel(ui, ctx);
                } else if let Some(ref id) = self.tpl_list_selected_id.clone() {
                    self.show_template_detail_panel(ui, ctx, id);
                } else {
                    ui.centered_and_justified(|ui| {
                        ui.label("请选择一个模板或点击「新建模板」");
                    });
                }
            });
        });
    }

    fn show_template_detail_panel(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        id: &str,
    ) {
        let images = TemplateImages::new();
        let has_builtin = images.has_builtin(id);
        let is_custom = images.is_custom(id);

        // ── 标题行 + 状态标签 ──
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("模板详情").strong().size(14.0));
            ui.separator();
            match (has_builtin, is_custom) {
                (true, false) => {
                    ui.label(egui::RichText::new("默认").weak());
                }
                (true, true) => {
                    ui.label(
                        egui::RichText::new("自定义覆盖")
                            .color(ui.visuals().warn_fg_color)
                            .strong(),
                    );
                }
                (false, true) => {
                    ui.label(
                        egui::RichText::new("纯自定义")
                            .color(ui.visuals().hyperlink_color)
                            .strong(),
                    );
                }
                (false, false) => {
                    ui.label(egui::RichText::new("不存在").color(ui.visuals().error_fg_color));
                }
            }
        });
        ui.add_space(4.0);

        // ── 标识符编辑 ──
        ui.horizontal(|ui| {
            ui.label("标识符:");
            let mut dummy = self.tpl_list_rename_buffer.clone();
            if has_builtin {
                // 内置模板：标识符只读
                ui.add(egui::TextEdit::singleline(&mut dummy).interactive(false));
                ui.label(
                    egui::RichText::new("(内置模板标识符不可修改)")
                        .small()
                        .weak(),
                );
            } else {
                ui.text_edit_singleline(&mut self.tpl_list_rename_buffer);
            }
            if !has_builtin {
                if ui.button("重命名").clicked() {
                    self.rename_selected_template();
                }
            }
            // 重置 / 删除 按钮
            match (has_builtin, is_custom) {
                (true, true) => {
                    if ui
                        .button(format!(
                            "{} 重置默认",
                            egui_phosphor::regular::ARROW_COUNTER_CLOCKWISE
                        ))
                        .clicked()
                    {
                        self.reset_selected_template(ctx);
                    }
                }
                (false, true) => {
                    if ui
                        .button(format!("{} 删除模板", egui_phosphor::regular::TRASH))
                        .clicked()
                    {
                        self.delete_selected_template();
                    }
                }
                _ => {}
            }
        });
        ui.add_space(4.0);

        // ── 预览图 ──
        if self.template_texture.is_none() {
            self.refresh_template_texture_for_id(ctx, id);
        }

        if has_builtin && is_custom {
            // 被覆盖的默认：双栏对比
            let builtin_path = images.builtin_path(id);
            let custom_path = images.custom_path(id);
            let builtin_meta = std::fs::metadata(&builtin_path).ok().map(|m| m.len());
            let custom_meta = std::fs::metadata(&custom_path).ok().map(|m| m.len());

            ui.horizontal_top(|ui| {
                if let Some(ref tex) = self.builtin_template_texture {
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("默认原图").small().weak());
                        let size = tex.size_vec2();
                        let max_h_scale = (120.0 / size.y.max(1.0)).min(2.0);
                        let max_w_scale = (ui.available_width() / 2.0) / size.x.max(1.0);
                        let scale = max_h_scale.min(max_w_scale);
                        ui.image(egui::load::SizedTexture::new(tex.id(), size * scale));
                        let info = format!(
                            "{}×{}px{}",
                            tex.size()[0],
                            tex.size()[1],
                            builtin_meta.map(|b| format!(" | {}", Self::format_file_size(b))).unwrap_or_default()
                        );
                        ui.label(egui::RichText::new(info).small().weak());
                    });
                } else {
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("默认原图").small().weak());
                        ui.label("无内置版本");
                    });
                }

                ui.separator();

                if let Some(ref tex) = self.template_texture {
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new("自定义（当前生效）")
                                .small()
                                .color(ui.visuals().warn_fg_color),
                        );
                        let size = tex.size_vec2();
                        let max_h_scale = (120.0 / size.y.max(1.0)).min(2.0);
                        let max_w_scale = (ui.available_width() / 2.0) / size.x.max(1.0);
                        let scale = max_h_scale.min(max_w_scale);
                        ui.image(egui::load::SizedTexture::new(tex.id(), size * scale));
                        let info = format!(
                            "{}×{}px{}",
                            tex.size()[0],
                            tex.size()[1],
                            custom_meta.map(|b| format!(" | {}", Self::format_file_size(b))).unwrap_or_default()
                        );
                        ui.label(egui::RichText::new(info).small().weak());
                    });
                }
            });
        } else {
            // 纯默认 或 纯自定义：单图
            let (label, label_color, file_path) = if has_builtin {
                ("默认", None, images.builtin_path(id))
            } else if is_custom {
                ("自定义", Some(ui.visuals().hyperlink_color), images.custom_path(id))
            } else {
                ("不存在", Some(ui.visuals().error_fg_color), std::path::PathBuf::new())
            };

            if let Some(ref tex) = self.template_texture {
                let file_size = if file_path.exists() {
                    std::fs::metadata(&file_path).ok().map(|m| m.len())
                } else {
                    None
                };
                ui.vertical(|ui| {
                    if let Some(color) = label_color {
                        ui.label(egui::RichText::new(label).small().color(color));
                    } else {
                        ui.label(egui::RichText::new(label).small().weak());
                    }
                    let size = tex.size_vec2();
                    let max_h_scale = (120.0 / size.y.max(1.0)).min(2.0);
                    let max_w_scale = ui.available_width() / size.x.max(1.0);
                    let scale = max_h_scale.min(max_w_scale);
                    ui.image(egui::load::SizedTexture::new(tex.id(), size * scale));
                    let mut info = format!("{}×{}px", tex.size()[0], tex.size()[1]);
                    if let Some(bytes) = file_size {
                        info.push_str(&format!(" | {}", Self::format_file_size(bytes)));
                    }
                    ui.label(egui::RichText::new(info).small().weak());
                });
            } else {
                ui.label(egui::RichText::new("无预览图").weak());
            }
        }
        ui.add_space(4.0);

        // ── 操作按钮（截图 + 保存） ──
        ui.horizontal(|ui| {
            if ui
                .button(format!("{} 截图", egui_phosphor::regular::CAMERA))
                .clicked()
            {
                self.take_screenshot(ctx);
            }
            let has_selection = self.selection.is_some();
            let can_save = has_selection && self.screenshot_image.is_some();
            if ui
                .add_enabled(
                    can_save,
                    egui::Button::new(format!("{} 保存", egui_phosphor::regular::FLOPPY_DISK)),
                )
                .clicked()
            {
                self.save_selection_to_template_id(ctx, id);
            }
        });

        if !self.status.is_empty() {
            ui.label(&self.status);
        }
        ui.add_space(4.0);
        self.show_screenshot_canvas(ui, ctx);
    }

    fn refresh_template_texture_for_id(&mut self, ctx: &egui::Context, id: &str) {
        let images = TemplateImages::new();
        if let Some(img) = images.get(id) {
            let rgba = img.to_rgba8();
            let (w, h) = (rgba.width(), rgba.height());
            let color_image =
                egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], rgba.as_raw());
            self.template_texture = Some(
                ctx.load_texture("tpl_preview", color_image, egui::TextureOptions::LINEAR),
            );
        } else {
            self.template_texture = None;
        }

        if images.is_custom(id) {
            if let Some(builtin_img) = images.get_builtin(id) {
                let rgba = builtin_img.to_rgba8();
                let (w, h) = (rgba.width(), rgba.height());
                let color_image =
                    egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], rgba.as_raw());
                self.builtin_template_texture = Some(
                    ctx.load_texture(
                        "tpl_builtin_preview",
                        color_image,
                        egui::TextureOptions::LINEAR,
                    ),
                );
            } else {
                self.builtin_template_texture = None;
            }
        } else {
            self.builtin_template_texture = None;
        }
    }

    fn save_selection_to_template_id(&mut self, _ctx: &egui::Context, id: &str) {
        let Some(ref img) = self.screenshot_image else {
            self.status = "没有截图可裁剪".into();
            return;
        };
        let Some([x, y, w, h]) = self.selection else {
            self.status = "请先框选区域".into();
            return;
        };
        let (x, y, w, h) = (x as u32, y as u32, w as u32, h as u32);
        if w == 0 || h == 0 {
            self.status = "选区太小".into();
            return;
        }
        let cropped = img.crop_imm(x, y, w, h);
        let images = TemplateImages::new();
        match images.save_custom(id, cropped) {
            Ok(()) => {
                self.status = format!("已保存 {}x{} 到模板 '{}'", w, h, id);
                self.template_texture = None;
                self.builtin_template_texture = None;
                self.refresh_template_list();
                if let Some(pos) = self.template_id_cache.iter().position(|s| s == id) {
                    let item = self.template_id_cache.remove(pos);
                    self.template_id_cache.insert(0, item);
                }
                self.tpl_list_selected_id = Some(id.to_string());
                self.reload_template_set();
            }
            Err(e) => self.status = format!("保存失败: {e}"),
        }
    }

    fn rename_selected_template(&mut self) {
        let Some(ref old_id) = self.tpl_list_selected_id else {
            return;
        };
        let new_id = self.tpl_list_rename_buffer.trim().to_string();
        if new_id.is_empty() {
            self.status = "请输入新标识符".into();
            return;
        }
        if !new_id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '/')
        {
            self.status = "标识符只能包含字母、数字、下划线、连字符和斜杠".into();
            return;
        }
        if new_id == *old_id {
            self.status = "新标识符与旧标识符相同".into();
            return;
        }
        let images = TemplateImages::new();
        match images.rename_custom_only(old_id, &new_id) {
            Ok(()) => {
                self.status = format!("已重命名 '{}' -> '{}'", old_id, new_id);
                self.refresh_template_list();
                self.tpl_list_selected_id = Some(new_id.clone());
                self.tpl_list_rename_buffer = new_id.clone();
                self.reload_template_set();
                self.template_texture = None;
                self.builtin_template_texture = None;
            }
            Err(e) => self.status = format!("重命名失败: {e}"),
        }
    }

    fn reset_selected_template(&mut self, ctx: &egui::Context) {
        let id = match self.tpl_list_selected_id.clone() {
            Some(id) => id,
            None => return,
        };
        let images = TemplateImages::new();
        images.remove_custom(&id);
        self.status = format!("已重置模板 '{}' 为默认版本", id);
        self.template_texture = None;
        self.builtin_template_texture = None;
        self.refresh_template_list();
        self.reload_template_set();
        self.refresh_template_texture_for_id(ctx, &id);
    }

    fn delete_selected_template(&mut self) {
        let Some(ref id) = self.tpl_list_selected_id else {
            return;
        };
        let images = TemplateImages::new();
        images.remove_custom_only(id);
        self.status = format!("已删除模板 '{}'", id);
        self.refresh_template_list();
        self.tpl_list_selected_id = None;
        self.tpl_list_rename_buffer.clear();
        self.template_texture = None;
        self.builtin_template_texture = None;
        self.reload_template_set();
        self.clear_screenshot();
    }

    // ═══════════════════════════════════════════════════════════
    // 新建模板面板
    // ═══════════════════════════════════════════════════════════

    fn show_create_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.label(
            egui::RichText::new("新建模板")
                .strong()
                .size(16.0),
        );
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label("模板标识符:");
            ui.text_edit_singleline(&mut self.new_template_id);
            ui.label(
                egui::RichText::new("(保存为 .tomestone/templates/{id}.png)")
                    .small()
                    .weak(),
            );
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui
                .button(format!("{} 截图", egui_phosphor::regular::CAMERA))
                .clicked()
            {
                self.take_screenshot(ctx);
            }
            let can_save = self.selection.is_some()
                && self.screenshot_image.is_some()
                && !self.new_template_id.trim().is_empty();
            if ui
                .add_enabled(
                    can_save,
                    egui::Button::new(format!("{} 保存", egui_phosphor::regular::FLOPPY_DISK)),
                )
                .clicked()
            {
                self.save_new_template();
            }
        });

        if !self.status.is_empty() {
            ui.label(&self.status);
        }

        ui.add_space(4.0);
        self.show_screenshot_canvas(ui, ctx);
    }

    fn save_new_template(&mut self) {
        let id = self.new_template_id.trim().to_string();
        if id.is_empty() {
            self.status = "请填写模板标识符".into();
            return;
        }
        if !id.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '/') {
            self.status = "标识符只能包含字母、数字、下划线、连字符和斜杠".into();
            return;
        }

        let Some(ref img) = self.screenshot_image else {
            self.status = "没有截图可裁剪".into();
            return;
        };
        let Some([x, y, w, h]) = self.selection else {
            self.status = "请先框选区域".into();
            return;
        };
        let (x, y, w, h) = (x as u32, y as u32, w as u32, h as u32);
        if w == 0 || h == 0 {
            self.status = "选区太小".into();
            return;
        }

        let cropped = img.crop_imm(x, y, w, h);
        let images = TemplateImages::new();
        match images.save_custom(&id, cropped) {
            Ok(()) => {
                self.status = format!("已创建新模板 '{}' ({}x{})", id, w, h);
                self.refresh_template_list();
                if let Some(pos) = self.template_id_cache.iter().position(|s| s == &id) {
                    let item = self.template_id_cache.remove(pos);
                    self.template_id_cache.insert(0, item);
                }
                self.tpl_list_selected_id = Some(id.clone());
                self.tpl_list_creating = false;
                self.reload_template_set();
                self.template_texture = None;
                self.builtin_template_texture = None;
                self.new_template_id.clear();
            }
            Err(e) => self.status = format!("保存失败: {e}"),
        }
    }

    // ═══════════════════════════════════════════════════════════
    // 截图
    // ═══════════════════════════════════════════════════════════

    fn take_screenshot(&mut self, ctx: &egui::Context) {
        self.status = format!("正在捕获 '{}'...", WINDOW_TITLE);
        self.clear_screenshot();
        match WindowsController::from_window_title(WINDOW_TITLE) {
            Ok(controller) => {
                thread::sleep(Duration::from_millis(200));
                match controller.screencap() {
                    Ok(img) => {
                        let (w, h) = (img.width(), img.height());
                        self.image_size = (w, h);
                        let rgba = img.to_rgba8();
                        let color_image = egui::ColorImage::from_rgba_unmultiplied(
                            [w as usize, h as usize],
                            rgba.as_raw(),
                        );
                        self.screenshot_texture = Some(ctx.load_texture(
                            "tpl_editor_screenshot",
                            color_image,
                            egui::TextureOptions::LINEAR,
                        ));
                        self.screenshot_image = Some(img);
                        self.zoom = 1.0;
                        self.pan_offset = egui::Vec2::ZERO;
                        self.status =
                            format!("已捕获 {}x{} — 左键框选, 中键平移, 滚轮缩放", w, h);
                    }
                    Err(e) => self.status = format!("截图失败: {e}"),
                }
            }
            Err(e) => self.status = format!("连接窗口失败: {e}"),
        }
    }

    // ═══════════════════════════════════════════════════════════
    // 截图画布（核心交互）
    // ═══════════════════════════════════════════════════════════

    fn show_screenshot_canvas(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        let Some(ref texture) = self.screenshot_texture else {
            return;
        };

        let img_w = self.image_size.0 as f32;
        let img_h = self.image_size.1 as f32;
        if img_w == 0.0 || img_h == 0.0 {
            return;
        }

        let zoom = self.zoom;

        // 计算显示尺寸
        let scaled_w = img_w * zoom;
        let scaled_h = img_h * zoom;

        // 分配画布区域（至少足够显示图片，或占据可用空间）
        let avail = ui.available_size();
        let canvas_size = egui::vec2(
            scaled_w.max(avail.x),
            scaled_h.max(avail.y.min(600.0)),
        );

        let (response, painter) =
            ui.allocate_painter(canvas_size, egui::Sense::click_and_drag());
        let canvas_rect = response.rect;

        // 图片在画布中的位置（考虑平移）
        let img_rect = egui::Rect::from_min_size(
            canvas_rect.min + self.pan_offset,
            egui::vec2(scaled_w, scaled_h),
        );

        // 绘制图片
        painter.image(
            texture.id(),
            img_rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );

        // 屏幕坐标 → 图像坐标（世界坐标）
        let screen_to_world = |sp: egui::Pos2| -> egui::Pos2 {
            egui::pos2(
                ((sp.x - img_rect.min.x) / zoom).clamp(0.0, img_w),
                ((sp.y - img_rect.min.y) / zoom).clamp(0.0, img_h),
            )
        };

        // 图像坐标 → 屏幕坐标
        let world_to_screen = |wp: egui::Pos2| -> egui::Pos2 {
            egui::pos2(
                img_rect.min.x + wp.x * zoom,
                img_rect.min.y + wp.y * zoom,
            )
        };

        // 获取鼠标位置
        let pointer_pos = ui.input(|i| i.pointer.hover_pos());
        let world_pos = pointer_pos.map(|p| screen_to_world(p));

        // ── 检测鼠标是否在选区边缘/内部 ──
        let (hover_edge, hover_inside) = if let Some(sel) = self.selection {
            self.detect_hover_edge(world_pos, sel)
        } else {
            (None, false)
        };

        // ── 输入处理 ──
        // 滚轮缩放（以鼠标位置为中心）
        let scroll = ui.input(|i| i.raw_scroll_delta.y);
        if scroll != 0.0 && response.hovered() {
            let old_zoom = zoom;
            let new_zoom = (old_zoom + scroll * 0.001).clamp(0.1, 5.0);
            self.zoom = new_zoom;

            // 以鼠标位置为中心缩放：调整 pan_offset
            if let Some(sp) = pointer_pos {
                let ratio = new_zoom / old_zoom;
                let dx = sp.x - img_rect.min.x;
                let dy = sp.y - img_rect.min.y;
                self.pan_offset.x += dx - dx * ratio;
                self.pan_offset.y += dy - dy * ratio;
            }

            ui.ctx().request_repaint();
        }

        // 左键交互
        if response.drag_started_by(egui::PointerButton::Primary) {
            if let Some(wp) = world_pos {
                if let Some(edge) = hover_edge {
                    // 开始调整大小
                    self.canvas_mode = CanvasMode::ResizingSelection(edge);
                    self.drag_start_world = Some(wp);
                    self.drag_current_world = Some(wp);
                } else if hover_inside {
                    // 开始移动选区
                    self.canvas_mode = CanvasMode::MovingSelection;
                    self.drag_start_world = Some(wp);
                    self.drag_current_world = Some(wp);
                } else {
                    // 开始创建新选区
                    self.canvas_mode = CanvasMode::CreatingSelection;
                    self.drag_start_world = Some(wp);
                    self.drag_current_world = Some(wp);
                    self.selection = None;
                }
            }
        }

        if response.dragged_by(egui::PointerButton::Primary) {
            if let Some(wp) = world_pos {
                self.drag_current_world = Some(wp);

                match self.canvas_mode {
                    CanvasMode::CreatingSelection => {
                        if let (Some(start), Some(end)) =
                            (self.drag_start_world, self.drag_current_world)
                        {
                            let shift_held = ui.input(|i| i.modifiers.shift);
                            if shift_held {
                                // Shift: 画正方形（以 drag_start 为锚点）
                                let dx = end.x - start.x;
                                let dy = end.y - start.y;
                                let size = dx.abs().max(dy.abs());
                                let x = if dx >= 0.0 {
                                    start.x
                                } else {
                                    start.x - size
                                };
                                let y = if dy >= 0.0 {
                                    start.y
                                } else {
                                    start.y - size
                                };
                                let w = size;
                                let h = size;
                                if w > 2.0 && h > 2.0 {
                                    self.selection = Some([
                                        x.clamp(0.0, img_w - w),
                                        y.clamp(0.0, img_h - h),
                                        w,
                                        h,
                                    ]);
                                }
                            } else {
                                let x = start.x.min(end.x);
                                let y = start.y.min(end.y);
                                let w = (end.x - start.x).abs();
                                let h = (end.y - start.y).abs();
                                if w > 2.0 && h > 2.0 {
                                    self.selection = Some([x, y, w, h]);
                                }
                            }
                        }
                    }
                    CanvasMode::MovingSelection => {
                        if let (Some(start), Some(end), Some(sel)) =
                            (self.drag_start_world, self.drag_current_world, self.selection)
                        {
                            let dx = end.x - start.x;
                            let dy = end.y - start.y;
                            let new_x = (sel[0] + dx).clamp(0.0, img_w - sel[2]);
                            let new_y = (sel[1] + dy).clamp(0.0, img_h - sel[3]);
                            self.selection = Some([new_x, new_y, sel[2], sel[3]]);
                            self.drag_start_world = Some(end);
                        }
                    }
                    CanvasMode::ResizingSelection(edge) => {
                        if let (Some(start), Some(end), Some(sel)) =
                            (self.drag_start_world, self.drag_current_world, self.selection)
                        {
                            let dx = end.x - start.x;
                            let dy = end.y - start.y;
                            let shift_held = ui.input(|i| i.modifiers.shift);
                            let (mut x, mut y, mut w, mut h) = (sel[0], sel[1], sel[2], sel[3]);

                            if shift_held {
                                // Shift: 保持原始宽高比
                                let ratio = w / h;
                                match edge {
                                    ResizeEdge::Left => {
                                        let new_w = (w - dx).max(4.0);
                                        let new_h = new_w / ratio;
                                        x += w - new_w;
                                        y += h - new_h;
                                        w = new_w;
                                        h = new_h;
                                    }
                                    ResizeEdge::Right => {
                                        w = (w + dx).max(4.0);
                                        h = w / ratio;
                                    }
                                    ResizeEdge::Top => {
                                        let new_h = (h - dy).max(4.0);
                                        let new_w = new_h * ratio;
                                        x += w - new_w;
                                        y += h - new_h;
                                        w = new_w;
                                        h = new_h;
                                    }
                                    ResizeEdge::Bottom => {
                                        h = (h + dy).max(4.0);
                                        w = h * ratio;
                                    }
                                    ResizeEdge::TopLeft => {
                                        if dx.abs() > dy.abs() * ratio {
                                            let new_w = (w - dx).max(4.0);
                                            let new_h = new_w / ratio;
                                            x += w - new_w;
                                            y += h - new_h;
                                            w = new_w;
                                            h = new_h;
                                        } else {
                                            let new_h = (h - dy).max(4.0);
                                            let new_w = new_h * ratio;
                                            x += w - new_w;
                                            y += h - new_h;
                                            w = new_w;
                                            h = new_h;
                                        }
                                    }
                                    ResizeEdge::TopRight => {
                                        if dx.abs() > dy.abs() * ratio {
                                            let new_w = (w + dx).max(4.0);
                                            let new_h = new_w / ratio;
                                            y += h - new_h;
                                            w = new_w;
                                            h = new_h;
                                        } else {
                                            let new_h = (h - dy).max(4.0);
                                            let new_w = new_h * ratio;
                                            y += h - new_h;
                                            w = new_w;
                                            h = new_h;
                                        }
                                    }
                                    ResizeEdge::BottomLeft => {
                                        if dx.abs() > dy.abs() * ratio {
                                            let new_w = (w - dx).max(4.0);
                                            let new_h = new_w / ratio;
                                            x += w - new_w;
                                            w = new_w;
                                            h = new_h;
                                        } else {
                                            let new_h = (h + dy).max(4.0);
                                            let new_w = new_h * ratio;
                                            x += w - new_w;
                                            w = new_w;
                                            h = new_h;
                                        }
                                    }
                                    ResizeEdge::BottomRight => {
                                        if dx.abs() > dy.abs() * ratio {
                                            let new_w = (w + dx).max(4.0);
                                            let new_h = new_w / ratio;
                                            w = new_w;
                                            h = new_h;
                                        } else {
                                            let new_h = (h + dy).max(4.0);
                                            let new_w = new_h * ratio;
                                            w = new_w;
                                            h = new_h;
                                        }
                                    }
                                }
                            } else {
                                match edge {
                                    ResizeEdge::Left => {
                                        let new_x = (x + dx).clamp(0.0, x + w - 4.0);
                                        w = x + w - new_x;
                                        x = new_x;
                                    }
                                    ResizeEdge::Right => {
                                        w = (w + dx).clamp(4.0, img_w - x);
                                    }
                                    ResizeEdge::Top => {
                                        let new_y = (y + dy).clamp(0.0, y + h - 4.0);
                                        h = y + h - new_y;
                                        y = new_y;
                                    }
                                    ResizeEdge::Bottom => {
                                        h = (h + dy).clamp(4.0, img_h - y);
                                    }
                                    ResizeEdge::TopLeft => {
                                        let new_x = (x + dx).clamp(0.0, x + w - 4.0);
                                        let new_y = (y + dy).clamp(0.0, y + h - 4.0);
                                        w = x + w - new_x;
                                        h = y + h - new_y;
                                        x = new_x;
                                        y = new_y;
                                    }
                                    ResizeEdge::TopRight => {
                                        let new_y = (y + dy).clamp(0.0, y + h - 4.0);
                                        w = (w + dx).clamp(4.0, img_w - x);
                                        h = y + h - new_y;
                                        y = new_y;
                                    }
                                    ResizeEdge::BottomLeft => {
                                        let new_x = (x + dx).clamp(0.0, x + w - 4.0);
                                        w = x + w - new_x;
                                        h = (h + dy).clamp(4.0, img_h - y);
                                        x = new_x;
                                    }
                                    ResizeEdge::BottomRight => {
                                        w = (w + dx).clamp(4.0, img_w - x);
                                        h = (h + dy).clamp(4.0, img_h - y);
                                    }
                                }
                            }

                            self.selection = Some([x, y, w, h]);
                            self.drag_start_world = Some(end);
                        }
                    }
                    _ => {}
                }
            }
        }

        if response.drag_stopped_by(egui::PointerButton::Primary) {
            self.canvas_mode = CanvasMode::Idle;
            self.drag_start_world = None;
            self.drag_current_world = None;
        }

        // 中键平移
        if response.drag_started_by(egui::PointerButton::Middle) {
            self.canvas_mode = CanvasMode::Panning;
        }
        if response.dragged_by(egui::PointerButton::Middle) {
            let delta = response.drag_delta();
            if delta != egui::Vec2::ZERO {
                self.pan_offset += delta;
            }
        }
        if response.drag_stopped_by(egui::PointerButton::Middle) {
            self.canvas_mode = CanvasMode::Idle;
        }

        // ── 绘制选区 ──
        if let Some(sel) = self.selection {
            let s_screen = world_to_screen(egui::pos2(sel[0], sel[1]));
            let e_screen = world_to_screen(egui::pos2(sel[0] + sel[2], sel[1] + sel[3]));
            let sel_rect = egui::Rect::from_two_pos(s_screen, e_screen);

            // 遮罩（暗化选区外）
            let dim = egui::Color32::from_black_alpha(120);
            painter.rect_filled(
                egui::Rect::from_min_max(
                    canvas_rect.min,
                    egui::pos2(canvas_rect.max.x, sel_rect.min.y),
                ),
                0.0,
                dim,
            );
            painter.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(canvas_rect.min.x, sel_rect.max.y),
                    canvas_rect.max,
                ),
                0.0,
                dim,
            );
            painter.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(canvas_rect.min.x, sel_rect.min.y),
                    egui::pos2(sel_rect.min.x, sel_rect.max.y),
                ),
                0.0,
                dim,
            );
            painter.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(sel_rect.max.x, sel_rect.min.y),
                    egui::pos2(canvas_rect.max.x, sel_rect.max.y),
                ),
                0.0,
                dim,
            );

            // 绿色边框
            let is_dragging = matches!(
                self.canvas_mode,
                CanvasMode::MovingSelection | CanvasMode::ResizingSelection(_)
            );
            let border_color = if is_dragging {
                egui::Color32::from_rgb(255, 255, 0)
            } else {
                egui::Color32::from_rgb(0, 255, 0)
            };
            painter.rect_stroke(
                sel_rect,
                0.0,
                egui::Stroke::new(2.0, border_color),
                egui::StrokeKind::Outside,
            );

            // 尺寸标签
            let label = format!("{:.0}x{:.0}", sel[2], sel[3]);
            painter.text(
                sel_rect.min,
                egui::Align2::LEFT_TOP,
                label,
                egui::FontId::proportional(12.0),
                egui::Color32::WHITE,
            );

            // 调整手柄（四角和四边中点）
            let handles = [
                (sel_rect.left_top(), ResizeEdge::TopLeft),
                (sel_rect.center_top(), ResizeEdge::Top),
                (sel_rect.right_top(), ResizeEdge::TopRight),
                (sel_rect.right_center(), ResizeEdge::Right),
                (sel_rect.right_bottom(), ResizeEdge::BottomRight),
                (sel_rect.center_bottom(), ResizeEdge::Bottom),
                (sel_rect.left_bottom(), ResizeEdge::BottomLeft),
                (sel_rect.left_center(), ResizeEdge::Left),
            ];
            for (pos, edge) in handles {
                let is_hovered = hover_edge == Some(edge);
                let color = if is_hovered {
                    egui::Color32::from_rgb(255, 255, 0)
                } else {
                    egui::Color32::from_rgb(0, 255, 0)
                };
                let size = if is_hovered {
                    HANDLE_SIZE + 2.0
                } else {
                    HANDLE_SIZE
                };
                painter.rect_filled(
                    egui::Rect::from_center_size(pos, egui::vec2(size, size)),
                    1.0,
                    color,
                );
            }
        }

        // 鼠标在画布上时显示十字准星和坐标
        if response.hovered() {
            if let Some(wp) = world_pos {
                let sp = world_to_screen(wp);
                // 十字线
                painter.line_segment(
                    [egui::pos2(sp.x, img_rect.min.y), egui::pos2(sp.x, img_rect.max.y)],
                    egui::Stroke::new(1.0, egui::Color32::from_white_alpha(80)),
                );
                painter.line_segment(
                    [egui::pos2(img_rect.min.x, sp.y), egui::pos2(img_rect.max.x, sp.y)],
                    egui::Stroke::new(1.0, egui::Color32::from_white_alpha(80)),
                );
                // 坐标标签
                let label = format!("({:.0}, {:.0})", wp.x, wp.y);
                painter.text(
                    sp + egui::vec2(8.0, -8.0),
                    egui::Align2::LEFT_BOTTOM,
                    label,
                    egui::FontId::proportional(11.0),
                    egui::Color32::WHITE,
                );
            }
        }

        // 设置光标
        if response.hovered() {
            if let Some(edge) = hover_edge {
                let cursor = match edge {
                    ResizeEdge::Left | ResizeEdge::Right => egui::CursorIcon::ResizeHorizontal,
                    ResizeEdge::Top | ResizeEdge::Bottom => egui::CursorIcon::ResizeVertical,
                    ResizeEdge::TopLeft | ResizeEdge::BottomRight => {
                        egui::CursorIcon::ResizeNwSe
                    }
                    ResizeEdge::TopRight | ResizeEdge::BottomLeft => {
                        egui::CursorIcon::ResizeNeSw
                    }
                };
                ui.output_mut(|o| o.cursor_icon = cursor);
            } else if hover_inside {
                ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::Move);
            }
        }
    }

    /// 检测鼠标悬浮在选区的哪个边缘
    fn detect_hover_edge(
        &self,
        world_pos: Option<egui::Pos2>,
        sel: [f32; 4],
    ) -> (Option<ResizeEdge>, bool) {
        let Some(wp) = world_pos else {
            return (None, false);
        };
        let (sx, sy, sw, sh) = (sel[0], sel[1], sel[2], sel[3]);

        // 检查是否在选区内
        let inside = wp.x >= sx && wp.x <= sx + sw && wp.y >= sy && wp.y <= sy + sh;

        // 检查是否在边缘上
        let on_left = (wp.x - sx).abs() < EDGE_THRESHOLD;
        let on_right = (wp.x - (sx + sw)).abs() < EDGE_THRESHOLD;
        let on_top = (wp.y - sy).abs() < EDGE_THRESHOLD;
        let on_bottom = (wp.y - (sy + sh)).abs() < EDGE_THRESHOLD;

        let edge = if on_top && on_left {
            Some(ResizeEdge::TopLeft)
        } else if on_top && on_right {
            Some(ResizeEdge::TopRight)
        } else if on_bottom && on_left {
            Some(ResizeEdge::BottomLeft)
        } else if on_bottom && on_right {
            Some(ResizeEdge::BottomRight)
        } else if on_top {
            Some(ResizeEdge::Top)
        } else if on_bottom {
            Some(ResizeEdge::Bottom)
        } else if on_left {
            Some(ResizeEdge::Left)
        } else if on_right {
            Some(ResizeEdge::Right)
        } else {
            None
        };

        (edge, inside)
    }

    // ═══════════════════════════════════════════════════════════
    // 测试匹配
    // ═══════════════════════════════════════════════════════════

    fn test_match(&mut self, ctx: &egui::Context) {
        self.status = "正在截图并测试匹配...".to_string();
        let controller = match WindowsController::from_window_title(WINDOW_TITLE) {
            Ok(c) => c,
            Err(e) => {
                self.status = format!("连接窗口失败: {e}");
                return;
            }
        };
        thread::sleep(Duration::from_millis(200));
        let screen = match controller.screencap() {
            Ok(img) => img,
            Err(e) => {
                self.status = format!("截图失败: {e}");
                return;
            }
        };

        let Some(ref template_set) = self.template_set else {
            return;
        };
        let tpl = &template_set.templates[self.selected_index];
        let def = MatchDefinition::new(
            tpl.matcher_options(),
            std::sync::Arc::new(tpl.image.clone()),
        );

        let screen_luma = screen.to_luma32f();
        let result = SingleMatcher::match_definition(&screen_luma, &def);

        let matched = result.result.is_some();
        let match_value = result.result.map(|m| m.value);

        // Tab1: 截图 + 绿色匹配框
        let mut screen_rgba = screen.to_rgba8();
        if let Some(m) = result.result {
            let r = m.rect;
            draw_green_rect(&mut screen_rgba, r.x, r.y, r.width, r.height);
        }
        let (sw, sh) = (screen_rgba.width(), screen_rgba.height());
        let screenshot_tex = ctx.load_texture(
            "test_screenshot",
            egui::ColorImage::from_rgba_unmultiplied(
                [sw as usize, sh as usize],
                screen_rgba.as_raw(),
            ),
            egui::TextureOptions::LINEAR,
        );

        // Tab2: 热力图
        let normalized = normalize_luma32f(&result.matched_image);
        let luma8 = luma32f_to_luma8(&normalized);
        let (mw, mh) = (luma8.width(), luma8.height());
        let heatmap_rgba: Vec<u8> = luma8
            .as_raw()
            .iter()
            .flat_map(|&v| [v, v, v, 255])
            .collect();
        let heatmap_tex = ctx.load_texture(
            "test_heatmap",
            egui::ColorImage::from_rgba_unmultiplied([mw as usize, mh as usize], &heatmap_rgba),
            egui::TextureOptions::LINEAR,
        );

        self.status = if matched {
            format!("匹配成功 — 值: {:.4}", match_value.unwrap_or(0.0))
        } else {
            "未匹配".into()
        };

        self.test_result = Some(TestResult {
            screenshot_tex,
            heatmap_tex,
            matched,
            match_value,
            active_tab: 0,
        });
    }

    fn show_test_result(&mut self, ui: &mut egui::Ui) {
        let Some(ref mut test) = self.test_result else {
            return;
        };

        ui.label(egui::RichText::new("测试结果").strong());
        ui.horizontal(|ui| {
            if test.matched {
                ui.colored_label(
                    egui::Color32::from_rgb(0, 200, 0),
                    format!("匹配成功 — 值: {:.4}", test.match_value.unwrap_or(0.0)),
                );
            } else {
                ui.colored_label(egui::Color32::from_rgb(200, 0, 0), "未匹配");
            }
        });

        ui.horizontal(|ui| {
            if ui
                .selectable_label(test.active_tab == 0, "截图+匹配框")
                .clicked()
            {
                test.active_tab = 0;
            }
            if ui.selectable_label(test.active_tab == 1, "热力图").clicked() {
                test.active_tab = 1;
            }
        });

        let tex = if test.active_tab == 0 {
            &test.screenshot_tex
        } else {
            &test.heatmap_tex
        };
        let size = tex.size_vec2();
        let max_w = ui.available_width();
        let scale = (max_w / size.x.max(1.0)).min(1.0);
        egui::ScrollArea::both().max_height(300.0).show(ui, |ui| {
            ui.image(egui::load::SizedTexture::new(tex.id(), size * scale));
        });
    }
}

fn draw_green_rect(img: &mut image::RgbaImage, x: u32, y: u32, w: u32, h: u32) {
    let green = image::Rgba([0, 255, 0, 255]);
    let thickness = 2u32;
    let (iw, ih) = (img.width(), img.height());
    for t in 0..thickness {
        for px in x..=(x + w).min(iw - 1) {
            if y + t < ih {
                img.put_pixel(px, y + t, green);
            }
            if y + h >= t && y + h - t < ih {
                img.put_pixel(px, y + h - t, green);
            }
        }
        for py in y..=(y + h).min(ih - 1) {
            if x + t < iw {
                img.put_pixel(x + t, py, green);
            }
            if x + w >= t && x + w - t < iw {
                img.put_pixel(x + w - t, py, green);
            }
        }
    }
}
