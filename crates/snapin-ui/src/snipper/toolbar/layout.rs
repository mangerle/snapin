//! 截图工具栏与二级属性栏自适应布局计算。
//!
//! 主栏根据选区与屏幕边界自适应定位；二级栏按当前工具的可用选项生成控件序列，
//! 控件坐标只在此处计算一次，渲染与命中测试共用同一份结果，避免两处各写一套坐标而漂移。

use crate::snipper::selection::Rect;
use crate::snipper::toolbar::items::{
    preset_colors, StrokeWidthLevel, ToolbarItem, CORNER_RADIUS_PRESETS,
};
use crate::snipper::toolbar::metrics::{button_size, main_toolbar_width, padding};

/// 二级栏下拉菜单种类
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SubDropdown {
    /// 描边线型（实线/虚线/点线）
    Line,
    /// 序号底板样式（实心/空心）
    Badge,
    /// 箭头形态样式（实线/虚线/锥形）
    Arrow,
    /// 圆角半径预设
    Corner,
}

/// 二级栏可交互控件
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SubControl {
    ShapeRect,
    ShapeEllipse,
    Stroke(StrokeWidthLevel),
    LineStyle,
    FillToggle,
    CornerRadius,
    BadgeStyle,
    ArrowStyle,
    /// 预设颜色，参数为调色板下标
    Color(usize),
}

/// 二级栏中的一个布局槽位（控件或纯视觉分隔线）
#[derive(Debug, Clone, Copy)]
pub(crate) struct SubEntry {
    /// `None` 表示分隔线等不可交互的占位
    pub control: Option<SubControl>,
    pub rect: Rect,
}

/// 下拉菜单布局
#[derive(Debug, Clone)]
pub(crate) struct DropdownLayout {
    pub kind: SubDropdown,
    pub rect: Rect,
    /// 每一行选项的热区，顺序与渲染顺序一致
    pub rows: Vec<Rect>,
}

/// 二级栏完整布局：面板、控件序列与可选的下拉菜单
#[derive(Debug, Clone)]
pub(crate) struct SubLayout {
    pub rect: Rect,
    pub entries: Vec<SubEntry>,
    pub dropdown: Option<DropdownLayout>,
}

impl SubLayout {
    /// 命中某个可交互控件
    pub fn hit(&self, px: f32, py: f32) -> Option<SubControl> {
        self.entries
            .iter()
            .find(|entry| entry.rect.contains(px, py))
            .and_then(|entry| entry.control)
    }

    /// 获取指定控件的热区
    pub fn rect_of(&self, control: SubControl) -> Option<Rect> {
        self.entries
            .iter()
            .find(|entry| entry.control == Some(control))
            .map(|entry| entry.rect)
    }

    /// 下拉菜单是否包含该坐标（含面板与行）
    pub fn dropdown_contains(&self, px: f32, py: f32) -> bool {
        self.dropdown
            .as_ref()
            .is_some_and(|dropdown| dropdown.rect.contains(px, py))
    }

    /// 命中下拉菜单中的第几行
    pub fn dropdown_row_hit(&self, px: f32, py: f32) -> Option<usize> {
        let dropdown = self.dropdown.as_ref()?;
        dropdown.rows.iter().position(|row| row.contains(px, py))
    }
}

