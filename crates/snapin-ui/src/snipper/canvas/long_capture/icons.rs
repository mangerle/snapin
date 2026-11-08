//! 长截图控制栏全套矢量图标与气泡光栅化绘制。
//!
//! # 设计原理
//!
//! 严格对齐 PixPin 官方桌面图标规范，全面采用基于 tiny-skia 的纯矢量路径绘制：
//! 1. 六点抓手点阵与双向箭头指示；
//! 2. 剪刀造型裁剪图标；
//! 3. 红色框线配实心播放/停止键的录制切换按钮；
//! 4. 便签折角与 45 度探出笔头的二次编辑图标；
//! 5. 贴图、保存、复制与关闭直接复用编辑器主工具栏图标；
//! 6. 裁剪子菜单的上/下/左/右裁剪定向图标与圆角复选框；
//! 7. 悬停气泡提示框。

use crate::drawing::{add_round_rect, RoundRect};
use crate::geometry::Rect;
use snapin_render::stitching::StitchDirection;
use snapin_render::{with_thread_local_text_renderer, TextDrawOptions};
use tiny_skia::{LineCap, LineJoin, Paint, PathBuilder, PixmapMut, Stroke, Transform};

/// 创建通用的填充画笔
fn make_fill_paint(r: u8, g: u8, b: u8, a: u8) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color_rgba8(r, g, b, a);
    paint
}

/// 创建平滑圆头描边
fn make_stroke(width: f32) -> Stroke {
    Stroke {
        width,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Default::default()
    }
}

