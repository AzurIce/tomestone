use crate::domain::{shade_group_name, StainEntry, SHADE_ORDER};
use eframe::egui;

pub fn show_dye_palette(
    ui: &mut egui::Ui,
    stains: &[StainEntry],
    selected_stain_ids: &mut [u32; 2],
    active_dye_channel: &mut usize,
    selected_shade: &mut u8,
    is_dual_dye: bool,
) -> bool {
    let prev_stains = *selected_stain_ids;
    let ch = *active_dye_channel;

    let stain_data: Vec<(u32, String, [u8; 3])> = stains
        .iter()
        .filter(|s| s.shade == *selected_shade)
        .map(|s| (s.id, s.name.clone(), s.color))
        .collect();

    if is_dual_dye {
        ui.horizontal(|ui| {
            ui.label("通道:");
            ui.selectable_value(active_dye_channel, 0, "通道1");
            ui.selectable_value(active_dye_channel, 1, "通道2");
        });
    }

    ui.horizontal_wrapped(|ui| {
        for &shade in SHADE_ORDER {
            let label = shade_group_name(shade);
            if ui
                .selectable_label(*selected_shade == shade, label)
                .clicked()
            {
                *selected_shade = shade;
            }
        }
    });

    ui.horizontal_wrapped(|ui| {
        let no_dye_selected = selected_stain_ids[ch] == 0;
        let (no_rect, no_resp) =
            ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::click());
        let no_bg = if no_dye_selected {
            egui::Color32::from_gray(180)
        } else {
            egui::Color32::from_gray(60)
        };
        ui.painter().rect_filled(no_rect, 2.0, no_bg);
        ui.painter().text(
            no_rect.center(),
            egui::Align2::CENTER_CENTER,
            "✕",
            egui::FontId::proportional(12.0),
            egui::Color32::WHITE,
        );
        if no_dye_selected {
            ui.painter().rect_stroke(
                no_rect,
                2.0,
                egui::Stroke::new(2.0, egui::Color32::WHITE),
                egui::StrokeKind::Outside,
            );
        }
        if no_resp.clicked() {
            selected_stain_ids[ch] = 0;
        }
        no_resp.on_hover_text("无染料");

        for (id, name, color_rgb) in &stain_data {
            let color = egui::Color32::from_rgb(color_rgb[0], color_rgb[1], color_rgb[2]);
            let selected = selected_stain_ids[ch] == *id;
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::click());
            ui.painter().rect_filled(rect, 2.0, color);
            if selected {
                ui.painter().rect_stroke(
                    rect,
                    2.0,
                    egui::Stroke::new(2.0, egui::Color32::WHITE),
                    egui::StrokeKind::Outside,
                );
            }
            if resp.clicked() {
                selected_stain_ids[ch] = *id;
            }
            resp.on_hover_text(name);
        }
    });

    ui.horizontal(|ui| {
        let current_id = selected_stain_ids[ch];
        if current_id == 0 {
            ui.label("当前: 无染料");
        } else if let Some(stain) = stains.iter().find(|s| s.id == current_id) {
            let color = egui::Color32::from_rgb(stain.color[0], stain.color[1], stain.color[2]);
            let (rect, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 2.0, color);
            ui.label(format!("当前: {}", stain.name));
        }
    });

    prev_stains != *selected_stain_ids
}
