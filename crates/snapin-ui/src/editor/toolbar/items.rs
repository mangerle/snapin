//! 工具栏按钮动作、笔触粗细与颜色选项数据契约。

use snapin_core::annotation::{ArrowStyle, BadgeStyle, LineStyle};
use tiny_skia::Color;

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
    /// 是否提供箭头样式
    pub arrow_style: bool,
    /// 是否提供预设调色板
    pub color_palette: bool,
    /// 是否提供智能打码入口
    pub smart_redact: bool,
    /// 是否提供字号控件
    pub font_size: bool,
    /// 是否提供聚光灯强度控件
    pub spotlight_intensity: bool,
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
        arrow_style: false,
        color_palette: true,
        smart_redact: false,
        font_size: false,
        spotlight_intensity: false,
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
        arrow_style: false,
        color_palette: true,
        smart_redact: false,
        font_size: false,
        spotlight_intensity: false,
    },
    ToolDescriptor {
        tool: Tool::Arrow,
        geometric_slot: false,
        has_sub_bar: true,
        stroke_width: true,
        shape_controls: false,
        corner_radius: false,
        line_style: false,
        fill_toggle: false,
        badge_style: false,
        arrow_style: true,
        color_palette: true,
        smart_redact: false,
        font_size: false,
        spotlight_intensity: false,
    },
    ToolDescriptor {
        tool: Tool::Pen,
        geometric_slot: false,
        has_sub_bar: true,
        stroke_width: true,
        shape_controls: false,
        corner_radius: false,
        line_style: false,
        fill_toggle: false,
        badge_style: false,
        arrow_style: false,
        color_palette: true,
        smart_redact: false,
        font_size: false,
        spotlight_intensity: false,
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
        arrow_style: false,
        color_palette: true,
        smart_redact: false,
        font_size: false,
        spotlight_intensity: false,
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
        arrow_style: false,
        color_palette: true,
        smart_redact: false,
        font_size: true,
        spotlight_intensity: false,
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
        arrow_style: false,
        color_palette: true,
        smart_redact: false,
        font_size: false,
        spotlight_intensity: false,
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
        arrow_style: false,
        color_palette: false,
        smart_redact: true,
        font_size: false,
        spotlight_intensity: false,
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
        arrow_style: false,
        color_palette: false,
        smart_redact: false,
        font_size: false,
        spotlight_intensity: false,
    },
    ToolDescriptor {
        tool: Tool::Spotlight,
        geometric_slot: false,
        has_sub_bar: true,
        stroke_width: false,
        shape_controls: false,
        corner_radius: false,
        line_style: false,
        fill_toggle: false,
        badge_style: false,
        arrow_style: false,
        color_palette: false,
        smart_redact: false,
        font_size: false,
        spotlight_intensity: true,
    },
];

/// 按工具查找描述元数据；表必须覆盖全部工具。
pub fn tool_descriptor(tool: Tool) -> &'static ToolDescriptor {
    TOOL_DESCRIPTORS
        .iter()
        .find(|descriptor| descriptor.tool == tool)
        .expect("工具描述表必须覆盖全部 Tool 变体")
}

/// 工具栏一次性命令动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolbarAction {
    /// 撤销上一步
    Undo,
    /// 重做
    Redo,
    /// OCR 文字识别并复制到剪贴板
    Ocr,
    /// 识别全部文字并进入马赛克/模糊预览
    SmartRedact,
    /// 识别选区中的二维码
    Qr,
    /// 截图原位翻译
    Translate,
    /// 切换贴图鼠标穿透（仅贴图工具栏使用）
    MouseThrough,
    /// 贴图置顶到屏幕 (F3)
    Pin,
    /// 复制到剪贴板并退出
    Copy,
    /// 保存为图片文件
    Save,
    /// 取消并关闭截图
    Close,
}

/// 工具栏主按钮：工具激活与一次性动作在类型层面分离。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolbarItem {
    /// 可保持激活状态的标注工具
    Tool(Tool),
    /// 点击即执行的命令动作
    Action(ToolbarAction),
}

impl ToolbarItem {
    /// 构造工具按钮。
    pub const fn tool(tool: Tool) -> Self {
        Self::Tool(tool)
    }

    /// 构造动作按钮。
    pub const fn action(action: ToolbarAction) -> Self {
        Self::Action(action)
    }

    /// 提取工具；动作按钮返回 `None`。
    pub const fn as_tool(self) -> Option<Tool> {
        match self {
            Self::Tool(tool) => Some(tool),
            Self::Action(_) => None,
        }
    }

