use anyhow::{Context, Result};
use auto_play::cv::matcher::SingleMatcher;
use auto_play::MatchDefinition;
use image::DynamicImage;
use std::path::Path;
use std::sync::Arc;

// ═══════════════════════════════════════════════════════════
// 公共 API
// ═══════════════════════════════════════════════════════════

/// 矩形区域定义（支持负坐标，用于相对偏移）
///
/// x, y 可以为负数，表示相对于参考点（如按钮左上角）的偏移。
/// width, height 为区域尺寸，必须为正。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RegionRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl RegionRect {
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// 根据基准点计算绝对坐标区域
    ///
    /// 基准点通常是"作业中止"按钮在截图中的匹配位置（左上角）。
    pub fn absolute_from(&self, base_x: u32, base_y: u32) -> Self {
        Self {
            x: base_x as i32 + self.x,
            y: base_y as i32 + self.y,
            width: self.width,
            height: self.height,
        }
    }

    /// 确保矩形不超出图像边界，返回可用于 crop_imm 的坐标
    fn clamp_to_image(&self, img_w: u32, img_h: u32) -> (u32, u32, u32, u32) {
        let x = self.x.max(0) as u32;
        let y = self.y.max(0) as u32;
        let width = self.width.min(img_w.saturating_sub(x));
        let height = self.height.min(img_h.saturating_sub(y));
        (x, y, width, height)
    }
}

/// 从制造窗口提取的关键信息
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CraftInfo {
    /// 耐久 (当前值, 最大值)，如 (50, 80)
    pub durability: (u32, u32),
    /// 进展 (当前值, 最大值)，如 (0, 28)
    pub progress: (u32, u32),
    /// 品质 (当前值, 最大值)，如 (166, 171)
    pub quality: (u32, u32),
    /// 优质率 (%), 如 94。如果未配置区域则为 None
    pub hq_rate: Option<u32>,
    /// 匹配到的中止按钮位置（调试用）
    pub button_rect: Option<RegionRect>,
}

/// 制造信息提取配置
///
/// 基于"作业中止"按钮模板匹配来计算各信息区域的相对位置。
/// 所有 offset 坐标均相对于中止按钮左上角。
///
/// # 标定方法
/// 1. 先截取一张制造窗口截图，用图像编辑工具找到"作业中止"按钮左上角坐标 (btn_x, btn_y)
/// 2. 找到耐久/进展/品质数值区域的左上角坐标，计算偏移：offset = 区域坐标 - 按钮坐标
/// 3. 测量各区域的宽度和高度
/// 4. 将按钮区域裁切为模板图像：`config.set_abort_button_template(img)`
/// 5. 使用 `debug_extract` 验证各区域裁切是否正确
///
/// # 示例偏移参考（基于 1920x1080，需根据实际环境调整）
/// - 耐久：约 (-900, -280) 大小约 (150, 30)
/// - 进展：约 (200, -280) 大小约 (180, 30)
/// - 品质：约 (200, -220) 大小约 (180, 30)
/// - 优质率：约 (150, -160) 大小约 (120, 30)
#[derive(Clone)]
pub struct CraftInfoConfig {
    /// "作业中止"按钮模板图像（用于定位制造窗口）
    ///
    /// 与 `abort_button_match_def` 二选一，优先使用 `abort_button_match_def`。
    abort_button_template: Option<Arc<DynamicImage>>,
    /// "作业中止"按钮模板匹配定义（复用 auto_craft::CraftTemplates.stop）
    ///
    /// 如果设置了此项，将优先使用它而不是 `abort_button_template`。
    /// 这是推荐用法，可直接复用已有的 `MatchDefinition`。
    abort_button_match_def: Option<MatchDefinition>,
    /// 模板匹配阈值（0.0-1.0，建议 0.5-0.7）
    ///
    /// 仅在通过 `abort_button_template` 创建 `MatchDefinition` 时使用。
    /// 如果传入了 `abort_button_match_def`，则使用其中已配置的阈值。
    pub match_threshold: f32,
    /// 耐久数值区域：相对于中止按钮的偏移
    pub durability_offset: RegionRect,
    /// 进展数值区域：相对于中止按钮的偏移
    pub progress_offset: RegionRect,
    /// 品质数值区域：相对于中止按钮的偏移
    pub quality_offset: RegionRect,
    /// 优质率区域：相对于中止按钮的偏移（可选）
    pub hq_rate_offset: Option<RegionRect>,
    /// OCR 检测模型路径
    pub detection_model_path: String,
    /// OCR 识别模型路径
    pub recognition_model_path: String,
}

