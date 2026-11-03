//! 二级属性栏通用控件绘制辅助函数。

use super::super::icons::{add_round_rect, make_paint, RoundRect};
use super::super::layout::{SubControl, SubDropdown};
use super::super::Toolbar;
use super::sub_bar_icons::{
    render_badge_sequence_icon, render_magnifier_smooth_icon, SubBarIconSpec,
};
use super::widgets::{
    fill_button_highlight, fill_circle, render_badge_style_icon, render_corner_radius_icon,
    BadgeStyleIconSpec, CircleSpec, CornerRadiusIconSpec,
};
use crate::geometry::Rect;
use snapin_render::{with_thread_local_text_renderer, CenteredTextOptions, TextDrawOptions};
use tiny_skia::{Color, PathBuilder, PixmapMut, Stroke, Transform};

/// 二级栏控件的共享几何与高亮状态。
///
/// 控件矩形、垂直中心、缩放与高亮色在所有二级栏绘制函数中始终成套出现；
/// 收敛为值对象后，调用点不会因字段顺序调整而把中心线与缩放比例混淆。
#[derive(Clone, Copy)]
pub(super) struct SubBarGeometry {
    pub(super) rect: Rect,
    pub(super) center_y: f32,
    pub(super) scale: f32,
    pub(super) highlight: Color,
}

