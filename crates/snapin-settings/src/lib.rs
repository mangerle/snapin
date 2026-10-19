//! Snapin 偏好设置中心图形界面模块（基于 Slint 声明式 GUI）。
//!
//! 采用独立轻量进程架构，提供现代化的卡片式设置面板，
//! 负责配置文件双向响应式同步、快捷键同步与系统集成。

slint::include_modules!();

pub mod shortcut;

use slint::{ComponentHandle, SharedString};
use snapin_core::config::{self, Config, HexColor, Hotkey};
use snapin_core::error::SnapinError;
use snapin_platform::{
    suspend_hotkeys_for_settings, sync_autostart_desktop_file, sync_hotkeys_to_gnome,
};
use std::sync::{Arc, Mutex};

/// 预设强调色表（顺序与 Slint interface_tab 中的 ComboBox 严格一一对应）
const ACCENT_COLORS: &[&str] = &[
    "#00AEFF", // 0: 天海晴蓝
    "#10B981", // 1: 翡翠青绿
    "#F59E0B", // 2: 日光亮橙
    "#8B5CF6", // 3: 优雅紫罗兰
];

/// 预设标注颜色表（顺序与 Slint snip_tab 中的 ComboBox 严格一一对应）
const DEFAULT_SNIP_COLORS: &[&str] = &[
    "#EF4444", // 0: 醒目赤红
    "#00AEFF", // 1: 天海晴蓝
    "#10B981", // 2: 清新翠绿
    "#F59E0B", // 3: 明艳亮黄
    "#FFFFFF", // 4: 纯洁雪白
    "#000000", // 5: 深邃墨黑
];

/// 预设放大镜倍率表（顺序与 Slint interface_tab 中的 ComboBox 严格一一对应）
const MAGNIFIER_SCALES: &[u32] = &[4, 8, 16];

fn accent_color_to_index(color: &HexColor) -> i32 {
    let hex = color.to_hex_string();
    ACCENT_COLORS
        .iter()
        .position(|&c| c.eq_ignore_ascii_case(&hex))
        .unwrap_or(0) as i32
}

fn index_to_accent_color(index: i32) -> HexColor {
    let idx = index.clamp(0, (ACCENT_COLORS.len() - 1) as i32) as usize;
    HexColor::from_hex(ACCENT_COLORS[idx]).unwrap_or_else(|| HexColor::new(0, 174, 255, 255))
}

fn snip_color_to_index(color: &HexColor) -> i32 {
    let hex = color.to_hex_string();
    DEFAULT_SNIP_COLORS
        .iter()
        .position(|&c| c.eq_ignore_ascii_case(&hex))
        .unwrap_or(0) as i32
}

fn index_to_snip_color(index: i32) -> HexColor {
    let idx = index.clamp(0, (DEFAULT_SNIP_COLORS.len() - 1) as i32) as usize;
    HexColor::from_hex(DEFAULT_SNIP_COLORS[idx]).unwrap_or_else(|| HexColor::new(239, 68, 68, 255))
}

fn magnifier_scale_to_index(scale: u32) -> i32 {
    MAGNIFIER_SCALES
        .iter()
        .position(|&s| s == scale)
        .unwrap_or(1) as i32
}

fn index_to_magnifier_scale(index: i32) -> u32 {
    let idx = index.clamp(0, (MAGNIFIER_SCALES.len() - 1) as i32) as usize;
    MAGNIFIER_SCALES[idx]
}

