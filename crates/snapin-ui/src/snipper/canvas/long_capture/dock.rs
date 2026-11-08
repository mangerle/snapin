//! 长截图侧边缩略预览悬浮窗与绿色视口指示器光栅化渲染。

use super::CanvasSize;
use crate::drawing::{add_round_rect, RoundRect};
use crate::geometry::Rect;
use crate::snipper::window::state::long_capture::LongCaptureSession;
use snapin_render::{with_thread_local_text_renderer, StitchDirection, TextDrawOptions};
use tiny_skia::{Paint, PathBuilder, PixmapMut, Stroke, Transform};

/// 绘制长截图侧边缩略预览悬浮窗（依附于选区右侧）
///
/// 遵循 PixPin 官方规范：开始录制状态右方显示预览窗口，停止状态无预览窗口。
pub(crate) fn render_long_capture_dock(
    pixmap: &mut PixmapMut,
    session: &mut LongCaptureSession,
    selection_rect: &Rect,
    screen_size: CanvasSize,
    ui_scale: f32,
) {
    // 停止状态下无预览窗口，直接清理热区并返回
    if !session.is_capturing {
        session.thumbnail_rect = None;
        return;
    }

    let s = ui_scale;
    let dock_w = (140.0 * s).round();
    let dock_h = (280.0 * s).round();

    // 默认放在选区右侧；如果超出屏幕右边界则自适应翻折至左侧
    let mut dock_x = selection_rect.x + selection_rect.width + 12.0 * s;
    if dock_x + dock_w > (screen_size.width as f32) - 8.0 * s {
        dock_x = (selection_rect.x - dock_w - 12.0 * s).max(8.0 * s);
    }
    let dock_y = selection_rect
        .y
        .clamp(8.0 * s, (screen_size.height as f32) - dock_h - 8.0 * s);

    let dock_rect = Rect {
        x: dock_x,
        y: dock_y,
        width: dock_w,
        height: dock_h,
    };
    session.thumbnail_rect = Some(dock_rect);

    // 1. 绘制纯白圆角磨砂卡片底板与轻阴影
    let mut pb = PathBuilder::new();
    add_round_rect(
        &mut pb,
        RoundRect {
            x: dock_x,
            y: dock_y,
            width: dock_w,
            height: dock_h,
            radius: 8.0 * s,
        },
    );
    if let Some(path) = pb.finish() {
        let mut bg_paint = Paint::default();
        bg_paint.set_color_rgba8(255, 255, 255, 245);
        pixmap.fill_path(
            &path,
            &bg_paint,
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );

        let mut stroke_paint = Paint::default();
        stroke_paint.set_color_rgba8(0, 0, 0, 30);
        let stroke = Stroke {
            width: 1.0 * s,
            ..Default::default()
        };
        pixmap.stroke_path(&path, &stroke_paint, &stroke, Transform::identity(), None);
    }

    // 2. 绘制标题与尺寸信息
    let (cw, ch) = session.stitcher.canvas().dimensions();
    let title = format!("长图预览 {cw}×{ch}");
    let font_size = 11.0 * s;
    with_thread_local_text_renderer(|renderer| {
        renderer.draw_text_rgba(
            pixmap,
            &title,
            TextDrawOptions::new(
                dock_x + 8.0 * s,
                dock_y + 8.0 * s,
                font_size,
                (45, 52, 65, 255),
            ),
        );
    });

    // 3. 绘制长图缩略图
    let preview_top = dock_y + 28.0 * s;
    let preview_max_w = (dock_w - 16.0 * s).round() as u32;
    let preview_max_h = (dock_h - 36.0 * s).round() as u32;

    let thumb = session
        .stitcher
        .get_or_render_thumbnail(preview_max_w, preview_max_h);
    let (tw, th) = thumb.dimensions();
    let thumb_x = dock_x + 8.0 * s + ((preview_max_w.saturating_sub(tw)) as f32 / 2.0);
    let thumb_y = preview_top;

    blit_rgba_to_pixmap(
        pixmap,
        thumb,
        thumb_x.round() as u32,
        thumb_y.round() as u32,
    );

    // 4. 绘制绿色视口指示框（匹配成功时亮绿显示当前视窗在长图中的映射位置）
    let vp = session.viewport_window();
    if vp.total > 0.0 && th > 0 {
        let ratio_start = vp.start / vp.total;
        let ratio_size = (vp.size / vp.total).max(0.05);

        let (vp_x, vp_y, vp_w, vp_h) = match session.direction {
            StitchDirection::Vertical => {
                let top = thumb_y + ratio_start * (th as f32);
                let height = (ratio_size * (th as f32)).min((thumb_y + th as f32) - top);
                (thumb_x, top, tw as f32, height)
            }
            StitchDirection::Horizontal => {
                let left = thumb_x + ratio_start * (tw as f32);
                let width = (ratio_size * (tw as f32)).min((thumb_x + tw as f32) - left);
                (left, thumb_y, width, th as f32)
            }
        };

        let mut vp_pb = PathBuilder::new();
        add_round_rect(
            &mut vp_pb,
            RoundRect {
                x: vp_x,
                y: vp_y,
                width: vp_w.max(4.0 * s),
                height: vp_h.max(4.0 * s),
                radius: 2.0 * s,
            },
        );
        if let Some(vp_path) = vp_pb.finish() {
            // 绿色半透明指示填充
            let mut vp_fill = Paint::default();
            if session.is_matched {
                vp_fill.set_color_rgba8(34, 197, 94, 60);
            } else {
                vp_fill.set_color_rgba8(234, 179, 8, 45); // 匹配暂停黄色提示
            }
            pixmap.fill_path(
                &vp_path,
                &vp_fill,
                tiny_skia::FillRule::Winding,
                Transform::identity(),
                None,
            );

            // 绿色高亮描边
            let mut vp_stroke = Paint::default();
            if session.is_matched {
                vp_stroke.set_color_rgba8(34, 197, 94, 230);
            } else {
                vp_stroke.set_color_rgba8(234, 179, 8, 200);
            }
            let stroke = Stroke {
                width: 1.5 * s,
                ..Default::default()
            };
            pixmap.stroke_path(&vp_path, &vp_stroke, &stroke, Transform::identity(), None);
        }
    }
}

/// 辅助函数：将 RGBA 内存行直接贴入目标 PixmapMut
pub(crate) fn blit_rgba_to_pixmap(
    pixmap: &mut PixmapMut,
    src: &image::RgbaImage,
    dest_x: u32,
    dest_y: u32,
) {
    let (sw, sh) = src.dimensions();
    let pw = pixmap.width();
    let ph = pixmap.height();

    let copy_w = sw.min(pw.saturating_sub(dest_x));
    let copy_h = sh.min(ph.saturating_sub(dest_y));
    if copy_w == 0 || copy_h == 0 {
        return;
    }

    let src_raw = src.as_raw();
    let dest_data = pixmap.data_mut();

    let src_stride = sw as usize * 4;
    let dest_stride = pw as usize * 4;
    let row_bytes = copy_w as usize * 4;

    for row in 0..copy_h as usize {
        let s_offset = row * src_stride;
        let d_offset = (dest_y as usize + row) * dest_stride + dest_x as usize * 4;

        if let (Some(d), Some(s)) = (
            dest_data.get_mut(d_offset..d_offset + row_bytes),
            src_raw.get(s_offset..s_offset + row_bytes),
        ) {
            d.copy_from_slice(s);
        }
    }
}
