//! 截图工具栏与二级属性栏自适应布局计算。
//!
//! 主栏根据选区与屏幕边界自适应定位；二级栏按当前工具的可用选项生成控件序列，
//! 控件坐标只在此处计算一次，渲染与命中测试共用同一份结果，避免两处各写一套坐标而漂移。

use crate::editor::toolbar::items::{
    preset_colors, tool_descriptor, MosaicMode, Tool, FONT_FAMILY_PRESETS,
};
use crate::editor::toolbar::metrics::{button_size, padding};
use crate::geometry::Rect;

mod types;

pub(crate) use types::{DropdownLayout, SubControl, SubDropdown, SubEntry, SubLayout};

/// 影响二级栏控件集合的运行时选项。
#[derive(Debug, Clone, Copy)]
pub(crate) struct SubLayoutOptions {
    /// 当前马赛克工具模式；智能马赛克会隐藏绘制模式和强度控件。
    pub mosaic_mode: MosaicMode,
}

/// 根据选区与屏幕宽高重新计算主工具栏在屏幕上的自适应摆放位置。
///
/// `width` 由调用方给出，因为主栏是否带拖动手柄只有 `Toolbar` 自身知道。
pub(crate) fn calculate_main_layout(
    selection: &Rect,
    screen_w: f32,
    screen_h: f32,
    ui_scale: f32,
    width: f32,
) -> Rect {
    let s = ui_scale;
    let height = padding(s) * 2.0 + button_size(s);

    let mut x = (selection.x + selection.width) - width;
    let mut y = selection.y + selection.height + 8.0 * s;

    if x + width > screen_w - 8.0 * s {
        x = screen_w - width - 8.0 * s;
    }
    if x < 8.0 * s {
        x = 8.0 * s;
    }

    if y + height > screen_h - 8.0 * s {
        let above_y = selection.y - height - 8.0 * s;
        if above_y >= 8.0 * s {
            y = above_y;
        } else {
            y = (selection.y + selection.height - height - 8.0 * s).max(8.0 * s);
        }
    }

    Rect {
        x,
        y,
        width,
        height,
    }
}

/// 二级栏控件游标：按顺序摆放控件并累计横坐标
struct EntryCursor<'a> {
    entries: &'a mut Vec<SubEntry>,
    x: f32,
    center_y: f32,
    scale: f32,
}

impl EntryCursor<'_> {
    fn push(&mut self, control: Option<SubControl>, width: f32) {
        let s = self.scale;
        self.entries.push(SubEntry {
            control,
            rect: Rect {
                x: self.x,
                y: self.center_y - 12.0 * s,
                width,
                height: 24.0 * s,
            },
        });
        self.x += width;
    }
}

