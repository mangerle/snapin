use super::*;
use crate::mapping::{
    SettingEnum, ACCENT_COLORS, CUSTOM_ACCENT_INDEX, CUSTOM_MAGNIFIER_INDEX, CUSTOM_SNIP_INDEX,
    DEFAULT_SNIP_COLORS, MAGNIFIER_SCALES,
};
use crate::SettingsWindow;
use slint::SharedString;
use snapin_core::config::{
    ColorFormat, CoverMode, DoubleClickAction, HexColor, ImageFormat, LogLevel, TargetLanguage,
    TranslateProvider, ZoomAnchor,
};
use snapin_core::config::{Config, Hotkey};
use snapin_core::render::{BeautifyBackground, SocialSize};

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
    cfg.snip.remember_toolbar_settings = true;
    cfg.snip.default_color =
        HexColor::from_hex(DEFAULT_SNIP_COLORS[3]).expect("预设标注色必须可解析");
    cfg.snip.default_stroke_width = 3.5;
    cfg.snip.default_font_size = 20.0;

    cfg.paster.initial_opacity = 0.75;
    cfg.paster.zoom_anchor = ZoomAnchor::from_index(1);
    cfg.paster.double_click_action = DoubleClickAction::from_index(2);

    cfg.output.quick_save_dir = "/tmp/snapin-shots".to_string();
    cfg.output.file_name_template = "Shot_{YYYY}{MM}{DD}".to_string();
    cfg.output.default_format = ImageFormat::from_index(1);
    cfg.output.jpeg_quality = 82;
    cfg.output.notify_on_copy = true;
    cfg.output.notify_on_save = false;
    cfg.output.prompt_save_path = true;

    cfg.beautify.enabled = true;
    cfg.beautify.social_size = SocialSize::Portrait;
    cfg.beautify.background = BeautifyBackground::Sunset;
    cfg.beautify.corner_radius = 24.0;
    cfg.beautify.shadow = false;
    cfg.beautify.watermark = "Snapin Test".to_string();

    cfg.ocr.local.model_dir = "/opt/models/ppocrv6".to_string();

    cfg.translate.provider = TranslateProvider::from_index(1);
    cfg.translate.target_language = TargetLanguage::from_index(2);
    cfg.translate.cover_mode = CoverMode::from_index(1);
    cfg.translate.api_keys.baidu_app_id = "baidu-id".into();
    cfg.translate.api_keys.baidu_secret_key = "baidu-secret".into();
    cfg.translate.api_keys.youdao_app_key = "youdao-key".into();
    cfg.translate.api_keys.youdao_app_secret = "youdao-secret".into();
    cfg.translate.api_keys.aliyun_access_key_id = "ali-ak".into();
    cfg.translate.api_keys.aliyun_access_key_secret = "ali-sk".into();

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

    assert!(
        slint::select_bundled_translation("en").is_ok(),
        "英文 Slint 翻译包必须可加载"
    );
    assert!(
        slint::select_bundled_translation("").is_ok(),
        "中文 Slint 翻译包必须可加载"
    );

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