impl std::fmt::Debug for CraftInfoConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CraftInfoConfig")
            .field("has_template", &self.abort_button_template.is_some())
            .field("has_match_def", &self.abort_button_match_def.is_some())
            .field("match_threshold", &self.match_threshold)
            .field("durability_offset", &self.durability_offset)
            .field("progress_offset", &self.progress_offset)
            .field("quality_offset", &self.quality_offset)
            .field("hq_rate_offset", &self.hq_rate_offset)
            .field("detection_model", &self.detection_model_path)
            .field("recognition_model", &self.recognition_model_path)
            .finish()
    }
}

impl CraftInfoConfig {
    /// 创建默认配置，需要设置模板和偏移
    pub fn new(match_threshold: f32) -> Self {
        Self {
            abort_button_template: None,
            abort_button_match_def: None,
            match_threshold,
            // 默认偏移量（基于实际截图标定，相对于"作业中止"按钮左上角）
            durability_offset: RegionRect::new(-498, -377, 160, 35),
            progress_offset: RegionRect::new(50, -380, 100, 40),
            quality_offset: RegionRect::new(50, -320, 100, 40),
            hq_rate_offset: Some(RegionRect::new(-250, -280, 80, 40)),
            detection_model_path: r"assets\models\text-detection.rten".to_string(),
            recognition_model_path: r"assets\models\text-recognition.rten".to_string(),
        }
    }

    /// 设置"作业中止"按钮模板图像
    pub fn set_abort_button_template(&mut self, template: DynamicImage) {
        self.abort_button_template = Some(Arc::new(template));
    }

    /// 设置"作业中止"按钮模板匹配定义（复用 auto_craft 模板）
    ///
    /// 如果你已经有 `auto_craft::CraftTemplates`，可以直接传入 `templates.stop`：
    /// ```ignore
    /// config.set_abort_button_match_def(templates.stop);
    /// ```
    pub fn set_abort_button_match_def(&mut self, match_def: MatchDefinition) {
        self.abort_button_match_def = Some(match_def);
    }

    /// 从全屏截图中裁切指定区域作为按钮模板
    pub fn crop_template_from_screenshot(
        &mut self,
        screenshot: &DynamicImage,
        region: &RegionRect,
    ) {
        let (x, y, w, h) = region.clamp_to_image(screenshot.width(), screenshot.height());
        let cropped = screenshot.crop_imm(x, y, w, h);
        self.set_abort_button_template(cropped);
    }

    /// 从文件加载按钮模板图像
    pub fn load_template_from_file(&mut self, path: &Path) -> Result<()> {
        let img = image::open(path).with_context(|| format!("加载模板图片失败: {:?}", path))?;
        self.set_abort_button_template(img);
        Ok(())
    }
}

/// 制造信息提取器
///
/// 通过模板匹配"作业中止"按钮来定位制造窗口，然后基于相对偏移提取耐久、进展、品质等信息。
/// 使用 ocrs 神经网络 OCR 引擎识别数值，通过二值化+反色预处理适配深色 UI 背景。
pub struct CraftInfoExtractor {
    config: CraftInfoConfig,
    engine: Option<ocrs::OcrEngine>,
}

impl CraftInfoExtractor {
    pub fn new(config: CraftInfoConfig) -> Self {
        Self {
            config,
            engine: None,
        }
    }

