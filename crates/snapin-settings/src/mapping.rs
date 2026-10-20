//! 偏好设置中心图形界面与核心领域枚举之间的索引映射适配层。
//!
//! 将 Slint UI 声明式下拉框组件的整数索引与 `snapin-core` 强类型领域枚举进行解耦转换，
//! 避免表现层细节反向污染核心领域模型。对于颜色与放大倍率保留“自定义”槽位，
//! 防止用户手工配置的非预设值在打开设置并保存后被静默重置。

use snapin_core::config::{
    ColorFormat, CoverMode, DefaultTool, DoubleClickAction, HexColor, ImageFormat, LogLevel,
    TargetLanguage, TranslateProvider, ZoomAnchor,
};

/// 预设强调色表（顺序与 Slint interface_tab 中的 ComboBox 严格一一对应）
pub const ACCENT_COLORS: &[&str] = &["#00AEFF", "#10B981", "#F59E0B", "#8B5CF6"];

/// 预设标注颜色表（顺序与 Slint snip_tab 中的 ComboBox 严格一一对应）
pub const DEFAULT_SNIP_COLORS: &[&str] = &[
    "#EF4444", "#00AEFF", "#10B981", "#F59E0B", "#FFFFFF", "#000000",
];

/// 预设放大镜倍率表（顺序与 Slint interface_tab 中的 ComboBox 严格一一对应）
pub const MAGNIFIER_SCALES: &[u32] = &[4, 8, 16];

/// 强调色下拉框中的“保留自定义颜色”槽位
pub const CUSTOM_ACCENT_INDEX: i32 = ACCENT_COLORS.len() as i32;
/// 标注色下拉框中的“保留自定义颜色”槽位
pub const CUSTOM_SNIP_INDEX: i32 = DEFAULT_SNIP_COLORS.len() as i32;
/// 放大倍率下拉框中的“保留自定义倍率”槽位
pub const CUSTOM_MAGNIFIER_INDEX: i32 = MAGNIFIER_SCALES.len() as i32;

/// 将强调色映射为下拉框索引；非预设颜色返回自定义槽位。
pub fn accent_color_to_index(color: &HexColor) -> i32 {
    let hex = color.to_hex_string();
    ACCENT_COLORS
        .iter()
        .position(|&candidate| candidate.eq_ignore_ascii_case(&hex))
        .map(|index| index as i32)
        .unwrap_or(CUSTOM_ACCENT_INDEX)
}

/// 从下拉框索引恢复强调色；自定义槽位保持现有配置值不变。
pub fn index_to_accent_color(index: i32, current: HexColor) -> HexColor {
    let preset_index = usize::try_from(index)
        .ok()
        .filter(|i| *i < ACCENT_COLORS.len());
    match preset_index {
        Some(preset) => HexColor::from_hex(ACCENT_COLORS[preset])
            .unwrap_or_else(|| HexColor::new(0, 174, 255, 255)),
        None => current,
    }
}

/// 将标注色映射为下拉框索引；非预设颜色返回自定义槽位。
pub fn snip_color_to_index(color: &HexColor) -> i32 {
    let hex = color.to_hex_string();
    DEFAULT_SNIP_COLORS
        .iter()
        .position(|&candidate| candidate.eq_ignore_ascii_case(&hex))
        .map(|index| index as i32)
        .unwrap_or(CUSTOM_SNIP_INDEX)
}

/// 从下拉框索引恢复标注色；自定义槽位保持现有配置值不变。
pub fn index_to_snip_color(index: i32, current: HexColor) -> HexColor {
    let preset_index = usize::try_from(index)
        .ok()
        .filter(|i| *i < DEFAULT_SNIP_COLORS.len());
    match preset_index {
        Some(preset) => HexColor::from_hex(DEFAULT_SNIP_COLORS[preset])
            .unwrap_or_else(|| HexColor::new(239, 68, 68, 255)),
        None => current,
    }
}

/// 将放大倍率映射为下拉框索引；非预设倍率返回自定义槽位。
pub fn magnifier_scale_to_index(scale: u32) -> i32 {
    MAGNIFIER_SCALES
        .iter()
        .position(|&candidate| candidate == scale)
        .map(|index| index as i32)
        .unwrap_or(CUSTOM_MAGNIFIER_INDEX)
}

/// 从下拉框索引恢复放大倍率；自定义槽位保持现有配置值不变。
pub fn index_to_magnifier_scale(index: i32, current: u32) -> u32 {
    usize::try_from(index)
        .ok()
        .and_then(|preset| MAGNIFIER_SCALES.get(preset).copied())
        .unwrap_or(current)
}

