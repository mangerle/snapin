//! 偏好设置中心图形界面与核心领域枚举之间的索引映射适配层。
//!
//! 将 Slint UI 声明式下拉框组件的整数索引与 `snapin-core` 强类型领域枚举进行解耦转换，
//! 避免表现层细节反向污染核心领域模型。

use snapin_core::config::{
    ColorFormat, DefaultTool, DoubleClickAction, HexColor, ImageFormat, LogLevel, Theme, TrayTheme,
    ZoomAnchor,
};

/// 预设强调色表（顺序与 Slint interface_tab 中的 ComboBox 严格一一对应）
pub const ACCENT_COLORS: &[&str] = &[
    "#00AEFF", // 0: 天海晴蓝
    "#10B981", // 1: 翡翠青绿
    "#F59E0B", // 2: 日光亮橙
    "#8B5CF6", // 3: 优雅紫罗兰
];

/// 预设标注颜色表（顺序与 Slint snip_tab 中的 ComboBox 严格一一对应）
pub const DEFAULT_SNIP_COLORS: &[&str] = &[
    "#EF4444", // 0: 醒目赤红
    "#00AEFF", // 1: 天海晴蓝
    "#10B981", // 2: 清新翠绿
    "#F59E0B", // 3: 明艳亮黄
    "#FFFFFF", // 4: 纯洁雪白
    "#000000", // 5: 深邃墨黑
];

/// 预设放大镜倍率表（顺序与 Slint interface_tab 中的 ComboBox 严格一一对应）
pub const MAGNIFIER_SCALES: &[u32] = &[4, 8, 16];

pub fn accent_color_to_index(color: &HexColor) -> i32 {
    let hex = color.to_hex_string();
    ACCENT_COLORS
        .iter()
        .position(|&c| c.eq_ignore_ascii_case(&hex))
        .unwrap_or(0) as i32
}

pub fn index_to_accent_color(index: i32) -> HexColor {
    let idx = index.clamp(0, (ACCENT_COLORS.len() - 1) as i32) as usize;
    HexColor::from_hex(ACCENT_COLORS[idx]).unwrap_or_else(|| HexColor::new(0, 174, 255, 255))
}

pub fn snip_color_to_index(color: &HexColor) -> i32 {
    let hex = color.to_hex_string();
    DEFAULT_SNIP_COLORS
        .iter()
        .position(|&c| c.eq_ignore_ascii_case(&hex))
        .unwrap_or(0) as i32
}

pub fn index_to_snip_color(index: i32) -> HexColor {
    let idx = index.clamp(0, (DEFAULT_SNIP_COLORS.len() - 1) as i32) as usize;
    HexColor::from_hex(DEFAULT_SNIP_COLORS[idx]).unwrap_or_else(|| HexColor::new(239, 68, 68, 255))
}

pub fn magnifier_scale_to_index(scale: u32) -> i32 {
    MAGNIFIER_SCALES
        .iter()
        .position(|&s| s == scale)
        .unwrap_or(1) as i32
}

pub fn index_to_magnifier_scale(index: i32) -> u32 {
    let idx = index.clamp(0, (MAGNIFIER_SCALES.len() - 1) as i32) as usize;
    MAGNIFIER_SCALES[idx]
}

/// UI 设置项与领域枚举双向映射契约 Trait
pub trait SettingEnum: Sized + Copy + PartialEq + Eq + 'static {
    /// 该枚举所有合法变体按照 UI ComboBox 呈现顺序排列的静态切片
    fn all_variants() -> &'static [Self];

    /// 将枚举实例推导转换为对应的 0-indexed 下标
    fn to_index(self) -> i32 {
        Self::all_variants()
            .iter()
            .position(|&v| v == self)
            .unwrap_or(0) as i32
    }

    /// 从界面下标安全反向解析枚举实例，越界时自动优雅回退至首个默认变体
    fn from_index(index: i32) -> Self {
        let variants = Self::all_variants();
        if index >= 0 && (index as usize) < variants.len() {
            variants[index as usize]
        } else {
            variants[0]
        }
    }
}

