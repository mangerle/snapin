//! 工具栏整体状态、默认值与截图主栏按钮序列。

use super::items::{SubToolOptions, Tool, ToolbarAction, ToolbarItem};
use super::layout::{SubDropdown, SubLayout};
use crate::geometry::Rect;

/// 工具栏整体状态与布局管理器
#[derive(Debug, Clone)]
pub struct Toolbar {
    /// 界面缩放倍率 (HiDPI 缩放因子)
    pub ui_scale: f32,
    /// 主栏按钮序列。
    ///
    /// 默认使用截图工具栏的完整按钮集；贴图窗口可注入自身的精简列表，
    /// 从而在不复制布局、命中与渲染代码的前提下复用同一套工具栏实现。
    /// 使用 `&'static` 切片是为了让工具栏保持轻量可克隆，且按钮序列在运行期恒定不变。
    pub main_items: &'static [ToolbarItem],
    /// 当前处于激活状态的标注工具 (None 表示未进入标注模式)
    pub active_tool: Option<Tool>,
    /// 当前鼠标悬停的项目
    pub hovered_item: Option<ToolbarItem>,
    /// 二级属性
    pub sub_options: SubToolOptions,
    /// 主工具栏当前在屏幕上的包围盒
    pub main_rect: Rect,
    /// 二级属性栏当前在屏幕上的包围盒 (若激活)
    pub sub_rect: Option<Rect>,
    /// 二级栏完整布局：控件热区与下拉菜单，渲染与命中共用
    pub(super) sub_layout: Option<SubLayout>,
    /// 当前展开的下拉菜单
    pub(crate) open_dropdown: Option<SubDropdown>,
    /// 下拉菜单当前悬停行
    pub(crate) dropdown_hover: Option<usize>,
    /// 屏幕物理尺寸，用于下拉菜单避让
    pub(super) screen_size: (f32, f32),
    /// 是否在主栏右端显示拖动手柄。
    ///
    /// 仅截图工具栏启用：截图工具栏按选区自动摆放，可能遮挡待标注区域，需要用户手动挪开；
    /// 贴图工具栏锚定在图像下方且窗口尺寸随之计算，不提供拖动。
    pub draggable: bool,
    /// 用户拖动后固定的主栏左上角；`None` 表示按选区自适应摆放
    pub(super) manual_pos: Option<(f32, f32)>,
    /// 拖动进行中时，光标相对主栏左上角的偏移；`None` 表示未在拖动
    pub(super) drag_offset: Option<(f32, f32)>,
}

impl Default for Toolbar {
    fn default() -> Self {
        Self {
            ui_scale: 1.0,
            main_items: Self::MAIN_ITEMS,
            active_tool: None,
            hovered_item: None,
            sub_options: SubToolOptions::default(),
            main_rect: Rect {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
            },
            sub_rect: None,
            sub_layout: None,
            open_dropdown: None,
            dropdown_hover: None,
            screen_size: (0.0, 0.0),
            draggable: false,
            manual_pos: None,
            drag_offset: None,
        }
    }
}

impl Toolbar {
    /// 主工具列表定义 (严格对齐经典工具栏规范)
    pub const MAIN_ITEMS: &'static [ToolbarItem] = &[
        ToolbarItem::Tool(Tool::Rect),
        ToolbarItem::Tool(Tool::Badge),
        ToolbarItem::Tool(Tool::Arrow),
        ToolbarItem::Tool(Tool::Pen),
        ToolbarItem::Tool(Tool::Marker),
        ToolbarItem::Tool(Tool::Mosaic),
        ToolbarItem::Tool(Tool::Blur),
        ToolbarItem::Tool(Tool::Spotlight),
        ToolbarItem::Tool(Tool::Text),
        ToolbarItem::Action(ToolbarAction::Undo),
        ToolbarItem::Action(ToolbarAction::Redo),
        ToolbarItem::Action(ToolbarAction::Ocr),
        ToolbarItem::Action(ToolbarAction::Translate),
        ToolbarItem::Action(ToolbarAction::Close),
        ToolbarItem::Action(ToolbarAction::Pin),
        ToolbarItem::Action(ToolbarAction::Save),
        ToolbarItem::Action(ToolbarAction::Copy),
    ];
}
