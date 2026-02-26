use std::sync::mpsc;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use auto_play::{AutoPlay, ControllerTrait, MatcherOptions, WindowsController};
use image::DynamicImage;

use crate::template::TemplateDef;

pub const TEMPLATES: &[TemplateDef] = &[
    TemplateDef {
        name: "开始制作",
        filename: "start_crafting.png",
        default_bytes: include_bytes!("../assets/start_crafting.png"),
        threshold: 0.1,
    },
    TemplateDef {
        name: "停止制作",
        filename: "stop_crafting.png",
        default_bytes: include_bytes!("../assets/stop_crafting.png"),
        threshold: 0.2,
    },
];
pub const TPL_START: usize = 0;
pub const TPL_STOP: usize = 1;

/// 传入后台线程的模板数据
pub struct CraftTemplates {
    pub start: DynamicImage,
    pub stop: DynamicImage,
    pub options: MatcherOptions,
    pub options_strict: MatcherOptions,
}

const WINDOW_TITLE: &str = "最终幻想XIV";
const CRAFT_START_TIMEOUT: Duration = Duration::from_secs(5);
const CRAFT_FINISH_TIMEOUT: Duration = Duration::from_secs(120);
const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// 后台线程发送给 UI 的消息
#[derive(Debug, Clone)]
pub enum CraftMessage {
    /// 状态文本更新
    Status(String),
    /// 进度更新 (已完成, 总数)
    Progress(u32, u32),
    /// 单次制作完成
    CraftDone { index: u32, elapsed_secs: f32 },
    /// 单次制作失败
    CraftFailed { index: u32, reason: String },
    /// 全部完成
    Finished { success: u32, total: u32 },
    /// 出错终止
    Error(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum CraftState {
    Ready,
    InProgress,
    Unknown,
}

/// 在后台线程中运行自动制作循环
pub fn run_auto_craft(
    count: u32,
    macro_key: char,
    templates: CraftTemplates,
    tx: mpsc::Sender<CraftMessage>,
    cancel: Arc<AtomicBool>,
) {
    if let Err(e) = run_auto_craft_inner(count, macro_key, &templates, &tx, &cancel) {
        let _ = tx.send(CraftMessage::Error(format!("{}", e)));
    }
}

fn run_auto_craft_inner(
    count: u32,
    macro_key: char,
    templates: &CraftTemplates,
    tx: &mpsc::Sender<CraftMessage>,
    cancel: &Arc<AtomicBool>,
) -> anyhow::Result<()> {
    let _ = tx.send(CraftMessage::Status(format!(
        "连接窗口 '{}'...",
        WINDOW_TITLE
    )));
    let controller = WindowsController::from_window_title(WINDOW_TITLE)?;
    let (w, h) = controller.screen_size();
    let _ = tx.send(CraftMessage::Status(format!("已连接: {}x{}", w, h)));

    let ap = AutoPlay::new(controller);

    let tpl_start = &templates.start;
    let tpl_stop = &templates.stop;
    let options = &templates.options;

    // 检查初始状态
    let _ = tx.send(CraftMessage::Status("检测当前状态...".to_string()));
    let state = detect_state(&ap, tpl_start, tpl_stop, options, &templates.options_strict)?;
    if state != CraftState::Ready {
        let _ = tx.send(CraftMessage::Error(
            "请先打开制作笔记并选择配方".to_string(),
        ));
        return Ok(());
    }

    let _ = tx.send(CraftMessage::Status("开始自动制作".to_string()));
    let mut success = 0u32;

    for i in 1..=count {
        if cancel.load(Ordering::Relaxed) {
            let _ = tx.send(CraftMessage::Status("已取消".to_string()));
            let _ = tx.send(CraftMessage::Finished {
                success,
                total: count,
            });
            return Ok(());
        }

        let _ = tx.send(CraftMessage::Progress(i - 1, count));
        let start = Instant::now();

        match craft_once(&ap, tpl_start, tpl_stop, options, &templates.options_strict, macro_key, cancel) {
            Ok(true) => {
                success += 1;
                let elapsed = start.elapsed().as_secs_f32();
                let _ = tx.send(CraftMessage::CraftDone {
                    index: i,
                    elapsed_secs: elapsed,
                });
                let _ = tx.send(CraftMessage::Progress(i, count));
                // 短暂等待再开始下一次
                std::thread::sleep(Duration::from_millis(500));
            }
            Ok(false) => {
                let _ = tx.send(CraftMessage::CraftFailed {
                    index: i,
                    reason: "未找到制作按钮或超时".to_string(),
                });
                let _ = tx.send(CraftMessage::Finished {
                    success,
                    total: count,
                });
                return Ok(());
            }
            Err(e) => {
                let _ = tx.send(CraftMessage::Error(format!("第{}次出错: {}", i, e)));
                return Ok(());
            }
        }
    }

    let _ = tx.send(CraftMessage::Finished {
        success,
        total: count,
    });
    Ok(())
}

fn detect_state(
    ap: &AutoPlay,
    tpl_start: &DynamicImage,
    tpl_stop: &DynamicImage,
    options: &MatcherOptions,
    options_strict: &MatcherOptions,
) -> anyhow::Result<CraftState> {
    if ap.find_image(tpl_stop, options)?.is_some() {
        return Ok(CraftState::InProgress);
    }
    if ap.find_image(tpl_start, options_strict)?.is_some() {
        return Ok(CraftState::Ready);
    }
    Ok(CraftState::Unknown)
}

fn wait_for_state(
    ap: &AutoPlay,
    tpl_start: &DynamicImage,
    tpl_stop: &DynamicImage,
    options: &MatcherOptions,
    options_strict: &MatcherOptions,
    target: CraftState,
    timeout: Duration,
    cancel: &Arc<AtomicBool>,
) -> anyhow::Result<bool> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if cancel.load(Ordering::Relaxed) {
            return Ok(false);
        }
        if detect_state(ap, tpl_start, tpl_stop, options, options_strict)? == target {
            return Ok(true);
        }
        std::thread::sleep(POLL_INTERVAL);
    }
    Ok(false)
}

fn craft_once(
    ap: &AutoPlay,
    tpl_start: &DynamicImage,
    tpl_stop: &DynamicImage,
    options: &MatcherOptions,
    options_strict: &MatcherOptions,
    macro_key: char,
    cancel: &Arc<AtomicBool>,
) -> anyhow::Result<bool> {
    // 1. 找到并点击 "开始制作作业"
    let Some(rect) = ap.find_image(tpl_start, options)? else {
        return Ok(false);
    };
    let win: &WindowsController = ap.controller_ref().unwrap();
    let click_x = rect.x + rect.width / 2;
    let click_y = rect.y + rect.height / 2;
    win.focus_click(click_x, click_y)?;

    std::thread::sleep(Duration::from_millis(500));

    // 2. 等待制作窗口出现
    if !wait_for_state(
        ap,
        tpl_start,
        tpl_stop,
        options,
        options_strict,
        CraftState::InProgress,
        CRAFT_START_TIMEOUT,
        cancel,
    )? {
        return Ok(false);
    }

    std::thread::sleep(Duration::from_millis(300));

    // 3. 按宏键
    win.focus_press(auto_play::controller::Key::Unicode(macro_key))?;

    // 4. 等待制作完成
    if !wait_for_state(
        ap,
        tpl_start,
        tpl_stop,
        options,
        options_strict,
        CraftState::Ready,
        CRAFT_FINISH_TIMEOUT,
        cancel,
    )? {
        return Ok(false);
    }

    Ok(true)
}
