//! 工具栏交互控制器：布局摆放、拖动、命中测试与二级控件状态流转。
//!
//! 本模块只修改 [`Toolbar`] 状态并输出动作结果，不负责像素渲染；
//! 渲染层读取同一份布局结果，避免交互与绘制各维护一套坐标。

use super::interaction::hit_test_main_items;
use super::items::{preset_colors, tool_descriptor, LineMode};
use super::layout::{
    calculate_main_layout, calculate_sub_layout_with_options, SubControl, SubDropdown, SubLayout,
    SubLayoutOptions,
};
use super::metrics;
use super::state::store_remembered_sub_options;
use super::sub_controls::{next_f32, next_u8};
use super::{Tool, Toolbar, ToolbarAction, ToolbarItem};
use crate::geometry::Rect;
use snapin_core::annotation::{MagnifierShape, SpotlightShape};

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

    /// 点击项目切换工具、调整样式或返回一次性动作。
    ///
    /// 工具点击只更新 `active_tool` 并返回 `None`；动作点击不改变激活工具，
    /// 返回 [`ToolbarAction`] 交由宿主派发，避免把“保持激活”与“执行一次”混为一谈。
    pub fn handle_click(&mut self, px: f32, py: f32) -> Option<ToolbarAction> {
        // 1. 下拉菜单优先：命中选项行直接应用；点在面板空白处吞掉；点在面板外则先收起再继续
        let dropdown_row = self
            .sub_layout
            .as_ref()
            .and_then(|layout| layout.dropdown_row_hit(px, py));
        if let Some(row) = dropdown_row {
            let dropdown = self.open_dropdown;
            self.apply_dropdown_row(row);
            return (dropdown == Some(SubDropdown::MosaicMode) && row == 2)
                .then_some(ToolbarAction::SmartRedact);
        }
        if self
            .sub_layout
            .as_ref()
            .is_some_and(|layout| layout.dropdown_contains(px, py))
        {
            return None;
        }
        if self.open_dropdown.is_some() {
            self.open_dropdown = None;
            self.dropdown_hover = None;
            self.update_sub_rect();
        }

        // 2. 主栏按钮
        if let Some(item) = self.hit_test_main(px, py) {
            match item {
                ToolbarItem::Tool(Tool::Rect) => {
                    // 形状按钮只做开合：点击选中矩形（默认形状），再点取消。
                    // 椭圆是同一个工具的第二形态，只能从二级属性栏显式选择——
                    // 旧实现在此循环"矩形→椭圆→关闭"，导致用户想点开选项时反而被切成椭圆。
                    let shape_active = self
                        .active_tool
                        .is_some_and(|tool| tool_descriptor(tool).geometric_slot);
                    self.set_active_tool(if shape_active { None } else { Some(Tool::Rect) });
                    return None;
                }
                ToolbarItem::Tool(tool) => {
                    let next = if self.active_tool == Some(tool) {
                        None
                    } else {
                        Some(tool)
                    };
                    self.set_active_tool(next);
                    return None;
                }
                ToolbarItem::Action(action) => return Some(action),
            }
        }

        // 3. 二级栏控件
        let control = self
            .sub_layout
            .as_ref()
            .and_then(|layout| layout.hit(px, py));
        if let Some(control) = control {
            self.focused_sub_control = Some(control);
        }
        match control {
            Some(SubControl::ShapeRect) => self.set_active_tool(Some(Tool::Rect)),
            Some(SubControl::ShapeEllipse) => self.set_active_tool(Some(Tool::Ellipse)),
            Some(SubControl::Size) => {
                // 点击仅聚焦尺寸控件；滚轮在控件内或点击后于二级栏内继续调节。
            }
            Some(SubControl::EffectStrength) => {
                if let Some(rect) = self
                    .sub_layout
                    .as_ref()
                    .and_then(|layout| layout.rect_of(SubControl::EffectStrength))
                {
                    let left = rect.x + 12.0 * self.ui_scale;
                    let right = rect.x + rect.width - 12.0 * self.ui_scale;
                    let ratio = ((px - left) / (right - left).max(1.0)).clamp(0.0, 1.0);
                    self.sub_options.effect_strength = (16.0 + ratio * 144.0).round() as u8;
                }
            }
            Some(SubControl::LineStyle) => {
                self.open_dropdown = if self.open_dropdown == Some(SubDropdown::Line) {
                    None
                } else {
                    Some(SubDropdown::Line)
                };
                self.dropdown_hover = None;
                self.update_sub_rect();
            }
            Some(SubControl::FillToggle) => {
                self.sub_options.filled = !self.sub_options.filled;
            }
            // 圆角半径：单击仅聚焦，滚轮连续调整图标后的数值。
            Some(SubControl::CornerRadius) => {}
            Some(SubControl::BadgeStyle) => {
                self.open_dropdown = if self.open_dropdown == Some(SubDropdown::Badge) {
                    None
                } else {
                    Some(SubDropdown::Badge)
                };
                self.dropdown_hover = None;
                self.update_sub_rect();
            }
            Some(SubControl::ArrowStyle) => {
                self.open_dropdown = if self.open_dropdown == Some(SubDropdown::Arrow) {
                    None
                } else {
                    Some(SubDropdown::Arrow)
                };
                self.dropdown_hover = None;
                self.update_sub_rect();
            }
            Some(SubControl::Color(index)) => {
                if let Some(color) = preset_colors().get(index) {
                    self.sub_options.color = *color;
                }
            }
            Some(SubControl::FontSize) => {
                self.open_dropdown = if self.open_dropdown == Some(SubDropdown::FontSize) {
                    None
                } else {
                    Some(SubDropdown::FontSize)
                };
                self.dropdown_hover = None;
                self.update_sub_rect();
            }
            Some(SubControl::StartArrow) => {
                self.open_dropdown = Some(SubDropdown::StartArrow);
                self.update_sub_rect();
            }
            Some(SubControl::EndArrow) => {
                self.open_dropdown = Some(SubDropdown::EndArrow);
                self.update_sub_rect();
            }
            Some(SubControl::Bold) => self.sub_options.text_bold = !self.sub_options.text_bold,
            Some(SubControl::Italic) => {
                self.sub_options.text_italic = !self.sub_options.text_italic
            }
            Some(SubControl::TextStroke) => {
                self.sub_options.text_stroke_width = if self.sub_options.text_stroke_width > 0.0 {
                    0.0
                } else {
                    2.0
                };
            }
            Some(SubControl::TextBackground) => {
                self.sub_options.text_background_enabled =
                    !self.sub_options.text_background_enabled;
            }
            Some(SubControl::BadgeSequence) => {
                self.open_dropdown = Some(SubDropdown::BadgeSequence);
                self.update_sub_rect();
            }
            Some(SubControl::MarkerShape) => {
                self.open_dropdown = Some(SubDropdown::MarkerShape);
                self.dropdown_hover = None;
                self.update_sub_rect();
            }
            Some(SubControl::MarkerBlendMode) => {
                self.open_dropdown = Some(SubDropdown::MarkerBlendMode);
                self.dropdown_hover = None;
                self.update_sub_rect();
            }
            Some(SubControl::EraserMode) => {
                self.sub_options.eraser_rectangle_mode = !self.sub_options.eraser_rectangle_mode;
            }
            Some(SubControl::ClearAnnotations) => {
                return Some(ToolbarAction::ClearAnnotations);
            }
            Some(SubControl::LineMode) => {
                self.sub_options.line_mode = match self.sub_options.line_mode {
                    LineMode::Straight => LineMode::Polyline,
                    LineMode::Polyline => LineMode::Straight,
                };
            }
            Some(SubControl::ArrowEnabled) => {
                self.sub_options.line_arrow_enabled = !self.sub_options.line_arrow_enabled;
            }
            Some(SubControl::MosaicBrush) => {
                self.sub_options.mosaic_brush_mode = true;
                self.sub_options.custom_stroke_width =
                    self.sub_options.custom_stroke_width.max(12.0);
            }
            Some(SubControl::MosaicRectangle) => {
                self.sub_options.mosaic_brush_mode = false;
            }
            Some(SubControl::MosaicMode) => {
                self.open_dropdown = Some(SubDropdown::MosaicMode);
                self.dropdown_hover = None;
                self.update_sub_rect();
            }
            Some(SubControl::FontFamily) => {
                self.open_dropdown = Some(SubDropdown::FontFamily);
                self.dropdown_hover = None;
                self.update_sub_rect();
            }
            Some(SubControl::TextBackgroundOpacity) => {
                self.sub_options.text_background_opacity = next_u8(
                    self.sub_options.text_background_opacity,
                    &[40, 80, 120, 160, 200, 240],
                );
            }
            Some(SubControl::TextBackgroundPadding) => {
                self.sub_options.text_background_padding = next_f32(
                    self.sub_options.text_background_padding,
                    &[0.0, 2.0, 4.0, 8.0, 12.0, 16.0, 24.0],
                );
            }
            Some(SubControl::TextBackgroundCornerRadius) => {
                self.sub_options.text_background_corner_radius = next_f32(
                    self.sub_options.text_background_corner_radius,
                    &[0.0, 4.0, 8.0, 12.0, 20.0, 30.0, 48.0],
                );
            }
            Some(SubControl::SpotlightShapeRect) => {
                self.sub_options.spotlight_shape = SpotlightShape::Rectangle;
            }
            Some(SubControl::SpotlightShapeEllipse) => {
                self.sub_options.spotlight_shape = SpotlightShape::Ellipse;
            }
            Some(SubControl::SpotlightOpacity) => {
                self.sub_options.spotlight_intensity = next_u8(
                    self.sub_options.spotlight_intensity,
                    &[64, 96, 128, 160, 192, 224],
                );
            }
            Some(SubControl::SpotlightOutline) => {
                self.sub_options.spotlight_outline = !self.sub_options.spotlight_outline;
            }
            Some(SubControl::MagnifierShapeRect) => {
                self.sub_options.magnifier_shape = MagnifierShape::Rectangle;
            }
            Some(SubControl::MagnifierShapeEllipse) => {
                self.sub_options.magnifier_shape = MagnifierShape::Ellipse;
            }
            Some(SubControl::MagnifierScale) => {
                if let Some(rect) = self
                    .sub_layout
                    .as_ref()
                    .and_then(|layout| layout.rect_of(SubControl::MagnifierScale))
                {
                    let left = rect.x + 42.0 * self.ui_scale;
                    let right = rect.x + rect.width - 12.0 * self.ui_scale;
                    let ratio = ((px - left) / (right - left).max(1.0)).clamp(0.0, 1.0);
                    self.sub_options.magnifier_scale = 1.0 + ratio * 7.0;
                }
            }
            Some(SubControl::MagnifierSmooth) => {
                self.sub_options.magnifier_smooth = !self.sub_options.magnifier_smooth;
            }
            Some(SubControl::MagnifierShadow) => {
                self.sub_options.magnifier_shadow = !self.sub_options.magnifier_shadow;
            }
            Some(SubControl::MagnifierConnector) => {
                self.open_dropdown = Some(SubDropdown::MagnifierConnector);
                self.dropdown_hover = None;
                self.update_sub_rect();
            }
            None => {}
        }

        self.persist_remembered_options();
        None
    }
}
