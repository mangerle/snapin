//! 工具栏二级属性选项、尺寸档位与调色板。

use super::tools::{LineMode, MosaicMode};
use snapin_core::annotation::{
    ArrowHeadStyle, ArrowStyle, BadgeSequence, BadgeStyle, LineStyle, MagnifierConnector,
    MagnifierShape, MarkerBlendMode, MarkerShape, SpotlightShape,
};
use tiny_skia::Color;

/// 笔刷粗细预设档位
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrokeWidthLevel {
    /// 细 (2px)
    Thin,
    /// 中 (4px)
    Medium,
    /// 粗 (8px)
    Thick,
}

impl StrokeWidthLevel {
    /// 获取实际像素数值
    pub fn to_pixel(self) -> f32 {
        match self {
            Self::Thin => 2.0,
            Self::Medium => 4.0,
            Self::Thick => 8.0,
        }
    }
}

/// 文字字号预设（像素），顺序与字号下拉菜单一致。
pub const FONT_SIZE_PRESETS: &[f32] = &[12.0, 16.0, 20.0, 24.0, 32.0, 48.0];

/// 字体族预设；`None` 表示使用系统默认无衬线字体。
pub const FONT_FAMILY_PRESETS: &[(&str, Option<&str>)] = &[
    ("系统默认", None),
    ("微软雅黑", Some("Microsoft YaHei")),
    ("苹方", Some("PingFang SC")),
    ("思源黑体", Some("Source Han Sans SC")),
    ("Arial", Some("Arial")),
    ("Consolas", Some("Consolas")),
    ("宋体", Some("SimSun")),
];

/// 二级属性栏选项
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SubToolOptions {
    /// 预设笔触粗细档位
    pub stroke_width: StrokeWidthLevel,
    /// 自由连续笔触粗细 (像素值，受鼠标滚轮连续动态微调，范围 1.0 ~ 50.0)
    pub custom_stroke_width: f32,
    /// 当前前景色 RGBA [0.0, 1.0]
    pub color: Color,
    /// 字体大小 (用于文本标注，受鼠标滚轮连续动态微调，范围 12.0 ~ 72.0)
    pub font_size: f32,
    /// 矩形/椭圆的描边线型
    pub line_style: LineStyle,
    /// 矩形/椭圆是否填充当前颜色（否则只描边）
    pub filled: bool,
    /// 矩形圆角半径（像素），在二级栏圆角控件上滚动鼠标滚轮连续调节
    pub corner_radius: f32,
    /// 序号底板样式
    pub badge_style: BadgeStyle,
    /// 序号格式
    pub badge_sequence: BadgeSequence,
    /// 箭头形态样式
    pub arrow_style: ArrowStyle,
    /// 起点箭头样式
    pub start_arrow_head: ArrowHeadStyle,
    /// 终点箭头样式
    pub end_arrow_head: ArrowHeadStyle,
    pub text_bold: bool,
    pub text_italic: bool,
    pub text_stroke_width: f32,
    pub text_background_enabled: bool,
    /// 文字背景不透明度（0 到 255）。
    pub text_background_opacity: u8,
    /// 文字背景内边距。
    pub text_background_padding: f32,
    /// 文字背景圆角半径。
    pub text_background_corner_radius: f32,
    pub font_family: Option<&'static str>,
    pub marker_shape: MarkerShape,
    pub marker_blend_mode: MarkerBlendMode,
    pub eraser_rectangle_mode: bool,
    /// 聚光灯压暗强度（RGBA alpha）
    pub spotlight_intensity: u8,
    /// 直线工具当前交互模式。
    pub line_mode: LineMode,
    /// 直线/折线工具是否显示端点箭头。
    pub line_arrow_enabled: bool,
    /// 马赛克工具当前处理模式。
    pub mosaic_mode: MosaicMode,
    /// 马赛克工具是否使用画笔涂抹，否则使用矩形框选。
    pub mosaic_brush_mode: bool,
    /// 马赛克与模糊的效果强度。
    pub effect_strength: u8,
    /// 聚光灯聚焦区域形状。
    pub spotlight_shape: SpotlightShape,
    /// 聚光灯是否绘制聚焦区域描边。
    pub spotlight_outline: bool,
    /// 放大镜外框形状。
    pub magnifier_shape: MagnifierShape,
    /// 放大镜倍率。
    pub magnifier_scale: f32,
    /// 放大镜是否使用平滑插值。
    pub magnifier_smooth: bool,
    /// 是否显示放大图像阴影。
    pub magnifier_shadow: bool,
    /// 放大镜连接线样式。
    pub magnifier_connector: MagnifierConnector,
}