    /// 悬浮提示中展示的中文名称。
    pub const fn label(self) -> &'static str {
        match self {
            Self::Tool(tool) => tool.label(),
            Self::Action(action) => action.label(),
        }
    }

    /// 悬浮提示中展示的快捷键；工具与无键位动作返回 `None`。
    pub const fn shortcut(self) -> Option<&'static str> {
        match self {
            Self::Tool(_) => None,
            Self::Action(action) => action.shortcut(),
        }
    }
}

impl Tool {
    /// 工具中文名称，用于悬浮提示。
    pub const fn label(self) -> &'static str {
        match self {
            Self::Rect => "矩形",
            Self::Ellipse => "椭圆",
            Self::Arrow => "箭头",
            Self::Pen => "画笔",
            Self::Marker => "荧光笔",
            Self::Text => "文字",
            Self::Badge => "序号",
            Self::Mosaic => "马赛克与智能打码",
            Self::Blur => "模糊",
            Self::Spotlight => "聚光灯",
        }
    }
}

impl ToolbarAction {
    /// 动作中文名称，用于悬浮提示。
    pub const fn label(self) -> &'static str {
        match self {
            Self::Undo => "撤销",
            Self::Redo => "重做",
            Self::Ocr => "文字识别",
            Self::SmartRedact => "智能打码",
            Self::Qr => "二维码",
            Self::Translate => "翻译",
            Self::MouseThrough => "鼠标穿透",
            Self::Pin => "贴图置顶",
            Self::Copy => "复制",
            Self::Save => "保存",
            Self::Close => "关闭",
        }
    }

    /// 截图会话中真实生效的快捷键。
    ///
    /// 贴图窗口键位与截图不同，由 `Toolbar::show_shortcut_hints` 决定是否展示；
    /// 这里只描述截图侧已经实现的绑定，不能凭按钮语义臆造键位。
    pub const fn shortcut(self) -> Option<&'static str> {
        match self {
            Self::Undo => Some("Ctrl+Z"),
            Self::Redo => Some("Ctrl+Y"),
            Self::Ocr => Some("O"),
            Self::SmartRedact => Some("R"),
            Self::Qr => Some("Q"),
            Self::Translate => Some("T"),
            Self::Pin => Some("F3"),
            Self::Copy => Some("Ctrl+C"),
            Self::Save => Some("Ctrl+S"),
            Self::Close => Some("Esc"),
            Self::MouseThrough => None,
        }
    }
}

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

/// 圆角半径预设档位（像素），顺序与圆角下拉菜单一致
pub const CORNER_RADIUS_PRESETS: &[f32] = &[0.0, 6.0, 12.0, 20.0];

/// 文字字号预设（像素），顺序与字号下拉菜单一致。
pub const FONT_SIZE_PRESETS: &[f32] = &[12.0, 16.0, 20.0, 24.0, 32.0, 48.0];

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
    /// 箭头形态样式
    pub arrow_style: ArrowStyle,
    /// 聚光灯压暗强度（RGBA alpha）
    pub spotlight_intensity: u8,
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
            arrow_style: ArrowStyle::Tapered,
            spotlight_intensity: 160,
        }
    }
}

impl SubToolOptions {
    /// 获取当前生效的笔触粗细像素值
    pub fn current_stroke_width(&self) -> f32 {
        self.custom_stroke_width
    }
}

