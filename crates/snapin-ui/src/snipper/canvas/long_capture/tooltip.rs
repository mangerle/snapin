//! 长截图控制栏与子菜单悬浮提示气泡布局及光栅化渲染。
//!
//! 之所以将气泡布局独立成纯计算模块并用真实字体排版测量宽度，
//! 是为了同时消除两类典型失真：其一是按字数硬乘估算导致英文或标点溢出边框，
//! 其二是控制栏靠近屏幕底部时提示框仍死板向下偏移而飞出可视区域。

use super::CanvasSize;
use crate::drawing::{add_round_rect, RoundRect};
use crate::geometry::Rect;
use snapin_foundation::i18n::tr;
use snapin_render::{with_thread_local_text_renderer, TextDrawOptions, TextMeasureOptions};
use tiny_skia::{FillRule, Paint, PathBuilder, PixmapMut, Stroke, Transform};

/// 悬浮提示气泡绘制参数。
pub(crate) struct TooltipSpec<'a> {
    pub(crate) target_rect: &'a Rect,
    pub(crate) text: &'a str,
    pub(crate) screen_size: CanvasSize,
    pub(crate) ui_scale: f32,
    pub(crate) prefer_above: bool,
}

/// 在按钮附近绘制经过文本测宽与全向视口夹取的圆角气泡提示。
pub(crate) fn draw_tooltip_bubble(pixmap: &mut PixmapMut, spec: TooltipSpec<'_>) {
    let s = spec.ui_scale;
    let font_size = 11.0 * s;
    let localized = tr(spec.text);
    let text_width = with_thread_local_text_renderer(|renderer| {
        renderer
            .measure_text_input(
                localized,
                TextMeasureOptions::new(font_size, font_size * 1.35, 2000.0 * s, font_size * 2.0),
            )
            .max_line_width
    });
    let bubble_rect = resolve_tooltip_rect(
        spec.target_rect,
        text_width,
        spec.screen_size,
        s,
        spec.prefer_above,
    );

    let mut pb = PathBuilder::new();
    add_round_rect(
        &mut pb,
        RoundRect {
            x: bubble_rect.x,
            y: bubble_rect.y,
            width: bubble_rect.width,
            height: bubble_rect.height,
            radius: 3.5 * s,
        },
    );
    if let Some(path) = pb.finish() {
        let mut bg_paint = Paint::default();
        bg_paint.set_color_rgba8(255, 255, 255, 252);
        pixmap.fill_path(
            &path,
            &bg_paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );

        let mut stroke_paint = Paint::default();
        stroke_paint.set_color_rgba8(0, 0, 0, 36);
        let stroke = Stroke {
            width: 1.0 * s,
            ..Default::default()
        };
        pixmap.stroke_path(&path, &stroke_paint, &stroke, Transform::identity(), None);
    }

    with_thread_local_text_renderer(|renderer| {
        renderer.draw_text_rgba(
            pixmap,
            localized,
            TextDrawOptions::new(
                bubble_rect.x + 7.0 * s,
                bubble_rect.y + 3.5 * s,
                font_size,
                (30, 41, 59, 255),
            ),
        );
    });
}

/// 计算并夹取气泡提示框的最终屏幕矩形：默认优先在目标下方显示，触底或被要求避让时自动向上翻转。
pub(crate) fn resolve_tooltip_rect(
    target_rect: &Rect,
    text_width: f32,
    screen_size: CanvasSize,
    s: f32,
    prefer_above: bool,
) -> Rect {
    let margin = 6.0 * s;
    let max_bubble_w = ((screen_size.width as f32) - margin * 2.0).max(24.0 * s);
    let bubble_w = (text_width + 14.0 * s).min(max_bubble_w);
    let bubble_h = 22.0 * s;

    let max_x = ((screen_size.width as f32) - bubble_w - margin).max(margin);
    let bubble_x = (target_rect.x + (target_rect.width - bubble_w) / 2.0).clamp(margin, max_x);

    let below_y = target_rect.y + target_rect.height + 4.0 * s;
    let above_y = target_rect.y - bubble_h - 4.0 * s;
    let max_y = ((screen_size.height as f32) - bubble_h - margin).max(margin);

    let preferred_y = if prefer_above {
        if above_y >= margin || below_y > max_y {
            above_y
        } else {
            below_y
        }
    } else if below_y <= max_y || above_y < margin {
        below_y
    } else {
        above_y
    };

    Rect {
        x: bubble_x,
        y: preferred_y.clamp(margin, max_y),
        width: bubble_w,
        height: bubble_h,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tooltip_flips_above_at_bottom_and_clamps_horizontally() {
        let screen = CanvasSize {
            width: 800,
            height: 600,
        };
        let near_bottom_right = Rect::new(760.0, 565.0, 28.0, 28.0);
        let rect = resolve_tooltip_rect(&near_bottom_right, 80.0, screen, 1.0, false);

        assert!(
            rect.y + rect.height <= (screen.height as f32) - 6.0,
            "触底时提示必须留在屏幕内，实际 y={}",
            rect.y
        );
        assert!(
            rect.y < near_bottom_right.y,
            "底部空间不足时应自动翻转到按钮上方"
        );
        assert!(
            rect.x + rect.width <= (screen.width as f32) - 6.0 && rect.x >= 6.0,
            "提示框右边缘超出屏幕: {:?}",
            rect
        );
    }

    #[test]
    fn test_tooltip_clamps_safely_on_tiny_canvas_without_panic() {
        let tiny = CanvasSize {
            width: 100,
            height: 20,
        };
        let btn = Rect::new(10.0, 2.0, 28.0, 16.0);
        let rect = resolve_tooltip_rect(&btn, 200.0, tiny, 1.0, false);
        assert!(rect.x >= 6.0);
        assert!(rect.y >= 6.0);
        assert!(rect.width <= 88.0);
    }
}
