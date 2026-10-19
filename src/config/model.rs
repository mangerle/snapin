//! Snapin 配置文件强类型模型定义。
//!
//! 包含常规、界面、截图、贴图、输出、快捷键等所有板块的配置结构体，
//! 所有字段均标注 `#[serde(default)]`，提供完全防崩溃的开箱即用默认值。

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

    /// 日志记录级别 ("trace" | "debug" | "info" | "warn" | "error")
    #[serde(default = "default_log_level")]
    pub log_level: String,

    /// 托盘图标风格 ("auto" | "dark" | "light")
    #[serde(default = "default_tray_theme")]
    pub tray_theme: String,

    /// 守护进程启动成功时是否发送桌面通知
    #[serde(default)]
    pub notify_on_startup: bool,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            autostart: true,
            language: default_language(),
            log_level: default_log_level(),
            tray_theme: default_tray_theme(),
            notify_on_startup: false,
        }
    }
}

/// 界面视觉设置板块
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InterfaceConfig {
    /// 主题模式 ("system" | "dark" | "light")
    #[serde(default = "default_theme")]
    pub theme: String,

    /// 选区边框与高亮强调色（HEX 格式）
    #[serde(default = "default_accent_color")]
    pub accent_color: String,

    /// 是否开启鼠标跟随取色放大镜
    #[serde(default = "default_true")]
    pub enable_magnifier: bool,

    /// 放大镜像素放大倍率 (如 4, 8, 16)
    #[serde(default = "default_magnifier_scale")]
    pub magnifier_scale: u32,

    /// 默认取色格式 ("hex" | "rgb" | "hsl")
    #[serde(default = "default_color_format")]
    pub color_format: String,

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
            theme: default_theme(),
            accent_color: default_accent_color(),
            enable_magnifier: true,
            magnifier_scale: default_magnifier_scale(),
            color_format: default_color_format(),
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

    /// 首次进入截图时默认激活的标注工具 ("none" | "rectangle" | "pen" | "marker")
    #[serde(default = "default_tool")]
    pub default_tool: String,

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
            default_tool: default_tool(),
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

    /// 滚轮缩放时的锚点基准 ("cursor" | "center")
    #[serde(default = "default_zoom_anchor")]
    pub zoom_anchor: String,

    /// 双击贴图窗口时的响应行为 ("close" | "minimize" | "copy")
    #[serde(default = "default_double_click_action")]
    pub double_click_action: String,

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
            zoom_anchor: default_zoom_anchor(),
            double_click_action: default_double_click_action(),
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

    /// 默认图片编码格式 ("png" | "jpeg" | "webp")
    #[serde(default = "default_format")]
    pub default_format: String,

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
            default_format: default_format(),
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
fn default_log_level() -> String {
    "info".to_string()
}
fn default_tray_theme() -> String {
    "auto".to_string()
}
fn default_theme() -> String {
    "system".to_string()
}
fn default_accent_color() -> String {
    "#00AEFF".to_string()
}
fn default_magnifier_scale() -> u32 {
    8
}
fn default_color_format() -> String {
    "hex".to_string()
}
fn default_toolbar_scale() -> f32 {
    1.0
}
fn default_tool() -> String {
    "rectangle".to_string()
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
fn default_zoom_anchor() -> String {
    "cursor".to_string()
}
fn default_double_click_action() -> String {
    "close".to_string()
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
fn default_format() -> String {
    "png".to_string()
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
