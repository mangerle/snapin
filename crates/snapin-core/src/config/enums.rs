//! Snapin 强类型枚举与值转换定义。
//!
//! 包含系统日志级别、主题风格、取色格式、默认工具、缩放锚点、贴图交互与输出格式等代数数据类型定义。

use serde::{Deserialize, Serialize};

/// 系统日志记录级别强类型枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    #[default]
    Info,
    Warn,
    Error,
}

impl LogLevel {
    /// 与界面下拉框索引双向映射
    pub fn from_index(idx: i32) -> Self {
        match idx {
            0 => Self::Trace,
            1 => Self::Debug,
            3 => Self::Warn,
            4 => Self::Error,
            _ => Self::Info,
        }
    }

    /// 转换为界面下拉框索引
    pub fn to_index(self) -> i32 {
        match self {
            Self::Trace => 0,
            Self::Debug => 1,
            Self::Info => 2,
            Self::Warn => 3,
            Self::Error => 4,
        }
    }

    /// 转换为标准日志级别文本
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

/// 顶栏托盘图标色彩主题枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TrayTheme {
    #[default]
    Auto,
    Dark,
    Light,
}

impl TrayTheme {
    pub fn from_index(idx: i32) -> Self {
        match idx {
            1 => Self::Dark,
            2 => Self::Light,
            _ => Self::Auto,
        }
    }

    pub fn to_index(self) -> i32 {
        match self {
            Self::Auto => 0,
            Self::Dark => 1,
            Self::Light => 2,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }
}

/// 界面主题模式强类型枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Dark,
    Light,
}

impl Theme {
    pub fn from_index(idx: i32) -> Self {
        match idx {
            1 => Self::Dark,
            2 => Self::Light,
            _ => Self::System,
        }
    }

    pub fn to_index(self) -> i32 {
        match self {
            Self::System => 0,
            Self::Dark => 1,
            Self::Light => 2,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }
}

/// 放大镜取色文本格式强类型枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ColorFormat {
    #[default]
    Hex,
    Rgb,
    Hsl,
}

impl ColorFormat {
    pub fn from_index(idx: i32) -> Self {
        match idx {
            1 => Self::Rgb,
            2 => Self::Hsl,
            _ => Self::Hex,
        }
    }

    pub fn to_index(self) -> i32 {
        match self {
            Self::Hex => 0,
            Self::Rgb => 1,
            Self::Hsl => 2,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hex => "hex",
            Self::Rgb => "rgb",
            Self::Hsl => "hsl",
        }
    }

    /// 轮转切换至下一种取色格式 (Shift+C 触发)
    pub fn next(self) -> Self {
        match self {
            Self::Hex => Self::Rgb,
            Self::Rgb => Self::Hsl,
            Self::Hsl => Self::Hex,
        }
    }

    /// 格式化 RGBA 颜色为可读文本
    pub fn format_color(self, r: u8, g: u8, b: u8) -> String {
        match self {
            Self::Hex => format!("#{r:02X}{g:02X}{b:02X}"),
            Self::Rgb => format!("rgb({r}, {g}, {b})"),
            Self::Hsl => {
                let (h, s, l) = rgb_to_hsl(r, g, b);
                format!("hsl({h}, {s}%, {l}%)")
            }
        }
    }
}

/// 辅助计算 RGB 转 HSL
fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (u16, u8, u8) {
    let rf = r as f32 / 255.0;
    let gf = g as f32 / 255.0;
    let bf = b as f32 / 255.0;

    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let delta = max - min;

    let l = (max + min) / 2.0;

    if delta == 0.0 {
        return (0, 0, (l * 100.0).round() as u8);
    }

    let s = if l > 0.5 {
        delta / (2.0 - max - min)
    } else {
        delta / (max + min)
    };

    let mut h = if max == rf {
        (gf - bf) / delta + if gf < bf { 6.0 } else { 0.0 }
    } else if max == gf {
        (bf - rf) / delta + 2.0
    } else {
        (rf - gf) / delta + 4.0
    };
    h *= 60.0;

    (
        h.round() as u16,
        (s * 100.0).round() as u8,
        (l * 100.0).round() as u8,
    )
}

/// 首次进入截图时激活的初始工具枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DefaultTool {
    #[default]
    None,
    Rectangle,
    Pen,
    Marker,
    Arrow,
    Text,
    Badge,
}

impl DefaultTool {
    pub fn from_index(idx: i32) -> Self {
        match idx {
            1 => Self::Rectangle,
            2 => Self::Pen,
            3 => Self::Marker,
            4 => Self::Arrow,
            5 => Self::Text,
            6 => Self::Badge,
            _ => Self::None,
        }
    }

    pub fn to_index(self) -> i32 {
        match self {
            Self::None => 0,
            Self::Rectangle => 1,
            Self::Pen => 2,
            Self::Marker => 3,
            Self::Arrow => 4,
            Self::Text => 5,
            Self::Badge => 6,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Rectangle => "rectangle",
            Self::Pen => "pen",
            Self::Marker => "marker",
            Self::Arrow => "arrow",
            Self::Text => "text",
            Self::Badge => "badge",
        }
    }
}

/// 贴图滚轮缩放时的几何锚点基准枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ZoomAnchor {
    #[default]
    Cursor,
    Center,
}

impl ZoomAnchor {
    pub fn from_index(idx: i32) -> Self {
        if idx == 1 {
            Self::Center
        } else {
            Self::Cursor
        }
    }

    pub fn to_index(self) -> i32 {
        match self {
            Self::Cursor => 0,
            Self::Center => 1,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cursor => "cursor",
            Self::Center => "center",
        }
    }
}

/// 双击贴图窗口时的响应行为枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DoubleClickAction {
    #[default]
    Close,
    CopyAndClose,
    Copy,
}

impl DoubleClickAction {
    pub fn from_index(idx: i32) -> Self {
        match idx {
            1 => Self::CopyAndClose,
            2 => Self::Copy,
            _ => Self::Close,
        }
    }

    pub fn to_index(self) -> i32 {
        match self {
            Self::Close => 0,
            Self::CopyAndClose => 1,
            Self::Copy => 2,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Close => "close",
            Self::CopyAndClose => "copy_and_close",
            Self::Copy => "copy",
        }
    }
}

/// 图像输出存储编码格式枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ImageFormat {
    #[default]
    Png,
    Jpeg,
    Webp,
}

impl ImageFormat {
    pub fn from_index(idx: i32) -> Self {
        match idx {
            1 => Self::Jpeg,
            2 => Self::Webp,
            _ => Self::Png,
        }
    }

    pub fn to_index(self) -> i32 {
        match self {
            Self::Png => 0,
            Self::Jpeg => 1,
            Self::Webp => 2,
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Webp => "webp",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpeg",
            Self::Webp => "webp",
        }
    }
}
