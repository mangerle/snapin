//! 美化导出共享参数与样式枚举。
//!
//! 这些类型同时被配置模型（持久化字段）与渲染器（绘制输入）使用。放在基础契约层
//! 可以避免渲染 crate 反向依赖配置 schema，也避免配置为了复用渲染参数而引入
//! Tiny-Skia 与字体排版依赖。

use serde::{Deserialize, Serialize};

/// 社交平台输出尺寸。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SocialSize {
    /// 保持输入尺寸，仅增加四周留白
    #[default]
    Original,
    /// 1:1 方形画布 1080x1080
    Square,
    /// 4:5 竖版画布 1080x1350
    Portrait,
    /// 16:9 横版画布 1920x1080
    Wide,
}

impl SocialSize {
    /// 切换到下一个尺寸预设。
    pub fn next(self) -> Self {
        match self {
            Self::Original => Self::Square,
            Self::Square => Self::Portrait,
            Self::Portrait => Self::Wide,
            Self::Wide => Self::Original,
        }
    }

    /// 展示名称。
    pub fn label(self) -> &'static str {
        match self {
            Self::Original => "原始尺寸",
            Self::Square => "1:1 方形",
            Self::Portrait => "4:5 竖版",
            Self::Wide => "16:9 横版",
        }
    }

    /// 返回固定画布尺寸；原始尺寸返回 `None`。
    pub fn canvas_size(self) -> Option<(u32, u32)> {
        match self {
            Self::Original => None,
            Self::Square => Some((1080, 1080)),
            Self::Portrait => Some((1080, 1350)),
            Self::Wide => Some((1920, 1080)),
        }
    }
}

/// 美化背景样式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BeautifyBackground {
    /// 深色纯色背景
    #[default]
    Dark,
    /// 蓝紫渐变
    Indigo,
    /// 日落渐变
    Sunset,
    /// 薄荷渐变
    Mint,
}

impl BeautifyBackground {
    /// 切换到下一个背景。
    pub fn next(self) -> Self {
        match self {
            Self::Dark => Self::Indigo,
            Self::Indigo => Self::Sunset,
            Self::Sunset => Self::Mint,
            Self::Mint => Self::Dark,
        }
    }

    /// 展示名称。
    pub fn label(self) -> &'static str {
        match self {
            Self::Dark => "深色",
            Self::Indigo => "蓝紫渐变",
            Self::Sunset => "日落渐变",
            Self::Mint => "薄荷渐变",
        }
    }
}

/// 美化导出参数。
#[derive(Debug, Clone, PartialEq)]
pub struct BeautifyOptions {
    /// 社交画布尺寸
    pub social_size: SocialSize,
    /// 背景样式
    pub background: BeautifyBackground,
    /// 圆角半径
    pub corner_radius: f32,
    /// 图像四周留白
    pub padding: f32,
    /// 是否绘制阴影
    pub shadow: bool,
    /// 可选水印文字
    pub watermark: Option<String>,
}

impl Default for BeautifyOptions {
    fn default() -> Self {
        Self {
            social_size: SocialSize::Original,
            background: BeautifyBackground::Dark,
            corner_radius: 20.0,
            padding: 48.0,
            shadow: true,
            watermark: Some("Snapin".to_string()),
        }
    }
}
