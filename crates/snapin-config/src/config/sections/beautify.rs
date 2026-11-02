//! 美化导出设置模型。

use crate::config::defaults::default_true;
use crate::config::enums::{BeautifyBackground, SocialSize};
use serde::{Deserialize, Serialize};

/// 美化导出设置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BeautifyConfig {
    /// 是否在保存截图时自动应用美化
    #[serde(default)]
    pub enabled: bool,
    /// 社交输出尺寸
    #[serde(default)]
    pub social_size: SocialSize,
    /// 背景样式
    #[serde(default)]
    pub background: BeautifyBackground,
    /// 图像圆角半径
    #[serde(default = "default_corner_radius")]
    pub corner_radius: f32,
    /// 是否绘制阴影
    #[serde(default = "default_true")]
    pub shadow: bool,
    /// 水印文字；空字符串表示关闭
    #[serde(default = "default_watermark")]
    pub watermark: String,
}

impl Default for BeautifyConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            social_size: SocialSize::Original,
            background: BeautifyBackground::Dark,
            corner_radius: 20.0,
            shadow: true,
            watermark: "Snapin".to_string(),
        }
    }
}

fn default_corner_radius() -> f32 {
    20.0
}

fn default_watermark() -> String {
    "Snapin".to_string()
}
