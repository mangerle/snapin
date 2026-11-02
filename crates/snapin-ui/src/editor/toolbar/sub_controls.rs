//! 二级属性栏下拉选择与滚轮连续调节。

use super::items::{MosaicMode, FONT_FAMILY_PRESETS, FONT_SIZE_PRESETS};
use super::layout::{SubControl, SubDropdown};
use super::state::store_remembered_sub_options;
use super::Toolbar;
use snapin_annotation::{ArrowStyle, BadgeStyle, LineStyle, MagnifierConnector};

impl Toolbar {
    /// 应用下拉菜单中选中的行
    pub fn apply_dropdown_row(&mut self, row: usize) {
        match self.open_dropdown {
            Some(SubDropdown::Line) => {
                if let Some(style) = LineStyle::ALL.get(row) {
                    self.sub_options.line_style = *style;
                }
            }
            Some(SubDropdown::Badge) => {
                if row == 0 {
                    self.sub_options.badge_style = BadgeStyle::Filled;
                } else if row == 1 {
                    self.sub_options.badge_style = BadgeStyle::Outline;
                }
            }
            Some(SubDropdown::Arrow) => {
                if let Some(style) = ArrowStyle::ALL.get(row) {
                    self.sub_options.arrow_style = *style;
                }
            }
            Some(SubDropdown::FontSize) => {
                if let Some(font_size) = FONT_SIZE_PRESETS.get(row) {
                    self.sub_options.font_size = *font_size;
                }
            }
            Some(SubDropdown::StartArrow) => {
                if let Some(style) = snapin_annotation::style::ArrowHeadStyle::ALL.get(row) {
                    self.sub_options.start_arrow_head = *style;
                }
            }
            Some(SubDropdown::EndArrow) => {
                if let Some(style) = snapin_annotation::style::ArrowHeadStyle::ALL.get(row) {
                    self.sub_options.end_arrow_head = *style;
                }
            }
            Some(SubDropdown::BadgeSequence) => {
                if let Some(sequence) = snapin_annotation::BadgeSequence::ALL.get(row) {
                    self.sub_options.badge_sequence = *sequence;
                }
            }
            Some(SubDropdown::MosaicMode) => {
                self.sub_options.mosaic_mode = match row {
                    0 => MosaicMode::Mosaic,
                    1 => MosaicMode::Blur,
                    2 => MosaicMode::SmartRedact,
                    _ => self.sub_options.mosaic_mode,
                };
            }
            Some(SubDropdown::FontFamily) => {
                if let Some((_, family)) = FONT_FAMILY_PRESETS.get(row) {
                    self.sub_options.font_family = *family;
                }
            }
            Some(SubDropdown::MagnifierConnector) => {
                if let Some(connector) = MagnifierConnector::ALL.get(row) {
                    self.sub_options.magnifier_connector = *connector;
                }
            }
            Some(SubDropdown::MarkerShape) => {
                self.sub_options.marker_shape = match row {
                    0 => snapin_annotation::MarkerShape::Stroke,
                    1 => snapin_annotation::MarkerShape::Rectangle,
                    _ => self.sub_options.marker_shape,
                };
            }
            Some(SubDropdown::MarkerBlendMode) => {
                self.sub_options.marker_blend_mode = match row {
                    0 => snapin_annotation::MarkerBlendMode::Multiply,
                    1 => snapin_annotation::MarkerBlendMode::Normal,
                    _ => self.sub_options.marker_blend_mode,
                };
            }
            None => {}
        }
        self.open_dropdown = None;
        self.dropdown_hover = None;
        self.update_sub_rect();
    }