/// 运行 Slint 偏好设置中心图形界面
pub fn run_settings_gui() -> Result<(), SnapinError> {
    let window = SettingsWindow::new().map_err(|e| SnapinError::SoftbufferError {
        message: format!("初始化设置中心图形窗口失败: {e}"),
    })?;

    // 设置 Wayland XDG 应用标识，使 Dock 栏与 Alt-Tab 匹配矢量图标
    let _ = slint::set_xdg_app_id("snapin");

    window
        .window()
        .set_size(slint::LogicalSize::new(900.0, 620.0));
    window.set_app_version(SharedString::from(env!("CARGO_PKG_VERSION")));

    // 注册全局快捷键按键捕获与格式化回调
    let shortcut_helper = window.global::<ShortcutHelper>();
    shortcut_helper.on_format_key(|ctrl, alt, shift, meta, text| {
        let text_str = text.as_str();
        if let Some(formatted) = shortcut::format_key_combination(ctrl, alt, shift, meta, text_str)
        {
            SharedString::from(formatted)
        } else {
            SharedString::from("")
        }
    });

    let config = config::load_or_create_config(None)?;
    let config_arc = Arc::new(Mutex::new(config.clone()));

    // 1. 同步初始配置到 Slint 界面
    sync_config_to_window(&window, &config);

    // 2. 注册界面交互与保存回调
    register_callbacks(&window, config_arc);

    // 3. 临时挂起 Snapin 系统全局快捷键，确保录制时按键不被系统拦截触发
    let _ = suspend_hotkeys_for_settings();

    let run_res = window.run().map_err(|e| SnapinError::SoftbufferError {
        message: format!("设置中心事件循环运行异常: {e}"),
    });

    // 4. 设置窗口退出后，从磁盘恢复最新的有效全局快捷键
    if let Ok(latest_cfg) = config::load_or_create_config(None) {
        let _ = sync_hotkeys_to_gnome(&latest_cfg.hotkeys);
    }

    run_res?;
    Ok(())
}

/// 将 Config 结构体完整同步至 Slint 界面控件属性
fn sync_config_to_window(win: &SettingsWindow, cfg: &Config) {
    sync_general_to_window(win, &cfg.general);
    sync_interface_to_window(win, &cfg.interface);
    sync_snip_to_window(win, &cfg.snip);
    sync_paster_to_window(win, &cfg.paster);
    sync_output_to_window(win, &cfg.output);
    sync_hotkeys_to_window(win, &cfg.hotkeys);
}

/// 同步常规板块设置
fn sync_general_to_window(win: &SettingsWindow, g: &config::model::GeneralConfig) {
    win.set_autostart(g.autostart);
    win.set_notify_on_startup(g.notify_on_startup);
    win.set_log_level_index(g.log_level.to_index());
    win.set_tray_theme_index(g.tray_theme.to_index());
}

/// 同步界面外观板块设置
fn sync_interface_to_window(win: &SettingsWindow, i: &config::model::InterfaceConfig) {
    win.set_theme_index(i.theme.to_index());
    win.set_accent_color_index(accent_color_to_index(&i.accent_color));
    win.set_enable_magnifier(i.enable_magnifier);
    win.set_magnifier_scale_index(magnifier_scale_to_index(i.magnifier_scale));
    win.set_color_format_index(i.color_format.to_index());
    win.set_show_grid_lines(i.show_grid_lines);
    win.set_toolbar_scale_percent(i.toolbar_scale * 100.0);
}

/// 同步截图流程板块设置
fn sync_snip_to_window(win: &SettingsWindow, s: &config::model::SnipConfig) {
    win.set_capture_cursor(s.capture_cursor);
    win.set_smart_window_snapping(s.smart_window_snapping);
    win.set_remember_last_selection(s.remember_last_selection);
    win.set_default_tool_index(s.default_tool.to_index());
    win.set_default_color_index(snip_color_to_index(&s.default_color));
    win.set_default_stroke_width(s.default_stroke_width);
    win.set_snip_font_size(s.default_font_size);
}

/// 同步贴图板块设置
fn sync_paster_to_window(win: &SettingsWindow, p: &config::model::PasterConfig) {
    win.set_enable_above_extension(p.enable_above_extension);
    win.set_initial_opacity_percent(p.initial_opacity * 100.0);
    win.set_zoom_anchor_index(p.zoom_anchor.to_index());
    win.set_double_click_action_index(p.double_click_action.to_index());
    win.set_max_history_count(p.max_history_count as f32);
}