/// 获取预设调色板列表 (红、橙黄、绿、蓝、白、黑)
pub fn preset_colors() -> [Color; 6] {
    [
        Color::from_rgba8(239, 68, 68, 255),   // 红
        Color::from_rgba8(245, 158, 11, 255),  // 橙黄
        Color::from_rgba8(16, 185, 129, 255),  // 绿
        Color::from_rgba8(59, 130, 246, 255),  // 蓝
        Color::from_rgba8(255, 255, 255, 255), // 白
        Color::from_rgba8(24, 24, 27, 255),    // 黑
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 工具只在点击后保持激活，动作只执行一次，二者不得混用同一语义。
    #[test]
    fn test_toolbar_item_separates_tools_and_actions() {
        let tool = ToolbarItem::tool(Tool::Pen);
        assert_eq!(tool.as_tool(), Some(Tool::Pen));
        assert_eq!(
            ToolbarItem::tool(Tool::Ellipse).as_tool(),
            Some(Tool::Ellipse)
        );

        let action = ToolbarItem::action(ToolbarAction::Undo);
        assert_eq!(action.as_tool(), None);
        assert_eq!(ToolbarItem::action(ToolbarAction::Close).as_tool(), None);
    }

    /// 描述表必须覆盖全部工具，并保留聚光灯无属性栏、矩形几何入口等关键差异。
    #[test]
    fn test_tool_descriptors_cover_all_tools() {
        let tools = [
            Tool::Rect,
            Tool::Ellipse,
            Tool::Arrow,
            Tool::Pen,
            Tool::Marker,
            Tool::Text,
            Tool::Badge,
            Tool::Mosaic,
            Tool::Blur,
            Tool::Spotlight,
        ];
        assert_eq!(TOOL_DESCRIPTORS.len(), tools.len());
        for tool in tools {
            let descriptor = tool_descriptor(tool);
            assert_eq!(descriptor.tool, tool);
        }
        assert!(tool_descriptor(Tool::Rect).geometric_slot);
        assert!(tool_descriptor(Tool::Ellipse).geometric_slot);
        assert!(tool_descriptor(Tool::Spotlight).has_sub_bar);
        assert!(tool_descriptor(Tool::Spotlight).spotlight_intensity);
        assert!(tool_descriptor(Tool::Mosaic).smart_redact);
        assert!(tool_descriptor(Tool::Text).font_size);
        assert!(!tool_descriptor(Tool::Blur).color_palette);
        assert!(tool_descriptor(Tool::Badge).badge_style);
        assert!(tool_descriptor(Tool::Arrow).arrow_style);
    }

    /// 主栏必须同时包含可激活工具与一次性动作，且动作不会成为激活态。
    #[test]
    fn test_main_items_contain_both_tools_and_actions() {
        let tool_count = crate::editor::toolbar::Toolbar::MAIN_ITEMS
            .iter()
            .filter(|item| item.as_tool().is_some())
            .count();
        let action_count = crate::editor::toolbar::Toolbar::MAIN_ITEMS
            .iter()
            .filter(|item| matches!(item, ToolbarItem::Action(_)))
            .count();
        assert_eq!(tool_count, 9);
        assert_eq!(action_count, 9);

        let text_index = crate::editor::toolbar::Toolbar::MAIN_ITEMS
            .iter()
            .position(|item| *item == ToolbarItem::Tool(Tool::Text))
            .expect("文字工具必须存在");
        assert_eq!(
            crate::editor::toolbar::Toolbar::MAIN_ITEMS.get(text_index + 1),
            Some(&ToolbarItem::Action(ToolbarAction::Qr)),
            "二维码按钮必须紧跟文字工具"
        );
        assert!(
            !crate::editor::toolbar::Toolbar::MAIN_ITEMS
                .contains(&ToolbarItem::Action(ToolbarAction::SmartRedact)),
            "智能打码应合入马赛克二级栏，不能占据主栏"
        );
    }

    /// 每个工具与动作都必须有非空中文名称；工具没有键位，动作只声明真实存在的绑定。
    #[test]
    fn test_every_item_has_label_and_declared_shortcuts() {
        let tools = [
            Tool::Rect,
            Tool::Ellipse,
            Tool::Arrow,
            Tool::Pen,
            Tool::Marker,
            Tool::Text,
            Tool::Badge,
            Tool::Mosaic,
            Tool::Blur,
            Tool::Spotlight,
        ];
        for tool in tools {
            assert!(!tool.label().is_empty(), "{tool:?} 必须有中文名称");
            assert_eq!(ToolbarItem::tool(tool).shortcut(), None);
        }

        let actions = [
            ToolbarAction::Undo,
            ToolbarAction::Redo,
            ToolbarAction::Ocr,
            ToolbarAction::SmartRedact,
            ToolbarAction::Qr,
            ToolbarAction::Translate,
            ToolbarAction::MouseThrough,
            ToolbarAction::Pin,
            ToolbarAction::Copy,
            ToolbarAction::Save,
            ToolbarAction::Close,
        ];
        for action in actions {
            assert!(!action.label().is_empty(), "{action:?} 必须有中文名称");
        }
        assert_eq!(ToolbarAction::Undo.shortcut(), Some("Ctrl+Z"));
        assert_eq!(ToolbarAction::Copy.shortcut(), Some("Ctrl+C"));
        assert_eq!(ToolbarAction::MouseThrough.shortcut(), None);
    }
}
