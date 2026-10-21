//! 设置中心配置与 Slint 界面之间的双向同步。
//!
//! 本模块只负责界面属性与强类型配置结构之间的映射，不参与回调注册与窗口生命周期管理。

use crate::mapping::{
    accent_color_to_index, color_format_to_index, double_click_action_to_index,
    image_format_to_index, index_to_accent_color, index_to_color_format,
    index_to_double_click_action, index_to_image_format, index_to_log_level,
    index_to_magnifier_scale, index_to_snip_color, index_to_zoom_anchor, log_level_to_index,
    magnifier_scale_to_index, snip_color_to_index, zoom_anchor_to_index, SettingEnum,
};
use crate::SettingsWindow;
use slint::SharedString;
use snapin_core::config::{self, Config, CoverMode, Hotkey, TargetLanguage, TranslateProvider};

/// 将 Config 结构体完整同步至 Slint 界面控件属性
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

    win.set_baidu_app_id(SharedString::from(&t.api_keys.baidu_app_id));
    win.set_baidu_secret_key(SharedString::from(&t.api_keys.baidu_secret_key));
    win.set_youdao_app_key(SharedString::from(&t.api_keys.youdao_app_key));
    win.set_youdao_app_secret(SharedString::from(&t.api_keys.youdao_app_secret));
    win.set_aliyun_access_key_id(SharedString::from(&t.api_keys.aliyun_access_key_id));
    win.set_aliyun_access_key_secret(SharedString::from(&t.api_keys.aliyun_access_key_secret));
}

