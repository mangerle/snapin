//! 截图交互流程与标注工具默认设置模型定义。

use crate::config::enums::DefaultTool;
use serde::{Deserialize, Serialize};

/// 截图流程设置板块
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SnipConfig {
    /// 截图底图中是否包含鼠标光标指针
    #[serde(default)]
    pub capture_cursor: bool,

    /// 是否启用窗口与 UI 控件边缘智能吸附
    #[serde(default = "default_true")]
    pub smart_window_snapping: bool,

    /// 再次截图时是否默认记住并恢复上次选区
    #[serde(default = "default_true")]
    pub remember_last_selection: bool,

    /// 首次进入截图时默认激活的标注工具
    #[serde(default)]
    pub default_tool: DefaultTool,

    /// 默认标注颜色（HEX 格式）
    #[serde(default = "default_draw_color")]
    pub default_color: String,

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
            capture_cursor: false,
            smart_window_snapping: true,
            remember_last_selection: true,
            default_tool: DefaultTool::default(),
            default_color: default_draw_color(),
            default_stroke_width: default_stroke_width(),
            default_font_size: default_font_size(),
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_draw_color() -> String {
    "#FF3B30".to_string()
}

fn default_stroke_width() -> f32 {
    4.0
}

fn default_font_size() -> f32 {
    20.0
}
