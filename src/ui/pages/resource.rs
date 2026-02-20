use eframe::egui::{self, Margin, RichText};
use egui_table::{CellInfo, HeaderCellInfo, HeaderRow, Table, TableDelegate};
use physis::excel::Field;
use physis::exh::{ColumnDataType, SheetRowKind, EXH};
use physis::Language;
use std::sync::mpsc::Receiver;

use crate::domain::EquipSlot;
use crate::game::GameData;
use crate::schema::SchemaTaskRunner;
use crate::ui::components::show_progress_bar;

enum FilePreview {
    Hex { data: Vec<u8>, path: String },
}

struct ExdTableDelegate<'a> {
    flat_rows: &'a [(u32, Vec<Field>)],
    exh: &'a EXH,
    column_names: &'a [String],
    selected_row_idx: Option<usize>,
    clicked_row: Option<usize>,
}

impl TableDelegate for ExdTableDelegate<'_> {
    fn default_row_height(&self) -> f32 {
        18.0
    }

    fn header_cell_ui(&mut self, ui: &mut egui::Ui, cell: &HeaderCellInfo) {
        let col = cell.col_range.start;
        egui::Frame::NONE
            .inner_margin(Margin::symmetric(4, 0))
            .show(ui, |ui| {
                if col == 0 {
                    ui.strong("ID");
                } else {
                    let data_col = col - 1;
                    if data_col < self.exh.column_definitions.len() {
                        let def = &self.exh.column_definitions[data_col];
                        let type_short = column_type_short(def.data_type);
                        if data_col < self.column_names.len() {
                            ui.strong(format!(
                                "{} [{}] {}",
                                self.column_names[data_col], def.offset, type_short,
                            ));
                        } else {
                            ui.strong(format!("[{}] {} #{}", def.offset, type_short, data_col,));
                        }
                    }
                }
            });
    }

    fn row_ui(&mut self, ui: &mut egui::Ui, row_nr: u64) {
        let row_idx = row_nr as usize;
        let selected = self.selected_row_idx == Some(row_idx);

        if selected {
            ui.painter()
                .rect_filled(ui.max_rect(), 0.0, ui.visuals().selection.bg_fill);
        } else if ui.rect_contains_pointer(ui.max_rect()) {
            ui.painter()
                .rect_filled(ui.max_rect(), 0.0, ui.visuals().faint_bg_color);
        } else if row_idx % 2 == 1 {
            ui.painter().rect_filled(
                ui.max_rect(),
                0.0,
                ui.visuals().faint_bg_color.linear_multiply(0.5),
            );
        }

        if ui.response().interact(egui::Sense::click()).clicked() {
            self.clicked_row = Some(row_idx);
        }
    }

    fn cell_ui(&mut self, ui: &mut egui::Ui, cell: &CellInfo) {
        let row_idx = cell.row_nr as usize;
        let Some((row_id, columns)) = self.flat_rows.get(row_idx) else {
            return;
        };

        egui::Frame::NONE
            .inner_margin(Margin::symmetric(4, 0))
            .show(ui, |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                if cell.col_nr == 0 {
                    ui.label(row_id.to_string());
                } else {
                    let data_col = cell.col_nr - 1;
                    if let Some(field) = columns.get(data_col) {
                        ui.label(format_field(field));
                    }
                }
            });
    }
}

pub struct ResourceBrowserState {
    all_table_names: Vec<String>,
    filtered_indices: Vec<usize>,

    loaded_table_name: Option<String>,
    loaded_exh: Option<EXH>,
    flat_rows: Vec<(u32, Vec<Field>)>,

    search: String,
    prev_search: String,
    selected_table_idx: Option<usize>,
    selected_row_idx: Option<usize>,

    extracted_paths: Vec<String>,
    path_input: String,
    preview: Option<FilePreview>,
    preview_error: Option<String>,

    schema_columns: Vec<String>,

    schema_runner: SchemaTaskRunner,
    schema_fetch_rx: Option<Receiver<Result<Vec<String>, String>>>,
    schema_update_all_rx: Option<Receiver<usize>>,
}

impl ResourceBrowserState {
    pub fn new(all_table_names: Vec<String>) -> Self {
        let filtered_indices: Vec<usize> = (0..all_table_names.len()).collect();

        Self {
            all_table_names,
            filtered_indices,
            loaded_table_name: None,
            loaded_exh: None,
            flat_rows: Vec::new(),
            search: String::new(),
            prev_search: String::new(),
            selected_table_idx: None,
            selected_row_idx: None,
            extracted_paths: Vec::new(),
            path_input: String::new(),
            preview: None,
            preview_error: None,
            schema_columns: Vec::new(),
            schema_runner: SchemaTaskRunner::new(),
            schema_fetch_rx: None,
            schema_update_all_rx: None,
        }
    }