    /// 初始化 OCR 引擎（惰性加载，首次识别时自动调用）
    fn ensure_engine(&mut self) -> Result<&ocrs::OcrEngine> {
        if self.engine.is_none() {
            use rten::Model;

            let detection_model = Model::load_file(&self.config.detection_model_path)
                .with_context(|| {
                    format!(
                        "加载检测模型失败: {}\n请确认模型文件存在，可从以下地址下载:\n\
                         https://ocrs-models.s3-accelerate.amazonaws.com/text-detection.rten",
                        self.config.detection_model_path
                    )
                })?;
            let recognition_model = Model::load_file(&self.config.recognition_model_path)
                .with_context(|| {
                    format!(
                        "加载识别模型失败: {}\n请确认模型文件存在，可从以下地址下载:\n\
                         https://ocrs-models.s3-accelerate.amazonaws.com/text-recognition.rten",
                        self.config.recognition_model_path
                    )
                })?;

            let engine = ocrs::OcrEngine::new(ocrs::OcrEngineParams {
                detection_model: Some(detection_model),
                recognition_model: Some(recognition_model),
                ..Default::default()
            })
            .context("初始化 OCR 引擎失败")?;

            self.engine = Some(engine);
        }
        Ok(self.engine.as_ref().unwrap())
    }

    /// 从游戏窗口截图中提取制造信息
    ///
    /// 流程：
    /// 1. 在截图中模板匹配"作业中止"按钮
    /// 2. 根据匹配位置 + 配置的偏移量计算各信息区域
    /// 3. 裁切、预处理（二值化+反色）、OCR 识别各区域数值
    pub fn extract(&mut self, screenshot: &DynamicImage) -> Result<CraftInfo> {
        let config = self.config.clone();
        extract_craft_info(screenshot, &config, self.ensure_engine()?)
    }

    /// 调试模式：提取信息并保存调试图像到指定目录
    ///
    /// 输出文件：
    /// - `debug_button.png`：匹配到的中止按钮区域
    /// - `debug_{durability,progress,quality}_raw.png`：各区域原始裁切
    /// - `debug_{durability,progress,quality}_ocr.png`：二值化反色预处理后的图像
    /// - `debug_hq_rate_*`：优质率区域（如果配置了）
    pub fn debug_extract(
        &mut self,
        screenshot: &DynamicImage,
        output_dir: &Path,
    ) -> Result<CraftInfo> {
        std::fs::create_dir_all(output_dir)
            .with_context(|| format!("创建调试目录失败: {:?}", output_dir))?;

        // 先 clone config，避免与 ensure_engine 的 &mut self 冲突
        let config = self.config.clone();
        let engine = self.ensure_engine()?;

        // 1. 匹配按钮
        let (btn_x, btn_y, btn_rect) = match_abort_button(screenshot, &config)?;
        println!(
            "[DEBUG] 匹配到中止按钮: ({}, {}) {}x{}",
            btn_x, btn_y, btn_rect.width, btn_rect.height
        );

        // 保存匹配到的按钮区域
        let (bx, by, bw, bh) = btn_rect.clamp_to_image(screenshot.width(), screenshot.height());
        let button_img = screenshot.crop_imm(bx, by, bw, bh);
        let button_path = output_dir.join("debug_button.png");
        button_img
            .save(&button_path)
            .with_context(|| format!("保存调试图片失败: {:?}", button_path))?;

        // 2. 调试各区域
        let regions = [
            ("durability", &config.durability_offset),
            ("progress", &config.progress_offset),
            ("quality", &config.quality_offset),
        ];

        for (name, offset) in &regions {
            let region = offset.absolute_from(btn_x, btn_y);
            let (x, y, w, h) = region.clamp_to_image(screenshot.width(), screenshot.height());
            if w > 0 && h > 0 {
                let cropped = screenshot.crop_imm(x, y, w, h);

                let raw_path = output_dir.join(format!("debug_{}_raw.png", name));
                cropped
                    .save(&raw_path)
                    .with_context(|| format!("保存调试图片失败: {:?}", raw_path))?;

                // 预处理并保存
                let preprocessed = preprocess_for_ocr(&cropped);
                let ocr_path = output_dir.join(format!("debug_{}_ocr.png", name));
                preprocessed
                    .save(&ocr_path)
                    .with_context(|| format!("保存调试图片失败: {:?}", ocr_path))?;

                // OCR 识别
                match recognize_with_engine(&preprocessed, engine) {
                    Some(text) => println!("[DEBUG] {} 识别结果: '{}'", name, text),
                    None => println!("[DEBUG] {} 未能识别出文本", name),
                }
            }
        }

        if let Some(offset) = &config.hq_rate_offset {
            let region = offset.absolute_from(btn_x, btn_y);
            let (x, y, w, h) = region.clamp_to_image(screenshot.width(), screenshot.height());
            if w > 0 && h > 0 {
                let cropped = screenshot.crop_imm(x, y, w, h);
                let raw_path = output_dir.join("debug_hq_rate_raw.png");
                cropped
                    .save(&raw_path)
                    .with_context(|| format!("保存调试图片失败: {:?}", raw_path))?;

                let preprocessed = preprocess_for_ocr(&cropped);
                let ocr_path = output_dir.join("debug_hq_rate_ocr.png");
                preprocessed
                    .save(&ocr_path)
                    .with_context(|| format!("保存调试图片失败: {:?}", ocr_path))?;

                match recognize_with_engine(&preprocessed, engine) {
                    Some(text) => println!("[DEBUG] hq_rate 识别结果: '{}'", text),
                    None => println!("[DEBUG] hq_rate 未能识别出文本"),
                }
            }
        }

        extract_craft_info(screenshot, &config, engine)
    }
}

