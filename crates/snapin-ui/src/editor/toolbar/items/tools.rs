//! 标注工具标识、行为描述与工具栏布局元数据。

/// 直线工具交互方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineMode {
    /// 按下并拖动，松开后生成单段直线。
    Straight,
    /// 连续单击追加节点，右键或回车结束折线。
    Polyline,
}

/// 马赛克工具的处理模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MosaicMode {
    /// 像素块马赛克。
    Mosaic,
    /// 高斯模糊。
    Blur,
    /// OCR 识别敏感文字后进入智能打码。
    SmartRedact,
}

impl MosaicMode {
    /// 二级栏展示名称。
    pub const fn label(self) -> &'static str {
        match self {
            Self::Mosaic => "马赛克",
            Self::Blur => "模糊",
            Self::SmartRedact => "智能打码",
        }
    }
}

/// 可激活的标注工具。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    /// 矩形或椭圆标注
    Rect,
    /// 椭圆标注
    Ellipse,
    /// 箭头标注
    Arrow,
    /// 自由画笔
    Pen,
    /// 直线与折线
    Line,
    /// 荧光记号笔
    Marker,
    /// 文本输入
    Text,
    /// 步骤数字序号气泡
    Badge,
    /// 马赛克打码
    Mosaic,
    /// 局部高斯模糊遮挡
    Blur,
    /// 聚光灯遮罩
    Spotlight,
    /// 区域放大镜
    Magnifier,
    /// 橡皮擦
    Eraser,
}

/// 工具栏工具的描述元数据。
///
/// 二级属性栏控件组合、主栏几何入口、文字字号与聚光灯强度等差异集中在此，
/// 布局、命中与渲染不再各自维护工具特例。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolDescriptor {
    /// 工具标识
    pub tool: Tool,
    /// 主栏是否作为矩形/椭圆几何入口显示
    pub geometric_slot: bool,
    /// 是否提供二级属性栏
    pub has_sub_bar: bool,
    /// 是否提供笔触或遮挡尺寸控件
    pub stroke_width: bool,
    /// 是否提供矩形/椭圆形状切换
    pub shape_controls: bool,
    /// 是否提供圆角半径控件
    pub corner_radius: bool,
    /// 是否提供线型控件
    pub line_style: bool,
    /// 是否提供填充开关
    pub fill_toggle: bool,
    /// 是否提供序号底板样式
    pub badge_style: bool,
    /// 是否提供序号格式
    pub badge_sequence: bool,
    /// 是否提供箭头形态
    pub arrow_style: bool,
    /// 是否提供起点与终点箭头控件
    pub arrow_heads: bool,
    /// 是否提供预设调色板
    pub color_palette: bool,
    /// 是否提供智能打码入口
    pub smart_redact: bool,
    /// 是否提供字号控件
    pub font_size: bool,
    /// 是否提供文字富文本控件
    pub text_style: bool,
    /// 是否提供荧光笔模式控件
    pub marker_mode: bool,
    /// 是否提供橡皮擦模式控件
    pub eraser_mode: bool,
    /// 是否提供聚光灯强度控件
    pub spotlight_intensity: bool,
    /// 是否提供直线/折线模式控件
    pub line_mode: bool,
    /// 是否提供箭头总开关
    pub arrow_toggle: bool,
    /// 是否提供马赛克、模糊与智能打码模式
    pub mosaic_mode: bool,
    /// 是否提供放大镜倍率、平滑与连接线设置
    pub magnifier_options: bool,
    /// 是否提供聚光灯形状与描边设置
    pub spotlight_style: bool,
}