    pub fn poll_schema_downloads(&mut self) {
        if let Some(rx) = &self.schema_fetch_rx {
            if let Ok(result) = rx.try_recv() {
                self.schema_fetch_rx = None;
                if let Ok(columns) = result {
                    self.schema_columns = columns;
                }
            }
        }

        if let Some(rx) = &self.schema_update_all_rx {
            if let Ok(_count) = rx.try_recv() {
                self.schema_update_all_rx = None;
                if let Some(name) = &self.loaded_table_name {
                    self.schema_columns =
                        crate::schema::load_schema_from_cache(name).unwrap_or_default();
                }
            }
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, game: &GameData) {
        self.poll_schema_downloads();

        egui::SidePanel::left("exd_table_list")
            .default_width(220.0)
            .show(ctx, |ui| {
                self.show_left_panel(ui, game);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            self.show_central_panel(ui, game);
        });
    }

    fn show_left_panel(&mut self, ui: &mut egui::Ui, game: &GameData) {
        ui.heading("EXD 表");
        ui.separator();

        ui.horizontal(|ui| {
            ui.label("路径:");
            let resp = ui.text_edit_singleline(&mut self.path_input);
            if ui.button("读取").clicked()
                || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
            {
                self.do_read_file(game);
            }
        });

        if let Some(err) = &self.preview_error {
            ui.colored_label(egui::Color32::from_rgb(255, 100, 100), err);
        }

        if let Some(FilePreview::Hex { data, path }) = &self.preview {
            ui.horizontal(|ui| {
                ui.label(RichText::new(path).strong());
                ui.label(format!("({} 字节)", data.len()));
            });

            let lines = (data.len() + 15) / 16;
            let display_lines = lines.min(256);
            egui::ScrollArea::vertical()
                .id_salt("hex_dump_scroll")
                .auto_shrink([false, false])
                .max_height(120.0)
                .show_rows(ui, 16.0, display_lines, |ui, row_range| {
                    ui.style_mut().override_font_id = Some(egui::FontId::monospace(11.0));
                    for row_idx in row_range {
                        let offset = row_idx * 16;
                        let end = (offset + 16).min(data.len());
                        let chunk = &data[offset..end];

                        let mut hex_part = String::with_capacity(48);
                        let mut ascii_part = String::with_capacity(16);
                        for (i, &byte) in chunk.iter().enumerate() {
                            if i == 8 {
                                hex_part.push(' ');
                            }
                            hex_part.push_str(&format!("{:02X} ", byte));
                            ascii_part.push(if byte.is_ascii_graphic() || byte == b' ' {
                                byte as char
                            } else {
                                '.'
                            });
                        }
                        let missing = 16 - chunk.len();
                        for i in 0..missing {
                            if chunk.len() + i == 8 {
                                hex_part.push(' ');
                            }
                            hex_part.push_str("   ");
                        }

                        ui.label(format!("{:08X}  {}  {}", offset, hex_part, ascii_part));
                    }
                });

            if lines > 256 {
                ui.label(
                    RichText::new(format!("(仅显示前 4096 字节，共 {} 字节)", data.len())).weak(),
                );
            }
        }

        ui.separator();

        ui.horizontal(|ui| {
            let is_downloading =
                self.schema_fetch_rx.is_some() || self.schema_update_all_rx.is_some();

            if ui
                .add_enabled(!is_downloading, egui::Button::new("更新 Schema"))
                .clicked()
            {
                if let Some(name) = &self.loaded_table_name {
                    let rx = self.schema_runner.spawn_fetch(name.clone());
                    self.schema_fetch_rx = Some(rx);
                }
            }
            if ui
                .add_enabled(!is_downloading, egui::Button::new("更新全部"))
                .clicked()
            {
                let names = crate::schema::cached_schema_names();
                if !names.is_empty() {
                    let rx = self.schema_runner.spawn_fetch_all(names);
                    self.schema_update_all_rx = Some(rx);
                }
            }
        });

        let tracker = self.schema_runner.tracker();
        if tracker.is_active() {
            show_progress_bar(ui, &tracker);
        }

        ui.separator();

        ui.horizontal(|ui| {
            ui.label("搜索:");
            ui.text_edit_singleline(&mut self.search);
        });

        if self.search != self.prev_search {
            self.prev_search = self.search.clone();
            let search_lower = self.search.to_lowercase();
            if search_lower.is_empty() {
                self.filtered_indices = (0..self.all_table_names.len()).collect();
            } else {
                self.filtered_indices = self
                    .all_table_names
                    .iter()
                    .enumerate()
                    .filter(|(_, name)| name.to_lowercase().contains(&search_lower))
                    .map(|(idx, _)| idx)
                    .collect();
            }
        }

        ui.label(format!(
            "{} / {}",
            self.filtered_indices.len(),
            self.all_table_names.len()
        ));
        ui.separator();

        let filtered_count = self.filtered_indices.len();
        let mut click_table = None;

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_rows(ui, 18.0, filtered_count, |ui, range| {
                for row_idx in range {
                    let table_idx = self.filtered_indices[row_idx];
                    let name = &self.all_table_names[table_idx];
                    let selected = self.selected_table_idx == Some(table_idx);
                    if ui.selectable_label(selected, name).clicked() {
                        click_table = Some(table_idx);
                    }
                }
            });

        if let Some(idx) = click_table {
            self.select_table(idx);
        }
    }

    fn select_table(&mut self, idx: usize) {
        if self.selected_table_idx == Some(idx) {
            return;
        }
        self.selected_table_idx = Some(idx);
        self.selected_row_idx = None;
        self.extracted_paths.clear();
        self.loaded_table_name = None;
        self.loaded_exh = None;
        self.flat_rows.clear();
        self.schema_columns.clear();
    }

    fn show_central_panel(&mut self, ui: &mut egui::Ui, game: &GameData) {
        let Some(table_idx) = self.selected_table_idx else {
            ui.centered_and_justified(|ui| {
                ui.label("← 从左侧选择一张 EXD 表");
            });
            return;
        };

        let table_name = self.all_table_names[table_idx].clone();

        if self.loaded_table_name.as_deref() != Some(&table_name) {
            self.load_table(game, &table_name);
        }

        let Some(exh) = &self.loaded_exh else {
            ui.colored_label(
                egui::Color32::from_rgb(255, 100, 100),
                format!("无法加载表头: {}", table_name),
            );
            return;
        };

        ui.heading(&table_name);
        ui.horizontal(|ui| {
            ui.label(format!(
                "列: {}  行: {}  语言: {}  类型: {}",
                exh.column_definitions.len(),
                self.flat_rows.len(),
                exh.languages.len(),
                match exh.header.row_kind {
                    SheetRowKind::SingleRow => "SingleRow",
                    SheetRowKind::SubRows => "SubRows",
                },
            ));
        });
        ui.separator();

        let col_count = exh.column_definitions.len().min(50);
        let row_count = self.flat_rows.len();

        if row_count == 0 {
            ui.label("(无数据行)");
        } else {
            ui.style_mut().override_font_id = Some(egui::FontId::monospace(12.0));

            let id_col = egui_table::Column::new(60.0)
                .range(40.0..=120.0)
                .resizable(true);
            let mut columns = vec![id_col];

            for i in 0..col_count {
                let dt = exh.column_definitions[i].data_type;
                let w = match dt {
                    ColumnDataType::String => 150.0,
                    ColumnDataType::UInt64 | ColumnDataType::Int64 => 130.0,
                    ColumnDataType::Float32 => 70.0,
                    ColumnDataType::Bool
                    | ColumnDataType::PackedBool0
                    | ColumnDataType::PackedBool1
                    | ColumnDataType::PackedBool2
                    | ColumnDataType::PackedBool3
                    | ColumnDataType::PackedBool4
                    | ColumnDataType::PackedBool5
                    | ColumnDataType::PackedBool6
                    | ColumnDataType::PackedBool7 => 45.0,
                    _ => 60.0,
                };
                columns.push(
                    egui_table::Column::new(w)
                        .range(30.0..=500.0)
                        .resizable(true),
                );
            }

            let mut delegate = ExdTableDelegate {
                flat_rows: &self.flat_rows,
                exh,
                column_names: &self.schema_columns,
                selected_row_idx: self.selected_row_idx,
                clicked_row: None,
            };

            Table::new()
                .id_salt("exd_data_table")
                .num_rows(row_count as u64)
                .columns(columns)
                .num_sticky_cols(1)
                .headers([HeaderRow::new(20.0)])
                .show(ui, &mut delegate);

            ui.style_mut().override_font_id = None;

            if let Some(row_idx) = delegate.clicked_row {
                self.selected_row_idx = Some(row_idx);
                let (row_id, columns) = &self.flat_rows[row_idx];
                self.extracted_paths = extract_paths(&table_name, *row_id, columns);
            }
        }

        if !self.extracted_paths.is_empty() {
            ui.separator();
            ui.label(RichText::new("提取的文件路径:").strong());
            let mut copy_path = None;
            for path in &self.extracted_paths {
                ui.horizontal(|ui| {
                    ui.monospace(path);
                    if ui.small_button("复制到输入").clicked() {
                        copy_path = Some(path.clone());
                    }
                });
            }
            if let Some(p) = copy_path {
                self.path_input = p;
            }
        }
    }

    fn load_table(&mut self, game: &GameData, name: &str) {
        self.loaded_table_name = Some(name.to_string());
        self.loaded_exh = None;
        self.flat_rows.clear();
        self.selected_row_idx = None;
        self.extracted_paths.clear();

        match crate::schema::load_schema_from_cache(name) {
            Some(columns) => {
                self.schema_columns = columns;
            }
            None => {
                self.schema_columns.clear();
                let rx = self.schema_runner.spawn_fetch(name.to_string());
                self.schema_fetch_rx = Some(rx);
            }
        }

        let Some(exh) = game.read_excel_header(name) else {
            return;
        };

        let lang = if exh.languages.contains(&Language::ChineseSimplified) {
            Language::ChineseSimplified
        } else if let Some(&first) = exh.languages.first() {
            first
        } else {
            Language::None
        };

        if let Some(sheet) = game.read_excel_sheet(&exh, name, lang) {
            for page in &sheet.pages {
                for (row_id, row) in page.into_iter().flatten_subrows() {
                    self.flat_rows.push((row_id, row.columns.clone()));
                }
            }
        }

        self.loaded_exh = Some(exh);
    }

    fn do_read_file(&mut self, game: &GameData) {
        let path = self.path_input.trim().to_string();
        if path.is_empty() {
            self.preview_error = Some("请输入文件路径".to_string());
            self.preview = None;
            return;
        }

        match game.read_file(&path) {
            Ok(data) => {
                self.preview_error = None;
                self.preview = Some(FilePreview::Hex {
                    path: path.clone(),
                    data,
                });
            }
            Err(e) => {
                self.preview_error = Some(format!("读取失败: {}", e));
                self.preview = None;
            }
        }
    }
}

fn column_type_short(dt: ColumnDataType) -> &'static str {
    match dt {
        ColumnDataType::String => "str",
        ColumnDataType::Bool
        | ColumnDataType::PackedBool0
        | ColumnDataType::PackedBool1
        | ColumnDataType::PackedBool2
        | ColumnDataType::PackedBool3
        | ColumnDataType::PackedBool4
        | ColumnDataType::PackedBool5
        | ColumnDataType::PackedBool6
        | ColumnDataType::PackedBool7 => "bool",
        ColumnDataType::Int8 => "i8",
        ColumnDataType::UInt8 => "u8",
        ColumnDataType::Int16 => "i16",
        ColumnDataType::UInt16 => "u16",
        ColumnDataType::Int32 => "i32",
        ColumnDataType::UInt32 => "u32",
        ColumnDataType::Float32 => "f32",
        ColumnDataType::Int64 => "i64",
        ColumnDataType::UInt64 => "u64",
    }
}