/// 绘制六点抓手点阵
pub(crate) fn draw_grip_dots(pixmap: &mut PixmapMut, start_x: f32, start_y: f32, s: f32) {
    let paint = make_fill_paint(140, 150, 165, 255);
    for col in 0..2 {
        for row in 0..3 {
            let dot_x = start_x + (col as f32) * 3.5 * s;
            let dot_y = start_y + (row as f32) * 4.5 * s;
            let mut pb = PathBuilder::new();
            pb.push_circle(dot_x, dot_y, 1.0 * s);
            if let Some(path) = pb.finish() {
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
}

/// 绘制双向箭头（垂直模式为上下箭头，水平模式为左右箭头）
pub(crate) fn draw_bidirectional_arrow(
    pixmap: &mut PixmapMut,
    cx: f32,
    cy: f32,
    direction: StitchDirection,
    s: f32,
) {
    let paint = make_fill_paint(55, 65, 81, 255);
    let stroke = make_stroke(1.3 * s);
    let mut pb = PathBuilder::new();

    match direction {
        StitchDirection::Vertical => {
            // 竖直线段
            pb.move_to(cx, cy - 4.5 * s);
            pb.line_to(cx, cy + 4.5 * s);
            // 上端箭头
            pb.move_to(cx - 2.2 * s, cy - 2.5 * s);
            pb.line_to(cx, cy - 4.5 * s);
            pb.line_to(cx + 2.2 * s, cy - 2.5 * s);
            // 下端箭头
            pb.move_to(cx - 2.2 * s, cy + 2.5 * s);
            pb.line_to(cx, cy + 4.5 * s);
            pb.line_to(cx + 2.2 * s, cy + 2.5 * s);
        }
        StitchDirection::Horizontal => {
            // 水平线段
            pb.move_to(cx - 4.5 * s, cy);
            pb.line_to(cx + 4.5 * s, cy);
            // 左端箭头
            pb.move_to(cx - 2.5 * s, cy - 2.2 * s);
            pb.line_to(cx - 4.5 * s, cy);
            pb.line_to(cx - 2.5 * s, cy + 2.2 * s);
            // 右端箭头
            pb.move_to(cx + 2.5 * s, cy - 2.2 * s);
            pb.line_to(cx + 4.5 * s, cy);
            pb.line_to(cx + 2.5 * s, cy + 2.2 * s);
        }
    }

    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
}

/// 绘制下拉菜单小倒三角指示符
pub(crate) fn draw_dropdown_arrow(pixmap: &mut PixmapMut, cx: f32, cy: f32, s: f32) {
    let paint = make_fill_paint(90, 100, 115, 255);
    let stroke = make_stroke(1.3 * s);
    let mut pb = PathBuilder::new();
    pb.move_to(cx - 2.5 * s, cy - 1.2 * s);
    pb.line_to(cx, cy + 1.6 * s);
    pb.line_to(cx + 2.5 * s, cy - 1.2 * s);
    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
}

/// 绘制剪刀造型裁剪图标
pub(crate) fn draw_scissors_icon(pixmap: &mut PixmapMut, cx: f32, cy: f32, s: f32, is_hover: bool) {
    let (r, g, b) = if is_hover { (17, 24, 39) } else { (75, 85, 99) };
    let paint = make_fill_paint(r, g, b, 255);
    let stroke = make_stroke(1.3 * s);

    let mut pb = PathBuilder::new();
    // 两个指环手柄
    pb.push_circle(cx - 3.2 * s, cy + 3.2 * s, 1.8 * s);
    pb.push_circle(cx + 3.2 * s, cy + 3.2 * s, 1.8 * s);
    // 交叉双刀刃
    pb.move_to(cx - 2.0 * s, cy + 1.8 * s);
    pb.line_to(cx + 3.6 * s, cy - 4.5 * s);
    pb.move_to(cx + 2.0 * s, cy + 1.8 * s);
    pb.line_to(cx - 3.6 * s, cy - 4.5 * s);

    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }

    // 中间铆钉点
    let mut dot_pb = PathBuilder::new();
    dot_pb.push_circle(cx, cy - 0.2 * s, 0.7 * s);
    if let Some(dot_path) = dot_pb.finish() {
        pixmap.fill_path(
            &dot_path,
            &paint,
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

/// 绘制开始/停止录制红框按钮（带实心播放小三角或停止小正方形）
pub(crate) fn draw_capture_toggle_button(
    pixmap: &mut PixmapMut,
    rect: &Rect,
    is_capturing: bool,
    is_hovered: bool,
    s: f32,
) {
    let mut pb = PathBuilder::new();
    add_round_rect(
        &mut pb,
        RoundRect {
            x: rect.x + 0.5 * s,
            y: rect.y + 0.5 * s,
            width: rect.width - 1.0 * s,
            height: rect.height - 1.0 * s,
            radius: 4.0 * s,
        },
    );
    if let Some(path) = pb.finish() {
        // 悬停时添加浅红底色
        if is_hovered {
            let bg_paint = make_fill_paint(239, 68, 68, 24);
            pixmap.fill_path(
                &path,
                &bg_paint,
                tiny_skia::FillRule::Winding,
                Transform::identity(),
                None,
            );
        }

        // 鲜明红色边框
        let stroke_paint = make_fill_paint(239, 68, 68, 255);
        let stroke = make_stroke(1.2 * s);
        pixmap.stroke_path(&path, &stroke_paint, &stroke, Transform::identity(), None);
    }

    let cx = rect.x + rect.width / 2.0;
    let cy = rect.y + rect.height / 2.0;
    let icon_paint = make_fill_paint(239, 68, 68, 255);

    if !is_capturing {
        // 停止状态：绘制实心红色向右小播放三角形
        let mut tri_pb = PathBuilder::new();
        tri_pb.move_to(cx - 2.4 * s, cy - 3.8 * s);
        tri_pb.line_to(cx + 3.6 * s, cy);
        tri_pb.line_to(cx - 2.4 * s, cy + 3.8 * s);
        tri_pb.close();
        if let Some(tri_path) = tri_pb.finish() {
            pixmap.fill_path(
                &tri_path,
                &icon_paint,
                tiny_skia::FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    } else {
        // 录制状态：绘制实心红色小正方形停止块
        let mut sq_pb = PathBuilder::new();
        add_round_rect(
            &mut sq_pb,
            RoundRect {
                x: cx - 3.0 * s,
                y: cy - 3.0 * s,
                width: 6.0 * s,
                height: 6.0 * s,
                radius: 1.0 * s,
            },
        );
        if let Some(sq_path) = sq_pb.finish() {
            pixmap.fill_path(
                &sq_path,
                &icon_paint,
                tiny_skia::FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }
}

/// 绘制二次编辑图标（带铅笔的方框）
pub(crate) fn draw_edit_icon(pixmap: &mut PixmapMut, cx: f32, cy: f32, s: f32, is_hover: bool) {
    let (r, g, b) = if is_hover { (17, 24, 39) } else { (75, 85, 99) };
    let paint = make_fill_paint(r, g, b, 255);
    let stroke = make_stroke(1.3 * s);

    let mut pb = PathBuilder::new();
    // 缺角便签外框
    pb.move_to(cx + 1.2 * s, cy - 5.5 * s);
    pb.line_to(cx - 5.0 * s, cy - 5.5 * s);
    pb.line_to(cx - 5.0 * s, cy + 5.0 * s);
    pb.line_to(cx + 5.0 * s, cy + 5.0 * s);
    pb.line_to(cx + 5.0 * s, cy - 1.2 * s);

    // 45 度伸出笔身与笔尖
    pb.move_to(cx - 0.5 * s, cy + 0.8 * s);
    pb.line_to(cx + 4.8 * s, cy - 4.5 * s);
    pb.move_to(cx - 0.5 * s, cy + 0.8 * s);
    pb.line_to(cx + 1.5 * s, cy + 0.8 * s);

    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
}

/// 绘制子菜单内部定向裁剪图标（上剪/下剪/左剪/右剪）
pub(crate) fn draw_crop_icon(
    pixmap: &mut PixmapMut,
    cx: f32,
    cy: f32,
    direction: StitchDirection,
    is_head: bool,
    s: f32,
    is_hover: bool,
) {
    let (r, g, b) = if is_hover { (17, 24, 39) } else { (75, 85, 99) };
    let paint = make_fill_paint(r, g, b, 255);
    let stroke = make_stroke(1.2 * s);

    let mut pb = PathBuilder::new();
    match (direction, is_head) {
        (StitchDirection::Vertical, true) => {
            // 上裁剪：上方剪刀，下方开口向上的方框
            // 小剪刀交叉双刃
            pb.move_to(cx - 3.2 * s, cy - 6.0 * s);
            pb.line_to(cx + 3.2 * s, cy - 2.0 * s);
            pb.move_to(cx + 3.2 * s, cy - 6.0 * s);
            pb.line_to(cx - 3.2 * s, cy - 2.0 * s);
            // 开口向上矩形框
            pb.move_to(cx - 4.5 * s, cy);
            pb.line_to(cx - 4.5 * s, cy + 5.0 * s);
            pb.line_to(cx + 4.5 * s, cy + 5.0 * s);
            pb.line_to(cx + 4.5 * s, cy);
        }
        (StitchDirection::Vertical, false) => {
            // 下裁剪：上方开口向下的方框，下方剪刀
            // 开口向下矩形框
            pb.move_to(cx - 4.5 * s, cy);
            pb.line_to(cx - 4.5 * s, cy - 5.0 * s);
            pb.line_to(cx + 4.5 * s, cy - 5.0 * s);
            pb.line_to(cx + 4.5 * s, cy);
            // 小剪刀交叉双刃
            pb.move_to(cx - 3.2 * s, cy + 2.0 * s);
            pb.line_to(cx + 3.2 * s, cy + 6.0 * s);
            pb.move_to(cx + 3.2 * s, cy + 2.0 * s);
            pb.line_to(cx - 3.2 * s, cy + 6.0 * s);
        }
        (StitchDirection::Horizontal, true) => {
            // 左裁剪：左方剪刀，右方向左开口的框
            pb.move_to(cx - 6.0 * s, cy - 3.2 * s);
            pb.line_to(cx - 2.0 * s, cy + 3.2 * s);
            pb.move_to(cx - 6.0 * s, cy + 3.2 * s);
            pb.line_to(cx - 2.0 * s, cy - 3.2 * s);
            // 右方方框
            pb.move_to(cx, cy - 4.5 * s);
            pb.line_to(cx + 5.0 * s, cy - 4.5 * s);
            pb.line_to(cx + 5.0 * s, cy + 4.5 * s);
            pb.line_to(cx, cy + 4.5 * s);
        }
        (StitchDirection::Horizontal, false) => {
            // 右裁剪：左方方框，右方剪刀
            pb.move_to(cx, cy - 4.5 * s);
            pb.line_to(cx - 5.0 * s, cy - 4.5 * s);
            pb.line_to(cx - 5.0 * s, cy + 4.5 * s);
            pb.line_to(cx, cy + 4.5 * s);
            // 右方小剪刀
            pb.move_to(cx + 2.0 * s, cy - 3.2 * s);
            pb.line_to(cx + 6.0 * s, cy + 3.2 * s);
            pb.move_to(cx + 2.0 * s, cy + 3.2 * s);
            pb.line_to(cx + 6.0 * s, cy - 3.2 * s);
        }
    }

    if let Some(path) = pb.finish() {
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
}

/// 绘制子菜单内部自动裁剪圆角复选框
pub(crate) fn draw_checkbox(
    pixmap: &mut PixmapMut,
    box_x: f32,
    box_y: f32,
    checked: bool,
    s: f32,
    is_hover: bool,
) {
    let box_size = 13.0 * s;
    let mut pb = PathBuilder::new();
    add_round_rect(
        &mut pb,
        RoundRect {
            x: box_x,
            y: box_y,
            width: box_size,
            height: box_size,
            radius: 2.5 * s,
        },
    );

    if let Some(path) = pb.finish() {
        if checked {
            // 勾选状态：填充柔和主题蓝底 + 白色对勾
            let fill_paint = make_fill_paint(59, 130, 246, 255);
            pixmap.fill_path(
                &path,
                &fill_paint,
                tiny_skia::FillRule::Winding,
                Transform::identity(),
                None,
            );

            let check_paint = make_fill_paint(255, 255, 255, 255);
            let check_stroke = make_stroke(1.4 * s);
            let mut check_pb = PathBuilder::new();
            check_pb.move_to(box_x + 2.8 * s, box_y + 6.4 * s);
            check_pb.line_to(box_x + 5.2 * s, box_y + 9.2 * s);
            check_pb.line_to(box_x + 9.8 * s, box_y + 3.8 * s);
            if let Some(check_path) = check_pb.finish() {
                pixmap.stroke_path(
                    &check_path,
                    &check_paint,
                    &check_stroke,
                    Transform::identity(),
                    None,
                );
            }
        } else {
            // 未勾选状态：浅灰边框
            let stroke_paint = if is_hover {
                make_fill_paint(59, 130, 246, 200)
            } else {
                make_fill_paint(180, 190, 205, 255)
            };
            let stroke = make_stroke(1.2 * s);
            pixmap.stroke_path(&path, &stroke_paint, &stroke, Transform::identity(), None);
        }
    }
}

/// 绘制悬停气泡提示框（对应 PixPin 悬停于剪刀等按钮上时弹出的气泡）
pub(crate) fn draw_tooltip_bubble(
    pixmap: &mut PixmapMut,
    target_rect: &Rect,
    text: &str,
    s: f32,
    screen_w: f32,
) {
    let font_size = 11.0 * s;
    let text_w = (text.chars().count() as f32) * 12.0 * s;
    let bubble_w = text_w + 12.0 * s;
    let bubble_h = 22.0 * s;

    let mut bubble_x = target_rect.x + (target_rect.width - bubble_w) / 2.0;
    if bubble_x + bubble_w > screen_w - 6.0 * s {
        bubble_x = screen_w - bubble_w - 6.0 * s;
    }
    if bubble_x < 6.0 * s {
        bubble_x = 6.0 * s;
    }
    let bubble_y = target_rect.y + target_rect.height + 4.0 * s;

    let mut pb = PathBuilder::new();
    add_round_rect(
        &mut pb,
        RoundRect {
            x: bubble_x,
            y: bubble_y,
            width: bubble_w,
            height: bubble_h,
            radius: 3.0 * s,
        },
    );

    if let Some(path) = pb.finish() {
        let bg_paint = make_fill_paint(255, 255, 255, 252);
        pixmap.fill_path(
            &path,
            &bg_paint,
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );

        let stroke_paint = make_fill_paint(0, 0, 0, 36);
        let stroke = make_stroke(1.0 * s);
        pixmap.stroke_path(&path, &stroke_paint, &stroke, Transform::identity(), None);
    }

    with_thread_local_text_renderer(|renderer| {
        renderer.draw_text_rgba(
            pixmap,
            text,
            TextDrawOptions::new(
                bubble_x + 6.0 * s,
                bubble_y + 3.0 * s,
                font_size,
                (30, 41, 59, 255),
            ),
        );
    });
}
