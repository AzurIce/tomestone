use std::sync::mpsc;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use auto_play::MatcherOptions;
use eframe::egui;

use crate::app::App;
use crate::auto_craft::{self, CraftMessage, CraftTemplates};
use crate::template::TemplateSet;

#[derive(Default, Clone, Copy, PartialEq)]
pub enum ToolboxTab {
    #[default]
    AutoCraft,
    TemplateEditor,
}

/// 自动制作工具的运行状态
pub enum AutoCraftState {
    /// 空闲
    Idle,
    /// 运行中
    Running {
        receiver: mpsc::Receiver<CraftMessage>,
        cancel: Arc<AtomicBool>,
    },
}

/// 自动制作工具的 UI 状态
pub struct AutoCraftUi {
    pub state: AutoCraftState,
    pub count: u32,
    pub macro_key: String,
    pub progress: (u32, u32),
    pub status: String,
    pub log: Vec<String>,
    pub tab: ToolboxTab,
}

impl Default for AutoCraftUi {
    fn default() -> Self {
        Self {
            state: AutoCraftState::Idle,
            count: 10,
            macro_key: "r".to_string(),
            progress: (0, 0),
            status: "就绪".to_string(),
            log: Vec::new(),
            tab: ToolboxTab::default(),
        }
    }
}

impl App {
    pub fn show_toolbox_page(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            // Tab 栏
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.auto_craft.tab, ToolboxTab::AutoCraft, "工具箱");
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
                ToolboxTab::TemplateEditor => {
                    self.template_editor.ensure_loaded(auto_craft::TEMPLATES);
                    self.template_editor.show_inline(ui, ctx);
                }
            }

            self.poll_auto_craft_messages();

            if matches!(self.auto_craft.state, AutoCraftState::Running { .. }) {
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

            let is_running = matches!(self.auto_craft.state, AutoCraftState::Running { .. });

            ui.horizontal(|ui| {
                ui.label("制作次数:");
                ui.add_enabled(
                    !is_running,
                    egui::DragValue::new(&mut self.auto_craft.count).range(1..=999),
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
                        if let AutoCraftState::Running { ref cancel, .. } = self.auto_craft.state {
                            cancel.store(true, Ordering::Relaxed);
                        }
                    }
                } else if ui
                    .button(format!("{} 开始制作", egui_phosphor::regular::PLAY_CIRCLE))
                    .clicked()
                {
                    self.start_auto_craft();
                }

                ui.add_space(8.0);

                if self.auto_craft.progress.1 > 0 {
                    let (done, total) = self.auto_craft.progress;
                    let frac = done as f32 / total as f32;
                    ui.add(
                        egui::ProgressBar::new(frac)
                            .text(format!("{}/{}", done, total))
                            .desired_width(200.0),
                    );
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
        let macro_key = self.auto_craft.macro_key.chars().next().unwrap_or('r');

        let tpl_set = self
            .template_editor
            .template_set()
            .cloned()
            .unwrap_or_else(|| TemplateSet::load(auto_craft::TEMPLATES));

        let start_img = tpl_set.templates[auto_craft::TPL_START].image.clone();
        let stop_img = tpl_set.templates[auto_craft::TPL_STOP].image.clone();
        let start_threshold = tpl_set.templates[auto_craft::TPL_START].def.threshold;
        let stop_threshold = tpl_set.templates[auto_craft::TPL_STOP].def.threshold;

        let templates = CraftTemplates {
            start: start_img,
            stop: stop_img,
            options: MatcherOptions::default().with_threshold(stop_threshold),
            options_strict: MatcherOptions::default().with_threshold(start_threshold),
        };

        self.auto_craft.progress = (0, count);
        self.auto_craft.status = "启动中...".to_string();
        self.auto_craft.log.clear();

        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_clone = cancel.clone();

        std::thread::spawn(move || {
            auto_craft::run_auto_craft(count, macro_key, templates, tx, cancel_clone);
        });

        self.auto_craft.state = AutoCraftState::Running {
            receiver: rx,
            cancel,
        };
    }

    fn poll_auto_craft_messages(&mut self) {
        let messages: Vec<CraftMessage> =
            if let AutoCraftState::Running { ref receiver, .. } = self.auto_craft.state {
                receiver.try_iter().collect()
            } else {
                return;
            };

        let mut finished = false;
        for msg in messages {
            match msg {
                CraftMessage::Status(s) => {
                    self.auto_craft.status = s.clone();
                    self.auto_craft.log.push(s);
                }
                CraftMessage::Progress(done, total) => {
                    self.auto_craft.progress = (done, total);
                }
                CraftMessage::CraftDone {
                    index,
                    elapsed_secs,
                } => {
                    let line = format!("#{} 完成 ({:.1}s)", index, elapsed_secs);
                    self.auto_craft.status = line.clone();
                    self.auto_craft.log.push(line);
                }
                CraftMessage::CraftFailed { index, reason } => {
                    let line = format!("#{} 失败: {}", index, reason);
                    self.auto_craft.status = line.clone();
                    self.auto_craft.log.push(line);
                    finished = true;
                }
                CraftMessage::Finished { success, total } => {
                    let line = format!("完成: {}/{} 成功", success, total);
                    self.auto_craft.status = line.clone();
                    self.auto_craft.log.push(line);
                    finished = true;
                }
                CraftMessage::Error(e) => {
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
}
