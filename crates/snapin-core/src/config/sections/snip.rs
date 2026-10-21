//! 截图交互流程与标注工具默认设置模型定义。

use crate::config::types::HexColor;
use serde::{Deserialize, Serialize};

/// 截图流程设置板块
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SnipConfig {
    /// 再次截图时是否默认记住并恢复上次选区
    #[serde(default = "default_true")]
    pub remember_last_selection: bool,

    /// 默认标注颜色（强类型 HEX）
    #[serde(default = "default_draw_color")]
    pub default_color: HexColor,

    /// 默认矢量描边粗细像素
    #[serde(default = "default_stroke_width")]
    pub default_stroke_width: f32,

    /// 默认文字标注字号像素
    #[serde(default = "default_font_size")]
    pub default_font_size: f32,
}

impl Default for SnipConfig {
    fn default() -> Self {
        Self {
            remember_last_selection: true,
            default_color: default_draw_color(),
            default_stroke_width: default_stroke_width(),
            default_font_size: default_font_size(),
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_draw_color() -> HexColor {
    HexColor::new(255, 59, 48, 255)
}

fn default_stroke_width() -> f32 {
    4.0
}

fn default_font_size() -> f32 {
    20.0
}
