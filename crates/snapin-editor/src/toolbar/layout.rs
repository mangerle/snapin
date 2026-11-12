//! 截图工具栏与二级属性栏自适应布局计算。
//!
//! 主栏根据选区与屏幕边界自适应定位；二级栏按当前工具的可用选项生成控件序列，
//! 控件坐标只在此处计算一次，渲染与命中测试共用同一份结果，避免两处各写一套坐标而漂移。

use crate::geometry::Rect;
use crate::toolbar::items::{
    preset_colors, tool_descriptor, MosaicMode, Tool, ToolDescriptor, FONT_FAMILY_PRESETS,
};
use crate::toolbar::metrics::{button_size, padding};

mod types;

pub use types::{DropdownLayout, SubControl, SubDropdown, SubEntry, SubLayout};

/// 影响二级栏控件集合的运行时选项。
#[derive(Debug, Clone, Copy)]
pub(crate) struct SubLayoutOptions {
    /// 当前马赛克工具模式；智能马赛克会隐藏绘制模式和强度控件。
    pub mosaic_mode: MosaicMode,
    /// 是否显示智能马赛克模式项；长截图编辑阶段需将其禁用。
    pub smart_redact_enabled: bool,
}

/// 主工具栏自适应摆放参数。
#[derive(Debug, Clone, Copy)]
pub(crate) struct MainLayoutRequest<'a> {
    pub selection: &'a Rect,
    pub screen_width: f32,
    pub screen_height: f32,
    pub ui_scale: f32,
    pub width: f32,
}

/// 根据选区与屏幕宽高重新计算主工具栏在屏幕上的自适应摆放位置。
///
/// `width` 由调用方给出，因为主栏是否带拖动手柄只有 `Toolbar` 自身知道。
pub(crate) fn calculate_main_layout(request: MainLayoutRequest<'_>) -> Rect {
    let MainLayoutRequest {
        selection,
        screen_width: screen_w,
        screen_height: screen_h,
        ui_scale,
        width,
    } = request;
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

    /// 按当前 UI 缩放追加控件。
    fn push_scaled(&mut self, control: Option<SubControl>, width: f32) {
        self.push(control, width * self.scale);
    }

    /// 在相邻非空控件组之间追加一条分隔线。
    ///
    /// 重复调用不会生成连续分隔线，也不会在面板开头生成空白槽位。
    fn separator(&mut self) {
        if self
            .entries
            .last()
            .is_some_and(|entry| entry.control.is_some())
        {
            self.push_scaled(None, 12.0);
        }
    }

    /// 移除末尾分隔线并回退游标，保证二级栏最后一项始终是可交互控件。
    fn trim_trailing_separators(&mut self) {
        while self
            .entries
            .last()
            .is_some_and(|entry| entry.control.is_none())
        {
            if let Some(entry) = self.entries.pop() {
                self.x -= entry.rect.width;
            }
        }
    }
}

/// 按当前工具生成二级栏控件序列。
///
/// 每种工具只暴露与之相关的选项：几何形状提供矩形/椭圆、线型、填充与圆角；
/// 序号提供底板样式；其余画笔类工具只保留线宽与颜色。
struct EntryBuildContext {
    active_tool: Tool,
    mosaic_mode: MosaicMode,
    origin_x: f32,
    center_y: f32,
    scale: f32,
}

fn build_entries(context: EntryBuildContext, entries: &mut Vec<SubEntry>) -> f32 {
    let EntryBuildContext {
        active_tool,
        mosaic_mode,
        origin_x,
        center_y,
        scale,
    } = context;
    let mut cursor = EntryCursor {
        entries,
        x: origin_x,
        center_y,
        scale,
    };

    let descriptor = tool_descriptor(active_tool);
    push_geometry_entries(&mut cursor, descriptor);
    push_mosaic_entries(&mut cursor, descriptor, mosaic_mode);
    push_marker_and_eraser_entries(&mut cursor, descriptor);
    push_spotlight_entries(&mut cursor, descriptor);
    push_magnifier_entries(&mut cursor, descriptor);
    push_standard_entries(&mut cursor, descriptor);
    push_color_palette(&mut cursor, descriptor);

    cursor.trim_trailing_separators();
    // 末尾留出与内边距等宽的余量，避免最后一个色块贴边
    cursor.x + 4.0 * scale
}

