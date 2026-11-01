//! 配置默认值的单一来源。
//!
//! `Default` 实现、serde 字段默认函数与加载边界的 `sanitized()` 此前各自硬编码
//! 同一批数值；任一处调整都会造成“默认值”和“修复值”漂移。本模块集中定义这些
//! 常量，供三个入口共用。

use crate::config::types::HexColor;

/// 守护进程启动时默认开启自动更新检查
pub(crate) fn default_true() -> bool {
    true
}

/// 默认更新检查间隔（小时）
pub(crate) const DEFAULT_UPDATE_INTERVAL_HOURS: u32 = 24;

/// 默认强调色
pub(crate) const DEFAULT_ACCENT_COLOR: HexColor = HexColor::new(0, 174, 255, 255);

/// 默认放大镜倍率
pub(crate) const DEFAULT_MAGNIFIER_SCALE: u32 = 8;

/// 默认工具栏缩放
pub(crate) const DEFAULT_TOOLBAR_SCALE: f32 = 1.0;

/// 默认标注色
pub(crate) const DEFAULT_SNIP_COLOR: HexColor = HexColor::new(255, 59, 48, 255);

/// 默认描边粗细
pub(crate) const DEFAULT_STROKE_WIDTH: f32 = 4.0;

/// 默认文字字号
pub(crate) const DEFAULT_FONT_SIZE: f32 = 20.0;

/// 默认贴图不透明度
pub(crate) const DEFAULT_PASTER_OPACITY: f32 = 1.0;

/// 默认快捷保存目录
pub(crate) const DEFAULT_QUICK_SAVE_DIR: &str = "~/Pictures/Screenshots";

/// 默认文件名模板
pub(crate) const DEFAULT_FILE_NAME_TEMPLATE: &str = "Snapin_{YYYY}{MM}{DD}_{HH}{mm}{ss}";

/// 默认 JPEG 质量
pub(crate) const DEFAULT_JPEG_QUALITY: u8 = 90;
