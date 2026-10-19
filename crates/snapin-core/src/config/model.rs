//! Snapin 配置文件强类型模型定义。
//!
//! 包含常规、界面、截图、贴图、输出、快捷键等所有板块的配置结构体，
//! 所有字段均标注 `#[serde(default)]`，提供完全防崩溃的开箱即用默认值。
//! 全面采用代数数据类型（Enum）消除字符串偏执（Stringly Typed），杜绝非法状态。

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
}

/// 常规设置板块
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GeneralConfig {
    /// 是否开机自动启动
    #[serde(default = "default_true")]
    pub autostart: bool,

    /// 软件界面语言
    #[serde(default = "default_language")]
    pub language: String,

    /// 日志记录级别
    #[serde(default)]
    pub log_level: LogLevel,

    /// 托盘图标风格
    #[serde(default)]
    pub tray_theme: TrayTheme,

    /// 守护进程启动成功时是否发送桌面通知
    #[serde(default)]
    pub notify_on_startup: bool,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            autostart: true,
            language: default_language(),
            log_level: LogLevel::default(),
            tray_theme: TrayTheme::default(),
            notify_on_startup: false,
        }
    }
}

/// 界面视觉设置板块
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InterfaceConfig {
    /// 主题模式
    #[serde(default)]
    pub theme: Theme,

    /// 选区边框与高亮强调色（HEX 格式）
    #[serde(default = "default_accent_color")]
    pub accent_color: String,

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

/// 贴图窗口设置板块
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PasterConfig {
    /// 是否调用 GNOME Shell 扩展进行强制绝对置顶
    #[serde(default = "default_true")]
    pub enable_above_extension: bool,

    /// 新贴图初始不透明度 (0.1 ~ 1.0)
    #[serde(default = "default_opacity")]
    pub initial_opacity: f32,

    /// 滚轮缩放时的锚点基准
    #[serde(default)]
    pub zoom_anchor: ZoomAnchor,

    /// 双击贴图窗口时的响应行为
    #[serde(default)]
    pub double_click_action: DoubleClickAction,

    /// 暂存历史贴图的最大数量上限
    #[serde(default = "default_history_count")]
    pub max_history_count: usize,

    /// 当前激活的贴图分组 ID
    #[serde(default)]
    pub active_group_id: usize,
}

impl Default for PasterConfig {
    fn default() -> Self {
        Self {
            enable_above_extension: true,
            initial_opacity: default_opacity(),
            zoom_anchor: ZoomAnchor::default(),
            double_click_action: DoubleClickAction::default(),
            max_history_count: default_history_count(),
            active_group_id: 0,
        }
    }
}

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

    /// JPEG/WebP 编码质量 (1 ~ 100)
    #[serde(default = "default_jpeg_quality")]
    pub jpeg_quality: u8,

    /// 复制到剪贴板后是否触发桌面系统通知
    #[serde(default)]
    pub notify_on_copy: bool,

    /// 保存选区图片到文件后是否触发桌面系统通知
    #[serde(default)]
    pub notify_on_save: bool,

    /// 是否自动静默备份原始底图到缓存
    #[serde(default)]
    pub auto_backup_raw: bool,
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
            auto_backup_raw: false,
        }
    }
}

/// 全局快捷键设置板块
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HotkeyConfig {
    /// 区域截图快捷键
    #[serde(default = "default_snip_hotkey")]
    pub snip: String,

    /// 贴图快捷键
    #[serde(default = "default_paste_hotkey")]
    pub paste: String,

    /// 批量隐藏/显示当前组所有贴图
    #[serde(default = "default_toggle_hotkey")]
    pub toggle_pasters: String,

    /// 切换贴图活动分组快捷键
    #[serde(default = "default_switch_group_hotkey")]
    pub switch_group: String,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            snip: default_snip_hotkey(),
            paste: default_paste_hotkey(),
            toggle_pasters: default_toggle_hotkey(),
            switch_group: default_switch_group_hotkey(),
        }
    }
}

// 辅助默认值函数
fn default_true() -> bool {
    true
}
fn default_language() -> String {
    "zh-CN".to_string()
}
fn default_accent_color() -> String {
    "#00AEFF".to_string()
}
fn default_magnifier_scale() -> u32 {
    8
}
fn default_toolbar_scale() -> f32 {
    1.0
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
fn default_opacity() -> f32 {
    1.0
}
fn default_history_count() -> usize {
    20
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
fn default_snip_hotkey() -> String {
    "Ctrl+Alt+A".to_string()
}
fn default_paste_hotkey() -> String {
    "F1".to_string()
}
fn default_toggle_hotkey() -> String {
    "Ctrl+F3".to_string()
}
fn default_switch_group_hotkey() -> String {
    "Shift+F3".to_string()
}