/// 同步输出与存储板块设置
fn sync_output_to_window(win: &SettingsWindow, o: &config::model::OutputConfig) {
    win.set_quick_save_dir(SharedString::from(&o.quick_save_dir));
    win.set_file_name_template(SharedString::from(&o.file_name_template));
    win.set_format_index(o.default_format.to_index());
    win.set_jpeg_quality(o.jpeg_quality as f32);
    win.set_notify_on_copy(o.notify_on_copy);
    win.set_notify_on_save(o.notify_on_save);
    win.set_auto_backup_raw(o.auto_backup_raw);
}

/// 同步快捷键板块设置
fn sync_hotkeys_to_window(win: &SettingsWindow, h: &config::model::HotkeyConfig) {
    win.set_snip_hotkey(SharedString::from(h.snip.to_display_string()));
    win.set_paste_hotkey(SharedString::from(h.paste.to_display_string()));
    win.set_toggle_hotkey(SharedString::from(h.toggle_pasters.to_display_string()));
}

/// 从界面提取最新值更新到内存 Config 结构体
fn extract_config_from_window(win: &SettingsWindow, cfg: &mut Config) {
    extract_general_from_window(win, &mut cfg.general);
    extract_interface_from_window(win, &mut cfg.interface);
    extract_snip_from_window(win, &mut cfg.snip);
    extract_paster_from_window(win, &mut cfg.paster);
    extract_output_from_window(win, &mut cfg.output);
    extract_hotkeys_from_window(win, &mut cfg.hotkeys);
}

/// 提取常规设置
fn extract_general_from_window(win: &SettingsWindow, g: &mut config::model::GeneralConfig) {
    g.autostart = win.get_autostart();
    g.notify_on_startup = win.get_notify_on_startup();
    g.log_level = config::model::LogLevel::from_index(win.get_log_level_index());
    g.tray_theme = config::model::TrayTheme::from_index(win.get_tray_theme_index());
}

/// 提取界面外观设置
fn extract_interface_from_window(win: &SettingsWindow, i: &mut config::model::InterfaceConfig) {
    i.theme = config::model::Theme::from_index(win.get_theme_index());
    i.accent_color = index_to_accent_color(win.get_accent_color_index());
    i.enable_magnifier = win.get_enable_magnifier();
    i.magnifier_scale = index_to_magnifier_scale(win.get_magnifier_scale_index());
    i.color_format = config::model::ColorFormat::from_index(win.get_color_format_index());
    i.show_grid_lines = win.get_show_grid_lines();
    i.toolbar_scale = win.get_toolbar_scale_percent() / 100.0;
}

/// 提取截图设置
fn extract_snip_from_window(win: &SettingsWindow, s: &mut config::model::SnipConfig) {
    s.capture_cursor = win.get_capture_cursor();
    s.smart_window_snapping = win.get_smart_window_snapping();
    s.remember_last_selection = win.get_remember_last_selection();
    s.default_tool = config::model::DefaultTool::from_index(win.get_default_tool_index());
    s.default_color = index_to_snip_color(win.get_default_color_index());
    s.default_stroke_width = win.get_default_stroke_width();
    s.default_font_size = win.get_snip_font_size();
}

/// 提取贴图设置
fn extract_paster_from_window(win: &SettingsWindow, p: &mut config::model::PasterConfig) {
    p.enable_above_extension = win.get_enable_above_extension();
    p.initial_opacity = win.get_initial_opacity_percent() / 100.0;
    p.zoom_anchor = config::model::ZoomAnchor::from_index(win.get_zoom_anchor_index());
    p.double_click_action =
        config::model::DoubleClickAction::from_index(win.get_double_click_action_index());
    p.max_history_count = win.get_max_history_count() as usize;
}

