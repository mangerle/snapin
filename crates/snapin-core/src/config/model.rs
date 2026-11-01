//! Snapin 配置文件强类型模型定义。
//!
//! 包含常规、界面、截图、贴图、输出、快捷键等所有板块的配置结构体组合，
//! 所有字段均标注 `#[serde(default)]`，提供完全防崩溃的开箱即用默认值。

use crate::config::defaults::{
    DEFAULT_FILE_NAME_TEMPLATE, DEFAULT_FONT_SIZE, DEFAULT_MAGNIFIER_SCALE, DEFAULT_PASTER_OPACITY,
    DEFAULT_QUICK_SAVE_DIR, DEFAULT_STROKE_WIDTH, DEFAULT_TOOLBAR_SCALE,
};
pub use crate::config::enums::*;
pub use crate::config::sections::*;
use serde::{Deserialize, Serialize};

/// 当前配置文件 schema 版本。
pub const CONFIG_SCHEMA_VERSION: u32 = 1;

/// 配置文件顶层封装：schema 版本 + 扁平化的配置板块。
///
/// `schema_version` 允许未来字段迁移在读取时显式分支；`flatten` 保证现有
/// `[general]`、`[output]` 等板块布局不变，旧版无版本号文件按 0 处理并迁移。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigFile {
    /// 配置 schema 版本
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    /// 配置内容
    #[serde(flatten)]
    pub config: Config,
}

impl ConfigFile {
    /// 用当前 schema 版本封装配置。
    pub fn current(config: Config) -> Self {
        Self {
            schema_version: CONFIG_SCHEMA_VERSION,
            config,
        }
    }

    /// schema 版本是否受当前程序支持。
    pub fn is_supported(&self) -> bool {
        self.schema_version <= CONFIG_SCHEMA_VERSION
    }
}

fn default_schema_version() -> u32 {
    CONFIG_SCHEMA_VERSION
}

/// Snapin 顶层配置根结构体
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Config {
    /// 常规基础设置
    #[serde(default)]
    pub general: GeneralConfig,

    /// 界面交互与视觉风格设置
    #[serde(default)]
    pub interface: InterfaceConfig,

    /// 截图流程与默认标注工具设置
    #[serde(default)]
    pub snip: SnipConfig,

    /// 贴图窗口与历史栈设置
    #[serde(default)]
    pub paster: PasterConfig,

    /// 存储与剪贴板输出设置
    #[serde(default)]
    pub output: OutputConfig,

    /// 全局快捷键映射设置
    #[serde(default)]
    pub hotkeys: HotkeyConfig,

    /// OCR 文字识别设置
    #[serde(default)]
    pub ocr: OcrConfig,

    /// 机器翻译与原位覆盖设置
    #[serde(default)]
    pub translate: TranslateConfig,
}

impl Config {
    /// 将外部配置中的越界数值收敛到运行期安全范围。
    ///
    /// 配置文件是外部输入，历史版本、手工编辑或跨版本迁移都可能留下越界值。
    /// 这里在加载边界一次性收敛，避免不透明度、JPEG 质量、缩放倍率等值在
    /// 渲染或编码阶段触发除零、缓冲区异常或不可用界面。
    #[must_use]
    pub fn sanitized(mut self) -> Self {
        self.general.update_check_interval_hours = self.general.update_check_interval_hours.max(1);

        if !self.interface.toolbar_scale.is_finite() {
            self.interface.toolbar_scale = DEFAULT_TOOLBAR_SCALE;
        }
        self.interface.toolbar_scale = self.interface.toolbar_scale.clamp(0.5, 3.0);
        self.interface.magnifier_scale = self
            .interface
            .magnifier_scale
            .clamp(1, DEFAULT_MAGNIFIER_SCALE.saturating_mul(4));

        if !self.snip.default_stroke_width.is_finite() {
            self.snip.default_stroke_width = DEFAULT_STROKE_WIDTH;
        }
        self.snip.default_stroke_width = self.snip.default_stroke_width.clamp(1.0, 30.0);
        if !self.snip.default_font_size.is_finite() {
            self.snip.default_font_size = DEFAULT_FONT_SIZE;
        }
        self.snip.default_font_size = self.snip.default_font_size.clamp(8.0, 96.0);

        if !self.paster.initial_opacity.is_finite() {
            self.paster.initial_opacity = DEFAULT_PASTER_OPACITY;
        }
        self.paster.initial_opacity = self.paster.initial_opacity.clamp(0.1, 1.0);

        self.output.jpeg_quality = self.output.jpeg_quality.clamp(1, 100);
        if self.output.quick_save_dir.trim().is_empty() {
            self.output.quick_save_dir = DEFAULT_QUICK_SAVE_DIR.to_string();
        }
        let template = normalize_file_name_template(&self.output.file_name_template);
        self.output.file_name_template = if template.is_empty() {
            DEFAULT_FILE_NAME_TEMPLATE.to_string()
        } else {
            template
        };
        if self
            .general
            .custom_update_url
            .as_deref()
            .is_some_and(|url| url.trim().is_empty())
        {
            self.general.custom_update_url = None;
        }

        self
    }
}

/// 将文件名模板末尾的图片扩展名剥离，扩展名统一由输出格式决定。
///
/// 历史默认模板曾包含 `.png`，而保存路径又追加当前格式扩展名，导致 `.png.png`
/// 或 `.png.jpg`。此函数在配置边界一次性归一化，兼容用户手工保留扩展名的旧模板。
pub fn normalize_file_name_template(template: &str) -> String {
    // 模板只能贡献单个文件名：同时剥离 POSIX/Windows 目录分隔符，防止 `../`
    // 或绝对路径把保存位置带出用户选择的目录。
    let candidate = template
        .trim()
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .trim();
    if candidate.is_empty() || candidate == "." || candidate == ".." {
        return String::new();
    }

    let lower = candidate.to_ascii_lowercase();
    for extension in ["png", "jpg", "jpeg", "webp"] {
        let suffix = format!(".{extension}");
        if lower.ends_with(&suffix) {
            return candidate[..candidate.len().saturating_sub(suffix.len())].to_string();
        }
    }
    candidate.to_string()
}

#[cfg(test)]
mod template_tests {
    use super::normalize_file_name_template;

    /// 模板扩展名必须被剥离，最终扩展名只由输出格式决定。
    #[test]
    fn test_normalize_file_name_template_strips_image_extension() {
        assert_eq!(normalize_file_name_template("shot.png"), "shot");
        assert_eq!(normalize_file_name_template("shot.JPEG"), "shot");
        assert_eq!(normalize_file_name_template("shot.webp"), "shot");
        assert_eq!(
            normalize_file_name_template("archive.tar.gz"),
            "archive.tar.gz"
        );
        assert_eq!(normalize_file_name_template("  shot  "), "shot");
    }
}
