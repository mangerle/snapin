//! 界面视觉与外观设置模型定义。

use crate::config::defaults::{
    default_true, DEFAULT_ACCENT_COLOR, DEFAULT_MAGNIFIER_SCALE, DEFAULT_TOOLBAR_SCALE,
};
use crate::config::enums::ColorFormat;
use crate::config::types::HexColor;
use serde::{Deserialize, Serialize};

/// 界面视觉设置板块
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InterfaceConfig {
    /// 选区边框与高亮强调色（强类型 HEX）
    #[serde(default = "default_accent_color")]
    pub accent_color: HexColor,

    /// 是否开启鼠标跟随取色放大镜
    #[serde(default = "default_true")]
    pub enable_magnifier: bool,

    /// 放大镜像素放大倍率 (如 4, 8, 16)
    #[serde(default = "default_magnifier_scale")]
    pub magnifier_scale: u32,

    /// 默认取色格式
    #[serde(default)]
    pub color_format: ColorFormat,

    /// 放大镜中是否显示像素网格参考线
    #[serde(default = "default_true")]
    pub show_grid_lines: bool,

    /// 浮动工具栏 UI 缩放比例
    #[serde(default = "default_toolbar_scale")]
    pub toolbar_scale: f32,

    /// 截屏时是否包含鼠标指针（Linux GNOME 扩展通道支持）
    #[serde(default)]
    pub capture_mouse_cursor: bool,
}

impl Default for InterfaceConfig {
    fn default() -> Self {
        Self {
            accent_color: default_accent_color(),
            enable_magnifier: true,
            magnifier_scale: default_magnifier_scale(),
            color_format: ColorFormat::default(),
            show_grid_lines: true,
            toolbar_scale: default_toolbar_scale(),
            capture_mouse_cursor: false,
        }
    }
}

fn default_accent_color() -> HexColor {
    DEFAULT_ACCENT_COLOR
}

fn default_magnifier_scale() -> u32 {
    DEFAULT_MAGNIFIER_SCALE
}

fn default_toolbar_scale() -> f32 {
    DEFAULT_TOOLBAR_SCALE
}
