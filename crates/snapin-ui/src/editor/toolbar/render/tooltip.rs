//! 主栏按钮悬浮提示：在按钮下方固定位置显示工具名与快捷键。
//!
//! 提示位置锚定在按钮正下方而非跟随光标，避免鼠标在按钮内小幅移动时提示抖动；
//! 靠近画布底部时自动翻转到按钮上方，水平方向夹取在画布内，保证始终完整可见。

use super::super::items::ToolbarItem;
use super::super::Toolbar;
use crate::drawing::add_round_rect;
use crate::geometry::Rect;
use snapin_core::render::with_thread_local_text_renderer;
use tiny_skia::{FillRule, Paint, PathBuilder, PixmapMut, Stroke, Transform};

/// 提示文字字号（界面像素）
const FONT_SIZE: f32 = 12.0;
/// 文字与胶囊边缘的水平内边距（界面像素）
const PADDING_X: f32 = 8.0;
/// 文字与胶囊边缘的垂直内边距（界面像素）
const PADDING_Y: f32 = 4.0;
/// 胶囊与按钮之间的间距（界面像素）
const GAP: f32 = 6.0;
/// 胶囊与画布边缘的最小距离（界面像素）
const MARGIN: f32 = 4.0;
/// 提示胶囊背景色
const FILL: (u8, u8, u8, u8) = (28, 30, 38, 238);
/// 提示胶囊描边色
const BORDER: (u8, u8, u8, u8) = (255, 255, 255, 36);
/// 提示文字色
const TEXT: (u8, u8, u8, u8) = (240, 243, 250, 255);

/// 悬浮提示的几何布局与文字尺寸。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct TooltipLayout {
    /// 胶囊左上角 X
    pub(super) x: f32,
    /// 胶囊左上角 Y
    pub(super) y: f32,
    /// 胶囊宽度
    pub(super) width: f32,
    /// 胶囊高度
    pub(super) height: f32,
    /// 缩放后的文字字号
    pub(super) font_size: f32,
    /// 界面缩放因子
    pub(super) scale: f32,
}

/// 绘制悬浮提示；未悬停任何按钮时不做任何事。
pub(super) fn render_hover_tooltip(toolbar: &Toolbar, pixmap: &mut PixmapMut) {
    let Some(hovered) = toolbar.hovered_item else {
        return;
    };
    let Some(button_rect) = button_rect(toolbar, hovered) else {
        return;
    };
    let message = compose_message(toolbar.displayed_item(hovered), toolbar.show_shortcut_hints);
    let Some(layout) = layout(
        &message,
        &button_rect,
        toolbar.ui_scale,
        pixmap.width() as f32,
        pixmap.height() as f32,
    ) else {
        return;
    };
    draw(pixmap, &message, &layout);
}

/// 查找悬浮按钮在主栏中的矩形。
fn button_rect(toolbar: &Toolbar, hovered: ToolbarItem) -> Option<Rect> {
    super::super::interaction::main_item_rects(
        &toolbar.main_rect,
        toolbar.main_items,
        toolbar.ui_scale,
    )
    .into_iter()
    .find_map(|(item, rect)| (item == hovered).then_some(rect))
}

/// 组合提示文本：名称 +（可选的）快捷键。
fn compose_message(item: ToolbarItem, show_shortcut: bool) -> String {
    let label = item.label();
    match (show_shortcut, item.shortcut()) {
        (true, Some(shortcut)) => format!("{label}  {shortcut}"),
        _ => label.to_string(),
    }
}

/// 计算提示胶囊布局：默认在按钮下方居中，越界时翻转或夹取。
fn layout(
    message: &str,
    button_rect: &Rect,
    ui_scale: f32,
    screen_w: f32,
    screen_h: f32,
) -> Option<TooltipLayout> {
    let scale = ui_scale.max(0.1);
    let font_size = (FONT_SIZE * scale).max(9.0);
    let padding_x = PADDING_X * scale;
    let padding_y = PADDING_Y * scale;

    let (text_width, _) = with_thread_local_text_renderer(|renderer| {
        renderer.measure_text_input(
            message,
            font_size,
            font_size * 1.3,
            screen_w.max(1.0),
            font_size * 1.6,
        )
    });
    if text_width <= 0.0 {
        return None;
    }

    let width = text_width + padding_x * 2.0;
    let height = font_size * 1.4 + padding_y * 2.0;
    let margin = MARGIN * scale;
    let gap = GAP * scale;

    let max_x = (screen_w - width - margin).max(margin);
    let x = (button_rect.x + (button_rect.width - width) / 2.0).clamp(margin, max_x);

    let below = button_rect.y + button_rect.height + gap;
    let above = button_rect.y - height - gap;
    let y = if below + height <= screen_h - margin {
        below
    } else {
        above.max(margin)
    };

    Some(TooltipLayout {
        x,
        y,
        width,
        height,
        font_size,
        scale,
    })
}

