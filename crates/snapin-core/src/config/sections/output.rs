//! 输出存储与剪贴板设置模型定义。

use crate::config::enums::ImageFormat;
use serde::{Deserialize, Serialize};

/// 输出与存储设置板块
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutputConfig {
    /// 快捷保存默认目标文件夹路径
    #[serde(default = "default_quick_save_dir")]
    pub quick_save_dir: String,

    /// 文件自动命名时间戳模板
    #[serde(default = "default_file_name_template")]
    pub file_name_template: String,

    /// 默认图片编码格式
    #[serde(default)]
    pub default_format: ImageFormat,

    /// JPEG 编码质量 (1 ~ 100)；WebP 当前使用无损编码，不受此项影响
    #[serde(default = "default_jpeg_quality")]
    pub jpeg_quality: u8,

    /// 复制到剪贴板后是否触发桌面系统通知
    #[serde(default)]
    pub notify_on_copy: bool,

    /// 保存选区图片到文件后是否触发桌面系统通知
    #[serde(default)]
    pub notify_on_save: bool,

    /// 截图保存时是否弹出系统文件保存对话框让用户选择位置。
    ///
    /// 关闭时直接写入 `quick_save_dir`（默认行为）；开启时每次点击保存都先
    /// 经 XDG Portal 询问目标路径，便于按场景归档到不同目录。
    #[serde(default)]
    pub prompt_save_path: bool,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            quick_save_dir: default_quick_save_dir(),
            file_name_template: default_file_name_template(),
            default_format: ImageFormat::default(),
            jpeg_quality: default_jpeg_quality(),
            notify_on_copy: false,
            notify_on_save: false,
            prompt_save_path: false,
        }
    }
}

fn default_quick_save_dir() -> String {
    "~/Pictures/Screenshots".to_string()
}

fn default_file_name_template() -> String {
    "Snapin_{YYYY}{MM}{DD}_{HH}{mm}{ss}.png".to_string()
}

fn default_jpeg_quality() -> u8 {
    90
}