    /// 在二级栏数值控件上滚动滚轮时连续调节参数。
    ///
    /// 只处理光标明确落在数值热区内的滚轮事件；其余滚动仍交回宿主，避免
    /// 用户只是为了滚动画面时意外修改笔刷大小。返回 `true` 表示参数已变化。
    pub fn handle_sub_wheel(&mut self, px: f32, py: f32, delta: f32) -> bool {
        let Some(layout) = self.sub_layout.as_ref() else {
            return false;
        };
        let hovered = layout.hit(px, py);
        let control = match hovered {
            Some(control) if is_numeric_control(control) => Some(control),
            Some(_) => None,
            None => self
                .focused_sub_control
                .filter(|control| is_numeric_control(*control)),
        };
        let Some(control) = control else {
            return false;
        };
        let direction = if delta > 0.0 { 1.0 } else { -1.0 };
        match control {
            SubControl::Size => {
                self.sub_options.custom_stroke_width =
                    (self.sub_options.custom_stroke_width + direction).clamp(1.0, 48.0);
            }
            SubControl::EffectStrength => {
                self.sub_options.effect_strength = add_signed_u8(
                    self.sub_options.effect_strength,
                    if direction > 0.0 { 4 } else { -4 },
                    4,
                    255,
                );
            }
            SubControl::CornerRadius => {
                self.sub_options.corner_radius =
                    (self.sub_options.corner_radius + direction * 2.0).clamp(0.0, 90.0);
            }
            SubControl::FontSize => {
                self.sub_options.font_size =
                    (self.sub_options.font_size + direction * 2.0).clamp(12.0, 72.0);
            }
            SubControl::TextBackgroundOpacity => {
                self.sub_options.text_background_opacity = add_signed_u8(
                    self.sub_options.text_background_opacity,
                    if direction > 0.0 { 10 } else { -10 },
                    0,
                    255,
                );
            }
            SubControl::TextBackgroundPadding => {
                self.sub_options.text_background_padding =
                    (self.sub_options.text_background_padding + direction).clamp(0.0, 48.0);
            }
            SubControl::TextBackgroundCornerRadius => {
                self.sub_options.text_background_corner_radius =
                    (self.sub_options.text_background_corner_radius + direction * 2.0)
                        .clamp(0.0, 48.0);
            }
            SubControl::SpotlightOpacity => {
                self.sub_options.spotlight_intensity = add_signed_u8(
                    self.sub_options.spotlight_intensity,
                    if direction > 0.0 { 8 } else { -8 },
                    0,
                    255,
                );
            }
            SubControl::MagnifierScale => {
                self.sub_options.magnifier_scale =
                    (self.sub_options.magnifier_scale + direction * 0.25).clamp(1.0, 8.0);
            }
            _ => return false,
        }
        if self.remember_sub_options {
            store_remembered_sub_options(self.sub_options);
        }
        true
    }
}

/// 判断控件是否支持滚轮连续调节。
fn is_numeric_control(control: SubControl) -> bool {
    matches!(
        control,
        SubControl::Size
            | SubControl::EffectStrength
            | SubControl::CornerRadius
            | SubControl::FontSize
            | SubControl::TextBackgroundOpacity
            | SubControl::TextBackgroundPadding
            | SubControl::TextBackgroundCornerRadius
            | SubControl::SpotlightOpacity
            | SubControl::MagnifierScale
    )
}

/// 在给定档位中循环选择下一个值。
pub(super) fn next_u8(current: u8, presets: &[u8]) -> u8 {
    presets
        .iter()
        .copied()
        .find(|value| *value > current)
        .unwrap_or(presets[0])
}

/// 在给定浮点档位中循环选择下一个值。
pub(super) fn next_f32(current: f32, presets: &[f32]) -> f32 {
    presets
        .iter()
        .copied()
        .find(|value| *value > current + 0.01)
        .unwrap_or(presets[0])
}

/// 对 `u8` 参数执行带上下限的增减，避免外部滚动输入造成溢出。
fn add_signed_u8(value: u8, delta: i16, min: u8, max: u8) -> u8 {
    (i16::from(value) + delta).clamp(i16::from(min), i16::from(max)) as u8
}