fn format_field(field: &Field) -> String {
    match field {
        Field::String(s) => {
            if s.len() > 30 {
                format!("{}…", &s[..s.floor_char_boundary(30)])
            } else {
                s.clone()
            }
        }
        Field::Bool(b) => (if *b { "true" } else { "false" }).to_string(),
        Field::UInt64(v) => format!("{} (0x{:X})", v, v),
        Field::Float32(v) => format!("{:.2}", v),
        Field::Int8(v) => v.to_string(),
        Field::UInt8(v) => v.to_string(),
        Field::Int16(v) => v.to_string(),
        Field::UInt16(v) => v.to_string(),
        Field::Int32(v) => v.to_string(),
        Field::UInt32(v) => v.to_string(),
        Field::Int64(v) => v.to_string(),
    }
}

fn extract_paths(table_name: &str, _row_id: u32, row: &[Field]) -> Vec<String> {
    match table_name {
        "Item" => extract_item_paths(row),
        _ => vec![],
    }
}

fn extract_item_paths(row: &[Field]) -> Vec<String> {
    let model_main = match row.get(47) {
        Some(Field::UInt64(v)) => *v,
        _ => return vec![],
    };

    let set_id = (model_main & 0xFFFF) as u16;
    if set_id == 0 {
        return vec![];
    }

    let equip_cat = match row.get(17) {
        Some(Field::UInt8(v)) => *v,
        _ => return vec![],
    };

    let Some(slot) = EquipSlot::from_category(equip_cat) else {
        return vec![];
    };

    vec![format!(
        "chara/equipment/e{:04}/model/c0201e{:04}_{}.mdl",
        set_id,
        set_id,
        slot.slot_abbr()
    )]
}
