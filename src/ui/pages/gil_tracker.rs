use std::collections::BTreeMap;
use std::path::PathBuf;

use eframe::egui;
use egui_plot::*;

use crate::App;

#[derive(Debug, Clone)]
pub struct GilRecord {
    pub timestamp_secs: i64,
    pub total: i64,
    pub delta: i64,
    pub source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GilViewMode {
    Total,
    Delta,
}

pub struct DailySummary {
    pub day_offset: i32,
    pub close: i64,
    pub income: i64,
    pub expense: i64,
}

pub struct GilTrackerState {
    pub data_dir: String,
    pub records: Vec<GilRecord>,
    pub loaded: bool,
    pub error: Option<String>,
    pub source_filter: Option<String>,
    pub view_mode: GilViewMode,
    pub daily_summaries: Vec<DailySummary>,
    pub daily_mode: bool,
}

impl Default for GilTrackerState {
    fn default() -> Self {
        let default_dir = dirs_document_dir();
        Self {
            data_dir: default_dir,
            records: Vec::new(),
            loaded: false,
            error: None,
            source_filter: None,
            view_mode: GilViewMode::Total,
            daily_summaries: Vec::new(),
            daily_mode: false,
        }
    }
}

fn dirs_document_dir() -> String {
    std::env::var("USERPROFILE")
        .map(|p| format!("{}\\Documents\\GilTracker", p))
        .unwrap_or_default()
}

fn parse_datetime(s: &str) -> Option<i64> {
    let s = s.trim();
    let parts: Vec<&str> = s.split(' ').collect();
    if parts.len() != 2 {
        return None;
    }
    let date_parts: Vec<i32> = parts[0].split('-').filter_map(|x| x.parse().ok()).collect();
    let time_parts: Vec<i32> = parts[1].split(':').filter_map(|x| x.parse().ok()).collect();
    if date_parts.len() != 3 || time_parts.len() != 3 {
        return None;
    }
    let (y, m, d) = (date_parts[0], date_parts[1], date_parts[2]);
    let (h, min, sec) = (time_parts[0], time_parts[1], time_parts[2]);
    let days = date_to_days(y, m, d);
    Some(days as i64 * 86400 + h as i64 * 3600 + min as i64 * 60 + sec as i64)
}

fn date_to_days(y: i32, m: i32, d: i32) -> i32 {
    let a = (14 - m) / 12;
    let y4 = y + 4800 - a;
    let m4 = m + 12 * a - 3;
    d + (153 * m4 + 2) / 5 + 365 * y4 + y4 / 4 - y4 / 100 + y4 / 400 - 32045
}

fn secs_to_date_label(secs: i64) -> String {
    let days = secs / 86400;
    let rem = secs % 86400;
    let h = rem / 3600;
    let m = (rem % 3600) / 60;
    let (y, mo, d) = jd_to_date(days);
    format!("{:04}-{:02}-{:02} {:02}:{:02}", y, mo, d, h, m)
}

fn jd_to_date(days: i64) -> (i32, i32, i32) {
    let jd = days + 2460578;
    let a = jd + 32044;
    let b = (4 * a + 3) / 146097;
    let c = a - (146097 * b) / 4;
    let d = (4 * c + 3) / 1461;
    let e = c - (1461 * d) / 4;
    let m = (5 * e + 2) / 153;
    let day = e - (153 * m + 2) / 5 + 1;
    let month = m + 3 - 12 * (m / 10);
    let year = 100 * b + d - 4800 + m / 10;
    (year as i32, month as i32, day as i32)
}

fn format_gil(value: i64) -> String {
    let abs = value.abs();
    if abs >= 1_000_000 {
        format!("{:.1}M", value as f64 / 1_000_000.0)
    } else if abs >= 1_000 {
        format!("{:.1}K", value as f64 / 1_000.0)
    } else {
        format!("{}", value)
    }
}

impl GilTrackerState {
    fn load_data(&mut self) {
        self.records.clear();
        self.daily_summaries.clear();
        self.error = None;

        let data_dir = PathBuf::from(&self.data_dir);
        if !data_dir.exists() {
            self.error = Some(format!("数据目录不存在: {}", self.data_dir));
            self.loaded = true;
            return;
        }

        let mut all_records: Vec<GilRecord> = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&data_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Ok(sub_entries) = std::fs::read_dir(&path) {
                        for sub_entry in sub_entries.flatten() {
                            let sub_path = sub_entry.path();
                            if sub_path.extension().and_then(|e| e.to_str()) == Some("csv") {
                                Self::load_csv(&sub_path, &mut all_records);
                            }
                        }
                    }
                } else if path.extension().and_then(|e| e.to_str()) == Some("csv") {
                    Self::load_csv(&path, &mut all_records);
                }
            }
        }

        if all_records.is_empty() {
            self.error = Some(format!("未找到数据文件: {}", self.data_dir));
            self.loaded = true;
            return;
        }

        all_records.sort_by_key(|r| r.timestamp_secs);
        self.records = all_records;
        self.compute_daily_summaries();
        self.loaded = true;
    }

    fn load_csv(path: &PathBuf, records: &mut Vec<GilRecord>) {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return,
        };

        let mut has_header = false;
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if !has_header {
                has_header = true;
                continue;
            }

            let fields: Vec<&str> = line.split(',').collect();
            if fields.len() < 3 {
                continue;
            }

            let ts = match parse_datetime(fields[0]) {
                Some(t) => t,
                None => continue,
            };
            let delta: i64 = fields[1].trim().parse().unwrap_or(0);
            let total: i64 = fields[2].trim().parse().unwrap_or(0);
            let source = fields
                .get(3)
                .map(|s| s.trim().to_string())
                .unwrap_or_default();

            records.push(GilRecord {
                timestamp_secs: ts,
                total,
                delta,
                source,
            });
        }
    }

    fn compute_daily_summaries(&mut self) {
        let mut by_day: BTreeMap<i32, Vec<&GilRecord>> = BTreeMap::new();
        for r in &self.records {
            let day = (r.timestamp_secs / 86400) as i32;
            by_day.entry(day).or_default().push(r);
        }

        let first_day = by_day.keys().next().copied().unwrap_or(0);

        self.daily_summaries = by_day
            .into_iter()
            .map(|(day, recs)| {
                let close = recs.last().map(|r| r.total).unwrap_or(0);
                let income: i64 = recs.iter().filter(|r| r.delta > 0).map(|r| r.delta).sum();
                let expense: i64 = recs.iter().filter(|r| r.delta < 0).map(|r| r.delta).sum();
                DailySummary {
                    day_offset: day - first_day,
                    close,
                    income,
                    expense,
                }
            })
            .collect();
    }

    fn sources(&self) -> Vec<String> {
        let mut sources: Vec<String> = self
            .records
            .iter()
            .map(|r| r.source.clone())
            .filter(|s| !s.is_empty())
            .collect();
        sources.sort();
        sources.dedup();
        sources
    }

    fn filtered_records(&self) -> Vec<&GilRecord> {
        match &self.source_filter {
            Some(filter) => self
                .records
                .iter()
                .filter(|r| r.source == *filter || r.source.is_empty())
                .collect(),
            None => self.records.iter().collect(),
        }
    }
}

