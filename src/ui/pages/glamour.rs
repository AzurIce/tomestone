use eframe::egui;

use crate::app::App;
use crate::glamour;
use crate::glamour::{AppContext, GlamourEditor};
use crate::loading::{glamour_slot_summary, GameState};

impl App {
    pub fn show_glamour_manager_page(&mut self, ctx: &egui::Context, gs: &mut GameState) {
        if let Some(mut editor) = self.glamour_editor.take() {
            let app_ctx = AppContext {
                items: &gs.items,
                item_id_map: &gs.item_id_map,
                stains: &gs.stains,
                stm: gs.stm.as_ref(),
                game: &gs.game,
                equipment_sets: &gs.equipment_sets,
                set_id_to_set_idx: &gs.set_id_to_set_idx,
            };
            let action = editor.show(ctx, &app_ctx);
            match action {
                glamour::GlamourEditorAction::Save => {
                    if let Some(idx) = self.editing_glamour_idx {
                        gs.glamour_sets[idx] = editor.glamour_set.clone();
                        if let Err(e) = glamour::save_glamour_set(&gs.glamour_sets[idx]) {
                            eprintln!("保存失败: {}", e);
                        }
                    }
                    editor.dirty = false;
                    self.glamour_editor = Some(editor);
                }
                glamour::GlamourEditorAction::Close => {
                    self.glamour_editor = None;
                    self.editing_glamour_idx = None;
                }
                glamour::GlamourEditorAction::None => {
                    self.glamour_editor = Some(editor);
                }
            }
            return;
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("幻化管理");
            ui.separator();

            ui.horizontal(|ui| {
                ui.label("名称:");
                ui.text_edit_singleline(&mut self.new_glamour_name);
                if ui.button("新建").clicked() && !self.new_glamour_name.trim().is_empty() {
                    let new_gs = glamour::GlamourSet::new(self.new_glamour_name.trim());
                    if let Err(e) = glamour::save_glamour_set(&new_gs) {
                        eprintln!("保存失败: {}", e);
                    }
                    gs.glamour_sets.push(new_gs);
                    self.new_glamour_name.clear();
                }
            });

            ui.separator();

            if gs.glamour_sets.is_empty() {
                ui.label("暂无幻化组合，请新建一个。");
                return;
            }

            let mut delete_idx: Option<usize> = None;
            let mut edit_idx: Option<usize> = None;
            let mut confirm_rename: Option<usize> = None;
            let mut start_rename: Option<(usize, String)> = None;

            let summaries: Vec<(String, usize, String)> = gs
                .glamour_sets
                .iter()
                .map(|glamour_set| {
                    (
                        glamour_set.name.clone(),
                        glamour_set.slot_count(),
                        glamour_slot_summary(&gs.items, &gs.item_id_map, glamour_set),
                    )
                })
                .collect();

            egui::ScrollArea::vertical().show(ui, |ui| {
                for i in 0..summaries.len() {
                    ui.horizontal(|ui| {
                        if self.renaming_glamour_idx == Some(i) {
                            ui.text_edit_singleline(&mut self.rename_buffer);
                            if ui.button("确定").clicked()
                                || ui.input(|inp| inp.key_pressed(egui::Key::Enter))
                            {
                                confirm_rename = Some(i);
                            }
                            if ui.button("取消").clicked() {
                                self.renaming_glamour_idx = None;
                            }
                        } else {
                            let (name, slot_count, slot_summary) = &summaries[i];
                            ui.label(egui::RichText::new(name).strong());
                            ui.label(format!("({}/5 槽位)", slot_count));
                            if !slot_summary.is_empty() {
                                ui.label(slot_summary);
                            }

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.small_button("删除").clicked() {
                                        delete_idx = Some(i);
                                    }
                                    if ui.small_button("重命名").clicked() {
                                        start_rename = Some((i, name.clone()));
                                    }
                                    if ui.small_button("编辑").clicked() {
                                        edit_idx = Some(i);
                                    }
                                },
                            );
                        }
                    });
                    ui.separator();
                }
            });

            if let Some((idx, name)) = start_rename {
                self.renaming_glamour_idx = Some(idx);
                self.rename_buffer = name;
            }

            if let Some(idx) = confirm_rename {
                let new_name = self.rename_buffer.trim().to_string();
                if !new_name.is_empty() {
                    gs.glamour_sets[idx].name = new_name;
                    if let Err(e) = glamour::save_glamour_set(&gs.glamour_sets[idx]) {
                        eprintln!("保存失败: {}", e);
                    }
                }
                self.renaming_glamour_idx = None;
            }

            if let Some(idx) = delete_idx {
                let id = gs.glamour_sets[idx].id.clone();
                if let Err(e) = glamour::delete_glamour_set(&id) {
                    eprintln!("删除失败: {}", e);
                }
                gs.glamour_sets.remove(idx);
                if self.renaming_glamour_idx == Some(idx) {
                    self.renaming_glamour_idx = None;
                }
            }

            if let Some(idx) = edit_idx {
                let glamour_set = gs.glamour_sets[idx].clone();
                self.glamour_editor =
                    Some(GlamourEditor::new(glamour_set, self.render_state.clone()));
                self.editing_glamour_idx = Some(idx);
            }
        });
    }
}