// ═══════════════════════════════════════════════════════════
// 核心提取逻辑
// ═══════════════════════════════════════════════════════════

/// 从截图中提取制造信息
fn extract_craft_info(
    screenshot: &DynamicImage,
    config: &CraftInfoConfig,
    engine: &ocrs::OcrEngine,
) -> Result<CraftInfo> {
    // 1. 匹配"作业中止"按钮
    let (btn_x, btn_y, btn_rect) = match_abort_button(screenshot, config)?;

    // 2. 计算各区域绝对位置并提取信息
    let mut info = CraftInfo {
        button_rect: Some(btn_rect),
        ..Default::default()
    };

    let dur_region = config.durability_offset.absolute_from(btn_x, btn_y);
    info.durability =
        extract_fraction(screenshot, &dur_region, "耐久", engine).context("提取耐久信息失败")?;

    let prog_region = config.progress_offset.absolute_from(btn_x, btn_y);
    info.progress =
        extract_fraction(screenshot, &prog_region, "进展", engine).context("提取进展信息失败")?;

    let qual_region = config.quality_offset.absolute_from(btn_x, btn_y);
    info.quality =
        extract_fraction(screenshot, &qual_region, "品质", engine).context("提取品质信息失败")?;

    if let Some(offset) = &config.hq_rate_offset {
        let region = offset.absolute_from(btn_x, btn_y);
        let text = recognize_region(screenshot, &region, engine).context("识别优质率区域失败")?;
        info.hq_rate = parse_percentage(&text);
    }

    Ok(info)
}

