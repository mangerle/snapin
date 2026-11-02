//! 二级属性栏通用控件绘制辅助函数。

use super::super::icons::{add_round_rect, make_paint, RoundRect};
use super::super::layout::{SubControl, SubDropdown};
use super::super::Toolbar;
use super::widgets::{
    fill_button_highlight, fill_circle, render_badge_style_icon, render_corner_radius_icon,
};
use crate::geometry::Rect;
use snapin_render::with_thread_local_text_renderer;
use tiny_skia::{Color, PathBuilder, PixmapMut, Stroke, Transform};

pub(super) fn render_toggle_text(
    pixmap: &mut PixmapMut,
    rect: &Rect,
    s: f32,
    label: &'static str,
    active: bool,
) {
    if active {
        fill_button_highlight(pixmap, rect, Color::from_rgba8(37, 99, 235, 45), s);
    }
    with_thread_local_text_renderer(|renderer| {
        renderer.draw_text_rgba(
            pixmap,
            snapin_foundation::i18n::tr(label),
            rect.x + 7.0 * s,
            rect.y + 5.0 * s,
            12.0 * s,
            if active {
                (37, 99, 235, 255)
            } else {
                (45, 48, 56, 255)
            },
        );
    });
}

/// 绘制数值控件；尺寸与透明度均通过点击循环、滚轮连续调节。
pub(super) fn render_numeric(
    pixmap: &mut PixmapMut,
    rect: &Rect,
    center_y: f32,
    s: f32,
    label: &str,
    active: bool,
) {
    if active {
        fill_button_highlight(pixmap, rect, Color::from_rgba8(37, 99, 235, 36), s);
    }
    with_thread_local_text_renderer(|renderer| {
        renderer.draw_text_rgba(
            pixmap,
            label,
            rect.x + 7.0 * s,
            center_y - 6.0 * s,
            11.0 * s,
            if active {
                (37, 99, 235, 255)
            } else {
                (45, 48, 56, 255)
            },
        );
    });
}

/// 将 0 到 255 的压暗 alpha 显示为 0% 到 100%。
pub(super) fn spotlight_opacity_percent(intensity: u8) -> u8 {
    ((u16::from(intensity) * 100 + 127) / 255) as u8
}

/// 绘制下拉按钮右侧的展开箭头
pub(super) fn render_caret(pixmap: &mut PixmapMut, cx: f32, cy: f32, s: f32) {
    let mut pb = PathBuilder::new();
    pb.move_to(cx - 2.5 * s, cy - 1.5 * s);
    pb.line_to(cx, cy + 1.5 * s);
    pb.line_to(cx + 2.5 * s, cy - 1.5 * s);
    if let Some(path) = pb.finish() {
        let stroke = Stroke {
            width: 1.4 * s,
            ..Default::default()
        };
        pixmap.stroke_path(
            &path,
            &make_paint(Color::from_rgba8(100, 106, 120, 255)),
            &stroke,
            Transform::identity(),
            None,
        );
    }
}

/// 绘制“单圆点 + 数值”的尺寸控件，圆点直径随数值同步缩放。
pub(super) fn render_size_with_dot(
    pixmap: &mut PixmapMut,
    rect: &Rect,
    center_y: f32,
    s: f32,
    value: f32,
    focused: bool,
) {
    if focused {
        fill_button_highlight(pixmap, rect, Color::from_rgba8(37, 99, 235, 36), s);
    }
    let dot_radius = (1.0 + value.clamp(1.0, 48.0) / 48.0 * 8.0) * s;
    fill_circle(
        pixmap,
        rect.x + 8.0 * s,
        center_y,
        dot_radius,
        Color::from_rgba8(45, 48, 56, 255),
    );
    with_thread_local_text_renderer(|renderer| {
        renderer.draw_text_rgba(
            pixmap,
            &format!("{value:.0}"),
            rect.x + 24.0 * s,
            center_y - 6.0 * s,
            11.0 * s,
            (45, 48, 56, 255),
        );
    });
}

