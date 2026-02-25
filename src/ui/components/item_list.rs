use std::collections::HashMap;

use eframe::egui;

use crate::domain::ViewMode;
use crate::game::GameData;

/// 通用物品列表状态 (搜索、视图模式、图标大小)
pub struct ItemListState {
    pub search: String,
    pub view_mode: ViewMode,
    pub icon_size: f32,
}

impl ItemListState {
    pub fn new(default_mode: ViewMode) -> Self {
        Self {
            search: String::new(),
            view_mode: default_mode,
            icon_size: 48.0,
        }
    }

    /// 渲染搜索框 + 视图切换 + 图标大小滑块
    pub fn show_controls(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("搜索:");
            ui.text_edit_singleline(&mut self.search);
        });
        ui.horizontal(|ui| {
            ui.label("视图:");
            if ui
                .selectable_label(self.view_mode == ViewMode::List, ViewMode::List.label())
                .clicked()
            {
                self.view_mode = ViewMode::List;
            }
            if ui
                .selectable_label(self.view_mode == ViewMode::Grid, ViewMode::Grid.label())
                .clicked()
            {
                self.view_mode = ViewMode::Grid;
            }
        });
        if self.view_mode == ViewMode::Grid {
            ui.horizontal(|ui| {
                ui.label("图标:");
                ui.add(egui::Slider::new(&mut self.icon_size, 32.0..=128.0).suffix("px"));
            });
        }
    }

    /// 搜索过滤: 返回搜索词的小写形式 (空字符串表示不过滤)
    pub fn search_lower(&self) -> String {
        self.search.to_lowercase()
    }
}

/// 用于渲染的物品显示信息
pub struct DisplayItem<'a> {
    /// 调用方自定义的标识 (点击时原样返回)
    pub id: usize,
    pub name: &'a str,
    pub icon_id: u32,
    pub is_selected: bool,
}

/// 渲染一行列表物品 (图标 + 标签)，返回是否被点击
pub fn show_list_row(
    ui: &mut egui::Ui,
    item: &DisplayItem<'_>,
    label_text: &str,
    icon_cache: &mut HashMap<u32, Option<egui::TextureHandle>>,
    ctx: &egui::Context,
    game: &GameData,
) -> bool {
    let response = ui.horizontal(|ui| {
        if let Some(icon) = get_or_load_icon(icon_cache, ctx, game, item.icon_id) {
            ui.image(egui::load::SizedTexture::new(
                icon.id(),
                egui::vec2(20.0, 20.0),
            ));
        } else {
            ui.allocate_space(egui::vec2(20.0, 20.0));
        }
        ui.selectable_label(item.is_selected, label_text)
    });
    response.inner.clicked()
}

/// 渲染图标网格视图 (不含 ScrollArea，调用方自行包裹)
/// 返回被点击的 item id
pub fn show_grid(
    ui: &mut egui::Ui,
    items: &[DisplayItem<'_>],
    icon_size: f32,
    icon_cache: &mut HashMap<u32, Option<egui::TextureHandle>>,
    ctx: &egui::Context,
    game: &GameData,
) -> Option<usize> {
    if items.is_empty() {
        return None;
    }

    let available_width = ui.available_width();
    let cell_padding = 4.0;
    let text_height = 14.0;
    let text_lines = 2;
    let cell_width = (icon_size + cell_padding * 2.0).min(available_width);
    let cell_height = icon_size + cell_padding * 2.0 + text_height * text_lines as f32;
    let cols = ((available_width / cell_width).floor() as usize).max(1);
    let actual_cell_width = available_width / cols as f32;
    let total_rows = (items.len() + cols - 1) / cols;

    let mut clicked: Option<usize> = None;

    for row_idx in 0..total_rows {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let start = row_idx * cols;
            let end = (start + cols).min(items.len());
            for i in start..end {
                let item = &items[i];

                let (rect, response) = ui.allocate_exact_size(
                    egui::vec2(actual_cell_width, cell_height),
                    egui::Sense::click(),
                );

                // 背景高亮
                if item.is_selected || response.hovered() {
                    let bg = if item.is_selected {
                        ui.visuals().selection.bg_fill
                    } else {
                        ui.visuals().widgets.hovered.bg_fill
                    };
                    ui.painter().rect_filled(rect, 2.0, bg);
                }

                // 图标
                let icon_top = rect.top() + cell_padding;
                let icon_center_x = rect.center().x;
                let icon_rect = egui::Rect::from_center_size(
                    egui::pos2(icon_center_x, icon_top + icon_size / 2.0),
                    egui::vec2(icon_size, icon_size),
                );
                if let Some(icon) = get_or_load_icon(icon_cache, ctx, game, item.icon_id) {
                    ui.painter().image(
                        icon.id(),
                        icon_rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                }

                // 文字
                let text_top = icon_top + icon_size + cell_padding;
                let text_color = ui.visuals().text_color();
                let clipped = ui.painter().with_clip_rect(rect);
                clipped.text(
                    egui::pos2(rect.center().x, text_top),
                    egui::Align2::CENTER_TOP,
                    item.name,
                    egui::FontId::proportional(11.0),
                    text_color,
                );

                response.clone().on_hover_text(item.name);

                if response.clicked() {
                    clicked = Some(item.id);
                }
            }
        });
    }

    clicked
}

