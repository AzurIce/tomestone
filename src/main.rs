mod game_data;

use std::path::Path;

use eframe::egui;
use game_data::{EquipSlot, EquipmentItem, GameData};

const ALL_SLOTS: [EquipSlot; 5] = [
    EquipSlot::Head,
    EquipSlot::Body,
    EquipSlot::Gloves,
    EquipSlot::Legs,
    EquipSlot::Feet,
];

struct App {
    items: Vec<EquipmentItem>,
    search: String,
    selected_slot: Option<EquipSlot>,
    selected_item: Option<usize>,
}

impl App {
    fn new(items: Vec<EquipmentItem>) -> Self {
        Self {
            items,
            search: String::new(),
            selected_slot: None,
            selected_item: None,
        }
    }

    fn filtered_items(&self) -> Vec<(usize, &EquipmentItem)> {
        self.items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                if let Some(slot) = self.selected_slot {
                    if item.slot != slot {
                        return false;
                    }
                }
                if !self.search.is_empty() {
                    if !item.name.contains(&self.search) {
                        return false;
                    }
                }
                true
            })
            .collect()
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 左侧面板: 装备列表
        egui::SidePanel::left("equipment_list")
            .default_width(350.0)
            .show(ctx, |ui| {
                ui.heading("装备浏览器");
                ui.separator();

                // 搜索框
                ui.horizontal(|ui| {
                    ui.label("搜索:");
                    ui.text_edit_singleline(&mut self.search);
                });

                // 槽位过滤
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(self.selected_slot.is_none(), "全部")
                        .clicked()
                    {
                        self.selected_slot = None;
                    }
                    for slot in &ALL_SLOTS {
                        if ui
                            .selectable_label(
                                self.selected_slot == Some(*slot),
                                slot.display_name(),
                            )
                            .clicked()
                        {
                            self.selected_slot = Some(*slot);
                        }
                    }
                });

                ui.separator();

                let filtered: Vec<(usize, String)> = self
                    .filtered_items()
                    .into_iter()
                    .map(|(idx, item)| (idx, format!("[{}] {}", item.slot.slot_abbr(), item.name)))
                    .collect();
                ui.label(format!("{} 件", filtered.len()));

                // 装备列表 (虚拟滚动)
                egui::ScrollArea::vertical().show_rows(
                    ui,
                    18.0,
                    filtered.len(),
                    |ui, row_range| {
                        for row_idx in row_range {
                            if let Some((global_idx, label)) = filtered.get(row_idx) {
                                let selected = self.selected_item == Some(*global_idx);
                                if ui.selectable_label(selected, label).clicked() {
                                    self.selected_item = Some(*global_idx);
                                }
                            }
                        }
                    },
                );
            });

        // 中央面板: 装备详情
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(idx) = self.selected_item {
                if let Some(item) = self.items.get(idx) {
                    ui.heading(&item.name);
                    ui.separator();
                    egui::Grid::new("item_info").show(ui, |ui| {
                        ui.label("槽位:");
                        ui.label(item.slot.display_name());
                        ui.end_row();

                        ui.label("装备 ID:");
                        ui.label(format!("e{:04}", item.set_id));
                        ui.end_row();

                        ui.label("变体:");
                        ui.label(format!("v{:04}", item.variant_id));
                        ui.end_row();

                        ui.label("模型路径:");
                        ui.label(item.model_path());
                        ui.end_row();

                        ui.label("Item Row:");
                        ui.label(format!("{}", item.row_id));
                        ui.end_row();

                        ui.label("Icon ID:");
                        ui.label(format!("{}", item.icon_id));
                        ui.end_row();
                    });

                    ui.separator();
                    ui.label("(3D 模型预览将在后续版本实现)");
                } else {
                    ui.label("选择一件装备查看详情");
                }
            } else {
                ui.centered_and_justified(|ui| {
                    ui.label("← 从左侧列表选择一件装备");
                });
            }
        });
    }
}

fn main() {
    let install_dir = Path::new(r"G:\最终幻想XIV");

    println!("正在加载游戏数据...");
    let game = GameData::new(install_dir);

    println!("正在加载装备列表...");
    let items = game.load_equipment_list();
    println!("共加载 {} 件装备", items.len());

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 600.0])
            .with_title("FF14 装备浏览器"),
        ..Default::default()
    };

    eframe::run_native(
        "ff-tools",
        options,
        Box::new(|cc| {
            setup_fonts(cc);
            Ok(Box::new(App::new(items)))
        }),
    )
    .unwrap();
}

fn setup_fonts(cc: &eframe::CreationContext) {
    // Support Chinese
    let mut fonts = egui::FontDefinitions::default();

    fonts.font_data.insert(
        "Harmony OS Sans".to_string(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/HarmonyOS_Sans_SC_Regular.ttf"
        ))),
    );

    // Put my font first (highest priority):
    // fonts
    //     .families
    //     .get_mut(&FontFamily::Proportional)
    //     .unwrap()
    //     .insert(0, "Harmony OS Sans".to_owned());

    // Put my font as last fallback:
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
    cc.egui_ctx.set_fonts(fonts);
}
