use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use auto_play::{AutoPlay, ControllerTrait, MatchDefinition, WindowsController};

// ═══════════════════════════════════════════════════════════
// 公共 API
// ═══════════════════════════════════════════════════════════

/// 自动制作所需的模板数据
pub struct CraftTemplates {
    pub start: MatchDefinition,
    pub stop: MatchDefinition,
}

/// 自动制作配置
pub struct AutoCraftConfig {
    pub count: u32,
    pub infinite: bool,
    pub macro_key: char,
    pub templates: CraftTemplates,
}

/// 自动制作过程中产生的事件
#[derive(Debug, Clone)]
pub enum AutoCraftEvent {
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

/// 运行中的自动制作句柄，可用于接收事件和取消任务
pub struct AutoCraftHandle {
    pub receiver: Receiver<AutoCraftEvent>,
    cancel: Arc<AtomicBool>,
}

impl AutoCraftHandle {
    /// 请求取消当前自动制作任务
    pub fn stop(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// 检查是否已请求取消
    pub fn is_stopped(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

/// 自动制作算法封装
///
/// 与 UI 完全解耦，只负责图像匹配、按键模拟和事件上报。
/// 调用者通过 [`AutoCraftHandle`] 接收事件并控制取消。
pub struct AutoCraft;

impl AutoCraft {
    /// 启动自动制作循环，返回一个句柄用于接收事件和取消任务
    pub fn start(config: AutoCraftConfig) -> anyhow::Result<AutoCraftHandle> {
        let (tx, rx) = channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_clone = cancel.clone();

        std::thread::spawn(move || {
            if let Err(e) = run_loop(config, &tx, &cancel_clone) {
                let _ = tx.send(AutoCraftEvent::Error(format!("{}", e)));
            }
        });

        Ok(AutoCraftHandle { receiver: rx, cancel })
    }
}

// ═══════════════════════════════════════════════════════════
// 内部实现
// ═══════════════════════════════════════════════════════════

const WINDOW_TITLE: &str = "最终幻想XIV";
const CRAFT_START_TIMEOUT: Duration = Duration::from_secs(5);
const CRAFT_FINISH_TIMEOUT: Duration = Duration::from_secs(120);
const POLL_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Copy, PartialEq)]
enum CraftState {
    Ready,
    InProgress,
    Unknown,
}

fn run_loop(
    config: AutoCraftConfig,
    tx: &Sender<AutoCraftEvent>,
    cancel: &Arc<AtomicBool>,
) -> anyhow::Result<()> {
    let count = config.count;
    let infinite = config.infinite;
    let macro_key = config.macro_key;
    let templates = config.templates;

    let _ = tx.send(AutoCraftEvent::Status(format!(
        "连接窗口 '{}'...",
        WINDOW_TITLE
    )));
    let controller = WindowsController::from_window_title(WINDOW_TITLE)?;
    let (w, h) = controller.screen_size();
    let _ = tx.send(AutoCraftEvent::Status(format!("已连接: {}x{}", w, h)));

    let ap = AutoPlay::new(controller);

    // 检查初始状态
    let _ = tx.send(AutoCraftEvent::Status("检测当前状态...".to_string()));
    let state = detect_state(&ap, &templates)?;
    if state != CraftState::Ready {
        let _ = tx.send(AutoCraftEvent::Error(
            "请先打开制作笔记并选择配方".to_string(),
        ));
        return Ok(());
    }

    let _ = tx.send(AutoCraftEvent::Status("开始自动制作".to_string()));
    let mut success = 0u32;
    let mut i = 0u32;

    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = tx.send(AutoCraftEvent::Status("已取消".to_string()));
            let _ = tx.send(AutoCraftEvent::Finished {
                success,
                total: if infinite { success } else { count },
            });
            return Ok(());
        }

        i += 1;
        let _ = tx.send(AutoCraftEvent::Progress(success, if infinite { 0 } else { count }));
        let start = Instant::now();

        match craft_once(&ap, &templates, macro_key, cancel, infinite) {
            Ok(true) => {
                success += 1;
                let elapsed = start.elapsed().as_secs_f32();
                let _ = tx.send(AutoCraftEvent::CraftDone {
                    index: i,
                    elapsed_secs: elapsed,
                });
                let _ = tx.send(AutoCraftEvent::Progress(success, if infinite { 0 } else { count }));
                // 短暂等待再开始下一次
                std::thread::sleep(Duration::from_millis(500));
            }
            Ok(false) => {
                let _ = tx.send(AutoCraftEvent::CraftFailed {
                    index: i,
                    reason: "未找到制作按钮或超时".to_string(),
                });
                let _ = tx.send(AutoCraftEvent::Finished {
                    success,
                    total: if infinite { success } else { count },
                });
                return Ok(());
            }
            Err(e) => {
                let _ = tx.send(AutoCraftEvent::Error(format!("第{}次出错: {}", i, e)));
                return Ok(());
            }
        }

        // 非无限模式达到次数后退出
        if !infinite && i >= count {
            break;
        }
    }

    let _ = tx.send(AutoCraftEvent::Finished {
        success,
        total: if infinite { success } else { count },
    });
    Ok(())
}

fn detect_state(ap: &AutoPlay, templates: &CraftTemplates) -> anyhow::Result<CraftState> {
    if ap.find_image(&templates.stop)?.is_some() {
        return Ok(CraftState::InProgress);
    }
    if ap.find_image(&templates.start)?.is_some() {
        return Ok(CraftState::Ready);
    }
    Ok(CraftState::Unknown)
}

fn wait_for_state(
    ap: &AutoPlay,
    templates: &CraftTemplates,
    target: CraftState,
    timeout: Duration,
    cancel: &Arc<AtomicBool>,
) -> anyhow::Result<bool> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if cancel.load(Ordering::Relaxed) {
            return Ok(false);
        }
        if detect_state(ap, templates)? == target {
            return Ok(true);
        }
        std::thread::sleep(POLL_INTERVAL);
    }
    Ok(false)
}

fn craft_once(
    ap: &AutoPlay,
    templates: &CraftTemplates,
    macro_key: char,
    cancel: &Arc<AtomicBool>,
    infinite: bool,
) -> anyhow::Result<bool> {
    // 1. 找到并点击 "开始制作作业"
    let Some(rect) = ap.find_image(&templates.start)? else {
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
        templates,
        CraftState::InProgress,
        CRAFT_START_TIMEOUT,
        cancel,
    )? {
        return Ok(false);
    }

    std::thread::sleep(Duration::from_millis(300));

    // 3. 按宏键
    win.focus_press(auto_play::controller::Key::Unicode(macro_key))?;

    // 4. 等待制作完成（无限模式下不超时）
    let timeout = if infinite {
        Duration::from_secs(u64::MAX)
    } else {
        CRAFT_FINISH_TIMEOUT
    };
    if !wait_for_state(
        ap,
        templates,
        CraftState::Ready,
        timeout,
        cancel,
    )? {
        return Ok(false);
    }

    Ok(true)
}
