use eframe::egui;
use egui_wgpu::wgpu;
use tomestone_render::{BoundingBox, Camera, ModelRenderer};

pub struct ViewportState {
    pub render_state: egui_wgpu::RenderState,
    pub model_renderer: ModelRenderer,
    pub camera: Camera,
    pub texture_id: Option<egui::TextureId>,
    pub last_bbox: Option<BoundingBox>,
    /// 脏标记：仅在相机/模型/尺寸变化时重新渲染
    dirty: bool,
    last_vp_size: [u32; 2],
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
            dirty: true,
            last_vp_size: [0, 0],
        }
    }

    /// 外部通知需要重新渲染（模型/纹理数据变更时调用）
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn show(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context, empty_label: &str) {
        let available = ui.available_size();
        let vp_w = (available.x as u32).max(1);
        let vp_h = (available.y as u32).max(1);

        // 视口尺寸变化时标记脏
        if self.last_vp_size != [vp_w, vp_h] {
            self.last_vp_size = [vp_w, vp_h];
            self.dirty = true;
        }

        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(vp_w as f32, vp_h as f32),
            egui::Sense::click_and_drag(),
        );

        // 相机交互 — 有变化时标记脏
        if response.dragged_by(egui::PointerButton::Primary) {
            let delta = response.drag_delta();
            self.camera.yaw += delta.x * 0.01;
            self.camera.pitch = (self.camera.pitch + delta.y * 0.01).clamp(-1.5, 1.5);
            self.dirty = true;
        }
        if response.dragged_by(egui::PointerButton::Secondary) {
            let delta = response.drag_delta();
            self.camera.pan(delta.x, delta.y);
            self.dirty = true;
        }
        if response.double_clicked() {
            if let Some(bbox) = &self.last_bbox {
                self.camera.focus_on(bbox);
            } else {
                self.camera = Camera::default();
            }
            self.dirty = true;
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                self.camera.distance =
                    (self.camera.distance - scroll * 0.005).clamp(0.1, self.camera.max_distance);
                self.dirty = true;
            }
        }

        if self.model_renderer.has_mesh() {
            // 仅在脏时重新渲染
            if self.dirty {
                self.model_renderer.render_offscreen(
                    &self.render_state.device,
                    &self.render_state.queue,
                    vp_w,
                    vp_h,
                    &self.camera,
                );
                self.dirty = false;

                // 渲染后更新 egui 纹理
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
            } else if let Some(tid) = self.texture_id {
                // 未脏时直接复用上次的纹理
                ui.painter().image(
                    tid,
                    rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }

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
        self.dirty = true;
    }
}
