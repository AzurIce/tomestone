use std::thread;

use eframe::egui;


use crate::app::App;
use crate::screen_region::{CaptureResult, RegionRect, ScreenRegion};

/// 区域调试页面状态
pub struct RegionDebugState {
    /// 预定义区域列表（静态）
    pub predefined_regions: Vec<ScreenRegion>,
    /// 用户自定义区域列表
    pub custom_regions: Vec<ScreenRegion>,
    /// 当前选中的索引
    /// 0..predefined.len() 为预定义区域
    /// predefined.len().. 为自定义区域
    pub selected_index: usize,
    /// 上次捕获结果（DynamicImage，用于保存或进一步处理）
    pub last_capture: Option<CaptureResult>,
    /// 全屏截图纹理
    pub screenshot_texture: Option<egui::TextureHandle>,
    /// 裁切结果纹理
    pub cropped_texture: Option<egui::TextureHandle>,
    /// 是否正在捕获中
    pub is_capturing: bool,
    /// 捕获日志
    pub logs: Vec<String>,
    /// 后台捕获消息接收器
    pub receiver: Option<std::sync::mpsc::Receiver<CaptureMessage>>,
    /// 是否显示添加自定义区域的表单
    pub show_add_form: bool,
    /// 添加自定义区域的表单
    pub new_custom_name: String,
    pub new_custom_x: u32,
    pub new_custom_y: u32,
    pub new_custom_w: u32,
    pub new_custom_h: u32,
}

impl Default for RegionDebugState {
    fn default() -> Self {
        Self {
            predefined_regions: ScreenRegion::all_predefined(),
            custom_regions: Vec::new(),
            selected_index: 0,
            last_capture: None,
            screenshot_texture: None,
            cropped_texture: None,
            is_capturing: false,
            logs: Vec::new(),
            receiver: None,
            show_add_form: false,
            new_custom_name: "自定义区域".to_string(),
            new_custom_x: 0,
            new_custom_y: 0,
            new_custom_w: 200,
            new_custom_h: 200,
        }
    }
}

impl RegionDebugState {
    /// 获取总区域数量
    pub fn total_count(&self) -> usize {
        self.predefined_regions.len() + self.custom_regions.len()
    }

    /// 根据索引获取区域引用
    pub fn get_region(&self, index: usize) -> Option<&ScreenRegion> {
        let pre_count = self.predefined_regions.len();
        if index < pre_count {
            self.predefined_regions.get(index)
        } else {
            self.custom_regions.get(index - pre_count)
        }
    }

    /// 删除自定义区域
    pub fn remove_custom_region(&mut self, index: usize) {
        let pre_count = self.predefined_regions.len();
        if index >= pre_count {
            let custom_idx = index - pre_count;
            if custom_idx < self.custom_regions.len() {
                self.custom_regions.remove(custom_idx);
                if self.selected_index >= self.total_count() && self.selected_index > 0 {
                    self.selected_index -= 1;
                }
            }
        }
    }

    /// 添加自定义区域
    pub fn add_custom_region(&mut self) {
        let name = if self.new_custom_name.trim().is_empty() {
            format!("自定义区域 {}", self.custom_regions.len() + 1)
        } else {
            self.new_custom_name.trim().to_string()
        };
        let rect = RegionRect::new(self.new_custom_x, self.new_custom_y, self.new_custom_w, self.new_custom_h);
        self.custom_regions.push(ScreenRegion::Custom { name, rect });
    }

    /// 清除上次结果
    pub fn clear_result(&mut self) {
        self.last_capture = None;
        self.screenshot_texture = None;
        self.cropped_texture = None;
    }

    /// 启动后台捕获任务
    pub fn start_capture(&mut self, region: ScreenRegion) {
        self.is_capturing = true;
        self.clear_result();
        self.logs.clear();
        self.logs.push(format!("启动捕获: {}", region.name()));

        let (tx, rx) = std::sync::mpsc::channel();
        self.receiver = Some(rx);

        thread::spawn(move || {
            match region.capture() {
                Ok(result) => {
                    let _ = tx.send(CaptureMessage::Done(result));
                }
                Err(e) => {
                    let _ = tx.send(CaptureMessage::Error(format!("{}", e)));
                }
            }
        });
    }

    /// 轮询后台捕获结果
    pub fn poll_capture_result(&mut self, ctx: &egui::Context) {
        let messages: Vec<CaptureMessage> = if let Some(ref receiver) = self.receiver {
            receiver.try_iter().collect()
        } else {
            return;
        };

        let mut finished = false;
        for msg in messages {
            match msg {
                CaptureMessage::Done(result) => {
                    self.logs.extend(result.logs.clone());
                    self.last_capture = Some(result);
                    self.is_capturing = false;
                    finished = true;
                }
                CaptureMessage::Error(e) => {
                    self.logs.push(format!("错误: {}", e));
                    self.is_capturing = false;
                    finished = true;
                }
            }
        }

        if finished {
            self.receiver = None;
            ctx.request_repaint();
        }

        if self.is_capturing {
            ctx.request_repaint();
        }
    }
}

