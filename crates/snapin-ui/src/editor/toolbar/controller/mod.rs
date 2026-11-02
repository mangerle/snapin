//! 工具栏交互控制器：布局摆放、拖动、命中测试与二级控件状态流转。
//!
//! 本模块只修改 [`Toolbar`] 状态并输出动作结果，不负责像素渲染；
//! 渲染层读取同一份布局结果，避免交互与绘制各维护一套坐标。

mod click;

use super::layout::{
    calculate_main_layout, calculate_sub_layout_with_options, SubLayout, SubLayoutOptions,
};
use super::{Tool, Toolbar, ToolbarItem};

use super::interaction::hit_test_main_items;
use super::metrics;
use super::state::store_remembered_sub_options;
use crate::geometry::Rect;

/// 将当前工具栏状态收敛为布局层需要的只读选项。
fn calculate_sub_layout_options(options: &super::items::SubToolOptions) -> SubLayoutOptions {
    SubLayoutOptions {
        mosaic_mode: options.mosaic_mode,
    }
}

impl Toolbar {
    /// 根据选区与屏幕宽高重新计算工具栏摆放位置 (自适应 HiDPI 缩放)。
    ///
    /// 用户拖动过主栏后保持其手动位置（仅夹取到屏幕内），避免调整选区时工具栏又跳回遮挡处；
    /// 调用 [`Self::reset_manual_position`] 后恢复按选区自适应摆放。
    pub fn update_layout(&mut self, selection: &Rect, screen_w: f32, screen_h: f32) {
        self.screen_size = (screen_w, screen_h);
        self.main_rect = match self.manual_pos {
            Some((x, y)) => self.clamped_main_rect(x, y),
            None => calculate_main_layout(
                selection,
                screen_w,
                screen_h,
                self.ui_scale,
                self.main_width(),
            ),
        };
        self.update_sub_rect();
    }

    /// 放弃用户拖动后的手动位置，下次布局时重新按选区自适应摆放。
    pub fn reset_manual_position(&mut self) {
        self.manual_pos = None;
        self.drag_offset = None;
    }

    /// 坐标是否落在主栏右端的拖动手柄上（未启用拖动时恒为 `false`）。
    pub fn hit_test_drag_handle(&self, px: f32, py: f32) -> bool {
        if !self.draggable || !self.main_rect.contains(px, py) {
            return false;
        }
        let s = self.ui_scale;
        let handle_left = self.main_rect.x + self.main_rect.width
            - metrics::padding(s)
            - metrics::drag_handle_width(s);
        px >= handle_left
    }

    /// 若按下位置命中拖动手柄则开始拖动，返回是否已开始。
    ///
    /// 开始拖动时收起下拉菜单，避免菜单停留在旧位置。
    pub fn begin_drag(&mut self, px: f32, py: f32) -> bool {
        if !self.hit_test_drag_handle(px, py) {
            return false;
        }
        self.drag_offset = Some((px - self.main_rect.x, py - self.main_rect.y));
        self.open_dropdown = None;
        self.dropdown_hover = None;
        self.update_sub_rect();
        true
    }

    /// 拖动中跟随光标移动主栏（夹取到屏幕内），返回是否需要重绘。
    pub fn drag_to(&mut self, px: f32, py: f32) -> bool {
        let Some((dx, dy)) = self.drag_offset else {
            return false;
        };
        self.main_rect = self.clamped_main_rect(px - dx, py - dy);
        self.manual_pos = Some((self.main_rect.x, self.main_rect.y));
        self.update_sub_rect();
        true
    }

    /// 结束拖动，返回本次释放是否属于拖动操作。
    pub fn end_drag(&mut self) -> bool {
        self.drag_offset.take().is_some()
    }

    /// 是否正在拖动主栏
    pub fn is_dragging(&self) -> bool {
        self.drag_offset.is_some()
    }

    /// 以指定左上角构造主栏矩形，并夹取到屏幕范围内
    fn clamped_main_rect(&self, x: f32, y: f32) -> Rect {
        let width = self.main_width();
        let height = self.main_height();
        let (screen_w, screen_h) = self.screen_size;
        Rect {
            x: x.min(screen_w - width).max(0.0),
            y: y.min(screen_h - height).max(0.0),
            width,
            height,
        }
    }

    /// 直接指定主栏面板左上角位置，不参与选区自适应避让。
    ///
    /// 截图工具栏由 [`Self::update_layout`] 根据选区与屏幕边界自动摆放；
    /// 贴图窗口则需要把工具条精确锚定在缩放后图像的下方，位置完全由调用方计算。
    /// 这里直接写入 `main_rect` 并复用 [`Self::update_sub_rect`] 重新定位二级属性栏，
    /// 保证命中测试、渲染与下拉菜单避让共用同一份布局结果。
    pub fn place_main_bar(&mut self, x: f32, y: f32) {
        self.main_rect = Rect {
            x,
            y,
            width: self.main_width(),
            height: self.main_height(),
        };
        self.update_sub_rect();
    }