/// 按当前工具生成二级栏控件序列。
///
/// 每种工具只暴露与之相关的选项：几何形状提供矩形/椭圆、线型、填充与圆角；
/// 序号提供底板样式；其余画笔类工具只保留线宽与颜色。
fn build_entries(
    active_tool: Tool,
    mosaic_mode: MosaicMode,
    origin_x: f32,
    center_y: f32,
    s: f32,
    entries: &mut Vec<SubEntry>,
) -> f32 {
    let mut cursor = EntryCursor {
        entries,
        x: origin_x,
        center_y,
        scale: s,
    };

    let descriptor = tool_descriptor(active_tool);
    if descriptor.shape_controls {
        cursor.push(Some(SubControl::ShapeRect), 30.0 * s);
        cursor.push(Some(SubControl::ShapeEllipse), 30.0 * s);
        cursor.push(None, 14.0 * s);
    }

    if descriptor.line_mode {
        cursor.push(Some(SubControl::LineMode), 54.0 * s);
        cursor.push(None, 14.0 * s);
    }

    if descriptor.mosaic_mode {
        if mosaic_mode != MosaicMode::SmartRedact {
            cursor.push(Some(SubControl::Size), 50.0 * s);
        }
        cursor.push(Some(SubControl::MosaicMode), 78.0 * s);
        if mosaic_mode != MosaicMode::SmartRedact {
            cursor.push(Some(SubControl::MosaicBrush), 48.0 * s);
            cursor.push(Some(SubControl::MosaicRectangle), 48.0 * s);
            cursor.push(Some(SubControl::EffectStrength), 92.0 * s);
        }
        cursor.push(None, 14.0 * s);
    }

    if descriptor.marker_mode {
        cursor.push(Some(SubControl::MarkerShape), 54.0 * s);
        cursor.push(Some(SubControl::MarkerBlendMode), 62.0 * s);
        cursor.push(None, 14.0 * s);
    }

    if descriptor.eraser_mode {
        cursor.push(Some(SubControl::EraserMode), 54.0 * s);
    }

    if descriptor.spotlight_style {
        cursor.push(Some(SubControl::SpotlightShapeRect), 30.0 * s);
        cursor.push(Some(SubControl::SpotlightShapeEllipse), 30.0 * s);
        cursor.push(None, 14.0 * s);
        cursor.push(Some(SubControl::SpotlightOpacity), 64.0 * s);
        cursor.push(Some(SubControl::SpotlightOutline), 50.0 * s);
    }

    if descriptor.magnifier_options {
        cursor.push(Some(SubControl::MagnifierShapeRect), 30.0 * s);
        cursor.push(Some(SubControl::MagnifierShapeEllipse), 30.0 * s);
        cursor.push(None, 14.0 * s);
        cursor.push(Some(SubControl::MagnifierScale), 104.0 * s);
        cursor.push(Some(SubControl::MagnifierSmooth), 54.0 * s);
        cursor.push(Some(SubControl::MagnifierConnector), 64.0 * s);
        cursor.push(Some(SubControl::MagnifierShadow), 48.0 * s);
        cursor.push(None, 14.0 * s);
    }

    if descriptor.fill_toggle {
        cursor.push(Some(SubControl::FillToggle), 32.0 * s);
    }

    if descriptor.line_style {
        cursor.push(Some(SubControl::LineStyle), 56.0 * s);
    }

    if descriptor.stroke_width && !descriptor.mosaic_mode {
        cursor.push(Some(SubControl::Size), 50.0 * s);
    }

    if descriptor.font_size {
        cursor.push(Some(SubControl::FontSize), 66.0 * s);
    }

    if descriptor.text_style {
        cursor.push(Some(SubControl::Bold), 28.0 * s);
        cursor.push(Some(SubControl::Italic), 28.0 * s);
        cursor.push(Some(SubControl::TextStroke), 42.0 * s);
        cursor.push(Some(SubControl::TextBackground), 46.0 * s);
        cursor.push(Some(SubControl::TextBackgroundOpacity), 58.0 * s);
        cursor.push(Some(SubControl::TextBackgroundPadding), 64.0 * s);
        cursor.push(Some(SubControl::TextBackgroundCornerRadius), 64.0 * s);
        cursor.push(Some(SubControl::FontFamily), 92.0 * s);
    }

    if descriptor.corner_radius {
        cursor.push(Some(SubControl::CornerRadius), 54.0 * s);
    }
    if descriptor.badge_style {
        cursor.push(Some(SubControl::BadgeStyle), 82.0 * s);
    }
    if descriptor.badge_sequence {
        cursor.push(Some(SubControl::BadgeSequence), 68.0 * s);
    }
    if descriptor.arrow_style {
        cursor.push(Some(SubControl::ArrowStyle), 58.0 * s);
    }
    if descriptor.arrow_toggle {
        cursor.push(Some(SubControl::ArrowEnabled), 54.0 * s);
    }
    if descriptor.arrow_heads {
        cursor.push(Some(SubControl::StartArrow), 52.0 * s);
        cursor.push(Some(SubControl::EndArrow), 52.0 * s);
    }
    if descriptor.eraser_mode {
        cursor.push(Some(SubControl::ClearAnnotations), 54.0 * s);
    }

    if descriptor.color_palette {
        cursor.push(None, 14.0 * s);
        for (index, _) in preset_colors().iter().enumerate() {
            cursor.push(Some(SubControl::Color(index)), 24.0 * s);
        }
    }

    // 末尾留出与内边距等宽的余量，避免最后一个色块贴边
    cursor.x + 4.0 * s
}

/// 计算二级属性栏及其下拉菜单布局
#[cfg(test)]
pub(crate) fn calculate_sub_layout(
    main_rect: &Rect,
    active_tool: Option<Tool>,
    open_dropdown: Option<SubDropdown>,
    screen_w: f32,
    screen_h: f32,
    ui_scale: f32,
) -> Option<SubLayout> {
    calculate_sub_layout_with_options(
        main_rect,
        active_tool,
        open_dropdown,
        screen_w,
        screen_h,
        ui_scale,
        SubLayoutOptions {
            mosaic_mode: MosaicMode::Mosaic,
        },
    )
}

