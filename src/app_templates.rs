use crate::template::TemplateDef;

/// 自动制作工具使用的模板集
pub const AUTO_CRAFT_TEMPLATES: &[TemplateDef] = &[
    TemplateDef {
        name: "开始制作",
        id: "start_crafting",
        threshold: 0.1,
    },
    TemplateDef {
        name: "停止制作",
        id: "stop_crafting",
        threshold: 0.2,
    },
];
pub const AC_TPL_START: usize = 0;
pub const AC_TPL_STOP: usize = 1;

/// 通用 UI 关闭按钮模板（用于识别各种对话框）
pub const UI_CLOSE: TemplateDef = TemplateDef {
    name: "UI 关闭按钮",
    id: "ui_close",
    threshold: 0.55,
};
