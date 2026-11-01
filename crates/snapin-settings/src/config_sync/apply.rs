//! 将强类型配置写入 Slint 控件属性。

use super::super::mapping::{
    accent_color_to_index, color_format_to_index, double_click_action_to_index,
    image_format_to_index, log_level_to_index, magnifier_scale_to_index, snip_color_to_index,
    zoom_anchor_to_index, SettingEnum,
};
use super::super::SettingsWindow;
use slint::SharedString;
use snapin_core::config::{self, Config};

pub(crate) fn sync_config_to_window(win: &SettingsWindow, cfg: &Config) {
    sync_general_to_window(win, &cfg.general);
    sync_interface_to_window(win, &cfg.interface);
    sync_snip_to_window(win, &cfg.snip);
    sync_paster_to_window(win, &cfg.paster);
    sync_output_to_window(win, &cfg.output);
    sync_ocr_to_window(win, &cfg.ocr);
    sync_translate_to_window(win, &cfg.translate);
    sync_hotkeys_to_window(win, &cfg.hotkeys);
}

/// 同步常规板块设置
fn sync_general_to_window(win: &SettingsWindow, g: &config::model::GeneralConfig) {
    win.set_autostart(g.autostart);
    win.set_notify_on_startup(g.notify_on_startup);
    win.set_auto_check_update(g.auto_check_update);
    win.set_log_level_index(log_level_to_index(g.log_level));
}

/// 同步界面外观板块设置
fn sync_interface_to_window(win: &SettingsWindow, i: &config::model::InterfaceConfig) {
    win.set_accent_color_index(accent_color_to_index(&i.accent_color));
    win.set_enable_magnifier(i.enable_magnifier);
    win.set_magnifier_scale_index(magnifier_scale_to_index(i.magnifier_scale));
    win.set_color_format_index(color_format_to_index(i.color_format));
    win.set_show_grid_lines(i.show_grid_lines);
    win.set_toolbar_scale_percent(i.toolbar_scale * 100.0);
}

/// 同步截图流程板块设置
fn sync_snip_to_window(win: &SettingsWindow, s: &config::model::SnipConfig) {
    win.set_remember_last_selection(s.remember_last_selection);
    win.set_default_color_index(snip_color_to_index(&s.default_color));
    win.set_default_stroke_width(s.default_stroke_width);
    win.set_snip_font_size(s.default_font_size);
}

/// 同步贴图板块设置
fn sync_paster_to_window(win: &SettingsWindow, p: &config::model::PasterConfig) {
    win.set_initial_opacity_percent(p.initial_opacity * 100.0);
    win.set_zoom_anchor_index(zoom_anchor_to_index(p.zoom_anchor));
    win.set_double_click_action_index(double_click_action_to_index(p.double_click_action));
}

/// 同步输出与存储板块设置
fn sync_output_to_window(win: &SettingsWindow, o: &config::model::OutputConfig) {
    win.set_quick_save_dir(SharedString::from(&o.quick_save_dir));
    win.set_file_name_template(SharedString::from(&o.file_name_template));
    win.set_format_index(image_format_to_index(o.default_format));
    win.set_jpeg_quality(o.jpeg_quality as f32);
    win.set_notify_on_copy(o.notify_on_copy);
    win.set_notify_on_save(o.notify_on_save);
    win.set_prompt_save_path(o.prompt_save_path);
}

/// 同步快捷键板块设置
fn sync_hotkeys_to_window(win: &SettingsWindow, h: &config::model::HotkeyConfig) {
    win.set_snip_hotkey(SharedString::from(h.snip.to_display_string()));
    win.set_translate_hotkey(SharedString::from(h.translate.to_display_string()));
    win.set_paste_hotkey(SharedString::from(h.paste.to_display_string()));
    win.set_toggle_hotkey(SharedString::from(h.toggle_pasters.to_display_string()));
    win.set_switch_group_hotkey(SharedString::from(h.switch_group.to_display_string()));
}

/// 同步文字识别板块设置
fn sync_ocr_to_window(win: &SettingsWindow, o: &config::model::OcrConfig) {
    win.set_ocr_model_dir(SharedString::from(&o.local.model_dir));
}

/// 同步翻译板块设置
fn sync_translate_to_window(win: &SettingsWindow, t: &config::model::TranslateConfig) {
    win.set_translate_provider_index(t.provider.to_index());
    win.set_target_language_index(t.target_language.to_index());
    win.set_cover_mode_index(t.cover_mode.to_index());

    win.set_baidu_app_id(SharedString::from(t.api_keys.baidu_app_id.as_str()));
    win.set_baidu_secret_key(SharedString::from(t.api_keys.baidu_secret_key.as_str()));
    win.set_youdao_app_key(SharedString::from(t.api_keys.youdao_app_key.as_str()));
    win.set_youdao_app_secret(SharedString::from(t.api_keys.youdao_app_secret.as_str()));
    win.set_aliyun_access_key_id(SharedString::from(t.api_keys.aliyun_access_key_id.as_str()));
    win.set_aliyun_access_key_secret(SharedString::from(
        t.api_keys.aliyun_access_key_secret.as_str(),
    ));
}