/// 追加几何形状与直线模式控件。
fn push_geometry_entries(cursor: &mut EntryCursor<'_>, descriptor: &ToolDescriptor) {
    if descriptor.shape_controls {
        cursor.push_scaled(Some(SubControl::ShapeRect), 30.0);
        cursor.push_scaled(Some(SubControl::ShapeEllipse), 30.0);
        cursor.separator();
    }
    if descriptor.line_mode {
        cursor.push_scaled(Some(SubControl::LineMode), 30.0);
        cursor.separator();
    }
}

/// 追加马赛克模式及其依赖的尺寸控件。
///
/// 智能打码仍使用尺寸和效果强度控制文字区域内的马赛克颗粒度，因此必须保留这两个控件；
/// 画笔与矩形切换只服务于手动涂抹，在智能打码模式下隐藏。
fn push_mosaic_entries(
    cursor: &mut EntryCursor<'_>,
    descriptor: &ToolDescriptor,
    mosaic_mode: MosaicMode,
) {
    if !descriptor.mosaic_mode {
        return;
    }
    let editable = mosaic_mode != MosaicMode::SmartRedact;
    cursor.push_scaled(Some(SubControl::Size), 36.0);
    cursor.push_scaled(Some(SubControl::MosaicMode), 48.0);
    if editable {
        cursor.push_scaled(Some(SubControl::MosaicBrush), 28.0);
        cursor.push_scaled(Some(SubControl::MosaicRectangle), 28.0);
    }
    cursor.push_scaled(Some(SubControl::EffectStrength), 82.0);
    cursor.separator();
}

/// 追加荧光笔与橡皮擦专属控件。
fn push_marker_and_eraser_entries(cursor: &mut EntryCursor<'_>, descriptor: &ToolDescriptor) {
    if descriptor.marker_mode {
        cursor.push_scaled(Some(SubControl::MarkerShape), 44.0);
        cursor.push_scaled(Some(SubControl::MarkerBlendMode), 44.0);
        cursor.separator();
    }
    if descriptor.eraser_mode {
        cursor.push_scaled(Some(SubControl::EraserBrush), 28.0);
        cursor.push_scaled(Some(SubControl::EraserRectangle), 28.0);
    }
}

/// 追加聚光灯形状、遮罩强度与描边控件。
fn push_spotlight_entries(cursor: &mut EntryCursor<'_>, descriptor: &ToolDescriptor) {
    if !descriptor.spotlight_style {
        return;
    }
    cursor.push_scaled(Some(SubControl::SpotlightShapeRect), 30.0);
    cursor.push_scaled(Some(SubControl::SpotlightShapeEllipse), 30.0);
    cursor.separator();
    cursor.push_scaled(Some(SubControl::SpotlightIntensity), 42.0);
    cursor.push_scaled(Some(SubControl::SpotlightOutline), 30.0);
}

/// 追加放大镜形状、倍率、平滑、连接与阴影控件。
fn push_magnifier_entries(cursor: &mut EntryCursor<'_>, descriptor: &ToolDescriptor) {
    if !descriptor.magnifier_options {
        return;
    }
    cursor.push_scaled(Some(SubControl::MagnifierShapeRect), 30.0);
    cursor.push_scaled(Some(SubControl::MagnifierShapeEllipse), 30.0);
    cursor.separator();
    cursor.push_scaled(Some(SubControl::MagnifierScale), 78.0);
    cursor.push_scaled(Some(SubControl::MagnifierSmooth), 30.0);
    cursor.push_scaled(Some(SubControl::MagnifierConnector), 46.0);
    cursor.push_scaled(Some(SubControl::MagnifierShadow), 30.0);
    cursor.separator();
}

