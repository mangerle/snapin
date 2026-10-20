//! Snapin 强类型枚举与值转换定义。
//!
//! 包含系统日志级别、主题风格、取色格式、默认工具、缩放锚点、贴图交互与输出格式等代数数据类型定义。
//! 遵循关注点分离原则，核心领域枚举不耦合任何特定 UI 框架的控件下标索引。

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
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Close => "close",
            Self::CopyAndClose => "copy_and_close",
            Self::Copy => "copy",
        }
    }
}

/// OCR 引擎类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OcrEngineKind {
    /// 本地 PP-OCRv6 离线推理
    #[default]
    #[serde(rename = "local_ppocr")]
    LocalPpOcr,
}

/// 机器翻译服务商
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TranslateProvider {
    /// 微软必应免费接口
    #[default]
    BingFree,
    /// 谷歌翻译免费接口
    GoogleFree,
    /// 百度翻译开放平台
    Baidu,
    /// 网易有道开放平台
    Youdao,
    /// 阿里云机器翻译
    Aliyun,
}

/// 翻译目标语言
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TargetLanguage {
    /// 简体中文
    #[default]
    Zh,
    /// 英语
    En,
    /// 日语
    Ja,
    /// 韩语
    Ko,
    /// 法语
    Fr,
    /// 德语
    De,
    /// 西班牙语
    Es,
    /// 俄语
    Ru,
}

impl TargetLanguage {
    /// 转换为第三方翻译 API 使用的标准语言代码
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Zh => "zh",
            Self::En => "en",
            Self::Ja => "ja",
            Self::Ko => "ko",
            Self::Fr => "fr",
            Self::De => "de",
            Self::Es => "es",
            Self::Ru => "ru",
        }
    }
}

/// 原位翻译背景覆盖模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CoverMode {
    /// 智能取色实色填充
    #[default]
    SolidColor,
    /// 半透明磨砂覆盖
    FrostedGlass,
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
