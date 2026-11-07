//! 二级栏控件、下拉菜单与布局结果的数据契约。

use crate::geometry::Rect;
use crate::toolbar::items::Tool;

/// 二级栏下拉菜单种类
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubDropdown {
    /// 描边线型（实线/虚线/点线）
    Line,
    /// 序号底板样式（实心/空心）
    Badge,
    /// 箭头形态样式（实线/虚线/锥形）
    Arrow,
    /// 文字字号预设
    FontSize,
    /// 起点箭头
    StartArrow,
    /// 终点箭头
    EndArrow,
    /// 序号格式
    BadgeSequence,
    /// 马赛克、模糊与智能打码模式
    MosaicMode,
    /// 字体族
    FontFamily,
    /// 放大镜连接线
    MagnifierConnector,
    /// 荧光笔绘制形状
    MarkerShape,
    /// 荧光笔混合模式
    MarkerBlendMode,
}

/// 二级栏可交互控件
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubControl {
    ShapeRect,
    ShapeEllipse,
    /// 当前工具的连续尺寸数值
    Size,
    /// 马赛克与模糊效果强度
    EffectStrength,
    LineStyle,
    FillToggle,
    CornerRadius,
    BadgeStyle,
    ArrowStyle,
    /// 预设颜色，参数为调色板下标
    Color(usize),
    /// 文字字号
    FontSize,
    /// 起点箭头样式
    StartArrow,
    /// 终点箭头样式
    EndArrow,
    Bold,
    Italic,
    TextStroke,
    TextBackground,
    BadgeSequence,
    MarkerShape,
    MarkerBlendMode,
    /// 橡皮擦画笔模式
    EraserBrush,
    /// 橡皮擦框选模式
    EraserRectangle,
    ClearAnnotations,
    /// 直线与折线模式
    LineMode,
    /// 直线端点箭头总开关
    ArrowEnabled,
    /// 马赛克画笔模式
    MosaicBrush,
    /// 马赛克矩形模式
    MosaicRectangle,
    /// 马赛克、模糊与智能打码模式
    MosaicMode,
    /// 字体族
    FontFamily,
    /// 文字背景不透明度
    TextBackgroundOpacity,
    /// 文字背景内边距
    TextBackgroundPadding,
    /// 文字背景圆角
    TextBackgroundCornerRadius,
    /// 聚光灯矩形聚焦区域
    SpotlightShapeRect,
    /// 聚光灯椭圆聚焦区域
    SpotlightShapeEllipse,
    /// 聚光灯外围遮罩强度
    SpotlightIntensity,
    /// 聚光灯描边开关
    SpotlightOutline,
    /// 放大镜矩形区域
    MagnifierShapeRect,
    /// 放大镜椭圆区域
    MagnifierShapeEllipse,
    /// 放大镜倍率
    MagnifierScale,
    /// 放大镜平滑开关
    MagnifierSmooth,
    /// 放大镜阴影开关
    MagnifierShadow,
    /// 放大镜连接线样式
    MagnifierConnector,
}

impl SubControl {
    /// 悬浮提示中展示的控件名称。
    pub fn label(self) -> &'static str {
        match self {
            Self::ShapeRect | Self::SpotlightShapeRect | Self::MagnifierShapeRect => "矩形",
            Self::ShapeEllipse | Self::SpotlightShapeEllipse | Self::MagnifierShapeEllipse => {
                "椭圆"
            }
            Self::Size => "粗细",
            Self::EffectStrength => "效果强度",
            Self::LineStyle => "线型",
            Self::FillToggle => "填充",
            Self::CornerRadius | Self::TextBackgroundCornerRadius => "圆角",
            Self::BadgeStyle => "序号形态",
            Self::ArrowStyle => "箭头样式",
            Self::Color(_) => "颜色",
            Self::FontSize => "字号",
            Self::StartArrow => "起点箭头",
            Self::EndArrow => "终点箭头",
            Self::Bold => "粗体",
            Self::Italic => "斜体",
            Self::TextStroke => "文字描边",
            Self::TextBackground => "文字背景",
            Self::BadgeSequence => "数字形态",
            Self::MarkerShape => "绘制模式",
            Self::EraserBrush => "橡皮擦画笔",
            Self::EraserRectangle => "橡皮擦框选",
            Self::MosaicBrush => "画笔模式",
            Self::MosaicRectangle => "矩形模式",
            Self::MarkerBlendMode => "混合模式",
            Self::ClearAnnotations => "清空全部标注",
            Self::LineMode => "直线或折线",
            Self::ArrowEnabled => "箭头开关",
            Self::MosaicMode => "遮挡模式",
            Self::FontFamily => "字体",
            Self::TextBackgroundOpacity => "背景不透明度",
            Self::TextBackgroundPadding => "背景内边距",
            Self::SpotlightIntensity => "遮罩强度",
            Self::SpotlightOutline => "描边",
            Self::MagnifierScale => "放大倍率",
            Self::MagnifierSmooth => "抗锯齿",
            Self::MagnifierShadow => "阴影",
            Self::MagnifierConnector => "连接线",
        }
    }

    /// 根据当前工具补充更准确的悬浮提示名称。
    pub fn label_for_tool(self, tool: Option<Tool>) -> &'static str {
        match (self, tool) {
            (Self::Size, Some(Tool::Badge)) => "序号大小",
            (Self::Size, Some(Tool::Eraser)) => "橡皮擦大小",
            (Self::Size, Some(Tool::Magnifier)) => "边框粗细",
            _ => self.label(),
        }
    }
}

/// 二级栏中的一个布局槽位（控件或纯视觉分隔线）
#[derive(Debug, Clone, Copy)]
pub struct SubEntry {
    /// `None` 表示分隔线等不可交互的占位
    pub control: Option<SubControl>,
    pub rect: Rect,
}

/// 下拉菜单布局
#[derive(Debug, Clone)]
pub struct DropdownLayout {
    pub kind: SubDropdown,
    pub rect: Rect,
    /// 每一行选项的热区，顺序与渲染顺序一致
    pub rows: Vec<Rect>,
}

/// 二级栏完整布局：面板、控件序列与可选的下拉菜单
#[derive(Debug, Clone)]
pub struct SubLayout {
    pub rect: Rect,
    pub entries: Vec<SubEntry>,
    pub dropdown: Option<DropdownLayout>,
}

impl SubLayout {
    /// 命中某个可交互控件
    pub fn hit(&self, px: f32, py: f32) -> Option<SubControl> {
        self.entries
            .iter()
            .find(|entry| entry.rect.contains(px, py))
            .and_then(|entry| entry.control)
    }

    /// 获取指定控件的热区
    pub fn rect_of(&self, control: SubControl) -> Option<Rect> {
        self.entries
            .iter()
            .find(|entry| entry.control == Some(control))
            .map(|entry| entry.rect)
    }

    /// 下拉菜单是否包含该坐标（含面板与行）
    pub fn dropdown_contains(&self, px: f32, py: f32) -> bool {
        self.dropdown
            .as_ref()
            .is_some_and(|dropdown| dropdown.rect.contains(px, py))
    }

    /// 命中下拉菜单中的第几行
    pub fn dropdown_row_hit(&self, px: f32, py: f32) -> Option<usize> {
        let dropdown = self.dropdown.as_ref()?;
        dropdown.rows.iter().position(|row| row.contains(px, py))
    }
}