/// 后台捕获消息
#[derive(Debug)]
pub enum CaptureMessage {
    /// 成功完成
    Done(CaptureResult),
    /// 出错
    Error(String),
}

// ═══════════════════════════════════════════════════════════
// UI 实现
// ═══════════════════════════════════════════════════════════

impl App {
    pub fn show_region_debug_page(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                // ── 左侧列表 ──
                let left_width = 220.0;
                ui.vertical(|ui| {
                    ui.set_max_width(left_width);
                    ui.set_min_width(left_width);
                    self.show_region_list(ui);
                });

                ui.separator();

                // ── 右侧详情 ──
                ui.vertical(|ui| {
                    ui.set_max_width(ui.available_width());
                    self.show_region_detail(ui, ctx);
                });
            });
        });

        // 轮询后台捕获结果
        self.region_debug.poll_capture_result(ctx);
    }

    /// 左侧区域列表
    fn show_region_list(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("区域列表").strong().size(14.0));
        ui.add_space(4.0);

        let state = &mut self.region_debug;
        let pre_count = state.predefined_regions.len();

        egui::ScrollArea::vertical().show(ui, |ui| {
            // 预定义区域分组
            ui.label(egui::RichText::new("预定义区域").small().weak());
            for i in 0..pre_count {
                let region = state.predefined_regions[i].clone();
                let is_selected = state.selected_index == i;
                let label = format!("{}", region.name());
                if ui.selectable_label(is_selected, label).clicked() {
                    state.selected_index = i;
                    state.clear_result();
                }
                ui.label(
                    egui::RichText::new(region.description())
                        .small()
                        .weak(),
                );
                ui.add_space(2.0);
            }

            // 添加自定义区域表单
            if state.show_add_form {
                ui.add_space(4.0);
                ui.group(|ui| {
                    ui.label(egui::RichText::new("添加自定义区域").small().strong());
                    ui.horizontal(|ui| {
                        ui.label("名称:");
                        ui.text_edit_singleline(&mut state.new_custom_name);
                    });
                    ui.horizontal(|ui| {
                        ui.label("X:");
                        ui.add(egui::DragValue::new(&mut state.new_custom_x));
                        ui.label("Y:");
                        ui.add(egui::DragValue::new(&mut state.new_custom_y));
                    });
                    ui.horizontal(|ui| {
                        ui.label("宽:");
                        ui.add(egui::DragValue::new(&mut state.new_custom_w).range(1..=9999));
                        ui.label("高:");
                        ui.add(egui::DragValue::new(&mut state.new_custom_h).range(1..=9999));
                    });
                    if ui.button("添加").clicked() {
                        state.add_custom_region();
                        state.show_add_form = false;
                    }
                });
            }

            ui.add_space(4.0);
            ui.separator();
            ui.add_space(4.0);

            // 自定义区域分组
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("自定义区域").small().weak());
                if ui.button("+").clicked() {
                    state.show_add_form = !state.show_add_form;
                }
            });

            let custom_count = state.custom_regions.len();
            if custom_count == 0 {
                ui.label(egui::RichText::new("无自定义区域").small().weak());
            } else {
                for i in 0..custom_count {
                    let global_idx = pre_count + i;
                    let region = state.custom_regions[i].clone();
                    let is_selected = state.selected_index == global_idx;
                    let desc = region.description();
                    ui.horizontal(|ui| {
                        if ui.selectable_label(is_selected, region.name()).clicked() {
                            state.selected_index = global_idx;
                            state.clear_result();
                        }
                        if ui.button("×").clicked() {
                            state.remove_custom_region(global_idx);
                        }
                    });
                    ui.label(
                        egui::RichText::new(desc)
                            .small()
                            .weak(),
                    );
                    ui.add_space(2.0);
                }
            }
        });
    }

    /// 右侧区域详情与操作
    fn show_region_detail(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // 先只读获取 region，避免长时间借用 self.region_debug
        let region = {
            let state = &self.region_debug;
            state.get_region(state.selected_index).cloned()
        };

        let Some(region) = region else {
            ui.label("请选择一个区域");
            return;
        };

        // 标题
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(region.name())
                    .strong()
                    .size(16.0),
            );
            if region.is_predefined() {
                ui.label(
                    egui::RichText::new("预定义")
                        .small()
                        .color(ui.visuals().warn_fg_color),
                );
            } else {
                ui.label(
                    egui::RichText::new("自定义")
                        .small()
                        .color(ui.visuals().hyperlink_color),
                );
            }
        });

        ui.label(
            egui::RichText::new(region.description())
                .small()
                .weak(),
        );
        ui.add_space(8.0);

        // 操作按钮
        let mut should_capture = false;
        let mut should_clear = false;
        {
            let state = &self.region_debug;
            ui.horizontal(|ui| {
                let can_capture = !state.is_capturing;
                if ui
                    .add_enabled(
                        can_capture,
                        egui::Button::new(format!(
                            "{} 捕获",
                            egui_phosphor::regular::CAMERA
                        )),
                    )
                    .clicked()
                {
                    should_capture = true;
                }

                if state.last_capture.is_some()
                    && ui
                        .button(format!(
                            "{} 清除结果",
                            egui_phosphor::regular::TRASH
                        ))
                        .clicked()
                {
                    should_clear = true;
                }
            });
        }

        if should_capture {
            self.region_debug.start_capture(region);
        }
        if should_clear {
            let state = &mut self.region_debug;
            state.clear_result();
            state.logs.clear();
        }

        {
            let state = &self.region_debug;
            if state.is_capturing {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("正在捕获...");
                });
            }
        }

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);

        // 结果显示
        if self.region_debug.last_capture.is_some() {
            let capture = self.region_debug.last_capture.take().unwrap();
            show_capture_result(&mut self.region_debug, ui, ctx, &capture);
            self.region_debug.last_capture = Some(capture);
        } else {
            ui.label("点击「捕获」按钮进行测试");
        }

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);

        // 日志
        show_capture_logs(&mut self.region_debug, ui);
    }
}

