use eframe::egui;

fn main() {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 700.0])
            .with_title("FF14 装备浏览器"),
        ..Default::default()
    };

    eframe::run_native(
        "tomestone",
        options,
        Box::new(|cc| {
            tomestone::setup_fonts(&cc.egui_ctx);
            let render_state = cc
                .wgpu_render_state
                .as_ref()
                .expect("需要 wgpu 后端")
                .clone();
            Ok(Box::new(tomestone::App::new(render_state)))
        }),
    )
    .unwrap();
}
