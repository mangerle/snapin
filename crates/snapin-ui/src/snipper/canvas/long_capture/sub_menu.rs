//! 长截图裁剪展开子菜单光栅化渲染与命中测试。

use super::icons::{draw_checkbox, draw_crop_icon};
use crate::drawing::{add_round_rect, RoundRect};
use crate::geometry::Rect;
use crate::snipper::window::state::long_capture::{LongCaptureAction, LongCaptureSession};
use snapin_render::{with_thread_local_text_renderer, TextDrawOptions};
use tiny_skia::{Paint, PathBuilder, PixmapMut, Stroke, Transform};

/// 绘制裁剪展开子浮条（位于剪刀按钮正下方）
pub(crate) fn render_crop_sub_menu(
    pixmap: &mut PixmapMut,
    session: &mut LongCaptureSession,
    anchor_x: f32,
    anchor_y: f32,
    s: f32,
) {
    let sub_w = 172.0 * s;
    let sub_h = 32.0 * s;

    let sub_rect = Rect {
        x: anchor_x,
        y: anchor_y,
        width: sub_w,
        height: sub_h,
    };
    session.crop_submenu_rect = Some(sub_rect);

    // 1. 绘制纯白圆角卡片底板
    let mut pb = PathBuilder::new();
    add_round_rect(
        &mut pb,
        RoundRect {
            x: sub_rect.x,
            y: sub_rect.y,
            width: sub_rect.width,
            height: sub_rect.height,
            radius: 5.0 * s,
        },
    );
    if let Some(path) = pb.finish() {
        let mut bg_paint = Paint::default();
        bg_paint.set_color_rgba8(255, 255, 255, 252);
        pixmap.fill_path(
            &path,
            &bg_paint,
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );

        let mut stroke_paint = Paint::default();
        stroke_paint.set_color_rgba8(0, 0, 0, 26);
        let stroke = Stroke {
            width: 1.0 * s,
            ..Default::default()
        };
        pixmap.stroke_path(&path, &stroke_paint, &stroke, Transform::identity(), None);
    }

    let btn_y = sub_rect.y + 3.0 * s;
    let btn_h = sub_rect.height - 6.0 * s;
    let btn_w = 26.0 * s;

    // 2. 上剪/左剪图标按钮
    let head_rect = Rect {
        x: sub_rect.x + 6.0 * s,
        y: btn_y,
        width: btn_w,
        height: btn_h,
    };
    let is_head_hover = session.hovered_action == Some(LongCaptureAction::CropHead);
    draw_sub_icon_button(pixmap, &head_rect, is_head_hover, s);
    draw_crop_icon(
        pixmap,
        head_rect.x + head_rect.width / 2.0,
        head_rect.y + head_rect.height / 2.0,
        session.direction,
        true,
        s,
        is_head_hover,
    );

    // 3. 下剪/右剪图标按钮
    let tail_rect = Rect {
        x: sub_rect.x + 36.0 * s,
        y: btn_y,
        width: btn_w,
        height: btn_h,
    };
    let is_tail_hover = session.hovered_action == Some(LongCaptureAction::CropTail);
    draw_sub_icon_button(pixmap, &tail_rect, is_tail_hover, s);
    draw_crop_icon(
        pixmap,
        tail_rect.x + tail_rect.width / 2.0,
        tail_rect.y + tail_rect.height / 2.0,
        session.direction,
        false,
        s,
        is_tail_hover,
    );

    // 4. 竖直细分割线
    let div_x = sub_rect.x + 68.0 * s;
    let mut div_pb = PathBuilder::new();
    div_pb.move_to(div_x, sub_rect.y + 6.0 * s);
    div_pb.line_to(div_x, sub_rect.y + sub_rect.height - 6.0 * s);
    if let Some(div_path) = div_pb.finish() {
        let mut div_paint = Paint::default();
        div_paint.set_color_rgba8(0, 0, 0, 22);
        let stroke = Stroke {
            width: 1.0 * s,
            ..Default::default()
        };
        pixmap.stroke_path(&div_path, &div_paint, &stroke, Transform::identity(), None);
    }

    // 5. 自动裁剪复选框与文本
    let auto_box_x = sub_rect.x + 78.0 * s;
    let auto_box_y = sub_rect.y + (sub_rect.height - 13.0 * s) / 2.0;
    let is_auto_hover = session.hovered_action == Some(LongCaptureAction::ToggleAutoCrop);

    if is_auto_hover {
        let mut hover_pb = PathBuilder::new();
        add_round_rect(
            &mut hover_pb,
            RoundRect {
                x: auto_box_x - 3.0 * s,
                y: btn_y,
                width: 90.0 * s,
                height: btn_h,
                radius: 3.0 * s,
            },
        );
        if let Some(path) = hover_pb.finish() {
            let mut paint = Paint::default();
            paint.set_color_rgba8(0, 0, 0, 10);
            pixmap.fill_path(
                &path,
                &paint,
                tiny_skia::FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }

    draw_checkbox(
        pixmap,
        auto_box_x,
        auto_box_y,
        session.auto_crop,
        s,
        is_auto_hover,
    );

    with_thread_local_text_renderer(|renderer| {
        renderer.draw_text_rgba(
            pixmap,
            "自动裁剪",
            TextDrawOptions::new(
                auto_box_x + 18.0 * s,
                sub_rect.y + 7.5 * s,
                11.0 * s,
                (45, 52, 65, 255),
            ),
        );
    });
}

/// 绘制子菜单图标按钮背景
fn draw_sub_icon_button(pixmap: &mut PixmapMut, rect: &Rect, is_hovered: bool, s: f32) {
    if is_hovered {
        let mut pb = PathBuilder::new();
        add_round_rect(
            &mut pb,
            RoundRect {
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: rect.height,
                radius: 3.5 * s,
            },
        );
        if let Some(path) = pb.finish() {
            let mut paint = Paint::default();
            paint.set_color_rgba8(0, 0, 0, 16);
            pixmap.fill_path(
                &path,
                &paint,
                tiny_skia::FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }
}

/// 命中测试裁剪子菜单
pub(crate) fn hit_test_crop_sub_menu(
    session: &LongCaptureSession,
    cursor_x: f32,
    cursor_y: f32,
    s: f32,
) -> Option<LongCaptureAction> {
    let sub_rect = session.crop_submenu_rect?;
    if !sub_rect.contains(cursor_x, cursor_y) {
        return None;
    }

    let btn_y = sub_rect.y + 3.0 * s;
    let btn_h = sub_rect.height - 6.0 * s;
    let btn_w = 26.0 * s;

    // 上剪/左剪
    let head_rect = Rect {
        x: sub_rect.x + 6.0 * s,
        y: btn_y,
        width: btn_w,
        height: btn_h,
    };
    if head_rect.contains(cursor_x, cursor_y) {
        return Some(LongCaptureAction::CropHead);
    }

    // 下剪/右剪
    let tail_rect = Rect {
        x: sub_rect.x + 36.0 * s,
        y: btn_y,
        width: btn_w,
        height: btn_h,
    };
    if tail_rect.contains(cursor_x, cursor_y) {
        return Some(LongCaptureAction::CropTail);
    }

    // 自动裁剪复选框区域
    let auto_rect = Rect {
        x: sub_rect.x + 75.0 * s,
        y: btn_y,
        width: 90.0 * s,
        height: btn_h,
    };
    if auto_rect.contains(cursor_x, cursor_y) {
        return Some(LongCaptureAction::ToggleAutoCrop);
    }

    None
}
