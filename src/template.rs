use auto_play::MatcherOptions;
use image::DynamicImage;

use crate::config;

/// 编译时模板定义（每个工具用 const 数组定义自己的模板集）
pub struct TemplateDef {
    pub name: &'static str,
    pub filename: &'static str,
    pub default_bytes: &'static [u8],
    pub threshold: f32,
}

/// 运行时模板实例
#[derive(Clone)]
pub struct TemplateInstance {
    pub def: &'static TemplateDef,
    pub image: DynamicImage,
    pub is_custom: bool,
}

impl TemplateInstance {
    pub fn load(def: &'static TemplateDef) -> Self {
        let templates_dir = config::templates_dir();
        let user_path = templates_dir.join(def.filename);
        if let Ok(img) = image::open(&user_path) {
            return Self {
                def,
                image: img,
                is_custom: true,
            };
        }
        let img = image::load_from_memory(def.default_bytes)
            .unwrap_or_else(|_| panic!("无法加载默认模板: {}", def.filename));
        Self {
            def,
            image: img,
            is_custom: false,
        }
    }

    pub fn reset_to_default(&mut self) {
        let templates_dir = config::templates_dir();
        let user_path = templates_dir.join(self.def.filename);
        let _ = std::fs::remove_file(&user_path);
        self.image = image::load_from_memory(self.def.default_bytes)
            .unwrap_or_else(|_| panic!("无法加载默认模板: {}", self.def.filename));
        self.is_custom = false;
    }

    pub fn save_custom(&mut self, img: DynamicImage) -> anyhow::Result<()> {
        let templates_dir = config::templates_dir();
        let path = templates_dir.join(self.def.filename);
        img.save(&path)?;
        self.image = img;
        self.is_custom = true;
        Ok(())
    }

    pub fn matcher_options(&self) -> MatcherOptions {
        MatcherOptions::default().with_threshold(self.def.threshold)
    }
}

/// 模板集（一个工具的所有模板）
#[derive(Clone)]
pub struct TemplateSet {
    pub templates: Vec<TemplateInstance>,
}

impl TemplateSet {
    pub fn load(defs: &'static [TemplateDef]) -> Self {
        let templates = defs.iter().map(TemplateInstance::load).collect();
        Self { templates }
    }
}