impl SettingEnum for LogLevel {
    fn all_variants() -> &'static [Self] {
        &[
            Self::Trace,
            Self::Debug,
            Self::Info,
            Self::Warn,
            Self::Error,
        ]
    }
}

impl SettingEnum for TrayTheme {
    fn all_variants() -> &'static [Self] {
        &[Self::Auto, Self::Dark, Self::Light]
    }
}

impl SettingEnum for Theme {
    fn all_variants() -> &'static [Self] {
        &[Self::System, Self::Dark, Self::Light]
    }
}

impl SettingEnum for ColorFormat {
    fn all_variants() -> &'static [Self] {
        &[Self::Hex, Self::Rgb, Self::Hsl]
    }
}

impl SettingEnum for DefaultTool {
    fn all_variants() -> &'static [Self] {
        &[
            Self::None,
            Self::Rectangle,
            Self::Pen,
            Self::Marker,
            Self::Arrow,
            Self::Text,
            Self::Badge,
        ]
    }
}

impl SettingEnum for ZoomAnchor {
    fn all_variants() -> &'static [Self] {
        &[Self::Cursor, Self::Center]
    }
}

impl SettingEnum for DoubleClickAction {
    fn all_variants() -> &'static [Self] {
        &[Self::Close, Self::CopyAndClose, Self::Copy]
    }
}

impl SettingEnum for ImageFormat {
    fn all_variants() -> &'static [Self] {
        &[Self::Png, Self::Jpeg, Self::Webp]
    }
}

pub fn log_level_to_index(level: LogLevel) -> i32 {
    level.to_index()
}

pub fn index_to_log_level(idx: i32) -> LogLevel {
    SettingEnum::from_index(idx)
}

pub fn tray_theme_to_index(theme: TrayTheme) -> i32 {
    theme.to_index()
}

pub fn index_to_tray_theme(idx: i32) -> TrayTheme {
    SettingEnum::from_index(idx)
}

pub fn theme_to_index(theme: Theme) -> i32 {
    theme.to_index()
}

pub fn index_to_theme(idx: i32) -> Theme {
    SettingEnum::from_index(idx)
}

pub fn color_format_to_index(format: ColorFormat) -> i32 {
    format.to_index()
}

pub fn index_to_color_format(idx: i32) -> ColorFormat {
    SettingEnum::from_index(idx)
}

pub fn default_tool_to_index(tool: DefaultTool) -> i32 {
    tool.to_index()
}

pub fn index_to_default_tool(idx: i32) -> DefaultTool {
    SettingEnum::from_index(idx)
}

pub fn zoom_anchor_to_index(anchor: ZoomAnchor) -> i32 {
    anchor.to_index()
}

pub fn index_to_zoom_anchor(idx: i32) -> ZoomAnchor {
    SettingEnum::from_index(idx)
}

pub fn double_click_action_to_index(action: DoubleClickAction) -> i32 {
    action.to_index()
}

pub fn index_to_double_click_action(idx: i32) -> DoubleClickAction {
    SettingEnum::from_index(idx)
}

pub fn image_format_to_index(format: ImageFormat) -> i32 {
    format.to_index()
}

pub fn index_to_image_format(idx: i32) -> ImageFormat {
    SettingEnum::from_index(idx)
}

/// 预设翻译服务商列表（顺序与 Slint ocr_tab 中的 ComboBox 严格一一对应）
pub const TRANSLATE_PROVIDERS: &[&str] = &[
    "bing_free",   // 0: 微软必应 (免 Key)
    "google_free", // 1: 谷歌翻译 (免 Key)
    "baidu",       // 2: 百度翻译
    "youdao",      // 3: 网易有道翻译
    "aliyun",      // 4: 阿里云翻译
];