/// 绘制胶囊背景、描边与文字。
fn draw(pixmap: &mut PixmapMut, message: &str, layout: &TooltipLayout) {
    let radius = 5.0 * layout.scale;
    let mut path = PathBuilder::new();
    add_round_rect(
        &mut path,
        layout.x,
        layout.y,
        layout.width,
        layout.height,
        radius,
    );
    if let Some(path) = path.finish() {
        let mut paint = Paint::default();
        paint.set_color_rgba8(FILL.0, FILL.1, FILL.2, FILL.3);
        pixmap.fill_path(
            &path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );

        let mut border = Paint::default();
        border.set_color_rgba8(BORDER.0, BORDER.1, BORDER.2, BORDER.3);
        let stroke = Stroke {
            width: layout.scale.max(1.0),
            ..Default::default()
        };
        pixmap.stroke_path(&path, &border, &stroke, Transform::identity(), None);
    }

    with_thread_local_text_renderer(|renderer| {
        renderer.draw_text_rgba(
            pixmap,
            message,
            layout.x + PADDING_X * layout.scale,
            layout.y + PADDING_Y * layout.scale,
            layout.font_size,
            TEXT,
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn button() -> Rect {
        Rect {
            x: 100.0,
            y: 200.0,
            width: 32.0,
            height: 32.0,
        }
    }

    /// 默认位置应固定在按钮正下方，而不是跟随光标。
    #[test]
    fn test_layout_places_tooltip_below_button() {
        let layout =
            layout("撤销  Ctrl+Z", &button(), 1.0, 800.0, 600.0).expect("提示文本应可测量");
        assert!(layout.y > button().y + button().height);
        let button_center = button().x + button().width / 2.0;
        let tooltip_center = layout.x + layout.width / 2.0;
        assert!((button_center - tooltip_center).abs() < 1.0);
    }

    /// 靠近画布底部时翻转到按钮上方，避免被裁切。
    #[test]
    fn test_layout_flips_above_near_bottom_edge() {
        let low = Rect {
            x: 100.0,
            y: 560.0,
            width: 32.0,
            height: 32.0,
        };
        let layout = layout("保存  Ctrl+S", &low, 1.0, 800.0, 600.0).expect("提示文本应可测量");
        assert!(layout.y + layout.height <= low.y);
    }

    /// 贴图工具栏没有截图键位时不附带快捷键。
    #[test]
    fn test_message_omits_shortcut_when_disabled() {
        let item = ToolbarItem::Action(super::super::super::items::ToolbarAction::Undo);
        assert_eq!(compose_message(item, true), "撤销  Ctrl+Z");
        assert_eq!(compose_message(item, false), "撤销");
        assert_eq!(
            compose_message(
                ToolbarItem::Tool(super::super::super::items::Tool::Rect),
                true
            ),
            "矩形"
        );
    }

    /// 实际渲染应在按钮下方留下可见的提示胶囊像素。
    #[test]
    fn test_render_draws_tooltip_below_button() {
        use tiny_skia::Pixmap;

        let toolbar = Toolbar {
            ui_scale: 1.0,
            main_rect: Rect {
                x: 10.0,
                y: 10.0,
                width: 300.0,
                height: 44.0,
            },
            hovered_item: Some(ToolbarItem::Tool(super::super::super::items::Tool::Rect)),
            ..Toolbar::default()
        };

        let mut pixmap = Pixmap::new(400, 200).expect("创建测试位图失败");
        let mut pixmap_mut = pixmap.as_mut();
        render_hover_tooltip(&toolbar, &mut pixmap_mut);

        let has_pixel_below = (54..200)
            .any(|y| (0..400).any(|x| pixmap.pixel(x, y).is_some_and(|pixel| pixel.alpha() > 0)));
        assert!(has_pixel_below, "按钮下方应绘制悬浮提示");
    }
}