/// 在截图中匹配"作业中止"按钮
///
/// 返回：按钮左上角坐标 (x, y) 和匹配到的区域矩形
fn match_abort_button(
    screenshot: &DynamicImage,
    config: &CraftInfoConfig,
) -> Result<(u32, u32, RegionRect)> {
    let screen_luma = screenshot.to_luma32f();

    // 优先使用已传入的 MatchDefinition（如 auto_craft::CraftTemplates.stop）
    let result = if let Some(ref match_def) = config.abort_button_match_def {
        SingleMatcher::match_definition(&screen_luma, match_def)
    } else {
        let template = config.abort_button_template.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "未设置'作业中止'按钮模板。\n\
                     请使用以下任一方式设置：\n\
                     1. config.set_abort_button_match_def(templates.stop) — 复用 auto_craft 模板\n\
                     2. config.set_abort_button_template(img) — 传入模板图像\n\
                     3. config.crop_template_from_screenshot(&screenshot, &region) — 从截图裁切"
            )
        })?;
        let def = MatchDefinition::new(
            auto_play::MatcherOptions::default().with_threshold(config.match_threshold),
            template.clone(),
        );
        SingleMatcher::match_definition(&screen_luma, &def)
    };

    let matched = result.result.ok_or_else(|| {
        anyhow::anyhow!(
            "未找到'作业中止'按钮，请确认：\n\
             1. 制造窗口已打开\n\
             2. 按钮模板图像正确\n\
             3. 匹配阈值适当（MatchDefinition 内已配置）"
        )
    })?;

    let btn_rect = RegionRect::new(
        matched.rect.x as i32,
        matched.rect.y as i32,
        matched.rect.width,
        matched.rect.height,
    );

    Ok((matched.rect.x, matched.rect.y, btn_rect))
}

// ═══════════════════════════════════════════════════════════
// 图像预处理与 OCR 识别
// ═══════════════════════════════════════════════════════════

/// 预处理图像以适配 ocrs OCR
///
/// 针对 FF14 深色 UI 背景优化：
/// 1. 灰度化
/// 2. 二值化：亮色（文字）→ 黑(0)，暗色（背景）→ 白(255)
/// 3. 转为 RGB 格式（ocrs 要求）
fn preprocess_for_ocr(img: &DynamicImage) -> DynamicImage {
    let gray = img.to_luma8();
    let mut binary = gray.clone();
    for pixel in binary.pixels_mut() {
        // 亮色（文字）→ 黑(0)，暗色（背景）→ 白(255)
        pixel.0[0] = if pixel.0[0] >= 180 { 0 } else { 255 };
    }
    DynamicImage::ImageLuma8(binary)
}

/// 使用 ocrs 引擎识别图像中的文本
fn recognize_with_engine(img: &DynamicImage, engine: &ocrs::OcrEngine) -> Option<String> {
    let rgb = img.to_rgb8();
    let img_source = ocrs::ImageSource::from_bytes(rgb.as_raw(), rgb.dimensions()).ok()?;
    let ocr_input = engine.prepare_input(img_source).ok()?;
    engine
        .get_text(&ocr_input)
        .ok()
        .map(|t| t.trim().to_string())
}

/// 对指定区域裁切、预处理并使用 ocrs 识别文本
fn recognize_region(
    screenshot: &DynamicImage,
    region: &RegionRect,
    engine: &ocrs::OcrEngine,
) -> Result<String> {
    if region.width == 0 || region.height == 0 {
        return Err(anyhow::anyhow!("区域大小不能为零"));
    }

    let (x, y, w, h) = region.clamp_to_image(screenshot.width(), screenshot.height());
    if w == 0 || h == 0 {
        return Err(anyhow::anyhow!(
            "区域超出图像边界: region=({}, {}) {}x{}, image={}x{}",
            region.x,
            region.y,
            region.width,
            region.height,
            screenshot.width(),
            screenshot.height()
        ));
    }

    let cropped = screenshot.crop_imm(x, y, w, h);
    let preprocessed = preprocess_for_ocr(&cropped);

    match recognize_with_engine(&preprocessed, engine) {
        Some(text) if !text.is_empty() => Ok(text),
        _ => Err(anyhow::anyhow!("未能识别出文本")),
    }
}