/// 根据选区与屏幕宽高重新计算主工具栏在屏幕上的自适应摆放位置
pub(crate) fn calculate_main_layout(
    selection: &Rect,
    screen_w: f32,
    screen_h: f32,
    ui_scale: f32,
    items: &[ToolbarItem],
) -> Rect {
    let s = ui_scale;
    let width = main_toolbar_width(items, s);
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
    active_tool: ToolbarItem,
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

    let is_geo = matches!(
        active_tool,
        ToolbarItem::ToolRect | ToolbarItem::ToolEllipse
    );
    if is_geo {
        cursor.push(Some(SubControl::ShapeRect), 26.0 * s);
        cursor.push(Some(SubControl::ShapeEllipse), 26.0 * s);
        cursor.push(None, 16.0 * s);
    }

    for level in [
        StrokeWidthLevel::Thin,
        StrokeWidthLevel::Medium,
        StrokeWidthLevel::Thick,
    ] {
        cursor.push(Some(SubControl::Stroke(level)), 24.0 * s);
    }

    if is_geo {
        cursor.push(None, 16.0 * s);
        cursor.push(Some(SubControl::LineStyle), 52.0 * s);
        cursor.push(Some(SubControl::FillToggle), 28.0 * s);
        if active_tool == ToolbarItem::ToolRect {
            cursor.push(Some(SubControl::CornerRadius), 44.0 * s);
        }
    }

    if active_tool == ToolbarItem::ToolBadge {
        cursor.push(None, 16.0 * s);
        cursor.push(Some(SubControl::BadgeStyle), 44.0 * s);
    }

    if active_tool == ToolbarItem::ToolArrow {
        cursor.push(None, 16.0 * s);
        cursor.push(Some(SubControl::ArrowStyle), 52.0 * s);
    }

    cursor.push(None, 16.0 * s);
    // 模糊工具只使用粗细档位调节半径，颜色与模糊无关。
    if active_tool != ToolbarItem::ToolBlur {
        for (index, _) in preset_colors().iter().enumerate() {
            cursor.push(Some(SubControl::Color(index)), 26.0 * s);
        }
    }

    // 末尾留出与内边距等宽的余量，避免最后一个色块贴边
    cursor.x + 4.0 * s
}