    /// 设置窗口（屏幕）可用宽高，供二级栏下拉菜单做边缘避让。
    ///
    /// 贴图窗口尺寸会随图像、工具条与右键菜单动态变化，必须在重新摆放主栏前更新；
    /// 否则下拉菜单仍按上一帧的窗口边界翻转，可能被窗口边缘裁掉。
    pub fn set_screen_bounds(&mut self, width: f32, height: f32) {
        self.screen_size = (width, height);
    }

    /// 主栏面板宽度（由当前 `main_items`、是否带拖动手柄与界面缩放倍率共同决定）。
    ///
    /// 贴图窗口需要把工具栏锚定在图像内部，必须提前知道面板宽度才能摆放；
    /// 与命中测试、渲染共用同一份度量函数，避免三处坐标漂移。
    pub(crate) fn main_width(&self) -> f32 {
        let handle = if self.draggable {
            metrics::drag_handle_width(self.ui_scale)
        } else {
            0.0
        };
        metrics::main_toolbar_width(self.main_items, self.ui_scale) + handle
    }

    /// 主栏面板高度（含上下内边距）。
    pub(crate) fn main_height(&self) -> f32 {
        metrics::padding(self.ui_scale) * 2.0 + metrics::button_size(self.ui_scale)
    }

    /// 根据当前主工具栏位置重新定位二级属性栏 (自适应 HiDPI 缩放)
    pub(super) fn update_sub_rect(&mut self) {
        self.sub_layout = calculate_sub_layout_with_options(
            &self.main_rect,
            self.active_tool,
            self.open_dropdown,
            self.screen_size.0,
            self.screen_size.1,
            self.ui_scale,
            calculate_sub_layout_options(&self.sub_options),
        );
        self.sub_rect = self.sub_layout.as_ref().map(|layout| layout.rect);
        if self.sub_layout.is_none() {
            self.open_dropdown = None;
            self.dropdown_hover = None;
            self.hovered_sub_control = None;
        }
        self.persist_remembered_options();
    }

    /// 当前二级栏布局（供渲染层读取控件热区）
    pub(crate) fn sub_layout(&self) -> Option<&SubLayout> {
        self.sub_layout.as_ref()
    }

    /// 当前展开下拉菜单的面板矩形（供窗口输入区域计算使用）。
    pub(crate) fn dropdown_rect(&self) -> Option<Rect> {
        self.sub_layout
            .as_ref()
            .and_then(|layout| layout.dropdown.as_ref())
            .map(|dropdown| dropdown.rect)
    }

    /// 切换当前工具并收敛下拉菜单状态
    fn set_active_tool(&mut self, tool: Option<Tool>) {
        if self.active_tool != tool {
            self.open_dropdown = None;
            self.dropdown_hover = None;
            self.focused_sub_control = None;
            self.hovered_sub_control = None;
        }
        self.active_tool = tool;
        self.update_sub_rect();
    }

    /// 清除数值控件焦点；点击画布后滚轮恢复默认当前工具的尺寸行为。
    pub(crate) fn clear_sub_focus(&mut self) {
        self.focused_sub_control = None;
    }

    /// 用户开启跨截图记忆时，保存当前二级栏选项。
    fn persist_remembered_options(&self) {
        if self.remember_sub_options {
            store_remembered_sub_options(self.sub_options);
        }
    }

    /// 鼠标移动命中检测（主栏按钮与下拉菜单行）
    pub fn handle_mouse_move(&mut self, px: f32, py: f32) -> bool {
        let prev_item = self.hovered_item;
        let prev_row = self.dropdown_hover;
        let prev_sub = self.hovered_sub_control;
        self.hovered_item = self.hit_test_main(px, py);
        self.hovered_sub_control = if self.hovered_item.is_none() && self.open_dropdown.is_none() {
            self.sub_layout
                .as_ref()
                .and_then(|layout| layout.hit(px, py))
        } else {
            None
        };
        self.dropdown_hover = self
            .sub_layout
            .as_ref()
            .and_then(|layout| layout.dropdown_row_hit(px, py));
        prev_item != self.hovered_item
            || prev_sub != self.hovered_sub_control
            || prev_row != self.dropdown_hover
    }

    /// 主栏命中测试 (自适应 HiDPI 缩放)
    pub fn hit_test_main(&self, px: f32, py: f32) -> Option<ToolbarItem> {
        hit_test_main_items(&self.main_rect, self.main_items, self.ui_scale, px, py)
    }

    /// 检查指定坐标是否落在主栏、二级栏或展开的下拉菜单内（防止事件穿透到画布）
    pub fn contains_point(&self, px: f32, py: f32) -> bool {
        if self.main_rect.contains(px, py) {
            return true;
        }
        match &self.sub_layout {
            Some(layout) => layout.rect.contains(px, py) || layout.dropdown_contains(px, py),
            None => false,
        }
    }
}