pub(super) fn render_toggle_glyph(
    pixmap: &mut PixmapMut,
    geometry: SubBarGeometry,
    glyph: &'static str,
    active: bool,
) {
    let rect = geometry.rect;
    let color = if active {
        Color::from_rgba8(37, 99, 235, 255)
    } else {
        Color::from_rgba8(45, 48, 56, 255)
    };
    if active {
        fill_button_highlight(
            pixmap,
            &rect,
            Color::from_rgba8(37, 99, 235, 45),
            geometry.scale,
        );
    }
    with_thread_local_text_renderer(|renderer| {
        renderer.draw_centered_bold_text(
            pixmap,
            glyph,
            CenteredTextOptions {
                center: (rect.x + rect.width / 2.0, geometry.center_y),
                font_size: 12.0 * geometry.scale,
                max_extent: rect.width.max(1.0),
                rgba: (
                    (color.red() * 255.0).round() as u8,
                    (color.green() * 255.0).round() as u8,
                    (color.blue() * 255.0).round() as u8,
                    (color.alpha() * 255.0).round() as u8,
                ),
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
    geometry: SubBarGeometry,
    value: f32,
    focused: bool,
) {
    let (rect, center_y, s) = (&geometry.rect, geometry.center_y, geometry.scale);
    if focused {
        fill_button_highlight(pixmap, rect, Color::from_rgba8(37, 99, 235, 36), s);
    }
    let dot_radius = (1.0 + value.clamp(1.0, 48.0) / 48.0 * 8.0) * s;
    fill_circle(
        pixmap,
        CircleSpec {
            center_x: rect.x + 10.0 * s,
            center_y,
            radius: dot_radius,
            color: Color::from_rgba8(45, 48, 56, 255),
        },
    );
    with_thread_local_text_renderer(|renderer| {
        renderer.draw_text_rgba(
            pixmap,
            &format!("{value:.0}"),
            TextDrawOptions::new(
                rect.x + 20.0 * s,
                center_y - 6.0 * s,
                11.0 * s,
                (45, 48, 56, 255),
            ),
        );
    });
}

/// 绘制“圆角图标 + 数值”的控件，数值由聚焦后的滚轮连续调整。
pub(super) fn render_corner_radius_with_value(
    pixmap: &mut PixmapMut,
    geometry: SubBarGeometry,
    value: f32,
    focused: bool,
) {
    let (rect, center_y, s) = (&geometry.rect, geometry.center_y, geometry.scale);
    if focused {
        fill_button_highlight(pixmap, rect, Color::from_rgba8(37, 99, 235, 36), s);
    }
    render_corner_radius_icon(
        pixmap,
        CornerRadiusIconSpec {
            center_x: rect.x + 10.0 * s,
            center_y,
            scale: s,
            corner_radius: value,
        },
    );
    with_thread_local_text_renderer(|renderer| {
        renderer.draw_text_rgba(
            pixmap,
            &format!("{value:.0}"),
            TextDrawOptions::new(
                rect.x + 21.0 * s,
                center_y - 6.0 * s,
                11.0 * s,
                (45, 48, 56, 255),
            ),
        );
    });
}

/// 效果强度滑块的轨道与滑块几何。
#[derive(Clone, Copy)]
struct EffectSliderGeometry {
    left: f32,
    right: f32,
    center_y: f32,
    scale: f32,
    knob_x: f32,
}

/// 绘制效果强度滑块；左侧为小圆点，右侧为大圆点，滑块位置随数值变化。
pub(super) fn render_effect_strength_slider(
    pixmap: &mut PixmapMut,
    geometry: SubBarGeometry,
    value: u8,
    focused: bool,
) {
    let rect = &geometry.rect;
    let center_y = geometry.center_y;
    let scale = geometry.scale;
    if focused {
        fill_button_highlight(pixmap, rect, Color::from_rgba8(37, 99, 235, 36), scale);
    }
    let left = rect.x + 12.0 * scale;
    let right = rect.x + rect.width - 12.0 * scale;
    let ratio = (f32::from(value.clamp(16, 160)) - 16.0) / 144.0;
    let slider = EffectSliderGeometry {
        left,
        right,
        center_y,
        scale,
        knob_x: left + (right - left) * ratio,
    };
    draw_effect_slider_track(pixmap, slider);
    draw_effect_slider_knobs(pixmap, slider);
}

/// 绘制效果强度滑块的水平轨道。
fn draw_effect_slider_track(pixmap: &mut PixmapMut, slider: EffectSliderGeometry) {
    let mut path = PathBuilder::new();
    path.move_to(slider.left, slider.center_y);
    path.line_to(slider.right, slider.center_y);
    if let Some(path) = path.finish() {
        pixmap.stroke_path(
            &path,
            &make_paint(Color::from_rgba8(165, 170, 182, 255)),
            &Stroke {
                width: 2.0 * slider.scale,
                line_cap: tiny_skia::LineCap::Round,
                ..Default::default()
            },
            Transform::identity(),
            None,
        );
    }
}

/// 绘制轨道两端端点与当前滑块。
fn draw_effect_slider_knobs(pixmap: &mut PixmapMut, slider: EffectSliderGeometry) {
    fill_circle(
        pixmap,
        CircleSpec {
            center_x: slider.left,
            center_y: slider.center_y,
            radius: 2.0 * slider.scale,
            color: Color::from_rgba8(120, 126, 140, 255),
        },
    );
    fill_circle(
        pixmap,
        CircleSpec {
            center_x: slider.right,
            center_y: slider.center_y,
            radius: 5.0 * slider.scale,
            color: Color::from_rgba8(120, 126, 140, 255),
        },
    );
    fill_circle(
        pixmap,
        CircleSpec {
            center_x: slider.knob_x,
            center_y: slider.center_y,
            radius: 5.0 * slider.scale,
            color: Color::from_rgba8(37, 99, 235, 255),
        },
    );
}

/// 放大镜倍率滑块的轨道几何。
#[derive(Clone, Copy)]
struct MagnifierScaleGeometry {
    left: f32,
    right: f32,
    center_y: f32,
    scale: f32,
    knob_x: f32,
}

/// 绘制放大镜倍率滑块，范围固定为 1.0 到 8.0。
pub(super) fn render_magnifier_scale_slider(
    pixmap: &mut PixmapMut,
    geometry: SubBarGeometry,
    scale: f32,
    focused: bool,
) {
    if focused {
        fill_button_highlight(
            pixmap,
            &geometry.rect,
            Color::from_rgba8(37, 99, 235, 36),
            geometry.scale,
        );
    }
    draw_magnifier_scale_label(pixmap, geometry, scale);
    let left = geometry.rect.x + 43.0 * geometry.scale;
    let right = geometry.rect.x + geometry.rect.width - 12.0 * geometry.scale;
    let ratio = ((scale.clamp(1.0, 8.0) - 1.0) / 7.0).clamp(0.0, 1.0);
    draw_magnifier_scale_track(
        pixmap,
        MagnifierScaleGeometry {
            left,
            right,
            center_y: geometry.center_y,
            scale: geometry.scale,
            knob_x: left + (right - left) * ratio,
        },
    );
}

/// 绘制倍率图标与当前倍率数值。
fn draw_magnifier_scale_label(pixmap: &mut PixmapMut, geometry: SubBarGeometry, scale: f32) {
    render_magnifier_smooth_icon(
        pixmap,
        SubBarIconSpec {
            center_x: geometry.rect.x + 10.0 * geometry.scale,
            center_y: geometry.center_y,
            scale: geometry.scale,
            color: Color::from_rgba8(45, 48, 56, 255),
        },
    );
    with_thread_local_text_renderer(|renderer| {
        renderer.draw_text_rgba(
            pixmap,
            &format!("{scale:.1}x"),
            TextDrawOptions::new(
                geometry.rect.x + 20.0 * geometry.scale,
                geometry.center_y - 6.0 * geometry.scale,
                10.5 * geometry.scale,
                (45, 48, 56, 255),
            ),
        );
    });
}

/// 绘制倍率滑块轨道与当前滑块。
fn draw_magnifier_scale_track(pixmap: &mut PixmapMut, slider: MagnifierScaleGeometry) {
    let mut path = PathBuilder::new();
    path.move_to(slider.left, slider.center_y);
    path.line_to(slider.right, slider.center_y);
    if let Some(path) = path.finish() {
        pixmap.stroke_path(
            &path,
            &make_paint(Color::from_rgba8(165, 170, 182, 255)),
            &Stroke {
                width: 2.0 * slider.scale,
                line_cap: tiny_skia::LineCap::Round,
                ..Default::default()
            },
            Transform::identity(),
            None,
        );
    }
    fill_circle(
        pixmap,
        CircleSpec {
            center_x: slider.knob_x,
            center_y: slider.center_y,
            radius: 5.0 * slider.scale,
            color: Color::from_rgba8(37, 99, 235, 255),
        },
    );
}

/// 绘制“仅边框 / 边框+填充”开关图标
pub(super) fn render_fill_toggle(
    pixmap: &mut PixmapMut,
    geometry: SubBarGeometry,
    filled: bool,
    color: Color,
) {
    let (cx, cy, s) = (
        geometry.rect.x + geometry.rect.width / 2.0,
        geometry.center_y,
        geometry.scale,
    );
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
    geometry: SubBarGeometry,
    is_active: bool,
) {
    let (cx, cy, s) = (
        geometry.rect.x + geometry.rect.width / 2.0,
        geometry.center_y,
        geometry.scale,
    );
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
    geometry: SubBarGeometry,
    is_active: bool,
) {
    let (cx, cy, s) = (
        geometry.rect.x + geometry.rect.width / 2.0,
        geometry.center_y,
        geometry.scale,
    );
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
    geometry: SubBarGeometry,
) {
    let (rect, center_y, s, highlight) = (
        &geometry.rect,
        geometry.center_y,
        geometry.scale,
        geometry.highlight,
    );
    match control {
        SubControl::BadgeStyle => {
            let open = toolbar.open_dropdown == Some(SubDropdown::Badge);
            if open {
                fill_button_highlight(pixmap, rect, highlight, s);
            }
            render_badge_style_icon(
                pixmap,
                BadgeStyleIconSpec {
                    center_x: rect.x + 13.0 * s,
                    center_y,
                    scale: s,
                    style: toolbar.sub_options.badge_style,
                    color: toolbar.sub_options.color,
                },
            );
            render_caret(pixmap, rect.x + rect.width - 9.0 * s, center_y, s);
        }
        SubControl::BadgeSequence => {
            render_badge_sequence_icon(
                pixmap,
                SubBarIconSpec {
                    center_x: rect.x + 13.0 * s,
                    center_y,
                    scale: s,
                    color: Color::from_rgba8(45, 48, 56, 255),
                },
                toolbar.sub_options.badge_sequence,
            );
            render_caret(pixmap, rect.x + rect.width - 9.0 * s, center_y, s);
        }
        _ => {}
    }
}
