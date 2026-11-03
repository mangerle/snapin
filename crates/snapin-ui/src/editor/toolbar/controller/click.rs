//! 工具栏点击分发：下拉菜单、主栏动作与二级控件状态更新。

use super::super::items::{preset_colors, tool_descriptor, LineMode};
use super::super::layout::{SubControl, SubDropdown};
use super::super::sub_controls::{next_f32, next_u8};
use super::super::{Tool, Toolbar, ToolbarAction, ToolbarItem};
use snapin_annotation::{MagnifierShape, SpotlightShape};

/// 下拉菜单是否消费本次点击，以及是否产生一次性动作。
enum DropdownClickOutcome {
    /// 点击不位于下拉菜单区域，继续分发给其他控件
    NotHandled,
    /// 点击已被下拉菜单消费
    Consumed,
    /// 点击被消费并产生动作
    Action(ToolbarAction),
}

/// 滑杆命中换算所需的控件、左右内边距与指针横坐标。
#[derive(Clone, Copy)]
struct SliderInteraction {
    control: SubControl,
    left_inset: f32,
    right_inset: f32,
    pointer_x: f32,
}

impl Toolbar {
    /// 点击项目切换工具、调整样式或返回一次性动作。
    ///
    /// 工具点击只更新 `active_tool` 并返回 `None`；动作点击不改变激活工具，
    /// 返回 [`ToolbarAction`] 交由宿主派发，避免把“保持激活”与“执行一次”混为一谈。
    pub fn handle_click(&mut self, px: f32, py: f32) -> Option<ToolbarAction> {
        match self.handle_dropdown_click(px, py) {
            DropdownClickOutcome::Action(action) => return Some(action),
            DropdownClickOutcome::Consumed => return None,
            DropdownClickOutcome::NotHandled => {}
        }

        if let Some(item) = self.hit_test_main(px, py) {
            self.focused_sub_control = None;
            return self.handle_main_item_click(item);
        }

        let control = self
            .sub_layout
            .as_ref()
            .and_then(|layout| layout.hit(px, py));
        if let Some(control) = control {
            self.focused_sub_control = Some(control);
        }
        self.handle_sub_control_click(control, px)
    }

    /// 优先处理展开的下拉菜单；点击外部时先收起菜单再继续分发。
    fn handle_dropdown_click(&mut self, px: f32, py: f32) -> DropdownClickOutcome {
        let row = self
            .sub_layout
            .as_ref()
            .and_then(|layout| layout.dropdown_row_hit(px, py));
        if let Some(row) = row {
            let dropdown = self.open_dropdown;
            self.apply_dropdown_row(row);
            return if dropdown == Some(SubDropdown::MosaicMode) && row == 2 {
                DropdownClickOutcome::Action(ToolbarAction::SmartRedact)
            } else {
                DropdownClickOutcome::Consumed
            };
        }

        if self
            .sub_layout
            .as_ref()
            .is_some_and(|layout| layout.dropdown_contains(px, py))
        {
            return DropdownClickOutcome::Consumed;
        }

        if self.open_dropdown.is_some() {
            self.open_dropdown = None;
            self.dropdown_hover = None;
            self.update_sub_rect();
        }
        DropdownClickOutcome::NotHandled
    }

    /// 处理主栏工具或一次性动作。
    fn handle_main_item_click(&mut self, item: ToolbarItem) -> Option<ToolbarAction> {
        match item {
            ToolbarItem::Tool(Tool::Rect) => {
                let shape_active = self
                    .active_tool
                    .is_some_and(|tool| tool_descriptor(tool).geometric_slot);
                self.set_active_tool(if shape_active { None } else { Some(Tool::Rect) });
                None
            }
            ToolbarItem::Tool(tool) => {
                let next = if self.active_tool == Some(tool) {
                    None
                } else {
                    Some(tool)
                };
                self.set_active_tool(next);
                None
            }
            ToolbarItem::Action(action) => Some(action),
        }
    }