impl App {
    pub fn show_gil_tracker_page(&mut self, ctx: &egui::Context) {
        if !self.gil_tracker.loaded {
            self.gil_tracker.load_data();
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("金币追踪");
            ui.add_space(4.0);

            if let Some(err) = &self.gil_tracker.error {
                ui.colored_label(egui::Color32::RED, err);
                ui.add_space(8.0);
            }

            self.show_gil_tracker_dir_selector(ui);

            ui.add_space(4.0);
            ui.separator();
            ui.add_space(4.0);

            self.show_gil_tracker_stats(ui);

            ui.add_space(4.0);

            self.show_gil_tracker_controls(ui);

            ui.add_space(4.0);

            self.show_gil_tracker_chart(ui);

            ui.add_space(4.0);

            self.show_gil_tracker_table(ui);
        });
    }

    fn show_gil_tracker_dir_selector(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("数据目录:");
            ui.text_edit_singleline(&mut self.gil_tracker.data_dir);
            if ui.button("选择目录").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("选择 GilTracker 数据目录")
                    .pick_folder()
                {
                    self.gil_tracker.data_dir = path.to_string_lossy().to_string();
                    self.gil_tracker.loaded = false;
                    self.gil_tracker.error = None;
                }
            }
            if ui.button("刷新").clicked() {
                self.gil_tracker.loaded = false;
                self.gil_tracker.error = None;
            }
        });
    }

    fn show_gil_tracker_stats(&self, ui: &mut egui::Ui) {
        let records = &self.gil_tracker.records;
        if records.is_empty() {
            return;
        }

        let first = &records[0];
        let last = records.last().unwrap();

        let total_income: i64 = records
            .iter()
            .filter(|r| r.delta > 0)
            .map(|r| r.delta)
            .sum();
        let total_expense: i64 = records
            .iter()
            .filter(|r| r.delta < 0)
            .map(|r| r.delta)
            .sum();
        let net_change = last.total - first.total;

        ui.horizontal(|ui| {
            ui.group(|ui| {
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("当前余额").small().weak());
                    ui.label(
                        egui::RichText::new(format_gil(last.total))
                            .strong()
                            .size(18.0),
                    );
                });
            });
            ui.group(|ui| {
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("总收入").small().weak());
                    ui.label(
                        egui::RichText::new(format!("+{}", format_gil(total_income)))
                            .color(egui::Color32::from_rgb(100, 200, 100))
                            .strong()
                            .size(16.0),
                    );
                });
            });
            ui.group(|ui| {
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("总支出").small().weak());
                    ui.label(
                        egui::RichText::new(format_gil(total_expense))
                            .color(egui::Color32::from_rgb(200, 100, 100))
                            .strong()
                            .size(16.0),
                    );
                });
            });
            ui.group(|ui| {
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("净变化").small().weak());
                    let color = if net_change >= 0 {
                        egui::Color32::from_rgb(100, 200, 100)
                    } else {
                        egui::Color32::from_rgb(200, 100, 100)
                    };
                    let sign = if net_change >= 0 { "+" } else { "" };
                    ui.label(
                        egui::RichText::new(format!("{}{}", sign, format_gil(net_change)))
                            .color(color)
                            .strong()
                            .size(16.0),
                    );
                });
            });
            ui.group(|ui| {
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("记录数").small().weak());
                    ui.label(
                        egui::RichText::new(records.len().to_string())
                            .strong()
                            .size(16.0),
                    );
                });
            });
        });
    }

    fn show_gil_tracker_controls(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("视图:");
            ui.selectable_value(
                &mut self.gil_tracker.view_mode,
                GilViewMode::Total,
                "总余额",
            );
            ui.selectable_value(
                &mut self.gil_tracker.view_mode,
                GilViewMode::Delta,
                "变动量",
            );

            ui.add_space(16.0);

            ui.checkbox(&mut self.gil_tracker.daily_mode, "按日汇总");

            ui.add_space(16.0);

            ui.label("筛选来源:");
            let sources = self.gil_tracker.sources();
            let all_selected = self.gil_tracker.source_filter.is_none();
            if ui.selectable_label(all_selected, "全部").clicked() {
                self.gil_tracker.source_filter = None;
            }
            for source in &sources {
                let selected = self.gil_tracker.source_filter.as_ref() == Some(source);
                if ui.selectable_label(selected, source).clicked() {
                    if selected {
                        self.gil_tracker.source_filter = None;
                    } else {
                        self.gil_tracker.source_filter = Some(source.clone());
                    }
                }
            }
        });
    }

    fn show_gil_tracker_chart(&self, ui: &mut egui::Ui) {
        let records = self.gil_tracker.filtered_records();
        if records.is_empty() {
            ui.label("暂无数据");
            return;
        }

        let daily_mode = self.gil_tracker.daily_mode;

        let date_labels: Vec<String> = records
            .iter()
            .map(|r| {
                let (_y, m, d) = jd_to_date(r.timestamp_secs / 86400);
                format!("{}/{}", m, d)
            })
            .collect();

        match self.gil_tracker.view_mode {
            GilViewMode::Total => {
                if daily_mode {
                    let summaries = &self.gil_tracker.daily_summaries;
                    let summary_labels: Vec<String> = summaries
                        .iter()
                        .map(|s| {
                            let abs_day = s.day_offset as i64
                                + self
                                    .gil_tracker
                                    .records
                                    .first()
                                    .map(|r| r.timestamp_secs / 86400)
                                    .unwrap_or(0);
                            let (_y, m, d) = jd_to_date(abs_day);
                            format!("{}/{}", m, d)
                        })
                        .collect();

                    let points: PlotPoints = summaries
                        .iter()
                        .enumerate()
                        .map(|(i, s)| [i as f64, s.close as f64])
                        .collect();

                    let line = Line::new("日终余额", points)
                        .color(egui::Color32::from_rgb(77, 166, 255))
                        .width(2.0);

                    let plot = Plot::new("gil_chart")
                        .legend(Legend::default().position(Corner::LeftTop))
                        .show_x(true)
                        .show_y(true)
                        .x_axis_formatter(move |mark, _range| {
                            let idx = mark.value as usize;
                            summary_labels.get(idx).cloned().unwrap_or_default()
                        })
                        .y_axis_formatter(|mark, _range| format_gil(mark.value as i64));

                    plot.show(ui, |plot_ui| {
                        plot_ui.line(line);
                    });
                } else {
                    let points: PlotPoints = records
                        .iter()
                        .enumerate()
                        .map(|(i, r)| [i as f64, r.total as f64])
                        .collect();

                    let line = Line::new("总余额", points)
                        .color(egui::Color32::from_rgb(77, 166, 255))
                        .width(2.0);

                    let plot = Plot::new("gil_chart")
                        .legend(Legend::default().position(Corner::LeftTop))
                        .show_x(true)
                        .show_y(true)
                        .x_axis_formatter(move |mark, _range| {
                            let idx = mark.value as usize;
                            date_labels.get(idx).cloned().unwrap_or_default()
                        })
                        .y_axis_formatter(|mark, _range| format_gil(mark.value as i64));

                    plot.show(ui, |plot_ui| {
                        plot_ui.line(line);
                    });
                }
            }
            GilViewMode::Delta => {
                if daily_mode {
                    let summaries = &self.gil_tracker.daily_summaries;
                    let summary_labels: Vec<String> = summaries
                        .iter()
                        .map(|s| {
                            let abs_day = s.day_offset as i64
                                + self
                                    .gil_tracker
                                    .records
                                    .first()
                                    .map(|r| r.timestamp_secs / 86400)
                                    .unwrap_or(0);
                            let (_y, m, d) = jd_to_date(abs_day);
                            format!("{}/{}", m, d)
                        })
                        .collect();

                    let income_pts: PlotPoints = summaries
                        .iter()
                        .enumerate()
                        .map(|(i, s)| [i as f64, s.income as f64])
                        .collect();

                    let expense_pts: PlotPoints = summaries
                        .iter()
                        .enumerate()
                        .map(|(i, s)| [i as f64, s.expense as f64])
                        .collect();

                    let income_line = Line::new("日收入", income_pts)
                        .color(egui::Color32::from_rgb(100, 200, 100))
                        .width(2.0);

                    let expense_line = Line::new("日支出", expense_pts)
                        .color(egui::Color32::from_rgb(200, 100, 100))
                        .width(2.0);

                    let plot = Plot::new("gil_chart")
                        .legend(Legend::default().position(Corner::LeftTop))
                        .show_x(true)
                        .show_y(true)
                        .x_axis_formatter(move |mark, _range| {
                            let idx = mark.value as usize;
                            summary_labels.get(idx).cloned().unwrap_or_default()
                        })
                        .y_axis_formatter(|mark, _range| format_gil(mark.value as i64));

                    plot.show(ui, |plot_ui| {
                        plot_ui.line(income_line);
                        plot_ui.line(expense_line);
                    });
                } else {
                    let income_pts: PlotPoints = records
                        .iter()
                        .enumerate()
                        .filter(|(_, r)| r.delta > 0)
                        .map(|(i, r)| [i as f64, r.delta as f64])
                        .collect();

                    let expense_pts: PlotPoints = records
                        .iter()
                        .enumerate()
                        .filter(|(_, r)| r.delta < 0)
                        .map(|(i, r)| [i as f64, r.delta as f64])
                        .collect();

                    let income_line = Line::new("收入", income_pts)
                        .color(egui::Color32::from_rgb(100, 200, 100))
                        .width(2.0);

                    let expense_line = Line::new("支出", expense_pts)
                        .color(egui::Color32::from_rgb(200, 100, 100))
                        .width(2.0);

                    let plot = Plot::new("gil_chart")
                        .legend(Legend::default().position(Corner::LeftTop))
                        .show_x(true)
                        .show_y(true)
                        .x_axis_formatter(move |mark, _range| {
                            let idx = mark.value as usize;
                            date_labels.get(idx).cloned().unwrap_or_default()
                        })
                        .y_axis_formatter(|mark, _range| format_gil(mark.value as i64));

                    plot.show(ui, |plot_ui| {
                        plot_ui.line(income_line);
                        plot_ui.line(expense_line);
                    });
                }
            }
        }
    }

    fn show_gil_tracker_table(&self, ui: &mut egui::Ui) {
        let records = self.gil_tracker.filtered_records();
        if records.is_empty() {
            return;
        }

        ui.collapsing("详细记录", |ui| {
            let avail = ui.available_height() - 30.0;
            egui::ScrollArea::vertical()
                .max_height(avail.max(100.0))
                .show(ui, |ui| {
                    egui::Grid::new("gil_table")
                        .striped(true)
                        .min_col_width(120.0)
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new("时间").strong());
                            ui.label(egui::RichText::new("变动").strong());
                            ui.label(egui::RichText::new("余额").strong());
                            ui.label(egui::RichText::new("来源").strong());
                            ui.end_row();

                            let display_records: Vec<_> =
                                records.into_iter().rev().take(200).collect();
                            for r in display_records {
                                let date_str = secs_to_date_label(r.timestamp_secs);
                                ui.label(egui::RichText::new(date_str).small());

                                let delta_color = if r.delta > 0 {
                                    egui::Color32::from_rgb(100, 200, 100)
                                } else if r.delta < 0 {
                                    egui::Color32::from_rgb(200, 100, 100)
                                } else {
                                    egui::Color32::GRAY
                                };
                                let sign = if r.delta > 0 { "+" } else { "" };
                                ui.label(
                                    egui::RichText::new(format!("{}{}", sign, r.delta))
                                        .small()
                                        .color(delta_color),
                                );

                                ui.label(egui::RichText::new(format_gil(r.total)).small());

                                ui.label(egui::RichText::new(&r.source).small());
                                ui.end_row();
                            }
                        });
                });
        });
    }
}