pub fn translate_provider_to_index(provider: &str) -> i32 {
    TRANSLATE_PROVIDERS
        .iter()
        .position(|&p| p == provider)
        .unwrap_or(0) as i32
}

pub fn index_to_translate_provider(index: i32) -> &'static str {
    let idx = index.clamp(0, (TRANSLATE_PROVIDERS.len() - 1) as i32) as usize;
    TRANSLATE_PROVIDERS[idx]
}

/// 预设目标翻译语言列表（顺序与 Slint ocr_tab 中的 ComboBox 严格一一对应）
pub const TARGET_LANGUAGES: &[&str] = &[
    "zh", // 0: 中文 (简体)
    "en", // 1: 英语
    "ja", // 2: 日语
    "ko", // 3: 韩语
    "fr", // 4: 法语
    "de", // 5: 德语
    "es", // 6: 西班牙语
    "ru", // 7: 俄语
];

pub fn target_language_to_index(lang: &str) -> i32 {
    TARGET_LANGUAGES
        .iter()
        .position(|&l| l == lang)
        .unwrap_or(0) as i32
}

pub fn index_to_target_language(index: i32) -> &'static str {
    let idx = index.clamp(0, (TARGET_LANGUAGES.len() - 1) as i32) as usize;
    TARGET_LANGUAGES[idx]
}

/// 截图原位覆盖模式列表（顺序与 Slint ocr_tab 中的 ComboBox 严格一一对应）
pub const COVER_MODES: &[&str] = &[
    "solid_color",   // 0: 智能取色平铺填充
    "frosted_glass", // 1: 半透明磨砂高斯模糊
];

pub fn cover_mode_to_index(mode: &str) -> i32 {
    COVER_MODES.iter().position(|&m| m == mode).unwrap_or(0) as i32
}

pub fn index_to_cover_mode(index: i32) -> &'static str {
    let idx = index.clamp(0, (COVER_MODES.len() - 1) as i32) as usize;
    COVER_MODES[idx]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_setting_enums_roundtrip() {
        fn check_roundtrip<T: SettingEnum + std::fmt::Debug>() {
            for (expected_idx, &variant) in T::all_variants().iter().enumerate() {
                assert_eq!(variant.to_index(), expected_idx as i32);
                assert_eq!(T::from_index(expected_idx as i32), variant);
            }
            // 越界与负数安全断言（回退到首个默认变体）
            assert_eq!(T::from_index(-1), T::all_variants()[0]);
            assert_eq!(T::from_index(999), T::all_variants()[0]);
        }

        check_roundtrip::<LogLevel>();
        check_roundtrip::<TrayTheme>();
        check_roundtrip::<Theme>();
        check_roundtrip::<ColorFormat>();
        check_roundtrip::<DefaultTool>();
        check_roundtrip::<ZoomAnchor>();
        check_roundtrip::<DoubleClickAction>();
        check_roundtrip::<ImageFormat>();
    }

    #[test]
    fn test_translate_mappings() {
        assert_eq!(translate_provider_to_index("bing_free"), 0);
        assert_eq!(index_to_translate_provider(0), "bing_free");
        assert_eq!(target_language_to_index("zh"), 0);
        assert_eq!(index_to_target_language(0), "zh");
        assert_eq!(cover_mode_to_index("solid_color"), 0);
        assert_eq!(index_to_cover_mode(0), "solid_color");
    }

    #[test]
    fn test_log_level_roundtrip() {
        for idx in 0..=4 {
            let lvl = index_to_log_level(idx);
            assert_eq!(log_level_to_index(lvl), idx);
        }
    }

    #[test]
    fn test_theme_roundtrip() {
        for idx in 0..=2 {
            let t = index_to_theme(idx);
            assert_eq!(theme_to_index(t), idx);
        }
    }

    #[test]
    fn test_image_format_roundtrip() {
        for idx in 0..=2 {
            let fmt = index_to_image_format(idx);
            assert_eq!(image_format_to_index(fmt), idx);
        }
    }
}