    /// 处理当前命中的二级栏控件；无命中时只持久化可能已变化的焦点。
    fn handle_sub_control_click(
        &mut self,
        control: Option<SubControl>,
        px: f32,
    ) -> Option<ToolbarAction> {
        let Some(control) = control else {
            self.persist_remembered_options();
            return None;
        };

        if control == SubControl::ClearAnnotations {
            return Some(ToolbarAction::ClearAnnotations);
        }

        match control {
            SubControl::ShapeRect
            | SubControl::ShapeEllipse
            | SubControl::Size
            | SubControl::EffectStrength => self.apply_shape_control(control, px),
            SubControl::LineStyle
            | SubControl::FillToggle
            | SubControl::CornerRadius
            | SubControl::BadgeStyle
            | SubControl::ArrowStyle
            | SubControl::Color(_)
            | SubControl::FontSize
            | SubControl::StartArrow
            | SubControl::EndArrow => self.apply_stroke_control(control),
            SubControl::Bold
            | SubControl::Italic
            | SubControl::TextStroke
            | SubControl::TextBackground
            | SubControl::BadgeSequence
            | SubControl::FontFamily
            | SubControl::TextBackgroundOpacity
            | SubControl::TextBackgroundPadding
            | SubControl::TextBackgroundCornerRadius => self.apply_text_control(control),
            SubControl::LineMode | SubControl::ArrowEnabled => self.apply_line_control(control),
            SubControl::MosaicBrush | SubControl::MosaicRectangle | SubControl::MosaicMode => {
                self.apply_mosaic_control(control);
            }
            SubControl::SpotlightShapeRect
            | SubControl::SpotlightShapeEllipse
            | SubControl::SpotlightOpacity
            | SubControl::SpotlightOutline => self.apply_spotlight_control(control),
            SubControl::MagnifierShapeRect
            | SubControl::MagnifierShapeEllipse
            | SubControl::MagnifierScale
            | SubControl::MagnifierSmooth
            | SubControl::MagnifierShadow
            | SubControl::MagnifierConnector => self.apply_magnifier_control(control, px),
            SubControl::MarkerShape | SubControl::MarkerBlendMode | SubControl::EraserMode => {
                self.apply_marker_control(control);
            }
            SubControl::ClearAnnotations => unreachable!("清空动作已在上方提前返回"),
        }

        self.persist_remembered_options();
        None
    }

    /// 形状切换、尺寸聚焦与效果强度滑杆。
    fn apply_shape_control(&mut self, control: SubControl, px: f32) {
        match control {
            SubControl::ShapeRect => self.set_active_tool(Some(Tool::Rect)),
            SubControl::ShapeEllipse => self.set_active_tool(Some(Tool::Ellipse)),
            SubControl::EffectStrength => {
                if let Some(ratio) = self.slider_ratio(SliderInteraction {
                    control: SubControl::EffectStrength,
                    left_inset: 12.0,
                    right_inset: 12.0,
                    pointer_x: px,
                }) {
                    self.sub_options.effect_strength = (16.0 + ratio * 144.0).round() as u8;
                }
            }
            SubControl::Size => {}
            _ => unreachable!("形状组收到错误控件"),
        }
    }

    /// 线型、填充、颜色、字号与箭头等通用描边控件。
    fn apply_stroke_control(&mut self, control: SubControl) {
        match control {
            SubControl::LineStyle => self.toggle_dropdown(SubDropdown::Line),
            SubControl::FillToggle => self.sub_options.filled = !self.sub_options.filled,
            SubControl::CornerRadius => {}
            SubControl::BadgeStyle => self.toggle_dropdown(SubDropdown::Badge),
            SubControl::ArrowStyle => self.toggle_dropdown(SubDropdown::Arrow),
            SubControl::Color(index) => {
                if let Some(color) = preset_colors().get(index) {
                    self.sub_options.color = *color;
                }
            }
            SubControl::FontSize => self.toggle_dropdown(SubDropdown::FontSize),
            SubControl::StartArrow => self.show_dropdown(SubDropdown::StartArrow),
            SubControl::EndArrow => self.show_dropdown(SubDropdown::EndArrow),
            _ => unreachable!("描边组收到错误控件"),
        }
    }

    /// 文本样式和文字背景控件。
    fn apply_text_control(&mut self, control: SubControl) {
        match control {
            SubControl::Bold => self.sub_options.text_bold = !self.sub_options.text_bold,
            SubControl::Italic => self.sub_options.text_italic = !self.sub_options.text_italic,
            SubControl::TextStroke => {
                self.sub_options.text_stroke_width = if self.sub_options.text_stroke_width > 0.0 {
                    0.0
                } else {
                    2.0
                };
            }
            SubControl::TextBackground => {
                self.sub_options.text_background_enabled =
                    !self.sub_options.text_background_enabled;
            }
            SubControl::BadgeSequence => self.show_dropdown(SubDropdown::BadgeSequence),
            SubControl::FontFamily => self.show_dropdown(SubDropdown::FontFamily),
            SubControl::TextBackgroundOpacity => {
                self.sub_options.text_background_opacity = next_u8(
                    self.sub_options.text_background_opacity,
                    &[40, 80, 120, 160, 200, 240],
                );
            }
            SubControl::TextBackgroundPadding => {
                self.sub_options.text_background_padding = next_f32(
                    self.sub_options.text_background_padding,
                    &[0.0, 2.0, 4.0, 8.0, 12.0, 16.0, 24.0],
                );
            }
            SubControl::TextBackgroundCornerRadius => {
                self.sub_options.text_background_corner_radius = next_f32(
                    self.sub_options.text_background_corner_radius,
                    &[0.0, 4.0, 8.0, 12.0, 20.0, 30.0, 48.0],
                );
            }
            _ => unreachable!("文本组收到错误控件"),
        }
    }

