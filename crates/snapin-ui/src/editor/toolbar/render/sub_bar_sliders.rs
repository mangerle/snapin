//! 二级栏滑杆控件绘制。

use super::super::icons::make_paint;
use super::sub_bar_helpers::SubBarGeometry;
use super::sub_bar_icons::{render_magnifier_smooth_icon, SubBarIconSpec};
use super::widgets::{fill_button_highlight, fill_circle, CircleSpec};
use snapin_render::{with_thread_local_text_renderer, TextDrawOptions};
use tiny_skia::{Color, PathBuilder, PixmapMut, Stroke, Transform};

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
    draw_magnifier_scale_value(pixmap, geometry, scale);
    let left = geometry.rect.x + 44.0 * geometry.scale;
    let right = geometry.rect.x + geometry.rect.width - 10.0 * geometry.scale;
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

/// 绘制倍率滑块左侧的放大镜图标与当前倍率。
fn draw_magnifier_scale_value(pixmap: &mut PixmapMut, geometry: SubBarGeometry, scale: f32) {
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
