use std::path::PathBuf;

use eframe::egui;

use crate::app::{App, AppPhase};

impl App {
    pub fn show_setup_ui(&mut self, ctx: &egui::Context) {
        let (dir_input, error) = match &self.phase {
            AppPhase::Setup { dir_input, error } => (dir_input.clone(), error.clone()),
            _ => return,
        };

        let mut new_dir_input = dir_input;
        let mut confirm = false;
        let mut cancel = false;
        let has_game_state = self.game_state.is_some();

        egui::CentralPanel::default().show(ctx, |ui| {
            let panel_width = 500.0_f32;
            let panel_height = 120.0_f32;
            let center = ui.max_rect().center();
            let rect = egui::Rect::from_center_size(center, egui::vec2(panel_width, panel_height));
            ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                ui.vertical_centered(|ui| {
                    ui.heading("请选择 FF14 安装目录");
                    ui.add_space(16.0);

                    if let Some(err) = &error {
                        ui.colored_label(egui::Color32::RED, err);
                        ui.add_space(8.0);
                    }

                    ui.horizontal(|ui| {
                        ui.label("安装目录:");
                        ui.add_sized(
                            [ui.available_width() - 60.0, 20.0],
                            egui::TextEdit::singleline(&mut new_dir_input),
                        );
                        if ui.button("浏览...").clicked() {
                            if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                                new_dir_input = folder.display().to_string();
                            }
                        }
                    });

                    ui.add_space(8.0);

                    ui.horizontal(|ui| {
                        if ui.button("确定").clicked() {
                            confirm = true;
                        }
                        if has_game_state && ui.button("取消").clicked() {
                            cancel = true;
                        }
                    });
                });
            });
        });

        if confirm {
            let path = PathBuf::from(&new_dir_input);
            match crate::game::validate_install_dir(&path) {
                Ok(()) => {
                    self.config.game_install_dir = Some(path.clone());
                    if let Err(e) = crate::config::save_config(&self.config) {
                        eprintln!("保存配置失败: {}", e);
                    }
                    self.start_loading(path);
                }
                Err(e) => {
                    self.phase = AppPhase::Setup {
                        dir_input: new_dir_input,
                        error: Some(e),
                    };
                }
            }
        } else if cancel {
            self.phase = AppPhase::Ready;
        } else if let AppPhase::Setup { dir_input, .. } = &mut self.phase {
            *dir_input = new_dir_input;
        }
    }
}