    /// 直线与折线模式、箭头总开关。
    fn apply_line_control(&mut self, control: SubControl) {
        match control {
            SubControl::LineMode => {
                self.sub_options.line_mode = match self.sub_options.line_mode {
                    LineMode::Straight => LineMode::Polyline,
                    LineMode::Polyline => LineMode::Straight,
                };
            }
            SubControl::ArrowEnabled => {
                self.sub_options.line_arrow_enabled = !self.sub_options.line_arrow_enabled;
            }
            _ => unreachable!("直线组收到错误控件"),
        }
    }

    /// 马赛克画笔、矩形与遮挡模式。
    fn apply_mosaic_control(&mut self, control: SubControl) {
        match control {
            SubControl::MosaicBrush => {
                self.sub_options.mosaic_brush_mode = true;
                self.sub_options.custom_stroke_width =
                    self.sub_options.custom_stroke_width.max(12.0);
            }
            SubControl::MosaicRectangle => self.sub_options.mosaic_brush_mode = false,
            SubControl::MosaicMode => self.show_dropdown(SubDropdown::MosaicMode),
            _ => unreachable!("马赛克组收到错误控件"),
        }
    }

    /// 聚光灯形状、透明度与描边。
    fn apply_spotlight_control(&mut self, control: SubControl) {
        match control {
            SubControl::SpotlightShapeRect => {
                self.sub_options.spotlight_shape = SpotlightShape::Rectangle;
            }
            SubControl::SpotlightShapeEllipse => {
                self.sub_options.spotlight_shape = SpotlightShape::Ellipse;
            }
            SubControl::SpotlightOpacity => {
                self.sub_options.spotlight_intensity = next_u8(
                    self.sub_options.spotlight_intensity,
                    &[64, 96, 128, 160, 192, 224],
                );
            }
            SubControl::SpotlightOutline => {
                self.sub_options.spotlight_outline = !self.sub_options.spotlight_outline;
            }
            _ => unreachable!("聚光灯组收到错误控件"),
        }
    }

    /// 放大镜形状、倍率、平滑、阴影与连接线。
    fn apply_magnifier_control(&mut self, control: SubControl, px: f32) {
        match control {
            SubControl::MagnifierShapeRect => {
                self.sub_options.magnifier_shape = MagnifierShape::Rectangle;
            }
            SubControl::MagnifierShapeEllipse => {
                self.sub_options.magnifier_shape = MagnifierShape::Ellipse;
            }
            SubControl::MagnifierScale => {
                if let Some(ratio) = self.slider_ratio(SliderInteraction {
                    control: SubControl::MagnifierScale,
                    left_inset: 43.0,
                    right_inset: 12.0,
                    pointer_x: px,
                }) {
                    self.sub_options.magnifier_scale = 1.0 + ratio * 7.0;
                }
            }
            SubControl::MagnifierSmooth => {
                self.sub_options.magnifier_smooth = !self.sub_options.magnifier_smooth;
            }
            SubControl::MagnifierShadow => {
                self.sub_options.magnifier_shadow = !self.sub_options.magnifier_shadow;
            }
            SubControl::MagnifierConnector => self.show_dropdown(SubDropdown::MagnifierConnector),
            _ => unreachable!("放大镜组收到错误控件"),
        }
    }

    /// 荧光笔与橡皮擦模式。
    fn apply_marker_control(&mut self, control: SubControl) {
        match control {
            SubControl::MarkerShape => self.show_dropdown(SubDropdown::MarkerShape),
            SubControl::MarkerBlendMode => self.show_dropdown(SubDropdown::MarkerBlendMode),
            SubControl::EraserMode => {
                self.sub_options.eraser_rectangle_mode = !self.sub_options.eraser_rectangle_mode;
            }
            _ => unreachable!("画笔组收到错误控件"),
        }
    }

    /// 根据控件热区将指针位置换算为 0..=1 的滑杆比例。
    fn slider_ratio(&self, interaction: SliderInteraction) -> Option<f32> {
        let rect = self.sub_layout.as_ref()?.rect_of(interaction.control)?;
        let left = rect.x + interaction.left_inset * self.ui_scale;
        let right = rect.x + rect.width - interaction.right_inset * self.ui_scale;
        Some(((interaction.pointer_x - left) / (right - left).max(1.0)).clamp(0.0, 1.0))
    }

    /// 开合指定下拉菜单。
    fn toggle_dropdown(&mut self, dropdown: SubDropdown) {
        self.open_dropdown = if self.open_dropdown == Some(dropdown) {
            None
        } else {
            Some(dropdown)
        };
        self.dropdown_hover = None;
        self.update_sub_rect();
    }

    /// 打开指定下拉菜单；重复点击时保持展开。
    fn show_dropdown(&mut self, dropdown: SubDropdown) {
        self.open_dropdown = Some(dropdown);
        self.dropdown_hover = None;
        self.update_sub_rect();
    }
}