impl Default for SubToolOptions {
    fn default() -> Self {
        Self {
            stroke_width: StrokeWidthLevel::Medium,
            custom_stroke_width: 4.0,
            color: Color::from_rgba8(239, 68, 68, 255), // 默认红色
            font_size: 20.0,
            line_style: LineStyle::Solid,
            filled: false,
            corner_radius: 0.0,
            badge_style: BadgeStyle::Filled,
            badge_sequence: BadgeSequence::Number,
            arrow_style: ArrowStyle::Solid,
            start_arrow_head: ArrowHeadStyle::None,
            end_arrow_head: ArrowHeadStyle::Triangle,
            text_bold: false,
            text_italic: false,
            text_stroke_width: 0.0,
            text_background_enabled: false,
            text_background_opacity: 160,
            text_background_padding: 4.0,
            text_background_corner_radius: 4.0,
            font_family: None,
            marker_shape: MarkerShape::Stroke,
            marker_blend_mode: MarkerBlendMode::Multiply,
            eraser_rectangle_mode: false,
            spotlight_intensity: 160,
            line_mode: LineMode::Straight,
            line_arrow_enabled: true,
            mosaic_mode: MosaicMode::Mosaic,
            mosaic_brush_mode: false,
            effect_strength: 60,
            spotlight_shape: SpotlightShape::Rectangle,
            spotlight_outline: true,
            magnifier_shape: MagnifierShape::Rectangle,
            magnifier_scale: 2.0,
            magnifier_smooth: true,
            magnifier_shadow: true,
            magnifier_connector: MagnifierConnector::Solid,
        }
    }
}

impl SubToolOptions {
    /// 获取当前生效的笔触粗细像素值
    pub fn current_stroke_width(&self) -> f32 {
        self.custom_stroke_width
    }
}

/// 获取预设调色板列表，顺序与参考工具栏的十个色块一致。
pub fn preset_colors() -> [Color; 10] {
    [
        Color::from_rgba8(239, 68, 68, 255),   // 红
        Color::from_rgba8(249, 115, 22, 255),  // 橙
        Color::from_rgba8(250, 204, 21, 255),  // 黄
        Color::from_rgba8(34, 197, 94, 255),   // 绿
        Color::from_rgba8(14, 165, 233, 255),  // 青蓝
        Color::from_rgba8(59, 130, 246, 255),  // 蓝
        Color::from_rgba8(168, 85, 247, 255),  // 紫
        Color::from_rgba8(24, 24, 27, 255),    // 黑
        Color::from_rgba8(255, 255, 255, 255), // 白
        Color::from_rgba8(148, 163, 184, 255), // 灰
    ]
}

/// 返回与当前颜色最接近的预设色下标。
///
/// 旧配置可能保存了参考调色板的邻近历史色值，例如 `#FF3B30`。工具栏只展示预设色，
/// 因此用最接近项绘制选中圈，避免配置升级后出现“没有任何颜色被选中”的状态。
pub fn closest_preset_color_index(color: Color) -> usize {
    let target = [color.red(), color.green(), color.blue(), color.alpha()];
    preset_colors()
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| {
            color_distance_squared(**left, target)
                .total_cmp(&color_distance_squared(**right, target))
        })
        .map(|(index, _)| index)
        .unwrap_or(0)
}

fn color_distance_squared(color: Color, target: [f32; 4]) -> f32 {
    let channels = [color.red(), color.green(), color.blue(), color.alpha()];
    channels
        .into_iter()
        .zip(target)
        .map(|(actual, expected)| (actual - expected).powi(2))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 默认红和历史默认红都应稳定显示第一个预设色的选中圈。
    #[test]
    fn test_closest_preset_color_index_selects_red_for_default_values() {
        assert_eq!(
            closest_preset_color_index(Color::from_rgba8(239, 68, 68, 255)),
            0
        );
        assert_eq!(
            closest_preset_color_index(Color::from_rgba8(255, 59, 48, 255)),
            0
        );
    }
}
