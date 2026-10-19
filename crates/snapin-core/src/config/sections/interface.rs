//! 界面视觉与外观设置模型定义。

use crate::config::enums::{ColorFormat, Theme};
use crate::config::types::HexColor;
use serde::{Deserialize, Serialize};

/// 界面视觉设置板块
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InterfaceConfig {
    /// 主题模式
    #[serde(default)]
    pub theme: Theme,

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
}

impl Default for InterfaceConfig {
    fn default() -> Self {
        Self {
            theme: Theme::default(),
            accent_color: default_accent_color(),
            enable_magnifier: true,
            magnifier_scale: default_magnifier_scale(),
            color_format: ColorFormat::default(),
            show_grid_lines: true,
            toolbar_scale: default_toolbar_scale(),
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_accent_color() -> HexColor {
    HexColor::new(0, 174, 255, 255)
}

fn default_magnifier_scale() -> u32 {
    8
}

fn default_toolbar_scale() -> f32 {
    1.0
}