/// 从界面提取最新值更新到内存 Config 结构体
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::{
        ACCENT_COLORS, CUSTOM_ACCENT_INDEX, CUSTOM_MAGNIFIER_INDEX, CUSTOM_SNIP_INDEX,
        DEFAULT_SNIP_COLORS, MAGNIFIER_SCALES,
    };
    use snapin_core::config::{
        ColorFormat, CoverMode, DoubleClickAction, HexColor, ImageFormat, LogLevel, TargetLanguage,
        TranslateProvider, ZoomAnchor,
    };

    /// 构造一份逐字段都不同于默认值的样本配置。
    ///
    /// 用非默认值做往返断言，任何一侧漏写 `set_/get_` 或反向换算写反都会被立即捕获；
    /// 若沿用默认值，字段缺失会被"恰好等于默认值"掩盖。
    fn sample_config() -> Config {
        let mut cfg = Config::default();

        cfg.general.autostart = false;
        cfg.general.notify_on_startup = true;
        cfg.general.log_level = LogLevel::from_index(3);

        cfg.interface.accent_color =
            HexColor::from_hex(ACCENT_COLORS[1]).expect("预设强调色必须可解析");
        cfg.interface.enable_magnifier = false;
        cfg.interface.magnifier_scale = MAGNIFIER_SCALES[2];
        cfg.interface.color_format = ColorFormat::from_index(1);
        cfg.interface.show_grid_lines = false;
        // 选用二进制可精确表示的百分比，避免浮点往返误差污染断言
        cfg.interface.toolbar_scale = 1.25;

        cfg.snip.remember_last_selection = false;
        cfg.snip.default_color =
            HexColor::from_hex(DEFAULT_SNIP_COLORS[3]).expect("预设标注色必须可解析");
        cfg.snip.default_stroke_width = 3.5;
        cfg.snip.default_font_size = 20.0;

        // active_group_id 是运行期状态，不经界面映射，往返后必须原样保留
        cfg.paster.active_group_id = 3;
        cfg.paster.initial_opacity = 0.75;
        cfg.paster.zoom_anchor = ZoomAnchor::from_index(1);
        cfg.paster.double_click_action = DoubleClickAction::from_index(2);

        cfg.output.quick_save_dir = "/tmp/snapin-shots".to_string();
        cfg.output.file_name_template = "Shot_{YYYY}{MM}{DD}".to_string();
        cfg.output.default_format = ImageFormat::from_index(1);
        cfg.output.jpeg_quality = 82;
        cfg.output.notify_on_copy = true;
        cfg.output.notify_on_save = false;

        cfg.ocr.local.model_dir = "/opt/models/ppocrv6".to_string();

        cfg.translate.provider = TranslateProvider::from_index(1);
        cfg.translate.target_language = TargetLanguage::from_index(2);
        cfg.translate.cover_mode = CoverMode::from_index(1);
        cfg.translate.api_keys.baidu_app_id = "baidu-id".to_string();
        cfg.translate.api_keys.baidu_secret_key = "baidu-secret".to_string();
        cfg.translate.api_keys.youdao_app_key = "youdao-key".to_string();
        cfg.translate.api_keys.youdao_app_secret = "youdao-secret".to_string();
        cfg.translate.api_keys.aliyun_access_key_id = "ali-ak".to_string();
        cfg.translate.api_keys.aliyun_access_key_secret = "ali-sk".to_string();

        cfg.hotkeys.snip = expect_hotkey("F2");
        cfg.hotkeys.translate = expect_hotkey("Ctrl+Shift+T");
        cfg.hotkeys.paste = expect_hotkey("Ctrl+Alt+V");
        cfg.hotkeys.toggle_pasters = expect_hotkey("Super+F3");
        cfg.hotkeys.switch_group = expect_hotkey("Alt+PageUp");

        cfg
    }

    /// 样本内使用的快捷键字面量保证合法
    fn expect_hotkey(binding: &str) -> Hotkey {
        Hotkey::parse_str(binding).expect("测试样本快捷键必须合法")
    }

    /// 尝试构建设置窗口数据模型。
    ///
    /// Slint 的 winit 后端在纯命令行环境下无可用平台后端，此时返回 `None` 让用例安全跳过，
    /// 与真实 OCR 模型 e2e 用例在资产缺失时的处理策略保持一致。
    fn try_build_window() -> Option<SettingsWindow> {
        SettingsWindow::new().ok()
    }

    /// 断言设置界面与配置之间的双向映射在所有场景下都成立。
    ///
    /// 四种场景合并在单个用例内复用同一个窗口实例：Slint winit 后端在同一进程内只能创建
    /// 一次事件循环，拆成多个 `#[test]` 会因重复构造而直接失败。
    /// 每个场景都先执行一次完整 `sync_config_to_window`，保证属性互不串扰。
    #[test]
    fn test_window_sync_roundtrip_all_scenarios() {
        let Some(win) = try_build_window() else {
            return;
        };

        // 场景一：逐字段非默认值的配置经界面往返后必须完全一致，
        // 任一侧漏写 set_/get_ 或反向换算写反（如百分比未除以 100）都会被捕获
        let expected = sample_config();
        let mut round_trip = expected.clone();
        sync_config_to_window(&win, &round_trip);
        extract_config_from_window(&win, &mut round_trip);
        assert_eq!(round_trip, expected, "配置经设置界面往返后必须逐字段一致");

        // 场景二：用户手工配置的非预设颜色与倍率必须落到"保留自定义"槽位且往返后不被重置
        let custom_accent = HexColor::new(0x12, 0x34, 0x56, 255);
        let custom_snip = HexColor::new(0xFE, 0xDC, 0xBA, 255);
        let mut custom_cfg = Config::default();
        custom_cfg.interface.accent_color = custom_accent;
        custom_cfg.interface.magnifier_scale = 6;
        custom_cfg.snip.default_color = custom_snip;

        sync_config_to_window(&win, &custom_cfg);
        assert_eq!(
            win.get_accent_color_index(),
            CUSTOM_ACCENT_INDEX,
            "非预设强调色应落到保留自定义槽位"
        );
        assert_eq!(
            win.get_default_color_index(),
            CUSTOM_SNIP_INDEX,
            "非预设标注色应落到保留自定义槽位"
        );
        assert_eq!(
            win.get_magnifier_scale_index(),
            CUSTOM_MAGNIFIER_INDEX,
            "非预设放大倍率应落到保留自定义槽位"
        );

        extract_config_from_window(&win, &mut custom_cfg);
        assert_eq!(custom_cfg.interface.accent_color, custom_accent);
        assert_eq!(custom_cfg.snip.default_color, custom_snip);
        assert_eq!(custom_cfg.interface.magnifier_scale, 6);

        // 场景三：录入非法快捷键时保留原绑定，避免误录清空全局热键
        let mut hotkey_cfg = sample_config();
        sync_config_to_window(&win, &hotkey_cfg);
        let before = hotkey_cfg.hotkeys.clone();
        win.set_snip_hotkey(SharedString::from("Hyper+B"));
        win.set_translate_hotkey(SharedString::from("Ctrl+"));
        win.set_paste_hotkey(SharedString::from("Ctrl+"));
        extract_config_from_window(&win, &mut hotkey_cfg);
        assert_eq!(
            hotkey_cfg.hotkeys.snip, before.snip,
            "非法主键不应覆盖原截图热键"
        );
        assert_eq!(
            hotkey_cfg.hotkeys.translate, before.translate,
            "非法主键不应覆盖原截图翻译热键"
        );
        assert_eq!(
            hotkey_cfg.hotkeys.paste, before.paste,
            "缺失主键不应覆盖原贴图热键"
        );

        // 场景四：合法改动（禁用与改绑）应如实写回
        sync_config_to_window(&win, &hotkey_cfg);
        win.set_snip_hotkey(SharedString::from("已禁用"));
        win.set_paste_hotkey(SharedString::from("Ctrl+Shift+Home"));
        extract_config_from_window(&win, &mut hotkey_cfg);
        assert_eq!(
            hotkey_cfg.hotkeys.snip,
            expect_hotkey(""),
            "禁用状态应写回空主键"
        );
        assert_eq!(
            hotkey_cfg.hotkeys.paste,
            expect_hotkey("Ctrl+Shift+Home"),
            "合法改绑应写回新值"
        );
    }
}