/// 按描述元数据追加通用样式控件。
fn push_standard_entries(cursor: &mut EntryCursor<'_>, descriptor: &ToolDescriptor) {
    if descriptor.fill_toggle {
        cursor.push_scaled(Some(SubControl::FillToggle), 30.0);
    }
    if descriptor.line_style {
        cursor.push_scaled(Some(SubControl::LineStyle), 48.0);
    }
    if descriptor.stroke_width && !descriptor.mosaic_mode {
        cursor.push_scaled(Some(SubControl::Size), 36.0);
    }
    if descriptor.font_size {
        cursor.push_scaled(Some(SubControl::FontSize), 44.0);
    }
    if descriptor.text_style {
        push_text_style_entries(cursor);
    }
    if descriptor.corner_radius {
        cursor.push_scaled(Some(SubControl::CornerRadius), 38.0);
    }
    if descriptor.badge_style {
        cursor.push_scaled(Some(SubControl::BadgeStyle), 44.0);
    }
    if descriptor.badge_sequence {
        cursor.push_scaled(Some(SubControl::BadgeSequence), 44.0);
    }
    if descriptor.arrow_style {
        cursor.push_scaled(Some(SubControl::ArrowStyle), 48.0);
    }
    if descriptor.arrow_toggle {
        cursor.push_scaled(Some(SubControl::ArrowEnabled), 30.0);
    }
    if descriptor.arrow_heads {
        cursor.push_scaled(Some(SubControl::StartArrow), 44.0);
        cursor.push_scaled(Some(SubControl::EndArrow), 44.0);
    }
    if descriptor.eraser_mode {
        cursor.separator();
        cursor.push_scaled(Some(SubControl::ClearAnnotations), 30.0);
    }
}

/// 追加文本样式控件。
fn push_text_style_entries(cursor: &mut EntryCursor<'_>) {
    cursor.push_scaled(Some(SubControl::Bold), 28.0);
    cursor.push_scaled(Some(SubControl::Italic), 28.0);
    cursor.push_scaled(Some(SubControl::TextStroke), 30.0);
    cursor.push_scaled(Some(SubControl::TextBackground), 30.0);
    cursor.push_scaled(Some(SubControl::TextBackgroundOpacity), 46.0);
    cursor.push_scaled(Some(SubControl::TextBackgroundPadding), 36.0);
    cursor.push_scaled(Some(SubControl::TextBackgroundCornerRadius), 38.0);
    cursor.push_scaled(Some(SubControl::FontFamily), 44.0);
}

/// 追加预设颜色调色板。
fn push_color_palette(cursor: &mut EntryCursor<'_>, descriptor: &ToolDescriptor) {
    if !descriptor.color_palette {
        return;
    }
    cursor.separator();
    for index in 0..preset_colors().len() {
        cursor.push_scaled(Some(SubControl::Color(index)), 24.0);
    }
}

/// 二级栏布局计算所需的完整输入。
#[derive(Debug, Clone, Copy)]
pub(crate) struct SubLayoutRequest<'a> {
    pub main_rect: &'a Rect,
    pub active_tool: Option<Tool>,
    pub open_dropdown: Option<SubDropdown>,
    pub screen_width: f32,
    pub screen_height: f32,
    pub ui_scale: f32,
    pub options: SubLayoutOptions,
}

/// 计算二级属性栏及其下拉菜单布局。
pub(crate) fn calculate_sub_layout(request: SubLayoutRequest<'_>) -> Option<SubLayout> {
    let active_tool = request.active_tool?;
    if request.main_rect.width <= 0.0 || !tool_descriptor(active_tool).has_sub_bar {
        return None;
    }

    let (sub_rect, entries) = build_sub_panel(request, active_tool);
    let dropdown = request
        .open_dropdown
        .and_then(|kind| build_dropdown_layout(kind, &entries, sub_rect, request));
    Some(SubLayout {
        rect: sub_rect,
        entries,
        dropdown,
    })
}

