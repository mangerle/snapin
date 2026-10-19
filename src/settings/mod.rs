//! Snapin 偏好设置中心图形界面模块（基于 Slint 声明式 GUI）。
//!
//! 采用独立轻量进程架构，提供现代化的卡片式设置面板，
//! 负责配置文件双向响应式同步、快捷键同步与系统集成。

slint::include_modules!();

use crate::config::{self, Config};
use crate::error::SnapinError;
use crate::platform::hotkey_manager::sync_hotkeys_to_gnome;
use slint::{ComponentHandle, SharedString};
use std::sync::{Arc, Mutex};

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

    let config = config::load_or_create_config(None)?;
    let config_arc = Arc::new(Mutex::new(config.clone()));

    // 1. 同步初始配置到 Slint 界面
    sync_config_to_window(&window, &config);

    // 2. 注册界面交互与保存回调
    register_callbacks(&window, config_arc);

    window.run().map_err(|e| SnapinError::SoftbufferError {
        message: format!("设置中心事件循环运行异常: {e}"),
    })?;

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
    let log_idx = match g.log_level.as_str() {
        "trace" => 0,
        "debug" => 1,
        "warn" => 3,
        "error" => 4,
        _ => 2,
    };
    win.set_log_level_index(log_idx);
    let tray_idx = match g.tray_theme.as_str() {
        "dark" => 1,
        "light" => 2,
        _ => 0,
    };
    win.set_tray_theme_index(tray_idx);
}

/// 同步界面外观板块设置
fn sync_interface_to_window(win: &SettingsWindow, i: &config::model::InterfaceConfig) {
    let theme_idx = match i.theme.as_str() {
        "dark" => 1,
        "light" => 2,
        _ => 0,
    };
    win.set_theme_index(theme_idx);
    let accent_idx = match i.accent_color.to_uppercase().as_str() {
        "#10B981" => 1,
        "#F59E0B" => 2,
        "#8B5CF6" => 3,
        _ => 0,
    };
    win.set_accent_color_index(accent_idx);
    win.set_enable_magnifier(i.enable_magnifier);
    let mag_idx = match i.magnifier_scale {
        4 => 0,
        16 => 2,
        _ => 1,
    };
    win.set_magnifier_scale_index(mag_idx);
    let fmt_idx = match i.color_format.as_str() {
        "rgb" => 1,
        "hsl" => 2,
        _ => 0,
    };
    win.set_color_format_index(fmt_idx);
    win.set_show_grid_lines(i.show_grid_lines);
    win.set_toolbar_scale_percent(i.toolbar_scale * 100.0);
}

/// 同步截图流程板块设置
fn sync_snip_to_window(win: &SettingsWindow, s: &config::model::SnipConfig) {
    win.set_capture_cursor(s.capture_cursor);
    win.set_smart_window_snapping(s.smart_window_snapping);
    win.set_remember_last_selection(s.remember_last_selection);
    let tool_idx = match s.default_tool.as_str() {
        "rectangle" => 1,
        "pen" => 2,
        "marker" => 3,
        "arrow" => 4,
        "text" => 5,
        "badge" => 6,
        _ => 0,
    };
    win.set_default_tool_index(tool_idx);
    let col_idx = match s.default_color.to_uppercase().as_str() {
        "#00AEFF" => 1,
        "#10B981" => 2,
        "#F59E0B" => 3,
        "#FFFFFF" => 4,
        "#000000" => 5,
        _ => 0,
    };
    win.set_default_color_index(col_idx);
    win.set_default_stroke_width(s.default_stroke_width);
    win.set_snip_font_size(s.default_font_size);
}

/// 同步贴图板块设置
fn sync_paster_to_window(win: &SettingsWindow, p: &config::model::PasterConfig) {
    win.set_enable_above_extension(p.enable_above_extension);
    win.set_initial_opacity_percent(p.initial_opacity * 100.0);
    win.set_zoom_anchor_index(if p.zoom_anchor == "center" { 1 } else { 0 });
    let act_idx = match p.double_click_action.as_str() {
        "copy_and_close" => 1,
        "copy" => 2,
        _ => 0,
    };
    win.set_double_click_action_index(act_idx);
    win.set_max_history_count(p.max_history_count as f32);
}

/// 同步输出与存储板块设置
fn sync_output_to_window(win: &SettingsWindow, o: &config::model::OutputConfig) {
    win.set_quick_save_dir(SharedString::from(&o.quick_save_dir));
    win.set_file_name_template(SharedString::from(&o.file_name_template));
    let fmt_idx = match o.default_format.as_str() {
        "jpeg" | "jpg" => 1,
        "webp" => 2,
        _ => 0,
    };
    win.set_format_index(fmt_idx);
    win.set_jpeg_quality(o.jpeg_quality as f32);
    win.set_notify_on_copy(o.notify_on_copy);
    win.set_notify_on_save(o.notify_on_save);
    win.set_auto_backup_raw(o.auto_backup_raw);
}

/// 同步快捷键板块设置
fn sync_hotkeys_to_window(win: &SettingsWindow, h: &config::model::HotkeyConfig) {
    win.set_snip_hotkey(SharedString::from(&h.snip));
    win.set_paste_hotkey(SharedString::from(&h.paste));
    win.set_toggle_hotkey(SharedString::from(&h.toggle_pasters));
}