/// 绘制“圆角图标 + 数值”的控件，数值由聚焦后的滚轮连续调整。
pub(super) fn render_corner_radius_with_value(
    pixmap: &mut PixmapMut,
    rect: &Rect,
    center_y: f32,
    s: f32,
    value: f32,
    focused: bool,
) {
    if focused {
        fill_button_highlight(pixmap, rect, Color::from_rgba8(37, 99, 235, 36), s);
    }
    render_corner_radius_icon(pixmap, rect.x + 10.0 * s, center_y, s, value);
    with_thread_local_text_renderer(|renderer| {
        renderer.draw_text_rgba(
            pixmap,
            &format!("{value:.0}"),
            rect.x + 24.0 * s,
            center_y - 6.0 * s,
            11.0 * s,
            (45, 48, 56, 255),
        );
    });
}

/// 绘制效果强度滑块；左侧为小圆点，右侧为大圆点，滑块位置随数值变化。
pub(super) fn render_effect_strength_slider(
    pixmap: &mut PixmapMut,
    rect: &Rect,
    center_y: f32,
    s: f32,
    value: u8,
    focused: bool,
) {
    if focused {
        fill_button_highlight(pixmap, rect, Color::from_rgba8(37, 99, 235, 36), s);
    }
    let left = rect.x + 12.0 * s;
    let right = rect.x + rect.width - 12.0 * s;
    let ratio = (f32::from(value.clamp(16, 160)) - 16.0) / 144.0;
    let knob_x = left + (right - left) * ratio;
    let mut path = PathBuilder::new();
    path.move_to(left, center_y);
    path.line_to(right, center_y);
    if let Some(path) = path.finish() {
        pixmap.stroke_path(
            &path,
            &make_paint(Color::from_rgba8(165, 170, 182, 255)),
            &Stroke {
                width: 2.0 * s,
                line_cap: tiny_skia::LineCap::Round,
                ..Default::default()
            },
            Transform::identity(),
            None,
        );
    }
    fill_circle(
        pixmap,
        left,
        center_y,
        2.0 * s,
        Color::from_rgba8(120, 126, 140, 255),
    );
    fill_circle(
        pixmap,
        right,
        center_y,
        5.0 * s,
        Color::from_rgba8(120, 126, 140, 255),
    );
    fill_circle(
        pixmap,
        knob_x,
        center_y,
        5.0 * s,
        Color::from_rgba8(37, 99, 235, 255),
    );
}

/// 绘制放大镜倍率滑块，范围固定为 1.0 到 8.0。
pub(super) fn render_magnifier_scale_slider(
    pixmap: &mut PixmapMut,
    rect: &Rect,
    center_y: f32,
    s: f32,
    scale: f32,
    focused: bool,
) {
    if focused {
        fill_button_highlight(pixmap, rect, Color::from_rgba8(37, 99, 235, 36), s);
    }
    with_thread_local_text_renderer(|renderer| {
        renderer.draw_text_rgba(
            pixmap,
            &format!("{scale:.1}x"),
            rect.x + 7.0 * s,
            center_y - 6.0 * s,
            10.5 * s,
            (45, 48, 56, 255),
        );
    });
    let left = rect.x + 42.0 * s;
    let right = rect.x + rect.width - 12.0 * s;
    let ratio = ((scale.clamp(1.0, 8.0) - 1.0) / 7.0).clamp(0.0, 1.0);
    let knob_x = left + (right - left) * ratio;
    let mut path = PathBuilder::new();
    path.move_to(left, center_y);
    path.line_to(right, center_y);
    if let Some(path) = path.finish() {
        pixmap.stroke_path(
            &path,
            &make_paint(Color::from_rgba8(165, 170, 182, 255)),
            &Stroke {
                width: 2.0 * s,
                line_cap: tiny_skia::LineCap::Round,
                ..Default::default()
            },
            Transform::identity(),
            None,
        );
    }
    fill_circle(
        pixmap,
        knob_x,
        center_y,
        5.0 * s,
        Color::from_rgba8(37, 99, 235, 255),
    );
}

