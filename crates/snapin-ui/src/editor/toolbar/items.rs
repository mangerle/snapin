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

/// 工具栏一次性命令动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolbarAction {
    /// 撤销上一步
    Undo,
    /// 重做
    Redo,
    /// OCR 文字识别并复制到剪贴板
    Ocr,
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
        assert_eq!(action_count, 8);
    }
}
