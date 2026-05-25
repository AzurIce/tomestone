use std::collections::HashSet;
use std::path::PathBuf;

use image::DynamicImage;

use crate::config;

/// 模板图片管理器（类似字典，用标识符获取图片）
///
/// 模板图片存储在两个位置：
/// - 内置模板: assets/templates/ 下（随软件分发）
/// - 用户自定义模板: .tomestone/templates/ 下（运行时覆盖内置）
///
/// 标识符即文件名去掉 .png 后缀，如 "start_crafting" 对应 start_crafting.png
#[derive(Clone)]
pub struct TemplateImages {
    /// 内置模板目录
    builtin_dir: PathBuf,
    /// 用户自定义模板目录
    custom_dir: PathBuf,
}

impl Default for TemplateImages {
    fn default() -> Self {
        Self::new()
    }
}

impl TemplateImages {
    pub fn new() -> Self {
        let builtin_dir = Self::builtin_dir();
        let custom_dir = config::templates_dir();
        Self {
            builtin_dir,
            custom_dir,
        }
    }

    /// 内置模板目录路径
    ///
    /// 按优先级尝试以下候选路径，返回第一个存在的：
    /// 1. 当前工作目录下的 assets/templates/
    /// 2. exe 同级目录下的 assets/templates/
    /// 3. exe 上两级目录（兼容 cargo run 在 target/debug 或 target/release 下的情况）
    /// 4. 编译时确定的 CARGO_MANIFEST_DIR/assets/templates/
    fn builtin_dir() -> PathBuf {
        let candidates = [
            // 1. 当前工作目录
            Some(PathBuf::from("assets/templates")),
            // 2. exe 同级
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|p| p.join("assets/templates"))),
            // 3. exe 上两级（target/debug → 项目根目录）
            std::env::current_exe()
                .ok()
                .and_then(|p| {
                    p.parent()
                        .and_then(|p| p.parent())
                        .map(|p| p.join("assets/templates"))
                }),
            // 4. 编译时确定的项目根目录
            Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/templates")),
        ];

        for cand in candidates.iter().flatten() {
            if cand.exists() {
                return cand.clone();
            }
        }

        // 都不存在时 fallback 到编译时路径（panic 时路径清晰）
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/templates")
    }

    /// 仅获取内置模板图片（忽略自定义覆盖）
    pub fn get_builtin(&self, id: &str) -> Option<DynamicImage> {
        let builtin_path = self.builtin_dir.join(format!("{}.png", id));
        image::open(&builtin_path).ok()
    }

    /// 获取模板图片，优先返回用户自定义版本
    pub fn get(&self, id: &str) -> Option<DynamicImage> {
        // 1. 优先尝试用户自定义
        let custom_path = self.custom_dir.join(format!("{}.png", id));
        if let Ok(img) = image::open(&custom_path) {
            return Some(img);
        }
        // 2. 回退到内置模板
        let builtin_path = self.builtin_dir.join(format!("{}.png", id));
        image::open(&builtin_path).ok()
    }

    /// 获取模板图片，如果找不到则 panic（用于编译时已知的模板）
    pub fn get_expect(&self, id: &str) -> DynamicImage {
        self.get(id).unwrap_or_else(|| {
            panic!(
                "无法加载模板 '{}': 在 {:?} 和 {:?} 中均未找到",
                id, self.custom_dir, self.builtin_dir
            )
        })
    }

    /// 获取所有可用的模板标识符（内置 + 自定义合并去重）
    pub fn list_ids(&self) -> Vec<String> {
        let mut ids = HashSet::new();

        // 扫描内置目录
        if let Ok(entries) = std::fs::read_dir(&self.builtin_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let name = entry.file_name().to_string_lossy().to_string();
                if let Some(stem) = name.strip_suffix(".png") {
                    ids.insert(stem.to_string());
                }
            }
        }

        // 扫描自定义目录
        if let Ok(entries) = std::fs::read_dir(&self.custom_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let name = entry.file_name().to_string_lossy().to_string();
                if let Some(stem) = name.strip_suffix(".png") {
                    ids.insert(stem.to_string());
                }
            }
        }

        let mut result: Vec<String> = ids.into_iter().collect();
        result.sort();
        result
    }

    /// 保存用户自定义模板（覆盖内置）
    pub fn save_custom(&self, id: &str, img: DynamicImage) -> anyhow::Result<()> {
        let path = self.custom_dir.join(format!("{}.png", id));
        std::fs::create_dir_all(&self.custom_dir)?;
        img.save(&path)?;
        Ok(())
    }

    /// 删除用户自定义模板（恢复为内置）
    pub fn remove_custom(&self, id: &str) {
        let path = self.custom_dir.join(format!("{}.png", id));
        let _ = std::fs::remove_file(&path);
    }

    /// 检查是否存在用户自定义版本
    pub fn is_custom(&self, id: &str) -> bool {
        self.custom_dir.join(format!("{}.png", id)).exists()
    }

    /// 检查是否存在内置版本
    pub fn has_builtin(&self, id: &str) -> bool {
        self.builtin_dir.join(format!("{}.png", id)).exists()
    }

    /// 检查模板是否存在（内置或自定义）
    pub fn exists(&self, id: &str) -> bool {
        self.custom_dir.join(format!("{}.png", id)).exists()
            || self.builtin_dir.join(format!("{}.png", id)).exists()
    }

    /// 仅删除用户自定义版本（内置模板永不删除）
    pub fn remove_custom_only(&self, id: &str) {
        let path = self.custom_dir.join(format!("{}.png", id));
        let _ = std::fs::remove_file(&path);
    }

    /// 重命名自定义模板（内置模板不可重命名）
    pub fn rename_custom_only(&self, from_id: &str, to_id: &str) -> anyhow::Result<()> {
        if self.has_builtin(from_id) {
            anyhow::bail!("内置模板 '{}' 的标识符不可修改", from_id);
        }
        let custom_from = self.custom_dir.join(format!("{}.png", from_id));
        let custom_to = self.custom_dir.join(format!("{}.png", to_id));
        if !custom_from.exists() {
            anyhow::bail!("自定义模板 '{}' 不存在", from_id);
        }
        std::fs::create_dir_all(&self.custom_dir)?;
        std::fs::rename(&custom_from, &custom_to)?;
        Ok(())
    }

    /// 获取模板图片尺寸
    pub fn image_size(&self, id: &str) -> Option<(u32, u32)> {
        self.get(id).map(|img| (img.width(), img.height()))
    }

    /// 获取内置模板文件路径
    pub fn builtin_path(&self, id: &str) -> PathBuf {
        self.builtin_dir.join(format!("{}.png", id))
    }

    /// 获取自定义模板文件路径
    pub fn custom_path(&self, id: &str) -> PathBuf {
        self.custom_dir.join(format!("{}.png", id))
    }
}