/// 从界面提取最新值更新到内存 Config 结构体
fn extract_config_from_window(win: &SettingsWindow, cfg: &mut Config) {
    // 1. 常规
    cfg.general.autostart = win.get_autostart();
    cfg.general.notify_on_startup = win.get_notify_on_startup();
    cfg.general.log_level = match win.get_log_level_index() {
        0 => "trace",
        1 => "debug",
        3 => "warn",
        4 => "error",
        _ => "info",
    }
    .to_string();
    cfg.general.tray_theme = match win.get_tray_theme_index() {
        1 => "dark",
        2 => "light",
        _ => "auto",
    }
    .to_string();

    // 2. 界面外观
    cfg.interface.theme = match win.get_theme_index() {
        1 => "dark",
        2 => "light",
        _ => "system",
    }
    .to_string();
    cfg.interface.accent_color = match win.get_accent_color_index() {
        1 => "#10B981",
        2 => "#F59E0B",
        3 => "#8B5CF6",
        _ => "#00AEFF",
    }
    .to_string();
    cfg.interface.enable_magnifier = win.get_enable_magnifier();
    cfg.interface.magnifier_scale = match win.get_magnifier_scale_index() {
        0 => 4,
        2 => 16,
        _ => 8,
    };
    cfg.interface.color_format = match win.get_color_format_index() {
        1 => "rgb",
        2 => "hsl",
        _ => "hex",
    }
    .to_string();
    cfg.interface.show_grid_lines = win.get_show_grid_lines();
    cfg.interface.toolbar_scale = win.get_toolbar_scale_percent() / 100.0;

    // 3. 截图流程
    cfg.snip.capture_cursor = win.get_capture_cursor();
    cfg.snip.smart_window_snapping = win.get_smart_window_snapping();
    cfg.snip.remember_last_selection = win.get_remember_last_selection();
    cfg.snip.default_tool = match win.get_default_tool_index() {
        1 => "rectangle",
        2 => "pen",
        3 => "marker",
        4 => "arrow",
        5 => "text",
        6 => "badge",
        _ => "none",
    }
    .to_string();
    cfg.snip.default_color = match win.get_default_color_index() {
        1 => "#00AEFF",
        2 => "#10B981",
        3 => "#F59E0B",
        4 => "#FFFFFF",
        5 => "#000000",
        _ => "#EF4444",
    }
    .to_string();
    cfg.snip.default_stroke_width = win.get_default_stroke_width();
    cfg.snip.default_font_size = win.get_snip_font_size();

    // 4. 贴图
    cfg.paster.enable_above_extension = win.get_enable_above_extension();
    cfg.paster.initial_opacity = win.get_initial_opacity_percent() / 100.0;
    cfg.paster.zoom_anchor = if win.get_zoom_anchor_index() == 1 {
        "center".to_string()
    } else {
        "cursor".to_string()
    };
    cfg.paster.double_click_action = match win.get_double_click_action_index() {
        1 => "copy_and_close",
        2 => "copy",
        _ => "close",
    }
    .to_string();
    cfg.paster.max_history_count = win.get_max_history_count() as usize;

    // 5. 输出保存
    cfg.output.quick_save_dir = win.get_quick_save_dir().to_string();
    cfg.output.file_name_template = win.get_file_name_template().to_string();
    cfg.output.default_format = match win.get_format_index() {
        1 => "jpeg",
        2 => "webp",
        _ => "png",
    }
    .to_string();
    cfg.output.jpeg_quality = win.get_jpeg_quality() as u8;
    cfg.output.notify_on_copy = win.get_notify_on_copy();
    cfg.output.notify_on_save = win.get_notify_on_save();
    cfg.output.auto_backup_raw = win.get_auto_backup_raw();

    // 6. 快捷键
    cfg.hotkeys.snip = win.get_snip_hotkey().to_string();
    cfg.hotkeys.paste = win.get_paste_hotkey().to_string();
    cfg.hotkeys.toggle_pasters = win.get_toggle_hotkey().to_string();
}

/// 注册界面操作按钮回调事件
fn register_callbacks(win: &SettingsWindow, config_arc: Arc<Mutex<Config>>) {
    // 保存并应用
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
                w.set_status_message(SharedString::from("设置已成功保存并同步应用！"));
            }
        }
    });

    // 恢复默认配置
    let win_weak = win.as_weak();
    let cfg_clone = config_arc.clone();
    win.on_restore_defaults(move || {
        if let Some(w) = win_weak.upgrade() {
            let default_cfg = Config::default();
            sync_config_to_window(&w, &default_cfg);
            if let Ok(mut cfg_guard) = cfg_clone.lock() {
                *cfg_guard = default_cfg;
            }
            w.set_status_message(SharedString::from("已还原为默认推荐配置（点击保存后生效）"));
        }
    });

    // 打开配置文件夹
    win.on_open_config_folder(|| {
        if let Ok(cfg_path) = config::get_default_config_path() {
            if let Some(parent) = cfg_path.parent() {
                let _ = std::process::Command::new("xdg-open").arg(parent).spawn();
            }
        }
    });

    // 手动同步 GNOME 系统快捷键
    let win_weak = win.as_weak();
    let cfg_clone = config_arc;
    win.on_sync_gnome_hotkeys(move || {
        if let Some(w) = win_weak.upgrade() {
            if let Ok(cfg_guard) = cfg_clone.lock() {
                if sync_hotkeys_to_gnome(&cfg_guard.hotkeys).is_ok() {
                    w.set_status_message(SharedString::from("系统快捷键已成功重新同步至 GNOME！"));
                } else {
                    w.set_status_message(SharedString::from(
                        "同步 GNOME 快捷键失败，请检查 dconf 权限",
                    ));
                }
            }
        }
    });
}
