use std::sync::Arc;

use ap_xiv::auto_craft::{AutoCraft, AutoCraftConfig, AutoCraftEvent, AutoCraftHandle, CraftTemplates};
use ap_xiv::craft_info::{CraftInfo, CraftInfoConfig, CraftInfoExtractor};
use auto_play::{ControllerTrait, MatchDefinition, MatcherOptions, WindowsController};
use eframe::egui;

use crate::app::App;
use crate::app_templates::{AC_TPL_START, AC_TPL_STOP, AUTO_CRAFT_TEMPLATES};
use crate::template::TemplateSet;

#[derive(Default, Clone, Copy, PartialEq)]
pub enum ToolboxTab {
    #[default]
    AutoCraft,
    CraftInfo,
    TemplateEditor,
}

/// 自动制作工具的运行状态
pub enum AutoCraftState {
    /// 空闲
    Idle,
    /// 运行中
    Running(AutoCraftHandle),
}

/// 自动制作工具的 UI 状态
pub struct AutoCraftUi {
    pub state: AutoCraftState,
    pub count: u32,
    pub infinite: bool,
    pub macro_key: String,
    pub progress: (u32, u32),
    pub status: String,
    pub log: Vec<String>,
    pub tab: ToolboxTab,
    // 制造信息提取
    pub craft_info_result: Option<CraftInfo>,
    pub craft_info_status: String,
    pub craft_info_extracting: bool,
    pub craft_info_receiver: Option<std::sync::mpsc::Receiver<Result<CraftInfo, String>>>,
    pub craft_info_auto_refresh: bool,
    pub craft_info_last_refresh: f64,
}

impl Default for AutoCraftUi {
    fn default() -> Self {
        Self {
            state: AutoCraftState::Idle,
            count: 10,
            infinite: false,
            macro_key: "r".to_string(),
            progress: (0, 0),
            status: "就绪".to_string(),
            log: Vec::new(),
            tab: ToolboxTab::default(),
            craft_info_result: None,
            craft_info_status: "点击提取按钮获取制造信息".to_string(),
            craft_info_extracting: false,
            craft_info_receiver: None,
            craft_info_auto_refresh: false,
            craft_info_last_refresh: 0.0,
        }
    }
}