/// 显示捕获结果（截图 + 裁切图）
fn show_capture_result(
    state: &mut RegionDebugState,
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    capture: &CaptureResult,
) {
    // 懒加载纹理
    if state.screenshot_texture.is_none() {
        let rgba = capture.full_screenshot.to_rgba8();
        let (w, h) = (rgba.width(), rgba.height());
        let color_image = egui::ColorImage::from_rgba_unmultiplied(
            [w as usize, h as usize],
            rgba.as_raw(),
        );
        state.screenshot_texture = Some(ctx.load_texture(
            "region_debug_screenshot",
            color_image,
            egui::TextureOptions::LINEAR,
        ));
    }

    if state.cropped_texture.is_none() {
        let rgba = capture.cropped.to_rgba8();
        let (w, h) = (rgba.width(), rgba.height());
        let color_image = egui::ColorImage::from_rgba_unmultiplied(
            [w as usize, h as usize],
            rgba.as_raw(),
        );
        state.cropped_texture = Some(ctx.load_texture(
            "region_debug_cropped",
            color_image,
            egui::TextureOptions::LINEAR,
        ));
    }

    // ── 双栏布局：左侧裁切结果，右侧全屏截图 ──
    ui.horizontal_top(|ui| {
        // 左侧：裁切结果
        ui.vertical(|ui| {
            ui.set_max_width(300.0);
            ui.label(egui::RichText::new("裁切结果").strong());
            if let Some(ref tex) = state.cropped_texture {
                let size = tex.size_vec2();
                let max_w = ui.available_width().min(280.0);
                let scale = (max_w / size.x.max(1.0)).min(3.0);
                let display_size = size * scale;
                ui.image(egui::load::SizedTexture::new(tex.id(), display_size));
                ui.label(
                    egui::RichText::new(format!("尺寸: {}x{}", size.x as u32, size.y as u32))
                        .small()
                        .weak(),
                );
            }
        });

        ui.separator();

        // 右侧：全屏截图（带裁切框高亮）
        ui.vertical(|ui| {
            ui.label(egui::RichText::new("全屏截图（绿色框为裁切区域）").strong());
            if let Some(ref tex) = state.screenshot_texture {
                let size = tex.size_vec2();
                let max_w = ui.available_width().min(500.0);
                let scale = max_w / size.x.max(1.0);
                let display_size = size * scale;

                let (response, painter) =
                    ui.allocate_painter(display_size, egui::Sense::hover());
                let rect = response.rect;

                painter.image(
                    tex.id(),
                    rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );

                // 在截图上绘制绿色裁切框
                if let Some(ref det_rect) = capture.detected_rect {
                    let sx = rect.min.x + det_rect.x as f32 * scale;
                    let sy = rect.min.y + det_rect.y as f32 * scale;
                    let sw = det_rect.width as f32 * scale;
                    let sh = det_rect.height as f32 * scale;
                    let sel_rect = egui::Rect::from_min_size(
                        egui::pos2(sx, sy),
                        egui::vec2(sw, sh),
                    );
                    painter.rect_stroke(
                        sel_rect,
                        0.0,
                        egui::Stroke::new(2.0, egui::Color32::from_rgb(0, 255, 0)),
                        egui::StrokeKind::Outside,
                    );

                    // 标注坐标
                    let label = format!(
                        "({}, {}) {}x{}",
                        det_rect.x, det_rect.y, det_rect.width, det_rect.height
                    );
                    painter.text(
                        sel_rect.min + egui::vec2(2.0, 2.0),
                        egui::Align2::LEFT_TOP,
                        label,
                        egui::FontId::monospace(10.0),
                        egui::Color32::from_rgb(0, 255, 0),
                    );
                }
            }
        });
    });
}

/// 显示捕获日志
fn show_capture_logs(state: &mut RegionDebugState, ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("执行日志").strong());
    egui::ScrollArea::vertical()
        .max_height(200.0)
        .stick_to_bottom(true)
        .show(ui, |ui| {
            if state.logs.is_empty() {
                ui.label(egui::RichText::new("暂无日志").small().weak());
            } else {
                for line in &state.logs {
                    ui.label(egui::RichText::new(line).small().monospace());
                }
            }
        });
}
