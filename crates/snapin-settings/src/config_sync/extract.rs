//! 从 Slint 控件属性提取强类型配置。

use super::super::mapping::{
    index_to_accent_color, index_to_color_format, index_to_double_click_action,
    index_to_image_format, index_to_log_level, index_to_magnifier_scale, index_to_snip_color,
    index_to_zoom_anchor, SettingEnum,
};
use super::super::SettingsWindow;
use snapin_core::config::{self, Config, CoverMode, Hotkey, TargetLanguage, TranslateProvider};

pub(crate) fn extract_config_from_window(win: &SettingsWindow, cfg: &mut Config) {
    extract_general_from_window(win, &mut cfg.general);
    extract_interface_from_window(win, &mut cfg.interface);
    extract_snip_from_window(win, &mut cfg.snip);
    extract_paster_from_window(win, &mut cfg.paster);
    extract_output_from_window(win, &mut cfg.output);
    extract_ocr_from_window(win, &mut cfg.ocr);
    extract_translate_from_window(win, &mut cfg.translate);
    extract_hotkeys_from_window(win, &mut cfg.hotkeys);
}

/// 提取常规设置
fn extract_general_from_window(win: &SettingsWindow, g: &mut config::model::GeneralConfig) {
    g.autostart = win.get_autostart();
    g.notify_on_startup = win.get_notify_on_startup();
    g.auto_check_update = win.get_auto_check_update();
    g.log_level = index_to_log_level(win.get_log_level_index());
}

/// 提取界面外观设置
fn extract_interface_from_window(win: &SettingsWindow, i: &mut config::model::InterfaceConfig) {
    i.accent_color = index_to_accent_color(win.get_accent_color_index(), i.accent_color);
    i.enable_magnifier = win.get_enable_magnifier();
    i.magnifier_scale =
        index_to_magnifier_scale(win.get_magnifier_scale_index(), i.magnifier_scale);
    i.color_format = index_to_color_format(win.get_color_format_index());
    i.show_grid_lines = win.get_show_grid_lines();
    i.toolbar_scale = win.get_toolbar_scale_percent() / 100.0;
}

/// 提取截图设置
fn extract_snip_from_window(win: &SettingsWindow, s: &mut config::model::SnipConfig) {
    s.remember_last_selection = win.get_remember_last_selection();
    s.default_color = index_to_snip_color(win.get_default_color_index(), s.default_color);
    s.default_stroke_width = win.get_default_stroke_width();
    s.default_font_size = win.get_snip_font_size();
}

/// 提取贴图设置
fn extract_paster_from_window(win: &SettingsWindow, p: &mut config::model::PasterConfig) {
    p.initial_opacity = win.get_initial_opacity_percent() / 100.0;
    p.zoom_anchor = index_to_zoom_anchor(win.get_zoom_anchor_index());
    p.double_click_action = index_to_double_click_action(win.get_double_click_action_index());
}

/// 提取输出保存设置
fn extract_output_from_window(win: &SettingsWindow, o: &mut config::model::OutputConfig) {
    o.quick_save_dir = win.get_quick_save_dir().to_string();
    o.file_name_template = win.get_file_name_template().to_string();
    o.default_format = index_to_image_format(win.get_format_index());
    o.jpeg_quality = win.get_jpeg_quality() as u8;
    o.notify_on_copy = win.get_notify_on_copy();
    o.notify_on_save = win.get_notify_on_save();
    o.prompt_save_path = win.get_prompt_save_path();
}

/// 提取快捷键设置。
///
/// 录入值非法时保留原有绑定并告警，避免一次误录（或界面拼接出的畸形字符串）把用户既有快捷键清空。
fn extract_hotkeys_from_window(win: &SettingsWindow, h: &mut config::model::HotkeyConfig) {
    h.snip = parse_hotkey_or_keep("区域截图", win.get_snip_hotkey().as_str(), &h.snip);
    h.translate = parse_hotkey_or_keep(
        "截图翻译",
        win.get_translate_hotkey().as_str(),
        &h.translate,
    );
    h.paste = parse_hotkey_or_keep("贴图", win.get_paste_hotkey().as_str(), &h.paste);
    h.toggle_pasters = parse_hotkey_or_keep(
        "贴图显隐",
        win.get_toggle_hotkey().as_str(),
        &h.toggle_pasters,
    );
    h.switch_group = parse_hotkey_or_keep(
        "贴图分组",
        win.get_switch_group_hotkey().as_str(),
        &h.switch_group,
    );
}

/// 解析界面上的快捷键文本，失败时回退为当前值
fn parse_hotkey_or_keep(field: &str, raw: &str, current: &Hotkey) -> Hotkey {
    match Hotkey::parse_str(raw) {
        Ok(hotkey) => hotkey,
        Err(error) => {
            tracing::warn!("设置中心 [{field}] 快捷键录入值非法，已保留原绑定，原因: {error}");
            current.clone()
        }
    }
}

/// 提取文字识别设置
fn extract_ocr_from_window(win: &SettingsWindow, o: &mut config::model::OcrConfig) {
    o.local.model_dir = win.get_ocr_model_dir().to_string();
}

/// 提取翻译设置
fn extract_translate_from_window(win: &SettingsWindow, t: &mut config::model::TranslateConfig) {
    t.provider = TranslateProvider::from_index(win.get_translate_provider_index());
    t.target_language = TargetLanguage::from_index(win.get_target_language_index());
    t.cover_mode = CoverMode::from_index(win.get_cover_mode_index());

    t.api_keys.baidu_app_id = win.get_baidu_app_id().to_string();
    t.api_keys.baidu_secret_key = win.get_baidu_secret_key().to_string();
    t.api_keys.youdao_app_key = win.get_youdao_app_key().to_string();
    t.api_keys.youdao_app_secret = win.get_youdao_app_secret().to_string();
    t.api_keys.aliyun_access_key_id = win.get_aliyun_access_key_id().to_string();
    t.api_keys.aliyun_access_key_secret = win.get_aliyun_access_key_secret().to_string();
}