/// 绘制“仅边框 / 边框+填充”开关图标
pub(super) fn render_fill_toggle(
    pixmap: &mut PixmapMut,
    cx: f32,
    cy: f32,
    s: f32,
    filled: bool,
    color: Color,
) {
    let half = 7.0 * s;
    let mut pb = PathBuilder::new();
    add_round_rect(
        &mut pb,
        RoundRect {
            x: cx - half,
            y: cy - half,
            width: half * 2.0,
            height: half * 2.0,
            radius: 2.0 * s,
        },
    );
    if let Some(path) = pb.finish() {
        if filled {
            pixmap.fill_path(
                &path,
                &make_paint(color),
                tiny_skia::FillRule::Winding,
                Transform::identity(),
                None,
            );
        } else {
            let stroke = Stroke {
                width: 1.8 * s,
                ..Default::default()
            };
            pixmap.stroke_path(
                &path,
                &make_paint(Color::from_rgba8(90, 96, 110, 255)),
                &stroke,
                Transform::identity(),
                None,
            );
        }
    }
}

/// 绘制二级属性栏中的微型矩形形状图标
pub(super) fn render_sub_shape_rect(
    pixmap: &mut PixmapMut,
    cx: f32,
    cy: f32,
    s: f32,
    is_active: bool,
) {
    let mut pb = PathBuilder::new();
    add_round_rect(
        &mut pb,
        RoundRect {
            x: cx - 6.5 * s,
            y: cy - 5.5 * s,
            width: 13.0 * s,
            height: 11.0 * s,
            radius: 1.5 * s,
        },
    );
    if let Some(path) = pb.finish() {
        let color = if is_active {
            Color::from_rgba8(37, 99, 235, 255)
        } else {
            Color::from_rgba8(75, 85, 99, 255)
        };
        let paint = make_paint(color);
        let stroke = Stroke {
            width: 1.6 * s,
            ..Default::default()
        };
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
}

/// 绘制二级属性栏中的微型椭圆形状图标
pub(super) fn render_sub_shape_ellipse(
    pixmap: &mut PixmapMut,
    cx: f32,
    cy: f32,
    s: f32,
    is_active: bool,
) {
    let mut pb = PathBuilder::new();
    let a = 6.5 * s;
    let b = 5.5 * s;
    let ox = a * 0.552_284_8_f32;
    let oy = b * 0.552_284_8_f32;
    pb.move_to(cx - a, cy);
    pb.cubic_to(cx - a, cy - oy, cx - ox, cy - b, cx, cy - b);
    pb.cubic_to(cx + ox, cy - b, cx + a, cy - oy, cx + a, cy);
    pb.cubic_to(cx + a, cy + oy, cx + ox, cy + b, cx, cy + b);
    pb.cubic_to(cx - ox, cy + b, cx - a, cy + oy, cx - a, cy);
    pb.close();

    if let Some(path) = pb.finish() {
        let color = if is_active {
            Color::from_rgba8(37, 99, 235, 255)
        } else {
            Color::from_rgba8(75, 85, 99, 255)
        };
        let paint = make_paint(color);
        let stroke = Stroke {
            width: 1.6 * s,
            ..Default::default()
        };
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
}

/// 判断控件是否属于序号注释专用设置。
pub(super) fn is_badge_control(control: SubControl) -> bool {
    matches!(control, SubControl::BadgeStyle | SubControl::BadgeSequence)
}

/// 绘制序号注释专用设置控件。
pub(super) fn render_badge_control(
    toolbar: &Toolbar,
    control: SubControl,
    pixmap: &mut PixmapMut,
    rect: &Rect,
    center_y: f32,
    s: f32,
    highlight: Color,
) {
    match control {
        SubControl::BadgeStyle => {
            let open = toolbar.open_dropdown == Some(SubDropdown::Badge);
            if open {
                fill_button_highlight(pixmap, rect, highlight, s);
            }
            render_badge_style_icon(
                pixmap,
                rect.x + 12.0 * s,
                center_y,
                s,
                toolbar.sub_options.badge_style,
                toolbar.sub_options.color,
            );
            render_caret(pixmap, rect.x + rect.width - 8.0 * s, center_y, s);
        }
        SubControl::BadgeSequence => {
            with_thread_local_text_renderer(|renderer| {
                renderer.draw_text_rgba(
                    pixmap,
                    snapin_foundation::i18n::tr("序号"),
                    rect.x + 7.0 * s,
                    rect.y + 5.0 * s,
                    12.0 * s,
                    (45, 48, 56, 255),
                );
            });
            render_caret(pixmap, rect.x + rect.width - 9.0 * s, center_y, s);
        }
        _ => {}
    }
}
