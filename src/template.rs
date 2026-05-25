use auto_play::MatcherOptions;
use image::DynamicImage;

use crate::template_images::TemplateImages;

/// 编译时模板定义（每个工具用 const 数组定义自己的模板集）
pub struct TemplateDef {
    /// 显示名称
    pub name: &'static str,
    /// 模板标识符（对应 assets/templates/ 下的文件名，不含后缀）
    pub id: &'static str,
    /// 匹配阈值
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
    pub fn load(def: &'static TemplateDef, images: &TemplateImages) -> Self {
        let img = images.get_expect(def.id);
        let is_custom = images.is_custom(def.id);
        Self {
            def,
            image: img,
            is_custom,
        }
    }

    pub fn reset_to_default(&mut self, images: &TemplateImages) {
        images.remove_custom(self.def.id);
        self.image = images.get_expect(self.def.id);
        self.is_custom = false;
    }

    pub fn save_custom(
        &mut self,
        images: &TemplateImages,
        img: DynamicImage,
    ) -> anyhow::Result<()> {
        images.save_custom(self.def.id, img.clone())?;
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
    pub images: TemplateImages,
    pub templates: Vec<TemplateInstance>,
}

impl TemplateSet {
    pub fn load(images: TemplateImages, defs: &'static [TemplateDef]) -> Self {
        let templates = defs.iter().map(|def| TemplateInstance::load(def, &images)).collect();
        Self { images, templates }
    }
}