/// 提取输出保存设置
fn extract_output_from_window(win: &SettingsWindow, o: &mut config::model::OutputConfig) {
    o.quick_save_dir = win.get_quick_save_dir().to_string();
    o.file_name_template = win.get_file_name_template().to_string();
    o.default_format = config::model::ImageFormat::from_index(win.get_format_index());
    o.jpeg_quality = win.get_jpeg_quality() as u8;
    o.notify_on_copy = win.get_notify_on_copy();
    o.notify_on_save = win.get_notify_on_save();
    o.auto_backup_raw = win.get_auto_backup_raw();
}

/// 提取快捷键设置
fn extract_hotkeys_from_window(win: &SettingsWindow, h: &mut config::model::HotkeyConfig) {
    h.snip = Hotkey::parse_str(win.get_snip_hotkey().as_str());
    h.paste = Hotkey::parse_str(win.get_paste_hotkey().as_str());
    h.toggle_pasters = Hotkey::parse_str(win.get_toggle_hotkey().as_str());
}

/// 注册界面操作按钮回调事件
fn register_callbacks(win: &SettingsWindow, config_arc: Arc<Mutex<Config>>) {
    let win_weak = win.as_weak();
    let cfg_clone = config_arc.clone();
    win.on_save_and_apply(move || {
        if let Some(w) = win_weak.upgrade() {
            if let Ok(mut cfg_guard) = cfg_clone.lock() {
                extract_config_from_window(&w, &mut cfg_guard);
                if let Ok(path) = config::get_default_config_path() {
                    let _ = config::save_config(&cfg_guard, &path);
                }
                let _ = sync_hotkeys_to_gnome(&cfg_guard.hotkeys);
                sync_autostart_desktop_file(cfg_guard.general.autostart);
                w.set_status_message(SharedString::from("设置已成功保存并同步应用！"));
            }
        }
    });

    let win_weak2 = win.as_weak();
    win.on_restore_defaults(move || {
        if let Some(w) = win_weak2.upgrade() {
            let default_cfg = Config::default();
            sync_config_to_window(&w, &default_cfg);
            if let Ok(mut cfg_guard) = config_arc.lock() {
                *cfg_guard = default_cfg;
            }
            w.set_status_message(SharedString::from("已还原为默认推荐配置（点击保存后生效）"));
        }
    });

    win.on_open_config_folder(|| {
        if let Ok(cfg_path) = config::get_default_config_path() {
            if let Some(parent) = cfg_path.parent() {
                let _ = std::process::Command::new("xdg-open").arg(parent).spawn();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_accent_color_mapping() {
        let green = HexColor::from_hex("#10B981").unwrap();
        assert_eq!(accent_color_to_index(&green), 1);
        assert_eq!(index_to_accent_color(1), green);

        // 未知颜色兜底为默认天蓝 (索引 0)
        let unknown = HexColor::from_hex("#123456").unwrap();
        assert_eq!(accent_color_to_index(&unknown), 0);
    }

    #[test]
    fn test_snip_color_mapping() {
        let white = HexColor::from_hex("#FFFFFF").unwrap();
        assert_eq!(snip_color_to_index(&white), 4);
        assert_eq!(index_to_snip_color(4), white);

        let red = HexColor::from_hex("#EF4444").unwrap();
        assert_eq!(snip_color_to_index(&red), 0);
        assert_eq!(index_to_snip_color(0), red);
    }

    #[test]
    fn test_magnifier_scale_mapping() {
        assert_eq!(magnifier_scale_to_index(4), 0);
        assert_eq!(magnifier_scale_to_index(8), 1);
        assert_eq!(magnifier_scale_to_index(16), 2);
        assert_eq!(index_to_magnifier_scale(0), 4);
        assert_eq!(index_to_magnifier_scale(1), 8);
        assert_eq!(index_to_magnifier_scale(2), 16);
    }
}