/// 计算带运行时工具状态的二级栏布局；当前主要用于按智能打码模式裁剪控件。
pub(crate) fn calculate_sub_layout_with_options(
    main_rect: &Rect,
    active_tool: Option<Tool>,
    open_dropdown: Option<SubDropdown>,
    screen_w: f32,
    screen_h: f32,
    ui_scale: f32,
    options: SubLayoutOptions,
) -> Option<SubLayout> {
    let active_tool = active_tool?;
    if main_rect.width <= 0.0 {
        return None;
    }
    // 没有可调参数的工具不显示空的二级属性栏。
    if !tool_descriptor(active_tool).has_sub_bar {
        return None;
    }

    let s = ui_scale;
    let sub_h = 36.0 * s;
    let sub_x = main_rect.x;
    let sub_y = if main_rect.y >= sub_h + 8.0 * s {
        main_rect.y - sub_h - 4.0 * s
    } else {
        main_rect.y + main_rect.height + 4.0 * s
    };
    let center_y = sub_y + sub_h / 2.0;

    let mut entries = Vec::with_capacity(32);
    let pad = 12.0 * s;
    let end_x = build_entries(
        active_tool,
        options.mosaic_mode,
        sub_x + pad,
        center_y,
        s,
        &mut entries,
    );
    let width = (end_x - sub_x).max(120.0 * s);
    let max_x = (screen_w - width - 8.0 * s).max(8.0 * s);
    let clamped_x = sub_x.min(max_x).max(8.0 * s);
    let shift = clamped_x - sub_x;
    if shift.abs() > f32::EPSILON {
        for entry in &mut entries {
            entry.rect.x += shift;
        }
    }
    let sub_rect = Rect {
        x: clamped_x,
        y: sub_y,
        width,
        height: sub_h,
    };

    let dropdown = open_dropdown.and_then(|kind| {
        let anchor = match kind {
            SubDropdown::Line => SubControl::LineStyle,
            SubDropdown::Badge => SubControl::BadgeStyle,
            SubDropdown::Arrow => SubControl::ArrowStyle,
            SubDropdown::FontSize => SubControl::FontSize,
            SubDropdown::StartArrow => SubControl::StartArrow,
            SubDropdown::EndArrow => SubControl::EndArrow,
            SubDropdown::BadgeSequence => SubControl::BadgeSequence,
            SubDropdown::MosaicMode => SubControl::MosaicMode,
            SubDropdown::FontFamily => SubControl::FontFamily,
            SubDropdown::MagnifierConnector => SubControl::MagnifierConnector,
            SubDropdown::MarkerShape => SubControl::MarkerShape,
            SubDropdown::MarkerBlendMode => SubControl::MarkerBlendMode,
        };
        let anchor_rect = entries
            .iter()
            .find(|entry| entry.control == Some(anchor))?
            .rect;

        let rows = match kind {
            SubDropdown::Line => snapin_annotation::LineStyle::ALL.len(),
            SubDropdown::Badge => 2,
            SubDropdown::Arrow => 3,
            SubDropdown::FontSize => crate::editor::toolbar::items::FONT_SIZE_PRESETS.len(),
            SubDropdown::StartArrow | SubDropdown::EndArrow => {
                snapin_annotation::style::ArrowHeadStyle::ALL.len()
            }
            SubDropdown::BadgeSequence => snapin_annotation::BadgeSequence::ALL.len(),
            SubDropdown::MosaicMode => 3,
            SubDropdown::FontFamily => FONT_FAMILY_PRESETS.len(),
            SubDropdown::MagnifierConnector => snapin_annotation::MagnifierConnector::ALL.len(),
            SubDropdown::MarkerShape | SubDropdown::MarkerBlendMode => 2,
        };
        let row_h = 28.0 * s;
        let panel_h = rows as f32 * row_h + 8.0 * s;
        let panel_w = 116.0 * s;

        let mut y = sub_y + sub_h + 4.0 * s;
        if y + panel_h > screen_h - 8.0 * s {
            y = (sub_y - panel_h - 4.0 * s).max(8.0 * s);
        }
        let mut x = anchor_rect.x;
        if x + panel_w > screen_w - 8.0 * s {
            x = (screen_w - panel_w - 8.0 * s).max(8.0 * s);
        }

        let panel = Rect {
            x,
            y,
            width: panel_w,
            height: panel_h,
        };
        let rows = (0..rows)
            .map(|index| Rect {
                x: x + 4.0 * s,
                y: y + 4.0 * s + index as f32 * row_h,
                width: panel_w - 8.0 * s,
                height: row_h,
            })
            .collect();

        Some(DropdownLayout {
            kind,
            rect: panel,
            rows,
        })
    });

    Some(SubLayout {
        rect: sub_rect,
        entries,
        dropdown,
    })
}

#[cfg(test)]
mod tests;