/// 从指定区域提取 "当前/最大" 格式的数值
fn extract_fraction(
    screenshot: &DynamicImage,
    region: &RegionRect,
    label: &str,
    engine: &ocrs::OcrEngine,
) -> Result<(u32, u32)> {
    let text = recognize_region(screenshot, region, engine)
        .with_context(|| format!("识别 '{}' 区域失败", label))?;
    parse_fraction(&text).with_context(|| format!("解析 '{}' 数值失败: '{}'", label, text))
}

/// 解析 "50 / 80", "0/28", "166/171" 等分数格式
///
/// 提取所有数字，假设前两组分别是当前值和最大值。
fn parse_fraction(text: &str) -> Result<(u32, u32)> {
    let numbers: Vec<u32> = text
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect();

    if numbers.len() >= 2 {
        Ok((numbers[0], numbers[1]))
    } else {
        Err(anyhow::anyhow!(
            "期望至少两组数字，得到: '{}' (提取到 {} 组)",
            text,
            numbers.len()
        ))
    }
}

/// 解析百分比文本，如 "94 %" -> 94
fn parse_percentage(text: &str) -> Option<u32> {
    let digits: String = text.chars().filter(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

// ═══════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_fraction() {
        assert_eq!(parse_fraction("50 / 80").unwrap(), (50, 80));
        assert_eq!(parse_fraction("0/28").unwrap(), (0, 28));
        assert_eq!(parse_fraction("166/171").unwrap(), (166, 171));
        assert_eq!(parse_fraction("  123  /  456  ").unwrap(), (123, 456));
    }

    #[test]
    fn test_parse_fraction_single_number() {
        assert!(parse_fraction("50").is_err());
    }

    #[test]
    fn test_parse_percentage() {
        assert_eq!(parse_percentage("94 %"), Some(94));
        assert_eq!(parse_percentage("100%"), Some(100));
        assert_eq!(parse_percentage("0"), Some(0));
        assert_eq!(parse_percentage(""), None);
    }

    #[test]
    fn test_region_absolute_from() {
        let offset = RegionRect::new(-100, -50, 80, 30);
        let abs = offset.absolute_from(500, 400);
        assert_eq!(abs.x, 400);
        assert_eq!(abs.y, 350);
        assert_eq!(abs.width, 80);
        assert_eq!(abs.height, 30);
    }

    #[test]
    fn test_region_clamp() {
        let region = RegionRect::new(100, 100, 200, 200);
        let (x, y, w, h) = region.clamp_to_image(150, 150);
        assert_eq!(x, 100);
        assert_eq!(y, 100);
        assert_eq!(w, 50);
        assert_eq!(h, 50);
    }

    #[test]
    fn test_region_clamp_negative() {
        let region = RegionRect::new(-50, -50, 200, 200);
        let (x, y, w, h) = region.clamp_to_image(150, 150);
        assert_eq!(x, 0);
        assert_eq!(y, 0);
        assert_eq!(w, 150);
        assert_eq!(h, 150);
    }

    /// 测试：用 ocrs 识别本地数字样本
    ///
    /// 需要模型文件：
    /// - `assets/models/text-detection.rten`
    /// - `assets/models/text-recognition.rten`
    #[test]
    #[ignore = "需要 ocrs 模型文件"]
    fn test_ocrs_on_samples() {
        use std::path::Path;

        const SAMPLE_DIR: &str = r"..\..\packages\ap-xiv\assets\craft\number";

        let mut config = CraftInfoConfig::new(0.55);
        let mut extractor = CraftInfoExtractor::new(config);

        let sample_dir = Path::new(SAMPLE_DIR);
        if !sample_dir.exists() {
            println!("样本目录不存在: {}", SAMPLE_DIR);
            return;
        }

        println!("\n开始识别样本...\n");
        for entry in std::fs::read_dir(sample_dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("png") {
                continue;
            }

            let filename = path.file_name().unwrap().to_str().unwrap();
            println!("文件: {}", filename);

            let img = match image::open(&path) {
                Ok(i) => i,
                Err(e) => {
                    println!("  加载失败: {}", e);
                    continue;
                }
            };

            let preprocessed = preprocess_for_ocr(&img);
            let engine = extractor.ensure_engine().unwrap();

            match recognize_with_engine(&preprocessed, engine) {
                Some(text) if !text.is_empty() => {
                    println!("  结果: '{}'", text);
                }
                _ => {
                    println!("  结果: [空]");
                }
            }
            println!();
        }
    }

    /// 集成测试：直接截图 FF14 窗口并提取制造信息
    ///
    /// 直接加载 `assets/templates/stop_crafting.png` 作为按钮模板，无需手动裁切。
    ///
    /// # 使用方法
    /// 1. 确保 FF14 已打开且处于制造窗口
    /// 2. 运行: `cargo test test_extract_from_window -- --ignored --nocapture`
    /// 3. 查看 `output/` 目录中的调试图像
    /// 4. 根据识别结果调整下方偏移量，重复步骤 2-3
    #[test]
    #[ignore = "需要 FF14 窗口"]
    fn test_extract_from_window() {
        use auto_play::ControllerTrait;
        use std::path::PathBuf;

        const WINDOW_TITLE: &str = "最终幻想XIV";
        const TEMPLATE_PATH: &str = r"..\..\assets\templates\stop_crafting.png";
        const OUTPUT_DIR: &str = "output";

        // ── 连接窗口并截图 ──
        println!("\n[1/4] 连接窗口 '{}'...", WINDOW_TITLE);
        let controller = match auto_play::WindowsController::from_window_title(WINDOW_TITLE) {
            Ok(c) => c,
            Err(e) => {
                println!("错误: 无法连接窗口: {}", e);
                return;
            }
        };
        let (win_w, win_h) = controller.screen_size();
        println!("窗口尺寸: {}x{}", win_w, win_h);

        println!("[2/4] 截图...");
        std::thread::sleep(std::time::Duration::from_millis(500));
        let screenshot = match controller.screencap() {
            Ok(img) => img,
            Err(e) => {
                println!("错误: 截图失败: {}", e);
                return;
            }
        };
        println!("截图: {}x{}", screenshot.width(), screenshot.height());

        // ── 保存调试图像 ──
        let output_dir = PathBuf::from(OUTPUT_DIR);
        std::fs::create_dir_all(&output_dir).ok();
        let _ = screenshot.save(output_dir.join("debug_full_screenshot.png"));

        // ── 配置 ──
        println!("\n[3/4] 配置提取器...");
        let mut config = CraftInfoConfig::new(0.55);

        // 直接从文件加载模板
        match config.load_template_from_file(Path::new(TEMPLATE_PATH)) {
            Ok(()) => println!("模板加载成功: {}", TEMPLATE_PATH),
            Err(e) => {
                println!("模板加载失败: {}", e);
                println!("请确认文件存在: {}", TEMPLATE_PATH);
                return;
            }
        }

        config.detection_model_path = "../../assets/models/text-detection.rten".to_string();
        config.recognition_model_path = "../../assets/models/text-recognition.rten".to_string();

        // ── 提取 ──
        println!("\n[4/4] 提取制造信息...");
        let mut extractor = CraftInfoExtractor::new(config);

        match extractor.debug_extract(&screenshot, &output_dir) {
            Ok(info) => {
                println!("\n提取结果:");
                println!("  耐久: {}/{}", info.durability.0, info.durability.1);
                println!("  进展: {}/{}", info.progress.0, info.progress.1);
                println!("  品质: {}/{}", info.quality.0, info.quality.1);
                if let Some(hq) = info.hq_rate {
                    println!("  优质率: {}%", hq);
                }
                if let Some(rect) = info.button_rect {
                    println!(
                        "\n  按钮匹配位置: ({}, {}) {}x{}",
                        rect.x, rect.y, rect.width, rect.height
                    );
                }
            }
            Err(e) => {
                println!("\n提取失败: {}", e);
            }
        }

        println!("\n调试图像已保存到: {:?}", output_dir);
    }
}
