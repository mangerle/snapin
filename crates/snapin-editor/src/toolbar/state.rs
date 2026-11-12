//! 工具栏整体状态、默认值与截图主栏按钮序列。

use super::items::{tool_descriptor, SubToolOptions, Tool, ToolbarAction, ToolbarItem};
use super::layout::{SubControl, SubDropdown, SubLayout};
use crate::geometry::Rect;
use std::sync::{Mutex, MutexGuard};

/// 连续截图之间复用的标注工具栏选项；仅在用户开启记忆开关时写入。
static REMEMBERED_SUB_OPTIONS: Mutex<Option<SubToolOptions>> = Mutex::new(None);

/// 读取上次截图的工具栏选项。
pub fn remembered_sub_options() -> Option<SubToolOptions> {
    lock_remembered_options().as_ref().copied()
}

/// 保存当前工具栏选项，供下一次截图恢复。
pub(crate) fn store_remembered_sub_options(options: SubToolOptions) {
    *lock_remembered_options() = Some(options);
}

/// 清除跨截图工具栏选项。
pub fn clear_remembered_sub_options() {
    *lock_remembered_options() = None;
}

/// 工具栏记忆是独立的小型全局值，锁中毒后内部 Option 仍然完整，可直接恢复。
fn lock_remembered_options() -> MutexGuard<'static, Option<SubToolOptions>> {
    REMEMBERED_SUB_OPTIONS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

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
    /// 当前鼠标悬停的二级栏控件
    pub hovered_sub_control: Option<SubControl>,
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
    /// 最近点击的二级栏控件；用于点击后继续用滚轮调节数值
    pub(super) focused_sub_control: Option<SubControl>,
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
    /// 悬浮提示是否附带快捷键。
    ///
    /// 截图侧已实现 Ctrl+Z/Ctrl+C 等键位；贴图窗口的键盘绑定不同且更少，
    /// 打开该开关会展示并不存在的快捷键，因此在贴图工具栏中关闭。
    pub show_shortcut_hints: bool,
    /// 是否将当前标注工具栏选项复用到下一次截图。
    pub remember_sub_options: bool,
    /// 马赛克模式下拉中是否允许展示与触发“智能打码”（长图滚动编辑态关闭）。
    pub smart_redact_enabled: bool,
}

impl Default for Toolbar {
    fn default() -> Self {
        Self {
            ui_scale: 1.0,
            main_items: Self::MAIN_ITEMS,
            active_tool: None,
            hovered_item: None,
            hovered_sub_control: None,
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
            focused_sub_control: None,
            dropdown_hover: None,
            screen_size: (0.0, 0.0),
            draggable: false,
            manual_pos: None,
            drag_offset: None,
            show_shortcut_hints: true,
            remember_sub_options: false,
            smart_redact_enabled: true,
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
        ToolbarItem::Tool(Tool::Line),
        ToolbarItem::Tool(Tool::Marker),
        ToolbarItem::Tool(Tool::Mosaic),
        ToolbarItem::Tool(Tool::Magnifier),
        ToolbarItem::Tool(Tool::Spotlight),
        ToolbarItem::Tool(Tool::Eraser),
        ToolbarItem::Tool(Tool::Text),
        ToolbarItem::Action(ToolbarAction::Qr),
        ToolbarItem::Action(ToolbarAction::Undo),
        ToolbarItem::Action(ToolbarAction::Redo),
        ToolbarItem::Action(ToolbarAction::Ocr),
        ToolbarItem::Action(ToolbarAction::Translate),
        ToolbarItem::Action(ToolbarAction::ScreenRecording),
        ToolbarItem::Action(ToolbarAction::LongCapture),
        ToolbarItem::Action(ToolbarAction::Close),
        ToolbarItem::Action(ToolbarAction::Pin),
        ToolbarItem::Action(ToolbarAction::Save),
        ToolbarItem::Action(ToolbarAction::Copy),
    ];

    /// 长截图编辑态主工具列表：仅保留标注工具、撤销重做与贴图/保存/复制/关闭动作。
    ///
    /// 之所以剔除二维码/OCR/翻译/录屏/重入长截图，是因为这些动作依赖静态全屏底图或与长图会话互斥，
    /// 直接暴露给用户会产生坐标错乱或无效阻塞。
    pub const LONG_CAPTURE_EDIT_MAIN_ITEMS: &'static [ToolbarItem] = &[
        ToolbarItem::Tool(Tool::Rect),
        ToolbarItem::Tool(Tool::Badge),
        ToolbarItem::Tool(Tool::Arrow),
        ToolbarItem::Tool(Tool::Pen),
        ToolbarItem::Tool(Tool::Line),
        ToolbarItem::Tool(Tool::Marker),
        ToolbarItem::Tool(Tool::Mosaic),
        ToolbarItem::Tool(Tool::Magnifier),
        ToolbarItem::Tool(Tool::Spotlight),
        ToolbarItem::Tool(Tool::Eraser),
        ToolbarItem::Tool(Tool::Text),
        ToolbarItem::Action(ToolbarAction::Undo),
        ToolbarItem::Action(ToolbarAction::Redo),
        ToolbarItem::Action(ToolbarAction::Close),
        ToolbarItem::Action(ToolbarAction::Pin),
        ToolbarItem::Action(ToolbarAction::Save),
        ToolbarItem::Action(ToolbarAction::Copy),
    ];

    /// 主栏按钮在当前状态下实际展示的项：矩形与椭圆共用同一个几何槽位。
    ///
    /// 图标渲染与悬浮提示的名称都必须经由此处解析，否则椭圆激活时
    /// 提示仍会显示"矩形"，与用户看到的图标不符。
    pub(crate) fn displayed_item(&self, item: ToolbarItem) -> ToolbarItem {
        let is_geometric_slot = item
            .as_tool()
            .is_some_and(|tool| tool_descriptor(tool).geometric_slot);
        if is_geometric_slot && self.active_tool == Some(Tool::Ellipse) {
            ToolbarItem::Tool(Tool::Ellipse)
        } else {
            item
        }
    }
}
