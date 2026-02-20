use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum ProgressUnit {
    #[default]
    Bytes,
    Count,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum ProgressStatus {
    #[default]
    Ongoing,
    Completed,
    Failed,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ProgressKind {
    Determinate,
    Indeterminate,
}

#[derive(Clone)]
pub struct ProgressState {
    pub message: String,
    pub current: u64,
    pub total: u64,
    pub kind: ProgressKind,
    pub unit: ProgressUnit,
    pub status: ProgressStatus,
}

impl Default for ProgressState {
    fn default() -> Self {
        Self {
            message: String::new(),
            current: 0,
            total: 0,
            kind: ProgressKind::Determinate,
            unit: ProgressUnit::default(),
            status: ProgressStatus::default(),
        }
    }
}

impl ProgressState {
    pub fn fraction(&self) -> f32 {
        if self.total == 0 {
            return 0.0;
        }
        (self.current as f64 / self.total as f64).min(1.0) as f32
    }

    pub fn percent(&self) -> u8 {
        (self.fraction() * 100.0).min(100.0) as u8
    }

    pub fn format_bytes(bytes: u64) -> String {
        if bytes < 1024 {
            format!("{} B", bytes)
        } else if bytes < 1024 * 1024 {
            format!("{:.1} KB", bytes as f64 / 1024.0)
        } else {
            format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
        }
    }

    fn format_value(&self, value: u64) -> String {
        match self.unit {
            ProgressUnit::Bytes => Self::format_bytes(value),
            ProgressUnit::Count => value.to_string(),
        }
    }

    pub fn format_progress(&self) -> String {
        if self.total == 0 {
            self.format_value(self.current)
        } else {
            format!(
                "{} / {} ({}%)",
                self.format_value(self.current),
                self.format_value(self.total),
                self.percent()
            )
        }
    }
}

#[derive(Clone)]
pub struct ProgressTracker {
    state: Arc<Mutex<ProgressState>>,
}

impl Default for ProgressTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressTracker {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(ProgressState::default())),
        }
    }

    pub fn set_message(&self, msg: impl Into<String>) {
        let mut state = self.state.lock().unwrap();
        state.message = msg.into();
    }

    pub fn set_length(&self, total: u64) {
        let mut state = self.state.lock().unwrap();
        state.total = total;
        state.kind = ProgressKind::Determinate;
    }

    pub fn set_position(&self, pos: u64) {
        let mut state = self.state.lock().unwrap();
        state.current = pos;
    }

    pub fn set_unit(&self, unit: ProgressUnit) {
        let mut state = self.state.lock().unwrap();
        state.unit = unit;
    }

    pub fn set_completed(&self) {
        let mut state = self.state.lock().unwrap();
        state.status = ProgressStatus::Completed;
        state.kind = ProgressKind::Determinate;
        state.current = state.total;
    }

    pub fn set_failed(&self, error: impl Into<String>) {
        let mut state = self.state.lock().unwrap();
        state.status = ProgressStatus::Failed;
        state.message = error.into();
    }

    pub fn set_indeterminate(&self) {
        let mut state = self.state.lock().unwrap();
        state.kind = ProgressKind::Indeterminate;
        state.total = 0;
        state.current = 0;
    }

    pub fn clear(&self) {
        let mut state = self.state.lock().unwrap();
        state.message.clear();
        state.current = 0;
        state.total = 0;
        state.kind = ProgressKind::Determinate;
        state.unit = ProgressUnit::default();
        state.status = ProgressStatus::default();
    }

    pub fn state(&self) -> ProgressState {
        self.state.lock().unwrap().clone()
    }

    pub fn is_active(&self) -> bool {
        let state = self.state.lock().unwrap();
        !state.message.is_empty()
            || state.current > 0
            || state.kind == ProgressKind::Indeterminate
            || state.status != ProgressStatus::Ongoing
    }
}

pub struct ProgressBar<'a> {
    tracker: &'a ProgressTracker,
    height: f32,
    animate: bool,
}