/// 工具描述表；新增工具时在此补齐一条并同步图标。
///
/// 主栏按钮顺序仍由截图/贴图各自的显式序列维护：两套工具栏的动作集合不同，
/// 直接由描述表推导会迫使动作也进入工具模型，反而把命令与工具耦合得更紧。
/// 描述表负责控件组合与几何入口等行为差异，按钮顺序作为可见 UX 契约显式列出。
pub const TOOL_DESCRIPTORS: &[ToolDescriptor] = &[
    ToolDescriptor {
        tool: Tool::Rect,
        geometric_slot: true,
        has_sub_bar: true,
        stroke_width: true,
        shape_controls: true,
        corner_radius: true,
        line_style: true,
        fill_toggle: true,
        badge_style: false,
        badge_sequence: false,
        arrow_style: false,
        arrow_heads: false,
        color_palette: true,
        smart_redact: false,
        font_size: false,
        text_style: false,
        marker_mode: false,
        eraser_mode: false,
        spotlight_intensity: false,
        line_mode: false,
        arrow_toggle: false,
        mosaic_mode: false,
        magnifier_options: false,
        spotlight_style: false,
    },
    ToolDescriptor {
        tool: Tool::Ellipse,
        geometric_slot: true,
        has_sub_bar: true,
        stroke_width: true,
        shape_controls: true,
        corner_radius: false,
        line_style: true,
        fill_toggle: true,
        badge_style: false,
        badge_sequence: false,
        arrow_style: false,
        arrow_heads: false,
        color_palette: true,
        smart_redact: false,
        font_size: false,
        text_style: false,
        marker_mode: false,
        eraser_mode: false,
        spotlight_intensity: false,
        line_mode: false,
        arrow_toggle: false,
        mosaic_mode: false,
        magnifier_options: false,
        spotlight_style: false,
    },
    ToolDescriptor {
        tool: Tool::Arrow,
        geometric_slot: false,
        has_sub_bar: true,
        stroke_width: true,
        shape_controls: false,
        corner_radius: false,
        line_style: true,
        fill_toggle: false,
        badge_style: false,
        badge_sequence: false,
        arrow_style: true,
        arrow_heads: true,
        color_palette: true,
        smart_redact: false,
        font_size: false,
        text_style: false,
        marker_mode: false,
        eraser_mode: false,
        spotlight_intensity: false,
        line_mode: false,
        arrow_toggle: false,
        mosaic_mode: false,
        magnifier_options: false,
        spotlight_style: false,
    },
    ToolDescriptor {
        tool: Tool::Pen,
        geometric_slot: false,
        has_sub_bar: true,
        stroke_width: true,
        shape_controls: false,
        corner_radius: false,
        line_style: true,
        fill_toggle: false,
        badge_style: false,
        badge_sequence: false,
        arrow_style: false,
        arrow_heads: false,
        color_palette: true,
        smart_redact: false,
        font_size: false,
        text_style: false,
        marker_mode: false,
        eraser_mode: false,
        spotlight_intensity: false,
        line_mode: false,
        arrow_toggle: false,
        mosaic_mode: false,
        magnifier_options: false,
        spotlight_style: false,
    },
    ToolDescriptor {
        tool: Tool::Line,
        geometric_slot: false,
        has_sub_bar: true,
        stroke_width: true,
        shape_controls: false,
        corner_radius: false,
        line_style: true,
        fill_toggle: false,
        badge_style: false,
        badge_sequence: false,
        arrow_style: false,
        arrow_heads: true,
        color_palette: true,
        smart_redact: false,
        font_size: false,
        text_style: false,
        marker_mode: false,
        eraser_mode: false,
        spotlight_intensity: false,
        line_mode: true,
        arrow_toggle: true,
        mosaic_mode: false,
        magnifier_options: false,
        spotlight_style: false,
    },
    ToolDescriptor {
        tool: Tool::Marker,
        geometric_slot: false,
        has_sub_bar: true,
        stroke_width: true,
        shape_controls: false,
        corner_radius: false,
        line_style: false,
        fill_toggle: false,
        badge_style: false,
        badge_sequence: false,
        arrow_style: false,
        arrow_heads: false,
        color_palette: true,
        smart_redact: false,
        font_size: false,
        text_style: false,
        marker_mode: true,
        eraser_mode: false,
        spotlight_intensity: false,
        line_mode: false,
        arrow_toggle: false,
        mosaic_mode: false,
        magnifier_options: false,
        spotlight_style: false,
    },
    ToolDescriptor {
        tool: Tool::Text,
        geometric_slot: false,
        has_sub_bar: true,
        stroke_width: false,
        shape_controls: false,
        corner_radius: false,
        line_style: false,
        fill_toggle: false,
        badge_style: false,
        badge_sequence: false,
        arrow_style: false,
        arrow_heads: false,
        color_palette: true,
        smart_redact: false,
        font_size: true,
        text_style: true,
        marker_mode: false,
        eraser_mode: false,
        spotlight_intensity: false,
        line_mode: false,
        arrow_toggle: false,
        mosaic_mode: false,
        magnifier_options: false,
        spotlight_style: false,
    },
    ToolDescriptor {
        tool: Tool::Badge,
        geometric_slot: false,
        has_sub_bar: true,
        stroke_width: true,
        shape_controls: false,
        corner_radius: false,
        line_style: false,
        fill_toggle: false,
        badge_style: true,
        badge_sequence: true,
        arrow_style: false,
        arrow_heads: false,
        color_palette: true,
        smart_redact: false,
        font_size: false,
        text_style: false,
        marker_mode: false,
        eraser_mode: false,
        spotlight_intensity: false,
        line_mode: false,
        arrow_toggle: false,
        mosaic_mode: false,
        magnifier_options: false,
        spotlight_style: false,
    },
    ToolDescriptor {
        tool: Tool::Mosaic,
        geometric_slot: false,
        has_sub_bar: true,
        stroke_width: true,
        shape_controls: false,
        corner_radius: false,
        line_style: false,
        fill_toggle: false,
        badge_style: false,
        badge_sequence: false,
        arrow_style: false,
        arrow_heads: false,
        color_palette: false,
        smart_redact: true,
        font_size: false,
        text_style: false,
        marker_mode: false,
        eraser_mode: false,
        spotlight_intensity: false,
        line_mode: false,
        arrow_toggle: false,
        mosaic_mode: true,
        magnifier_options: false,
        spotlight_style: false,
    },
    ToolDescriptor {
        tool: Tool::Blur,
        geometric_slot: false,
        has_sub_bar: true,
        stroke_width: true,
        shape_controls: false,
        corner_radius: false,
        line_style: false,
        fill_toggle: false,
        badge_style: false,
        badge_sequence: false,
        arrow_style: false,
        arrow_heads: false,
        color_palette: false,
        smart_redact: false,
        font_size: false,
        text_style: false,
        marker_mode: false,
        eraser_mode: false,
        spotlight_intensity: false,
        line_mode: false,
        arrow_toggle: false,
        mosaic_mode: false,
        magnifier_options: false,
        spotlight_style: false,
    },
    ToolDescriptor {
        tool: Tool::Spotlight,
        geometric_slot: false,
        has_sub_bar: true,
        stroke_width: true,
        shape_controls: false,
        corner_radius: false,
        line_style: true,
        fill_toggle: false,
        badge_style: false,
        badge_sequence: false,
        arrow_style: false,
        arrow_heads: false,
        color_palette: true,
        smart_redact: false,
        font_size: false,
        text_style: false,
        marker_mode: false,
        eraser_mode: false,
        spotlight_intensity: true,
        line_mode: false,
        arrow_toggle: false,
        mosaic_mode: false,
        magnifier_options: false,
        spotlight_style: true,
    },
    ToolDescriptor {
        tool: Tool::Magnifier,
        geometric_slot: false,
        has_sub_bar: true,
        stroke_width: true,
        shape_controls: false,
        corner_radius: false,
        line_style: false,
        fill_toggle: false,
        badge_style: false,
        badge_sequence: false,
        arrow_style: false,
        arrow_heads: false,
        color_palette: true,
        smart_redact: false,
        font_size: false,
        text_style: false,
        marker_mode: false,
        eraser_mode: false,
        spotlight_intensity: false,
        line_mode: false,
        arrow_toggle: false,
        mosaic_mode: false,
        magnifier_options: true,
        spotlight_style: false,
    },
    ToolDescriptor {
        tool: Tool::Eraser,
        geometric_slot: false,
        has_sub_bar: true,
        stroke_width: true,
        shape_controls: false,
        corner_radius: false,
        line_style: false,
        fill_toggle: false,
        badge_style: false,
        badge_sequence: false,
        arrow_style: false,
        arrow_heads: false,
        color_palette: false,
        smart_redact: false,
        font_size: false,
        text_style: false,
        marker_mode: false,
        eraser_mode: true,
        spotlight_intensity: false,
        line_mode: false,
        arrow_toggle: false,
        mosaic_mode: false,
        magnifier_options: false,
        spotlight_style: false,
    },
];

/// 按工具查找描述元数据；表必须覆盖全部工具。
///
/// # Panics
///
/// `TOOL_DESCRIPTORS` 是受控静态表，并由 `test_tool_descriptors_cover_all_tools`
/// 校验覆盖全部 [`Tool`] 变体；若新增工具却遗漏描述项，本函数会立即 panic 暴露开发错误。
pub fn tool_descriptor(tool: Tool) -> &'static ToolDescriptor {
    TOOL_DESCRIPTORS
        .iter()
        .find(|descriptor| descriptor.tool == tool)
        .expect("工具描述表必须覆盖全部 Tool 变体")
}
