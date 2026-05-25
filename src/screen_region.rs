use std::sync::Arc;
use std::time::Duration;

use auto_play::cv::matcher::SingleMatcher;
use auto_play::{ControllerTrait, MatchDefinition, WindowsController};
use image::DynamicImage;

use crate::app_templates::UI_CLOSE;
use crate::template_images::TemplateImages;

const WINDOW_TITLE: &str = "最终幻想XIV";

/// 屏幕坐标与尺寸
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RegionRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl RegionRect {
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// 确保矩形不超过图像边界
    pub fn clamp_to_image(&self, img_w: u32, img_h: u32) -> Self {
        let x = self.x.min(img_w.saturating_sub(1));
        let y = self.y.min(img_h.saturating_sub(1));
        let width = self.width.min(img_w - x);
        let height = self.height.min(img_h - y);
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

/// 区域捕获结果
#[derive(Debug)]
pub struct CaptureResult {
    pub region_name: String,
    /// 原始全屏截图
    pub full_screenshot: DynamicImage,
    /// 裁切后的图像
    pub cropped: DynamicImage,
    /// 实际裁切使用的矩形（如果通过查找逻辑得到，可能与预期不同）
    pub detected_rect: Option<RegionRect>,
    /// 执行日志
    pub logs: Vec<String>,
}

/// 屏幕区域定义
///
/// 两种变体：
/// - 预定义区域：内置了查找/裁切逻辑，用于识别游戏中特定的 UI 元素
/// - 自定义区域：用户通过坐标直接指定的固定区域
#[derive(Debug, Clone)]
pub enum ScreenRegion {
    // === 钓鱼相关预定义区域 ===
    /// 钓鱼：拉力指示器（咬钩时的 ! / !! / !!! 提示区域）
    FishingTugIndicator,
    /// 钓鱼：抛竿/收竿进度条区域
    FishingCastBar,
    /// 钓鱼：鱼眼技能触发的鱼群提示图标区域
    FishingFishEyeHint,
    /// 钓鱼：钓竿状态 UI 区域（是否处于可抛竿状态）
    FishingRodStatus,
    /// 收藏品确认对话框（钓到收藏品鱼时弹出，如佐戈秃鹰）
    CollectableConfirm,

    // === 可配置区域 ===
    /// 用户自定义坐标区域
    Custom {
        name: String,
        rect: RegionRect,
    },
}

impl ScreenRegion {
    /// 显示名称
    pub fn name(&self) -> String {
        match self {
            Self::FishingTugIndicator => "钓鱼: 拉力指示器".to_string(),
            Self::FishingCastBar => "钓鱼: 抛竿进度条".to_string(),
            Self::FishingFishEyeHint => "钓鱼: 鱼眼提示".to_string(),
            Self::FishingRodStatus => "钓鱼: 钓竿状态".to_string(),
            Self::CollectableConfirm => "钓鱼: 收藏品确认对话框".to_string(),
            Self::Custom { name, .. } => name.clone(),
        }
    }

    /// 简短描述
    pub fn description(&self) -> String {
        match self {
            Self::FishingTugIndicator => {
                "检测咬钩时的 ! / !! / !!! 拉力提示".to_string()
            }
            Self::FishingCastBar => "检测抛竿或收竿时的进度条".to_string(),
            Self::FishingFishEyeHint => "检测鱼眼技能触发的特殊鱼群提示".to_string(),
            Self::FishingRodStatus => "检测当前是否处于可抛竿状态".to_string(),
            Self::CollectableConfirm => "检测收藏品确认对话框（关闭按钮模板匹配）".to_string(),
            Self::Custom { rect, .. } => format!(
                "固定坐标: ({}, {}) 大小: {}x{}",
                rect.x, rect.y, rect.width, rect.height
            ),
        }
    }

    /// 判断是否为预定义区域
    pub fn is_predefined(&self) -> bool {
        !matches!(self, Self::Custom { .. })
    }

    /// 获取所有预定义变体（用于调试页面遍历）
    pub fn all_predefined() -> Vec<Self> {
        vec![
            Self::FishingTugIndicator,
            Self::FishingCastBar,
            Self::FishingFishEyeHint,
            Self::FishingRodStatus,
            Self::CollectableConfirm,
        ]
    }

    /// 执行截图并裁切指定区域
    ///
    /// 流程：
    /// 1. 连接游戏窗口
    /// 2. 全屏截图
    /// 3. 根据区域类型执行查找/裁切逻辑
    /// 4. 返回裁切结果和执行日志
    pub fn capture(&self) -> anyhow::Result<CaptureResult> {
        let mut logs = Vec::new();
        logs.push(format!("开始捕获区域: {}", self.name()));

        // 连接窗口并截图
        let controller = WindowsController::from_window_title(WINDOW_TITLE)?;
        let (win_w, win_h) = controller.screen_size();
        logs.push(format!("游戏窗口尺寸: {}x{}", win_w, win_h));

        std::thread::sleep(Duration::from_millis(200));
        let screenshot = controller.screencap()?;
        logs.push(format!(
            "截图成功: {}x{}",
            screenshot.width(),
            screenshot.height()
        ));

        // 根据区域类型执行裁切
        let (cropped, detected_rect) = self.crop_from_screenshot(&screenshot, &mut logs)?;

        logs.push(format!(
            "裁切完成: {}x{}",
            cropped.width(),
            cropped.height()
        ));

        Ok(CaptureResult {
            region_name: self.name(),
            full_screenshot: screenshot,
            cropped,
            detected_rect,
            logs,
        })
    }

    /// 从截图中执行具体的查找/裁切逻辑
    fn crop_from_screenshot(
        &self,
        screenshot: &DynamicImage,
        logs: &mut Vec<String>,
    ) -> anyhow::Result<(DynamicImage, Option<RegionRect>)> {
        let img_w = screenshot.width();
        let img_h = screenshot.height();

        match self {
            Self::Custom { rect, .. } => {
                let clamped = rect.clamp_to_image(img_w, img_h);
                logs.push(format!(
                    "使用固定坐标裁切: ({}, {}) {}x{}",
                    clamped.x, clamped.y, clamped.width, clamped.height
                ));
                let cropped = screenshot.crop_imm(clamped.x, clamped.y, clamped.width, clamped.height);
                Ok((cropped, Some(clamped)))
            }

            // ── 钓鱼: 拉力指示器 ──
            // 位于屏幕中央略偏上，是显示 ! / !! / !!! 的区域
            Self::FishingTugIndicator => {
                logs.push("执行占位逻辑: 屏幕中央区域".to_string());
                // TODO: 实现基于模板匹配或颜色检测的精确查找
                // 目前使用屏幕中央固定区域作为占位
                let w = 300u32.min(img_w);
                let h = 200u32.min(img_h);
                let x = (img_w.saturating_sub(w)) / 2;
                let y = img_h.saturating_sub(h) / 3; // 偏上
                let rect = RegionRect::new(x, y, w, h).clamp_to_image(img_w, img_h);
                let cropped = screenshot.crop_imm(rect.x, rect.y, rect.width, rect.height);
                Ok((cropped, Some(rect)))
            }

            // ── 钓鱼: 抛竿进度条 ──
            // 位于屏幕底部中央
            Self::FishingCastBar => {
                logs.push("执行占位逻辑: 屏幕底部中央".to_string());
                // TODO: 实现基于进度条颜色/形状的检测
                let w = 400u32.min(img_w);
                let h = 80u32.min(img_h);
                let x = (img_w.saturating_sub(w)) / 2;
                let y = img_h.saturating_sub(h + 100);
                let rect = RegionRect::new(x, y, w, h).clamp_to_image(img_w, img_h);
                let cropped = screenshot.crop_imm(rect.x, rect.y, rect.width, rect.height);
                Ok((cropped, Some(rect)))
            }

            // ── 钓鱼: 鱼眼提示 ──
            // 位于屏幕右上角附近，显示特殊鱼群图标
            Self::FishingFishEyeHint => {
                logs.push("执行占位逻辑: 屏幕右上角".to_string());
                // TODO: 实现基于图标模板匹配
                let w = 200u32.min(img_w);
                let h = 200u32.min(img_h);
                let x = img_w.saturating_sub(w + 20);
                let y = 50;
                let rect = RegionRect::new(x, y, w, h).clamp_to_image(img_w, img_h);
                let cropped = screenshot.crop_imm(rect.x, rect.y, rect.width, rect.height);
                Ok((cropped, Some(rect)))
            }

            // ── 钓鱼: 钓竿状态 ──
            // 位于屏幕底部技能栏附近，显示当前是否装备了钓竿
            Self::FishingRodStatus => {
                logs.push("执行占位逻辑: 屏幕底部技能栏区域".to_string());
                // TODO: 实现基于钓竿图标检测
                let w = 600u32.min(img_w);
                let h = 120u32.min(img_h);
                let x = (img_w.saturating_sub(w)) / 2;
                let y = img_h.saturating_sub(h);
                let rect = RegionRect::new(x, y, w, h).clamp_to_image(img_w, img_h);
                let cropped = screenshot.crop_imm(rect.x, rect.y, rect.width, rect.height);
                Ok((cropped, Some(rect)))
            }

            // ── 收藏品确认对话框 ──
            // 使用右上角关闭按钮的模板匹配来定位整个对话框
            Self::CollectableConfirm => {
                logs.push("尝试模板匹配关闭按钮...".to_string());
                match find_collectable_dialog(screenshot, logs) {
                    Some((cropped, rect)) => {
                        logs.push(format!(
                            "找到对话框: ({}, {}) {}x{}",
                            rect.x, rect.y, rect.width, rect.height
                        ));
                        Ok((cropped, Some(rect)))
                    }
                    None => {
                        logs.push("未找到收藏品确认对话框".to_string());
                        // 返回全屏截图作为 fallback，并标记未找到
                        let fallback = screenshot.crop_imm(0, 0, img_w, img_h);
                        Ok((fallback, None))
                    }
                }
            }
        }
    }
}

/// 加载关闭按钮模板 (ui_close.png, 32x31)
fn close_btn_template() -> DynamicImage {
    TemplateImages::new().get_expect(UI_CLOSE.id)
}

/// 查找收藏品确认对话框
///
/// 策略：
/// 1. 使用右上角关闭按钮 (ui_close.png, 32x31) 作为模板
/// 2. 在全屏截图中进行模板匹配
/// 3. 找到后根据预设偏移计算对话框区域并裁切
///
/// 参数标定（基于 assets/fishing/collectable-confirm-A.png）：
/// - 对话框大小: 610x320（含余量）
/// - 模板左上角 → 对话框左上角偏移: (-540, +7)
fn find_collectable_dialog(
    screenshot: &DynamicImage,
    logs: &mut Vec<String>,
) -> Option<(DynamicImage, RegionRect)> {
    let tpl = close_btn_template();
    let (tpl_w, tpl_h) = (tpl.width(), tpl.height());
    logs.push(format!("模板尺寸: {}x{}", tpl_w, tpl_h));

    let screen_luma = screenshot.to_luma32f();

    // 使用相对宽松的阈值，因为不同分辨率下匹配值可能有差异
    let def = MatchDefinition::new(
        auto_play::MatcherOptions::default().with_threshold(0.55),
        Arc::new(tpl),
    );
    let result = SingleMatcher::match_definition(&screen_luma, &def);

    let matched = result.result?;
    logs.push(format!(
        "匹配成功: 位置=({}, {}), 大小={}x{}, 匹配值={:.4}",
        matched.rect.x, matched.rect.y, matched.rect.width, matched.rect.height, matched.value
    ));

    // 对话框参数（基于标定，留有一定余量）
    const DIALOG_WIDTH: u32 = 610;
    const DIALOG_HEIGHT: u32 = 320;
    const OFFSET_X: i32 = -540;
    const OFFSET_Y: i32 = 7;

    let dialog_x = (matched.rect.x as i32 + OFFSET_X).max(0) as u32;
    let dialog_y = (matched.rect.y as i32 + OFFSET_Y).max(0) as u32;

    let img_w = screenshot.width();
    let img_h = screenshot.height();

    let dialog_w = DIALOG_WIDTH.min(img_w - dialog_x);
    let dialog_h = DIALOG_HEIGHT.min(img_h - dialog_y);

    let rect = RegionRect::new(dialog_x, dialog_y, dialog_w, dialog_h);
    let cropped = screenshot.crop_imm(rect.x, rect.y, rect.width, rect.height);

    Some((cropped, rect))
}
