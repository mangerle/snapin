//! Snapin 配置文件强类型模型定义。
//!
//! 包含常规、界面、截图、贴图、输出、快捷键等所有板块的配置结构体组合，
//! 所有字段均标注 `#[serde(default)]`，提供完全防崩溃的开箱即用默认值。

pub use crate::config::enums::*;
pub use crate::config::sections::*;
use serde::{Deserialize, Serialize};

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
            self.interface.toolbar_scale = 1.0;
        }
        self.interface.toolbar_scale = self.interface.toolbar_scale.clamp(0.5, 3.0);
        self.interface.magnifier_scale = self.interface.magnifier_scale.clamp(1, 32);

        if !self.snip.default_stroke_width.is_finite() {
            self.snip.default_stroke_width = 4.0;
        }
        self.snip.default_stroke_width = self.snip.default_stroke_width.clamp(1.0, 30.0);
        if !self.snip.default_font_size.is_finite() {
            self.snip.default_font_size = 20.0;
        }
        self.snip.default_font_size = self.snip.default_font_size.clamp(8.0, 96.0);

        if !self.paster.initial_opacity.is_finite() {
            self.paster.initial_opacity = 1.0;
        }
        self.paster.initial_opacity = self.paster.initial_opacity.clamp(0.1, 1.0);

        self.output.jpeg_quality = self.output.jpeg_quality.clamp(1, 100);
        if self.output.quick_save_dir.trim().is_empty() {
            self.output.quick_save_dir = "~/Pictures/Screenshots".to_string();
        }
        if self.output.file_name_template.trim().is_empty() {
            self.output.file_name_template = "Snapin_{YYYY}{MM}{DD}_{HH}{mm}{ss}.png".to_string();
        }
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