/// 渲染带虚拟滚动的图标网格视图 (含 ScrollArea + show_rows)
/// 返回被点击的 item id
pub fn show_grid_scroll(
    ui: &mut egui::Ui,
    items: &[DisplayItem<'_>],
    icon_size: f32,
    id_salt: &str,
    icon_cache: &mut HashMap<u32, Option<egui::TextureHandle>>,
    ctx: &egui::Context,
    game: &GameData,
) -> Option<usize> {
    if items.is_empty() {
        return None;
    }

    let available_width = ui.available_width();
    let cell_padding = 4.0;
    let text_height = 14.0;
    let text_lines = 2;
    let cell_width = (icon_size + cell_padding * 2.0).min(available_width);
    let cell_height = icon_size + cell_padding * 2.0 + text_height * text_lines as f32;
    let cols = ((available_width / cell_width).floor() as usize).max(1);
    let actual_cell_width = available_width / cols as f32;
    let total_rows = (items.len() + cols - 1) / cols;

    let mut clicked: Option<usize> = None;

    egui::ScrollArea::vertical()
        .id_salt(format!("{}_grid_scroll", id_salt))
        .show_rows(ui, cell_height, total_rows, |ui, row_range| {
            for row_idx in row_range {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    let start = row_idx * cols;
                    let end = (start + cols).min(items.len());
                    for i in start..end {
                        let item = &items[i];

                        let (rect, response) = ui.allocate_exact_size(
                            egui::vec2(actual_cell_width, cell_height),
                            egui::Sense::click(),
                        );

                        if item.is_selected || response.hovered() {
                            let bg = if item.is_selected {
                                ui.visuals().selection.bg_fill
                            } else {
                                ui.visuals().widgets.hovered.bg_fill
                            };
                            ui.painter().rect_filled(rect, 2.0, bg);
                        }

                        let icon_top = rect.top() + cell_padding;
                        let icon_center_x = rect.center().x;
                        let icon_rect = egui::Rect::from_center_size(
                            egui::pos2(icon_center_x, icon_top + icon_size / 2.0),
                            egui::vec2(icon_size, icon_size),
                        );
                        if let Some(icon) = get_or_load_icon(icon_cache, ctx, game, item.icon_id) {
                            ui.painter().image(
                                icon.id(),
                                icon_rect,
                                egui::Rect::from_min_max(
                                    egui::pos2(0.0, 0.0),
                                    egui::pos2(1.0, 1.0),
                                ),
                                egui::Color32::WHITE,
                            );
                        }

                        let text_top = icon_top + icon_size + cell_padding;
                        let text_color = ui.visuals().text_color();
                        let clipped = ui.painter().with_clip_rect(rect);
                        clipped.text(
                            egui::pos2(rect.center().x, text_top),
                            egui::Align2::CENTER_TOP,
                            item.name,
                            egui::FontId::proportional(11.0),
                            text_color,
                        );

                        response.clone().on_hover_text(item.name);

                        if response.clicked() {
                            clicked = Some(item.id);
                        }
                    }
                });
            }
        });

    clicked
}

/// 从 icon_cache 获取或加载图标
pub fn get_or_load_icon(
    icon_cache: &mut HashMap<u32, Option<egui::TextureHandle>>,
    ctx: &egui::Context,
    game: &GameData,
    icon_id: u32,
) -> Option<egui::TextureHandle> {
    if icon_id == 0 {
        return None;
    }
    if let Some(cached) = icon_cache.get(&icon_id) {
        return cached.clone();
    }
    let result = game.load_icon(icon_id).map(|tex_data| {
        let size = [tex_data.width as _, tex_data.height as _];
        let pixels: Vec<egui::Color32> = tex_data
            .rgba
            .chunks_exact(4)
            .map(|p| egui::Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3]))
            .collect();
        let color_image = egui::ColorImage {
            size,
            pixels,
            source_size: egui::Vec2::new(40.0, 40.0),
        };
        ctx.load_texture(
            format!("icon_{}", icon_id),
            color_image,
            egui::TextureOptions::default(),
        )
    });
    icon_cache.insert(icon_id, result.clone());
    result
}
