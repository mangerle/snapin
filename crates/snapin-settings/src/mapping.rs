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

pub fn log_level_to_index(level: LogLevel) -> i32 {
    match level {
        LogLevel::Trace => 0,
        LogLevel::Debug => 1,
        LogLevel::Info => 2,
        LogLevel::Warn => 3,
        LogLevel::Error => 4,
    }
}

pub fn index_to_log_level(idx: i32) -> LogLevel {
    match idx {
        0 => LogLevel::Trace,
        1 => LogLevel::Debug,
        3 => LogLevel::Warn,
        4 => LogLevel::Error,
        _ => LogLevel::Info,
    }
}

pub fn tray_theme_to_index(theme: TrayTheme) -> i32 {
    match theme {
        TrayTheme::Auto => 0,
        TrayTheme::Dark => 1,
        TrayTheme::Light => 2,
    }
}

pub fn index_to_tray_theme(idx: i32) -> TrayTheme {
    match idx {
        1 => TrayTheme::Dark,
        2 => TrayTheme::Light,
        _ => TrayTheme::Auto,
    }
}

pub fn theme_to_index(theme: Theme) -> i32 {
    match theme {
        Theme::System => 0,
        Theme::Dark => 1,
        Theme::Light => 2,
    }
}

pub fn index_to_theme(idx: i32) -> Theme {
    match idx {
        1 => Theme::Dark,
        2 => Theme::Light,
        _ => Theme::System,
    }
}

pub fn color_format_to_index(format: ColorFormat) -> i32 {
    match format {
        ColorFormat::Hex => 0,
        ColorFormat::Rgb => 1,
        ColorFormat::Hsl => 2,
    }
}

pub fn index_to_color_format(idx: i32) -> ColorFormat {
    match idx {
        1 => ColorFormat::Rgb,
        2 => ColorFormat::Hsl,
        _ => ColorFormat::Hex,
    }
}

pub fn default_tool_to_index(tool: DefaultTool) -> i32 {
    match tool {
        DefaultTool::None => 0,
        DefaultTool::Rectangle => 1,
        DefaultTool::Pen => 2,
        DefaultTool::Marker => 3,
        DefaultTool::Arrow => 4,
        DefaultTool::Text => 5,
        DefaultTool::Badge => 6,
    }
}

pub fn index_to_default_tool(idx: i32) -> DefaultTool {
    match idx {
        1 => DefaultTool::Rectangle,
        2 => DefaultTool::Pen,
        3 => DefaultTool::Marker,
        4 => DefaultTool::Arrow,
        5 => DefaultTool::Text,
        6 => DefaultTool::Badge,
        _ => DefaultTool::None,
    }
}

pub fn zoom_anchor_to_index(anchor: ZoomAnchor) -> i32 {
    match anchor {
        ZoomAnchor::Cursor => 0,
        ZoomAnchor::Center => 1,
    }
}

pub fn index_to_zoom_anchor(idx: i32) -> ZoomAnchor {
    if idx == 1 {
        ZoomAnchor::Center
    } else {
        ZoomAnchor::Cursor
    }
}

pub fn double_click_action_to_index(action: DoubleClickAction) -> i32 {
    match action {
        DoubleClickAction::Close => 0,
        DoubleClickAction::CopyAndClose => 1,
        DoubleClickAction::Copy => 2,
    }
}

pub fn index_to_double_click_action(idx: i32) -> DoubleClickAction {
    match idx {
        1 => DoubleClickAction::CopyAndClose,
        2 => DoubleClickAction::Copy,
        _ => DoubleClickAction::Close,
    }
}

pub fn image_format_to_index(format: ImageFormat) -> i32 {
    match format {
        ImageFormat::Png => 0,
        ImageFormat::Jpeg => 1,
        ImageFormat::Webp => 2,
    }
}

pub fn index_to_image_format(idx: i32) -> ImageFormat {
    match idx {
        1 => ImageFormat::Jpeg,
        2 => ImageFormat::Webp,
        _ => ImageFormat::Png,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