impl<'a> ProgressBar<'a> {
    pub fn new(tracker: &'a ProgressTracker) -> Self {
        Self {
            tracker,
            height: 6.0,
            animate: true,
        }
    }

    pub fn height(mut self, h: f32) -> Self {
        self.height = h;
        self
    }

    pub fn animate(mut self, animate: bool) -> Self {
        self.animate = animate;
        self
    }

    pub fn show(&self, ui: &mut eframe::egui::Ui) {
        let state = self.tracker.state();

        let min_width = 120.0;
        let available = ui.available_rect_before_wrap();
        let rect = if available.width() < min_width {
            eframe::egui::Rect::from_min_size(
                available.min,
                eframe::egui::vec2(min_width, available.height()),
            )
        } else {
            available
        };
        let bar_rect = eframe::egui::Rect::from_min_size(
            rect.min,
            eframe::egui::vec2(rect.width(), self.height),
        );

        let visuals = ui.visuals();
        let bg_color = visuals.extreme_bg_color;
        let fg_color = match state.status {
            ProgressStatus::Ongoing => visuals.selection.bg_fill,
            ProgressStatus::Completed => eframe::egui::Color32::from_rgb(76, 175, 80),
            ProgressStatus::Failed => eframe::egui::Color32::from_rgb(244, 67, 54),
        };

        ui.painter().rect_filled(bar_rect, 0.0, bg_color);

        let fill_rect = if state.kind == ProgressKind::Indeterminate {
            let fill_length = rect.width() * 0.4;
            let time = ui.input(|i| i.time) as f32;
            let offset = if self.animate {
                (time * 0.5 * rect.width()).rem_euclid(rect.width() + fill_length) - fill_length
            } else {
                0.0
            };
            let raw_rect = eframe::egui::Rect::from_min_size(
                eframe::egui::pos2(rect.min.x + offset, rect.min.y),
                eframe::egui::vec2(fill_length, self.height),
            );
            raw_rect.intersect(bar_rect)
        } else {
            let fill_width = rect.width() * state.fraction();
            eframe::egui::Rect::from_min_size(rect.min, eframe::egui::vec2(fill_width, self.height))
        };

        if fill_rect.is_positive() {
            ui.painter().rect_filled(fill_rect, 0.0, fg_color);
        }

        if state.kind == ProgressKind::Determinate && state.total > 0 {
            let percent_text = format!("{}%", state.percent());
            let font_id = eframe::egui::FontId::proportional(10.0);
            let text_color = if state.fraction() > 0.5 {
                visuals.extreme_bg_color
            } else {
                visuals.text_color()
            };
            let text_pos = eframe::egui::pos2(
                rect.min.x + rect.width() * 0.5,
                rect.min.y + self.height * 0.5,
            );
            ui.painter().text(
                text_pos,
                eframe::egui::Align2::CENTER_CENTER,
                percent_text,
                font_id,
                text_color,
            );
        }

        if self.animate && state.kind == ProgressKind::Indeterminate {
            ui.ctx().request_repaint();
        }

        ui.allocate_space(eframe::egui::vec2(rect.width(), self.height));

        ui.horizontal(|ui| {
            if !state.message.is_empty() {
                let text = eframe::egui::RichText::new(&state.message).small();
                let text = match state.status {
                    ProgressStatus::Failed => {
                        text.color(eframe::egui::Color32::from_rgb(244, 67, 54))
                    }
                    ProgressStatus::Completed => {
                        text.color(eframe::egui::Color32::from_rgb(76, 175, 80))
                    }
                    _ => text.weak(),
                };
                ui.label(text);
            }
            if state.kind == ProgressKind::Determinate && state.current > 0 {
                ui.with_layout(
                    eframe::egui::Layout::right_to_left(eframe::egui::Align::Center),
                    |ui| {
                        ui.label(
                            eframe::egui::RichText::new(state.format_progress())
                                .small()
                                .weak(),
                        );
                    },
                );
            }
        });
    }
}

pub fn show_progress_bar(ui: &mut eframe::egui::Ui, tracker: &ProgressTracker) {
    ProgressBar::new(tracker).show(ui);
}
