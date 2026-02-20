use eframe::egui;
use egui_wgpu::wgpu;
use tomestone_render::{BoundingBox, Camera, ModelRenderer};

pub struct ViewportState {
    pub render_state: egui_wgpu::RenderState,
    pub model_renderer: ModelRenderer,
    pub camera: Camera,
    pub texture_id: Option<egui::TextureId>,
    pub last_bbox: Option<BoundingBox>,
}

impl ViewportState {
    pub fn new(render_state: egui_wgpu::RenderState) -> Self {
        let model_renderer = ModelRenderer::new(&render_state.device);
        Self {
            render_state,
            model_renderer,
            camera: Camera::default(),
            texture_id: None,
            last_bbox: None,
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, empty_label: &str) {
        let available = ui.available_size();
        let vp_w = (available.x as u32).max(1);
        let vp_h = (available.y as u32).max(1);

        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(vp_w as f32, vp_h as f32),
            egui::Sense::click_and_drag(),
        );

        if response.dragged_by(egui::PointerButton::Primary) {
            let delta = response.drag_delta();
            self.camera.yaw += delta.x * 0.01;
            self.camera.pitch = (self.camera.pitch + delta.y * 0.01).clamp(-1.5, 1.5);
        }
        if response.dragged_by(egui::PointerButton::Secondary) {
            let delta = response.drag_delta();
            self.camera.pan(delta.x, delta.y);
        }
        if response.double_clicked() {
            if let Some(bbox) = &self.last_bbox {
                self.camera.focus_on(bbox);
            } else {
                self.camera = Camera::default();
            }
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                self.camera.distance = (self.camera.distance - scroll * 0.005).clamp(0.5, 20.0);
            }
        }

        if self.model_renderer.has_mesh() {
            self.model_renderer.render_offscreen(
                &self.render_state.device,
                &self.render_state.queue,
                vp_w,
                vp_h,
                &self.camera,
            );

            if let Some(view) = self.model_renderer.color_view() {
                let tid = match self.texture_id {
                    Some(tid) => {
                        self.render_state
                            .renderer
                            .write()
                            .update_egui_texture_from_wgpu_texture(
                                &self.render_state.device,
                                view,
                                wgpu::FilterMode::Linear,
                                tid,
                            );
                        tid
                    }
                    None => {
                        let tid = self.render_state.renderer.write().register_native_texture(
                            &self.render_state.device,
                            view,
                            wgpu::FilterMode::Linear,
                        );
                        self.texture_id = Some(tid);
                        tid
                    }
                };

                ui.painter().image(
                    tid,
                    rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }

            ctx.request_repaint();

            ui.painter().text(
                egui::pos2(rect.left() + 8.0, rect.bottom() - 8.0),
                egui::Align2::LEFT_BOTTOM,
                "左键旋转 | 右键平移 | 滚轮缩放 | 双击重置",
                egui::FontId::proportional(12.0),
                egui::Color32::from_rgba_premultiplied(180, 180, 180, 160),
            );
        } else {
            ui.painter()
                .rect_filled(rect, 0.0, egui::Color32::from_rgb(30, 30, 36));
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                empty_label,
                egui::FontId::default(),
                egui::Color32::GRAY,
            );
        }
    }

    pub fn free_texture(&mut self) {
        if let Some(tid) = self.texture_id.take() {
            self.render_state.renderer.write().free_texture(&tid);
        }
    }
}