/// 计算二级属性栏及其下拉菜单布局
pub(crate) fn calculate_sub_layout(
    main_rect: &Rect,
    active_tool: Option<ToolbarItem>,
    open_dropdown: Option<SubDropdown>,
    screen_w: f32,
    screen_h: f32,
    ui_scale: f32,
) -> Option<SubLayout> {
    let active_tool = active_tool?;
    if main_rect.width <= 0.0 {
        return None;
    }
    // 聚光灯没有可调参数，不显示空的二级属性栏。
    if active_tool == ToolbarItem::ToolSpotlight {
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

    let mut entries = Vec::with_capacity(16);
    let pad = 12.0 * s;
    let end_x = build_entries(active_tool, sub_x + pad, center_y, s, &mut entries);
    let sub_rect = Rect {
        x: sub_x,
        y: sub_y,
        width: (end_x - sub_x).max(120.0 * s),
        height: sub_h,
    };

    let dropdown = open_dropdown.and_then(|kind| {
        let anchor = match kind {
            SubDropdown::Line => SubControl::LineStyle,
            SubDropdown::Badge => SubControl::BadgeStyle,
            SubDropdown::Arrow => SubControl::ArrowStyle,
            SubDropdown::Corner => SubControl::CornerRadius,
        };
        let anchor_rect = entries
            .iter()
            .find(|entry| entry.control == Some(anchor))?
            .rect;

        let rows = match kind {
            SubDropdown::Line => 3,
            SubDropdown::Badge => 2,
            SubDropdown::Arrow => 3,
            SubDropdown::Corner => CORNER_RADIUS_PRESETS.len(),
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
mod tests {
    use super::*;

    fn main_rect() -> Rect {
        Rect {
            x: 100.0,
            y: 400.0,
            width: 600.0,
            height: 40.0,
        }
    }

    /// 全屏选区的工具条必须回落到屏幕内，不能停到屏幕底边之外
    #[test]
    fn test_fullscreen_selection_toolbar_inside_screen() {
        use crate::snipper::toolbar::Toolbar;

        let selection = Rect {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
        };
        let layout = calculate_main_layout(&selection, 1920.0, 1080.0, 1.0, Toolbar::MAIN_ITEMS);

        assert!(layout.x >= 8.0);
        assert!(layout.y >= 8.0);
        assert!(layout.x + layout.width <= 1920.0 - 8.0 + 0.01);
        assert!(layout.y + layout.height <= 1080.0 - 8.0 + 0.01);
    }

    /// 每种工具只暴露相关控件：矩形有圆角、椭圆没有，序号有底板样式
    #[test]
    fn test_sub_controls_follow_active_tool() {
        let main = main_rect();

        let rect_layout = calculate_sub_layout(
            &main,
            Some(ToolbarItem::ToolRect),
            None,
            1920.0,
            1080.0,
            1.0,
        )
        .expect("矩形工具应生成二级栏");
        assert!(rect_layout
            .entries
            .iter()
            .any(|e| e.control == Some(SubControl::CornerRadius)));
        assert!(rect_layout
            .entries
            .iter()
            .any(|e| e.control == Some(SubControl::FillToggle)));

        let ellipse_layout = calculate_sub_layout(
            &main,
            Some(ToolbarItem::ToolEllipse),
            None,
            1920.0,
            1080.0,
            1.0,
        )
        .expect("椭圆工具应生成二级栏");
        assert!(!ellipse_layout
            .entries
            .iter()
            .any(|e| e.control == Some(SubControl::CornerRadius)));

        let badge_layout = calculate_sub_layout(
            &main,
            Some(ToolbarItem::ToolBadge),
            None,
            1920.0,
            1080.0,
            1.0,
        )
        .expect("序号工具应生成二级栏");
        assert!(badge_layout
            .entries
            .iter()
            .any(|e| e.control == Some(SubControl::BadgeStyle)));
        assert!(!badge_layout
            .entries
            .iter()
            .any(|e| e.control == Some(SubControl::LineStyle)));
    }

    /// 控件热区必须落在面板内且互不重叠，面板宽度应覆盖到最后一个控件
    #[test]
    fn test_sub_layout_hit_areas_are_inside_panel_and_disjoint() {
        let layout = calculate_sub_layout(
            &main_rect(),
            Some(ToolbarItem::ToolRect),
            None,
            1920.0,
            1080.0,
            1.0,
        )
        .expect("应生成二级栏");

        let interactive: Vec<&Rect> = layout
            .entries
            .iter()
            .filter(|e| e.control.is_some())
            .map(|e| &e.rect)
            .collect();
        assert!(interactive.len() >= 9, "矩形工具应至少有 9 个可交互控件");

        for rect in &interactive {
            assert!(
                rect.x >= layout.rect.x && rect.x + rect.width <= layout.rect.x + layout.rect.width,
                "控件超出面板范围: {rect:?} 面板: {:?}",
                layout.rect
            );
        }
        for (i, a) in interactive.iter().enumerate() {
            for b in interactive.iter().skip(i + 1) {
                let overlap = a.x < b.x + b.width
                    && b.x < a.x + a.width
                    && a.y < b.y + b.height
                    && b.y < a.y + a.height;
                assert!(!overlap, "控件热区重叠: {a:?} 与 {b:?}");
            }
        }
    }

    /// 下拉菜单应锚定在对应控件附近，且行热区完全落在面板内
    #[test]
    fn test_dropdown_rows_stay_inside_panel() {
        let layout = calculate_sub_layout(
            &main_rect(),
            Some(ToolbarItem::ToolRect),
            Some(SubDropdown::Line),
            1920.0,
            1080.0,
            1.0,
        )
        .expect("应生成二级栏");

        let dropdown = layout.dropdown.as_ref().expect("应生成线型下拉菜单");
        assert_eq!(dropdown.rows.len(), 3, "线型下拉应有实线/虚线/点线三行");
        for row in &dropdown.rows {
            assert!(
                row.x >= dropdown.rect.x
                    && row.x + row.width <= dropdown.rect.x + dropdown.rect.width
                    && row.y >= dropdown.rect.y
                    && row.y + row.height <= dropdown.rect.y + dropdown.rect.height,
                "下拉行超出面板: {row:?}"
            );
        }
        assert!(
            layout.dropdown_row_hit(dropdown.rows[1].x + 2.0, dropdown.rows[1].y + 2.0) == Some(1)
        );
    }
}
