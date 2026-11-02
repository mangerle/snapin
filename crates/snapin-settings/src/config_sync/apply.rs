//! 将强类型配置写入 Slint 控件属性。

use super::super::mapping::{
    accent_color_to_index, magnifier_scale_to_index, snip_color_to_index, SettingEnum,
};
use super::super::SettingsWindow;
use super::scalar_table::apply_scalar_fields;
use slint::SharedString;
use snapin_core::config::{self, Config};

pub(crate) fn sync_config_to_window(win: &SettingsWindow, cfg: &Config) {
    apply_scalar_fields(win, cfg);
    sync_general_to_window(win, &cfg.general);
    sync_interface_to_window(win, &cfg.interface);
    sync_snip_to_window(win, &cfg.snip);
    sync_paster_to_window(win, &cfg.paster);
    sync_output_to_window(win, &cfg.output);
    sync_beautify_to_window(win, &cfg.beautify);
    sync_ocr_to_window(win, &cfg.ocr);
    sync_translate_to_window(win, &cfg.translate);
    sync_hotkeys_to_window(win, &cfg.hotkeys);
}

/// 同步常规板块设置
fn sync_general_to_window(win: &SettingsWindow, g: &config::model::GeneralConfig) {
    win.set_log_level_index(g.log_level.to_index());
}

/// 同步界面外观板块设置
fn sync_interface_to_window(win: &SettingsWindow, i: &config::model::InterfaceConfig) {
    win.set_accent_color_index(accent_color_to_index(&i.accent_color));
    win.set_magnifier_scale_index(magnifier_scale_to_index(i.magnifier_scale));
    win.set_color_format_index(i.color_format.to_index());
    win.set_toolbar_scale_percent(i.toolbar_scale * 100.0);
}

/// 同步截图流程板块设置
fn sync_snip_to_window(win: &SettingsWindow, s: &config::model::SnipConfig) {
    win.set_default_color_index(snip_color_to_index(&s.default_color));
}

/// 同步贴图板块设置
fn sync_paster_to_window(win: &SettingsWindow, p: &config::model::PasterConfig) {
    win.set_initial_opacity_percent(p.initial_opacity * 100.0);
    win.set_zoom_anchor_index(p.zoom_anchor.to_index());
    win.set_double_click_action_index(p.double_click_action.to_index());
}

/// 同步输出与存储板块设置
fn sync_output_to_window(win: &SettingsWindow, o: &config::model::OutputConfig) {
    win.set_format_index(o.default_format.to_index());
    win.set_jpeg_quality(o.jpeg_quality as f32);
}

/// 同步美化导出设置。
fn sync_beautify_to_window(win: &SettingsWindow, b: &config::model::BeautifyConfig) {
    win.set_beautify_social_size_index(b.social_size.to_index());
    win.set_beautify_background_index(b.background.to_index());
}

/// 同步快捷键板块设置
fn sync_hotkeys_to_window(win: &SettingsWindow, h: &config::model::HotkeyConfig) {
    win.set_snip_hotkey(SharedString::from(h.snip.to_display_string()));
    win.set_translate_hotkey(SharedString::from(h.translate.to_display_string()));
    win.set_paste_hotkey(SharedString::from(h.paste.to_display_string()));
    win.set_toggle_hotkey(SharedString::from(h.toggle_pasters.to_display_string()));
    win.set_switch_group_hotkey(SharedString::from(h.switch_group.to_display_string()));
    win.set_smart_redact_hotkey(SharedString::from(h.smart_redact.to_display_string()));
    win.set_qr_hotkey(SharedString::from(h.qr.to_display_string()));
    win.set_beautify_hotkey(SharedString::from(h.beautify.to_display_string()));
    win.set_workspace_hotkey(SharedString::from(h.workspace.to_display_string()));
    win.set_clipboard_history_hotkey(SharedString::from(h.clipboard_history.to_display_string()));
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