/// UI 设置项与领域枚举双向映射契约 Trait
pub trait SettingEnum: Sized + Copy + PartialEq + Eq + 'static {
    /// 该枚举所有合法变体按照 UI ComboBox 呈现顺序排列的静态切片
    fn all_variants() -> &'static [Self];

    /// 将枚举实例推导转换为对应的 0-indexed 下标
    fn to_index(self) -> i32 {
        Self::all_variants()
            .iter()
            .position(|&variant| variant == self)
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

impl SettingEnum for TranslateProvider {
    fn all_variants() -> &'static [Self] {
        &[
            Self::BingFree,
            Self::GoogleFree,
            Self::Baidu,
            Self::Youdao,
            Self::Aliyun,
        ]
    }
}

impl SettingEnum for TargetLanguage {
    fn all_variants() -> &'static [Self] {
        &[
            Self::Zh,
            Self::En,
            Self::Ja,
            Self::Ko,
            Self::Fr,
            Self::De,
            Self::Es,
            Self::Ru,
        ]
    }
}

impl SettingEnum for CoverMode {
    fn all_variants() -> &'static [Self] {
        &[Self::SolidColor, Self::FrostedGlass]
    }
}

pub fn log_level_to_index(level: LogLevel) -> i32 {
    level.to_index()
}

pub fn index_to_log_level(index: i32) -> LogLevel {
    SettingEnum::from_index(index)
}

pub fn color_format_to_index(format: ColorFormat) -> i32 {
    format.to_index()
}

pub fn index_to_color_format(index: i32) -> ColorFormat {
    SettingEnum::from_index(index)
}

pub fn default_tool_to_index(tool: DefaultTool) -> i32 {
    tool.to_index()
}

pub fn index_to_default_tool(index: i32) -> DefaultTool {
    SettingEnum::from_index(index)
}

pub fn zoom_anchor_to_index(anchor: ZoomAnchor) -> i32 {
    anchor.to_index()
}

pub fn index_to_zoom_anchor(index: i32) -> ZoomAnchor {
    SettingEnum::from_index(index)
}

pub fn double_click_action_to_index(action: DoubleClickAction) -> i32 {
    action.to_index()
}

pub fn index_to_double_click_action(index: i32) -> DoubleClickAction {
    SettingEnum::from_index(index)
}

pub fn image_format_to_index(format: ImageFormat) -> i32 {
    format.to_index()
}

pub fn index_to_image_format(index: i32) -> ImageFormat {
    SettingEnum::from_index(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_setting_enums_roundtrip() {
        fn check_roundtrip<T: SettingEnum + std::fmt::Debug>() {
            for (expected_index, &variant) in T::all_variants().iter().enumerate() {
                assert_eq!(variant.to_index(), expected_index as i32);
                assert_eq!(T::from_index(expected_index as i32), variant);
            }
            assert_eq!(T::from_index(-1), T::all_variants()[0]);
            assert_eq!(T::from_index(999), T::all_variants()[0]);
        }

        check_roundtrip::<LogLevel>();
        check_roundtrip::<ColorFormat>();
        check_roundtrip::<DefaultTool>();
        check_roundtrip::<ZoomAnchor>();
        check_roundtrip::<DoubleClickAction>();
        check_roundtrip::<ImageFormat>();
        check_roundtrip::<TranslateProvider>();
        check_roundtrip::<TargetLanguage>();
        check_roundtrip::<CoverMode>();
    }

    #[test]
    fn test_custom_color_is_preserved() {
        let custom_accent = HexColor::new(0x12, 0x34, 0x56, 255);
        let accent_index = accent_color_to_index(&custom_accent);
        assert_eq!(accent_index, CUSTOM_ACCENT_INDEX);
        assert_eq!(
            index_to_accent_color(accent_index, custom_accent),
            custom_accent
        );

        let custom_snip = HexColor::new(0xAB, 0xCD, 0xEF, 255);
        let snip_index = snip_color_to_index(&custom_snip);
        assert_eq!(snip_index, CUSTOM_SNIP_INDEX);
        assert_eq!(index_to_snip_color(snip_index, custom_snip), custom_snip);

        assert_eq!(
            index_to_accent_color(0, custom_accent).to_hex_string(),
            "#00AEFF"
        );
        assert_eq!(
            index_to_snip_color(0, custom_snip).to_hex_string(),
            "#EF4444"
        );
    }

    #[test]
    fn test_custom_magnifier_scale_is_preserved() {
        let custom_scale = 6_u32;
        let index = magnifier_scale_to_index(custom_scale);
        assert_eq!(index, CUSTOM_MAGNIFIER_INDEX);
        assert_eq!(index_to_magnifier_scale(index, custom_scale), custom_scale);
        assert_eq!(index_to_magnifier_scale(1, custom_scale), 8);
    }
}
