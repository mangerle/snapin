//! 工具栏按钮动作、工具类型与二级属性选项的统一导出入口。

mod options;
mod tools;

pub use options::*;
pub use tools::*;

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
    /// 一键清空全部标注
    ClearAnnotations,
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
    pub fn label(self) -> &'static str {
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
    pub fn label(self) -> &'static str {
        snapin_foundation::i18n::tr(match self {
            Self::Rect => "矩形",
            Self::Ellipse => "椭圆",
            Self::Arrow => "箭头",
            Self::Pen => "画笔",
            Self::Line => "直线与折线",
            Self::Marker => "荧光笔",
            Self::Text => "文字",
            Self::Badge => "序号注释",
            Self::Mosaic => "马赛克、模糊与智能打码",
            Self::Blur => "模糊",
            Self::Spotlight => "聚光灯",
            Self::Magnifier => "放大镜",
            Self::Eraser => "橡皮擦",
        })
    }
}

impl ToolbarAction {
    /// 动作中文名称，用于悬浮提示。
    pub fn label(self) -> &'static str {
        snapin_foundation::i18n::tr(match self {
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
            Self::ClearAnnotations => "清空全部标注",
            Self::Close => "关闭",
        })
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
            Self::MouseThrough | Self::ClearAnnotations => None,
        }
    }
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
            Tool::Line,
            Tool::Marker,
            Tool::Text,
            Tool::Badge,
            Tool::Mosaic,
            Tool::Blur,
            Tool::Spotlight,
            Tool::Magnifier,
            Tool::Eraser,
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
        assert!(tool_descriptor(Tool::Magnifier).magnifier_options);
        assert!(tool_descriptor(Tool::Eraser).eraser_mode);
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
        assert_eq!(tool_count, 11);
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
            Tool::Magnifier,
            Tool::Eraser,
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
