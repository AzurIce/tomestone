use std::sync::Arc;

use eframe::egui;

pub fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    fonts.font_data.insert(
        "Harmony OS Sans".to_string(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/HarmonyOS_Sans_SC_Regular.ttf"
        ))),
    );

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

    // Phosphor Icons (Regular + Fill)
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Fill);

    ctx.set_fonts(fonts);
}
