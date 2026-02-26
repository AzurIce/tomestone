use std::thread;
use std::time::Duration;

use auto_play::cv::matcher::SingleMatcher;
use auto_play::cv::utils::{luma32f_to_luma8, normalize_luma32f};
use auto_play::{ControllerTrait, WindowsController};
use eframe::egui;
use image::DynamicImage;

use crate::template::{TemplateDef, TemplateSet};

const WINDOW_TITLE: &str = "最终幻想XIV";

pub struct TestResult {
    pub screenshot_tex: egui::TextureHandle,
    pub heatmap_tex: egui::TextureHandle,
    pub matched: bool,
    pub match_value: Option<f32>,
    pub active_tab: usize,
}

pub struct TemplateEditorState {
    template_set: Option<TemplateSet>,
    template_defs: Option<&'static [TemplateDef]>,
    selected_index: usize,
    // 截图/裁剪
    screenshot_image: Option<DynamicImage>,
    screenshot_texture: Option<egui::TextureHandle>,
    image_size: (u32, u32),
    zoom: f32,
    drag_start: Option<egui::Pos2>,
    drag_end: Option<egui::Pos2>,
    selection: Option<[f32; 4]>,
    status: String,
    // 模板预览
    template_texture: Option<egui::TextureHandle>,
    // 测试结果
    test_result: Option<TestResult>,
}

impl Default for TemplateEditorState {
    fn default() -> Self {
        Self {
            template_set: None,
            template_defs: None,
            selected_index: 0,
            screenshot_image: None,
            screenshot_texture: None,
            image_size: (0, 0),
            zoom: 1.0,
            drag_start: None,
            drag_end: None,
            selection: None,
            status: String::new(),
            template_texture: None,
            test_result: None,
        }
    }
}

impl TemplateEditorState {
    /// 确保模板已加载（切换到模板编辑 tab 时调用）
    pub fn ensure_loaded(&mut self, defs: &'static [TemplateDef]) {
        if self.template_defs.is_some_and(|d| std::ptr::eq(d, defs)) && self.template_set.is_some()
        {
            return;
        }
        self.template_defs = Some(defs);
        self.template_set = Some(TemplateSet::load(defs));
        self.selected_index = 0;
        self.status = String::new();
        self.screenshot_image = None;
        self.screenshot_texture = None;
        self.selection = None;
        self.template_texture = None;
        self.test_result = None;
    }

    pub fn template_set(&self) -> Option<&TemplateSet> {
        self.template_set.as_ref()
    }

    /// 内联渲染（直接在 CentralPanel 里调用）
    pub fn show_inline(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.show_inner(ui, ctx);
    }

    fn show_inner(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // 先收集模板名称列表，避免借用冲突
        let template_labels: Vec<(usize, String)> = {
            let Some(ref template_set) = self.template_set else {
                return;
            };
            if template_set.templates.is_empty() {
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
            let res = ui.vertical(|ui| {
                ui.label(egui::RichText::new("模板列表").strong());

                let (rect, _) = ui.allocate_at_least(egui::vec2(0.0, 6.0), egui::Sense::empty());

                for (i, label) in &template_labels {
                    if ui
                        .selectable_label(self.selected_index == *i, label)
                        .clicked()
                    {
                        self.selected_index = *i;
                        self.template_texture = None;
                        self.test_result = None;
                    }
                }

                rect
            });

            let mut rect = res.inner;
            rect.set_width(res.response.rect.width());
            ui.painter().hline(
                rect.left()..=rect.right(),
                rect.center().y,
                ui.visuals().widgets.noninteractive.bg_stroke,
            );

            ui.separator();

            // 右侧: 编辑区（限制到剩余宽度）
            let right_width = ui.available_width();
            ui.vertical(|ui| {
                ui.set_max_width(right_width);
                self.show_right_panel(ui, ctx);
            });
        });
    }

    fn show_right_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let Some(ref template_set) = self.template_set else {
            return;
        };
        let tpl = &template_set.templates[self.selected_index];