/// 生成控件序列并根据屏幕宽度计算二级栏最终位置。
fn build_sub_panel(request: SubLayoutRequest<'_>, active_tool: Tool) -> (Rect, Vec<SubEntry>) {
    let scale = request.ui_scale;
    let height = 36.0 * scale;
    let origin_x = request.main_rect.x;
    let origin_y = if request.main_rect.y >= height + 8.0 * scale {
        request.main_rect.y - height - 4.0 * scale
    } else {
        request.main_rect.y + request.main_rect.height + 4.0 * scale
    };
    let center_y = origin_y + height / 2.0;
    let mut entries = Vec::with_capacity(32);
    let end_x = build_entries(
        EntryBuildContext {
            active_tool,
            mosaic_mode: request.options.mosaic_mode,
            origin_x: origin_x + 12.0 * scale,
            center_y,
            scale,
        },
        &mut entries,
    );
    let width = (end_x - origin_x).max(120.0 * scale);
    let max_x = (request.screen_width - width - 8.0 * scale).max(8.0 * scale);
    let clamped_x = origin_x.min(max_x).max(8.0 * scale);
    let shift = clamped_x - origin_x;
    if shift.abs() > f32::EPSILON {
        for entry in &mut entries {
            entry.rect.x += shift;
        }
    }
    (
        Rect {
            x: clamped_x,
            y: origin_y,
            width,
            height,
        },
        entries,
    )
}

/// 根据控件锚点与屏幕边界计算下拉面板和每一行的热区。
fn build_dropdown_layout(
    kind: SubDropdown,
    entries: &[SubEntry],
    sub_rect: Rect,
    request: SubLayoutRequest<'_>,
) -> Option<DropdownLayout> {
    let anchor = dropdown_anchor(kind);
    let anchor_rect = entries
        .iter()
        .find(|entry| entry.control == Some(anchor))?
        .rect;
    let scale = request.ui_scale;
    let rows = dropdown_row_count(kind, request.options.smart_redact_enabled);
    let row_height = 28.0 * scale;
    let panel_height = rows as f32 * row_height + 8.0 * scale;
    let panel_width = 108.0 * scale;
    let origin = dropdown_origin(anchor_rect, sub_rect, panel_height, request);
    let panel = Rect {
        x: origin.0,
        y: origin.1,
        width: panel_width,
        height: panel_height,
    };
    let row_rects = (0..rows)
        .map(|index| Rect {
            x: panel.x + 4.0 * scale,
            y: panel.y + 4.0 * scale + index as f32 * row_height,
            width: panel_width - 8.0 * scale,
            height: row_height,
        })
        .collect();
    Some(DropdownLayout {
        kind,
        rect: panel,
        rows: row_rects,
    })
}

/// 找到下拉菜单对应的二级栏锚点控件。
fn dropdown_anchor(kind: SubDropdown) -> SubControl {
    match kind {
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
    }
}

/// 返回下拉菜单包含的选项数量。
fn dropdown_row_count(kind: SubDropdown, smart_redact_enabled: bool) -> usize {
    match kind {
        SubDropdown::Line => snapin_annotation::LineStyle::ALL.len(),
        SubDropdown::Badge => 2,
        SubDropdown::Arrow => 3,
        SubDropdown::FontSize => crate::toolbar::items::FONT_SIZE_PRESETS.len(),
        SubDropdown::StartArrow | SubDropdown::EndArrow => {
            snapin_annotation::style::ArrowHeadStyle::ALL.len()
        }
        SubDropdown::BadgeSequence => snapin_annotation::BadgeSequence::ALL.len(),
        SubDropdown::MosaicMode => {
            if smart_redact_enabled {
                3
            } else {
                2
            }
        }
        SubDropdown::FontFamily => FONT_FAMILY_PRESETS.len(),
        SubDropdown::MagnifierConnector => snapin_annotation::MagnifierConnector::ALL.len(),
        SubDropdown::MarkerShape | SubDropdown::MarkerBlendMode => 2,
    }
}

/// 计算下拉面板的屏幕内坐标，必要时向上翻转或向左夹取。
fn dropdown_origin(
    anchor_rect: Rect,
    sub_rect: Rect,
    panel_height: f32,
    request: SubLayoutRequest<'_>,
) -> (f32, f32) {
    let scale = request.ui_scale;
    let panel_width = 108.0 * scale;
    let mut y = sub_rect.y + sub_rect.height + 4.0 * scale;
    if y + panel_height > request.screen_height - 8.0 * scale {
        y = (sub_rect.y - panel_height - 4.0 * scale).max(8.0 * scale);
    }
    let mut x = anchor_rect.x;
    if x + panel_width > request.screen_width - 8.0 * scale {
        x = (request.screen_width - panel_width - 8.0 * scale).max(8.0 * scale);
    }
    (x, y)
}

#[cfg(test)]
mod tests;
