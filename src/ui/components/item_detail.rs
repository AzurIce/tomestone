use eframe::egui;

use crate::domain::GameItem;

/// 物品详情头部的显示配置
pub struct ItemDetailConfig {
    /// 图标大小 (像素)
    pub icon_size: f32,
    /// 是否使用 heading 样式显示名称 (大标题 vs 小标题)
    pub use_heading: bool,
    /// 是否显示分类名称
    pub show_category: bool,
    /// 是否显示描述
    pub show_description: bool,
    /// 是否显示外部链接
    pub show_links: bool,
}

impl Default for ItemDetailConfig {
    fn default() -> Self {
        Self {
            icon_size: 40.0,
            use_heading: true,
            show_category: true,
            show_description: true,
            show_links: true,
        }
    }
}

impl ItemDetailConfig {
    /// 合成检索用的紧凑配置
    pub fn compact() -> Self {
        Self {
            icon_size: 32.0,
            use_heading: false,
            show_category: true,
            show_description: true,
            show_links: true,
        }
    }
}

/// 显示统一的物品详情头部 (图标 + 名称 + 分类 + 描述 + 外部链接)
///
/// 参数:
/// - `icon`: 已加载的图标纹理 (由调用方提供，避免借用冲突)
/// - `category_name`: UI 分类名称 (由调用方从 gs.ui_category_names 查询)
pub fn show_item_detail_header(
    ui: &mut egui::Ui,
    item: &GameItem,
    icon: Option<&egui::TextureHandle>,
    category_name: Option<&str>,
    config: &ItemDetailConfig,
) {
    // 图标 + 名称
    ui.horizontal(|ui| {
        if let Some(tex) = icon {
            ui.image(egui::load::SizedTexture::new(
                tex.id(),
                egui::vec2(config.icon_size, config.icon_size),
            ));
        }
        if config.use_heading {
            ui.heading(&item.name);
        } else {
            ui.label(egui::RichText::new(&item.name).strong().size(14.0));
        }
    });

    // 分类名称
    if config.show_category {
        if let Some(cat) = category_name {
            ui.label(egui::RichText::new(cat).small().weak());
        }
    }

    // 描述
    if config.show_description && !item.description.is_empty() {
        ui.add_space(2.0);
        ui.label(egui::RichText::new(&item.description).small().weak());
    }

    // 外部链接
    if config.show_links {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let wiki_url = format!("https://ff14.huijiwiki.com/wiki/物品:{}", item.name);
            if ui
                .link(format!("{} 灰机Wiki", egui_phosphor::regular::GLOBE))
                .clicked()
            {
                let _ = open::that(&wiki_url);
            }
            if item.is_marketable() {
                ui.label(" | ");
                let universalis_url = format!("https://universalis.app/market/{}", item.row_id);
                if ui
                    .link(format!(
                        "{} Universalis",
                        egui_phosphor::regular::CHART_LINE_UP
                    ))
                    .clicked()
                {
                    let _ = open::that(&universalis_url);
                }
            }
        });
    }
}