        // 标题行
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!("当前: {}", tpl.def.name))
                    .strong()
                    .size(14.0),
            );
            ui.separator();
            ui.label(if tpl.is_custom { "自定义" } else { "默认" });
        });

        // 模板缩略图预览
        self.show_template_preview(ui, ctx);

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
                    has_selection,
                    egui::Button::new(format!("{} 确认裁剪", egui_phosphor::regular::CROP)),
                )
                .clicked()
            {
                self.save_selection(ctx);
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

        // 缩放控制
        if self.screenshot_texture.is_some() {
            ui.horizontal(|ui| {
                ui.label("缩放:");
                ui.add(egui::Slider::new(&mut self.zoom, 0.1..=3.0).step_by(0.1));
                if ui.button("1:1").clicked() {
                    self.zoom = 1.0;
                }
                if ui.button("适应").clicked() {
                    let avail = ui.available_width();
                    let img_w = self.image_size.0 as f32;
                    if img_w > 0.0 {
                        self.zoom = (avail / img_w).min(1.0);
                    }
                }
            });
        }

        ui.add_space(4.0);
        self.show_screenshot_canvas(ui);

        // 测试结果
        if self.test_result.is_some() {
            ui.add_space(8.0);
            ui.separator();
            self.show_test_result(ui);
        }
    }

    fn show_template_preview(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let Some(ref template_set) = self.template_set else {
            return;
        };
        let tpl = &template_set.templates[self.selected_index];

        // 懒加载模板纹理
        if self.template_texture.is_none() {
            let rgba = tpl.image.to_rgba8();
            let (w, h) = (rgba.width(), rgba.height());
            let color_image =
                egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], rgba.as_raw());
            self.template_texture =
                Some(ctx.load_texture("tpl_preview", color_image, egui::TextureOptions::LINEAR));
        }

        if let Some(ref tex) = self.template_texture {
            let size = tex.size_vec2();
            let max_h_scale = (120.0 / size.y.max(1.0)).min(2.0);
            let max_w_scale = ui.available_width() / size.x.max(1.0);
            let scale = max_h_scale.min(max_w_scale);
            ui.image(egui::load::SizedTexture::new(tex.id(), size * scale));
        }
    }

    fn take_screenshot(&mut self, ctx: &egui::Context) {
        self.status = format!("正在捕获 '{}'...", WINDOW_TITLE);
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
                        self.selection = None;
                        self.drag_start = None;
                        self.drag_end = None;
                        self.status = format!("已捕获 {}x{} — 拖拽框选区域", w, h);
                    }
                    Err(e) => self.status = format!("截图失败: {e}"),
                }
            }
            Err(e) => self.status = format!("连接窗口失败: {e}"),
        }
    }

    fn save_selection(&mut self, ctx: &egui::Context) {
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
        let Some(ref mut template_set) = self.template_set else {
            return;
        };
        let tpl = &mut template_set.templates[self.selected_index];
        match tpl.save_custom(cropped) {
            Ok(()) => {
                self.status = format!("已保存 {}x{} 为自定义模板", w, h);
                self.template_texture = None; // 强制刷新预览
            }
            Err(e) => self.status = format!("保存失败: {e}"),
        }
        // 刷新模板预览纹理
        self.refresh_template_texture(ctx);
    }

    fn reset_current(&mut self, ctx: &egui::Context) {
        let Some(ref mut template_set) = self.template_set else {
            return;
        };
        template_set.templates[self.selected_index].reset_to_default();
        self.template_texture = None;
        self.status = "已重置为默认模板".into();
        self.refresh_template_texture(ctx);
    }

    fn refresh_template_texture(&mut self, ctx: &egui::Context) {
        let Some(ref template_set) = self.template_set else {
            return;
        };
        let tpl = &template_set.templates[self.selected_index];
        let rgba = tpl.image.to_rgba8();
        let (w, h) = (rgba.width(), rgba.height());
        let color_image =
            egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], rgba.as_raw());
        self.template_texture =
            Some(ctx.load_texture("tpl_preview", color_image, egui::TextureOptions::LINEAR));
    }

    fn test_match(&mut self, ctx: &egui::Context) {
        self.status = format!("正在截图并测试匹配...");
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
        let options = tpl.matcher_options();

        let screen_luma = screen.to_luma32f();
        let tpl_luma = tpl.image.to_luma32f();
        let result = SingleMatcher::match_template(&screen_luma, &tpl_luma, &options);

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

    fn show_screenshot_canvas(&mut self, ui: &mut egui::Ui) {
        let Some(ref texture) = self.screenshot_texture else {
            return;
        };

        egui::ScrollArea::both()
            .max_height(ui.available_height().max(200.0).min(400.0))
            .show(ui, |ui| {
                let img_w = self.image_size.0 as f32;
                let img_h = self.image_size.1 as f32;
                let scale = self.zoom;
                let display_w = (img_w * scale).min(ui.available_width().max(100.0));
                let display_h = img_h * (display_w / img_w.max(1.0));

                let (response, painter) =
                    ui.allocate_painter(egui::vec2(display_w, display_h), egui::Sense::drag());
                let rect = response.rect;

                painter.image(
                    texture.id(),
                    rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );

                // 滚轮缩放
                let scroll = ui.input(|i| i.raw_scroll_delta.y);
                if scroll != 0.0 && response.hovered() {
                    self.zoom = (self.zoom + scroll * 0.001).clamp(0.1, 3.0);
                }

                // 拖拽框选
                if response.drag_started() {
                    if let Some(pos) = response.interact_pointer_pos() {
                        self.drag_start = Some(pos);
                        self.drag_end = Some(pos);
                        self.selection = None;
                    }
                }
                if response.dragged() {
                    if let Some(pos) = response.interact_pointer_pos() {
                        self.drag_end = Some(pos);
                    }
                }
                if response.drag_stopped() {
                    if let (Some(start), Some(end)) = (self.drag_start, self.drag_end) {
                        let to_img = |sp: egui::Pos2| -> (f32, f32) {
                            let x = ((sp.x - rect.min.x) / scale).clamp(0.0, img_w);
                            let y = ((sp.y - rect.min.y) / scale).clamp(0.0, img_h);
                            (x, y)
                        };
                        let (x1, y1) = to_img(start);
                        let (x2, y2) = to_img(end);
                        let x = x1.min(x2);
                        let y = y1.min(y2);
                        let w = (x1 - x2).abs();
                        let h = (y1 - y2).abs();
                        if w > 2.0 && h > 2.0 {
                            self.selection = Some([x, y, w, h]);
                        }
                    }
                }

                // 遮罩 + 绿色边框
                let draw_selection = |start: egui::Pos2, end: egui::Pos2| {
                    let sel = egui::Rect::from_two_pos(start, end);
                    let dim = egui::Color32::from_black_alpha(100);
                    painter.rect_filled(
                        egui::Rect::from_min_max(rect.min, egui::pos2(rect.max.x, sel.min.y)),
                        0.0,
                        dim,
                    );
                    painter.rect_filled(
                        egui::Rect::from_min_max(egui::pos2(rect.min.x, sel.max.y), rect.max),
                        0.0,
                        dim,
                    );
                    painter.rect_filled(
                        egui::Rect::from_min_max(
                            egui::pos2(rect.min.x, sel.min.y),
                            egui::pos2(sel.min.x, sel.max.y),
                        ),
                        0.0,
                        dim,
                    );
                    painter.rect_filled(
                        egui::Rect::from_min_max(
                            egui::pos2(sel.max.x, sel.min.y),
                            egui::pos2(rect.max.x, sel.max.y),
                        ),
                        0.0,
                        dim,
                    );
                    painter.rect_stroke(
                        sel,
                        0.0,
                        egui::Stroke::new(2.0, egui::Color32::from_rgb(0, 255, 0)),
                        egui::StrokeKind::Outside,
                    );
                };

                if let (Some(start), Some(end)) = (self.drag_start, self.drag_end) {
                    if self.selection.is_none() {
                        draw_selection(start, end);
                    }
                }

                if let Some([x, y, w, h]) = self.selection {
                    let s = egui::pos2(rect.min.x + x * scale, rect.min.y + y * scale);
                    let e = egui::pos2(rect.min.x + (x + w) * scale, rect.min.y + (y + h) * scale);
                    draw_selection(s, e);
                }
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
            if ui
                .selectable_label(test.active_tab == 1, "热力图")
                .clicked()
            {
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
        // 上下边
        for px in x..=(x + w).min(iw - 1) {
            if y + t < ih {
                img.put_pixel(px, y + t, green);
            }
            if y + h >= t && y + h - t < ih {
                img.put_pixel(px, y + h - t, green);
            }
        }
        // 左右边
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