impl App {
    pub fn show_toolbox_page(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            // Tab 栏
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.auto_craft.tab, ToolboxTab::AutoCraft, "自动制作");
                ui.selectable_value(
                    &mut self.auto_craft.tab,
                    ToolboxTab::CraftInfo,
                    "制造信息",
                );
                ui.selectable_value(
                    &mut self.auto_craft.tab,
                    ToolboxTab::TemplateEditor,
                    "模板匹配设置",
                );
            });
            ui.separator();

            match self.auto_craft.tab {
                ToolboxTab::AutoCraft => {
                    self.show_auto_craft_content(ui);
                }
                ToolboxTab::CraftInfo => {
                    self.show_craft_info_content(ui);
                }
                ToolboxTab::TemplateEditor => {
                    self.template_editor.ensure_loaded(AUTO_CRAFT_TEMPLATES);
                    self.template_editor.show_inline(ui, ctx);
                }
            }

            self.poll_auto_craft_messages();

            if matches!(self.auto_craft.state, AutoCraftState::Running(..))
                || self.auto_craft.craft_info_auto_refresh
            {
                ctx.request_repaint();
            }
        });
    }

    fn show_auto_craft_content(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.label(egui::RichText::new("自动制作").strong().size(16.0));
            ui.label(
                egui::RichText::new(
                    "自动循环执行制作: 点击开始 → 等待制作窗口 → 按宏键 → 等待完成",
                )
                .small()
                .weak(),
            );
            ui.add_space(4.0);

            let is_running = matches!(self.auto_craft.state, AutoCraftState::Running(..));

            ui.horizontal(|ui| {
                ui.label("制作次数:");
                ui.add_enabled(
                    !is_running && !self.auto_craft.infinite,
                    egui::DragValue::new(&mut self.auto_craft.count).range(1..=999),
                );
                ui.add_space(8.0);
                ui.add_enabled(
                    !is_running,
                    egui::Checkbox::new(&mut self.auto_craft.infinite, "无限循环"),
                );
                ui.add_space(16.0);
                ui.label("宏按键:");
                ui.add_enabled(
                    !is_running,
                    egui::TextEdit::singleline(&mut self.auto_craft.macro_key).desired_width(30.0),
                );
            });

            ui.add_space(4.0);

            ui.horizontal(|ui| {
                if is_running {
                    if ui
                        .button(format!("{} 停止", egui_phosphor::regular::STOP_CIRCLE))
                        .clicked()
                    {
                        if let AutoCraftState::Running(ref handle) = self.auto_craft.state {
                            handle.stop();
                        }
                    }
                } else if ui
                    .button(format!("{} 开始制作", egui_phosphor::regular::PLAY_CIRCLE))
                    .clicked()
                {
                    self.start_auto_craft();
                }

                ui.add_space(8.0);

                let (done, total) = self.auto_craft.progress;
                if total > 0 {
                    let frac = done as f32 / total as f32;
                    ui.add(
                        egui::ProgressBar::new(frac)
                            .text(format!("{}/{}", done, total))
                            .desired_width(200.0),
                    );
                } else if self.auto_craft.infinite {
                    ui.label(format!("已完成 {} 次", done));
                }
            });

            ui.add_space(4.0);
            ui.label(&self.auto_craft.status);

            if !self.auto_craft.log.is_empty() {
                ui.add_space(4.0);
                ui.collapsing("日志", |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(200.0)
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            for line in &self.auto_craft.log {
                                ui.label(egui::RichText::new(line).small().monospace());
                            }
                        });
                });
            }
        });
    }

    fn start_auto_craft(&mut self) {
        let count = self.auto_craft.count;
        let infinite = self.auto_craft.infinite;
        let macro_key = self.auto_craft.macro_key.chars().next().unwrap_or('r');

        let tpl_set = self
            .template_editor
            .template_set()
            .cloned()
            .unwrap_or_else(|| TemplateSet::load(Default::default(), AUTO_CRAFT_TEMPLATES));

        let start_img = tpl_set.templates[AC_TPL_START].image.clone();
        let stop_img = tpl_set.templates[AC_TPL_STOP].image.clone();
        let start_threshold = tpl_set.templates[AC_TPL_START].def.threshold;
        let stop_threshold = tpl_set.templates[AC_TPL_STOP].def.threshold;

        let templates = CraftTemplates {
            start: MatchDefinition::new(
                MatcherOptions::default().with_threshold(start_threshold),
                Arc::new(start_img),
            ),
            stop: MatchDefinition::new(
                MatcherOptions::default().with_threshold(stop_threshold),
                Arc::new(stop_img),
            ),
        };

        self.auto_craft.progress = (0, if infinite { 0 } else { count });
        self.auto_craft.status = "启动中...".to_string();
        self.auto_craft.log.clear();

        match AutoCraft::start(AutoCraftConfig {
            count,
            infinite,
            macro_key,
            templates,
        }) {
            Ok(handle) => {
                self.auto_craft.state = AutoCraftState::Running(handle);
            }
            Err(e) => {
                let line = format!("启动失败: {}", e);
                self.auto_craft.status = line.clone();
                self.auto_craft.log.push(line);
            }
        }
    }

    fn poll_auto_craft_messages(&mut self) {
        let messages: Vec<AutoCraftEvent> =
            if let AutoCraftState::Running(ref handle) = self.auto_craft.state {
                handle.receiver.try_iter().collect()
            } else {
                return;
            };

        let mut finished = false;
        for msg in messages {
            match msg {
                AutoCraftEvent::Status(s) => {
                    self.auto_craft.status = s.clone();
                    self.auto_craft.log.push(s);
                }
                AutoCraftEvent::Progress(done, total) => {
                    self.auto_craft.progress = (done, total);
                }
                AutoCraftEvent::CraftDone {
                    index,
                    elapsed_secs,
                } => {
                    let line = format!("#{} 完成 ({:.1}s)", index, elapsed_secs);
                    self.auto_craft.status = line.clone();
                    self.auto_craft.log.push(line);
                }
                AutoCraftEvent::CraftFailed { index, reason } => {
                    let line = format!("#{} 失败: {}", index, reason);
                    self.auto_craft.status = line.clone();
                    self.auto_craft.log.push(line);
                    finished = true;
                }
                AutoCraftEvent::Finished { success, total } => {
                    let line = format!("完成: {}/{} 成功", success, total);
                    self.auto_craft.status = line.clone();
                    self.auto_craft.log.push(line);
                    finished = true;
                }
                AutoCraftEvent::Error(e) => {
                    let line = format!("错误: {}", e);
                    self.auto_craft.status = line.clone();
                    self.auto_craft.log.push(line);
                    finished = true;
                }
            }
        }

        if finished {
            self.auto_craft.state = AutoCraftState::Idle;
        }
    }

    fn show_craft_info_content(&mut self, ui: &mut egui::Ui) {
        // 自动刷新逻辑
        if self.auto_craft.craft_info_auto_refresh && !self.auto_craft.craft_info_extracting {
            let now = ui.ctx().input(|i| i.time);
            if now - self.auto_craft.craft_info_last_refresh >= 2.0 {
                self.auto_craft.craft_info_last_refresh = now;
                self.extract_craft_info();
            }
        }

        ui.group(|ui| {
            ui.label(egui::RichText::new("制造信息提取").strong().size(16.0));
            ui.label(
                egui::RichText::new("从制造窗口提取当前耐久、进展、品质信息")
                    .small()
                    .weak(),
            );
            ui.add_space(4.0);

            // 检查是否有提取结果返回
            if self.auto_craft.craft_info_extracting {
                if let Some(ref rx) = self.auto_craft.craft_info_receiver {
                    match rx.try_recv() {
                        Ok(result) => {
                            self.auto_craft.craft_info_extracting = false;
                            self.auto_craft.craft_info_receiver = None;
                            match result {
                                Ok(info) => {
                                    self.auto_craft.craft_info_result = Some(info.clone());
                                    self.auto_craft.craft_info_status = format!(
                                        "提取成功 - 耐久 {}/{}, 进展 {}/{}, 品质 {}/{}",
                                        info.durability.0,
                                        info.durability.1,
                                        info.progress.0,
                                        info.progress.1,
                                        info.quality.0,
                                        info.quality.1,
                                    );
                                }
                                Err(e) => {
                                    self.auto_craft.craft_info_status = format!("提取失败: {}", e);
                                }
                            }
                        }
                        Err(std::sync::mpsc::TryRecvError::Empty) => {
                            // 还在提取中，继续显示 spinner
                        }
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            self.auto_craft.craft_info_extracting = false;
                            self.auto_craft.craft_info_receiver = None;
                            self.auto_craft.craft_info_status = "提取线程异常终止".to_string();
                        }
                    }
                }
            }

            ui.horizontal(|ui| {
                if self.auto_craft.craft_info_extracting {
                    ui.add(egui::Spinner::new());
                    ui.label("正在提取...");
                } else {
                    if ui.button("提取信息").clicked() {
                        self.extract_craft_info();
                    }
                }

                ui.add_space(8.0);
                ui.checkbox(
                    &mut self.auto_craft.craft_info_auto_refresh,
                    "自动刷新 (2s)",
                );
            });

            ui.add_space(4.0);
            ui.label(&self.auto_craft.craft_info_status);

            if let Some(info) = &self.auto_craft.craft_info_result {
                ui.add_space(8.0);
                ui.group(|ui| {
                    ui.label(format!(
                        "耐久: {} / {}",
                        info.durability.0, info.durability.1
                    ));
                    ui.label(format!(
                        "进展: {} / {}",
                        info.progress.0, info.progress.1
                    ));
                    ui.label(format!(
                        "品质: {} / {}",
                        info.quality.0, info.quality.1
                    ));
                    if let Some(hq) = info.hq_rate {
                        ui.label(format!("优质率: {}%", hq));
                    }
                });
            }
        });
    }

    fn extract_craft_info(&mut self) {
        self.auto_craft.craft_info_extracting = true;
        self.auto_craft.craft_info_status = "正在提取...".to_string();
        self.auto_craft.craft_info_result = None;

        let tpl_set = self
            .template_editor
            .template_set()
            .cloned()
            .unwrap_or_else(|| TemplateSet::load(Default::default(), AUTO_CRAFT_TEMPLATES));

        let stop_img = tpl_set.templates[AC_TPL_STOP].image.clone();
        let stop_threshold = tpl_set.templates[AC_TPL_STOP].def.threshold;

        let match_def = MatchDefinition::new(
            MatcherOptions::default().with_threshold(stop_threshold),
            Arc::new(stop_img),
        );

        let mut config = CraftInfoConfig::new(0.55);
        config.set_abort_button_match_def(match_def);

        // 在主线程中获取截图（避免跨线程传递 Controller）
        // 尝试多个可能的窗口标题（国服/国际服）
        let screenshot = match Self::find_game_window_and_capture() {
            Ok(img) => img,
            Err(e) => {
                self.auto_craft.craft_info_extracting = false;
                self.auto_craft.craft_info_status = format!("无法找到游戏窗口: {}", e);
                return;
            }
        };

        let (tx, rx) = std::sync::mpsc::channel();
        self.auto_craft.craft_info_receiver = Some(rx);

        std::thread::spawn(move || {
            let mut extractor = CraftInfoExtractor::new(config);
            let result = extractor.extract(&screenshot);
            let _ = tx.send(result.map_err(|e| e.to_string()));
        });
    }

    /// 尝试多个可能的窗口标题来查找 FF14 游戏窗口
    fn find_game_window_and_capture() -> anyhow::Result<image::DynamicImage> {
        const POSSIBLE_TITLES: &[&str] = &["最终幻想XIV"];

        for title in POSSIBLE_TITLES {
            match WindowsController::from_window_title(title) {
                Ok(ctrl) => match ctrl.screencap() {
                    Ok(img) => return Ok(img),
                    Err(_) => continue,
                },
                Err(_) => continue,
            }
        }

        Err(anyhow::anyhow!(
            "未找到游戏窗口。请确保 FF14 正在运行。\n尝试的标题: {:?}",
            POSSIBLE_TITLES
        ))
    }
}
