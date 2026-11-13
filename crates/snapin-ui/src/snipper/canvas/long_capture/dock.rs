//! 长截图侧边缩略预览悬浮窗、自适应概览滚动槽与视口指示器光栅化渲染。
//!
//! # 设计原理
//!
//! 1. **暂停态全程保留预览**：用户停止录制后正是裁剪头尾与审查画布的高频阶段，仅进入滚动编辑框后隐藏；
//! 2. **方向自适应面板排版**：纵向长图配竖向卡片并置选区左右，横向长图配横向卡片并置上下且避让控制栏；
//! 3. **极端长宽比可读性保障**：透传 `DEFAULT_MIN_THUMBNAIL_CROSS` 触发尾部高清窗裁剪，并附加全局进度条
//!    将真实视口区间从全局 `[0..1]` 重映射到展示切片局部 `[slice_start_ratio..1.0]`，避免退化为细线。

use super::CanvasSize;
use crate::drawing::{add_round_rect, RoundRect};
use crate::geometry::Rect;
use crate::snipper::window::state::long_capture::LongCaptureSession;
use image::RgbaImage;
use snapin_foundation::{i18n::tr, trf};
use snapin_render::{
    blit_straight_rgba_region_to_pixmap, with_thread_local_text_renderer, BlitRgbaRegion,
    StitchDirection, TextDrawOptions, ThumbnailOverview, DEFAULT_MIN_THUMBNAIL_CROSS,
};
use tiny_skia::{FillRule, Paint, PathBuilder, PixmapMut, Stroke, Transform};

/// 绘制长截图侧边缩略预览悬浮窗（录制中、暂停待命与编辑态均保持可见以支持概览定位与返回补录）。
pub(crate) fn render_long_capture_dock(
    pixmap: &mut PixmapMut,
    session: &mut LongCaptureSession,
    selection_rect: &Rect,
    screen_size: CanvasSize,
    ui_scale: f32,
) {
    let s = ui_scale;
    let dock_rect = resolve_dock_rect(session, selection_rect, screen_size, s);
    // 编辑态下若屏幕空间不足导致概览窗必须盖在选区上方，则自动隐藏以免遮挡标注画布
    let overlaps_selection = dock_rect.x < selection_rect.x + selection_rect.width
        && dock_rect.x + dock_rect.width > selection_rect.x
        && dock_rect.y < selection_rect.y + selection_rect.height
        && dock_rect.y + dock_rect.height > selection_rect.y;
    if session.is_editing && overlaps_selection {
        session.thumbnail_rect = None;
        session.thumbnail_content_rect = None;
        session.resume_button_rect = None;
        return;
    }
    session.thumbnail_rect = Some(dock_rect);

    draw_dock_card(pixmap, &dock_rect, s);

    let (cw, ch) = session.stitcher.dimensions();
    let title = if session.is_editing {
        trf!("概览 {}×{}", "Overview {}×{}", cw, ch)
    } else if session.is_capturing {
        trf!("长图预览 {}×{}", "Preview {}×{}", cw, ch)
    } else {
        trf!("已暂停 {}×{}", "Paused {}×{}", cw, ch)
    };
    with_thread_local_text_renderer(|renderer| {
        renderer.draw_text_rgba(
            pixmap,
            &title,
            TextDrawOptions::new(
                dock_rect.x + 8.0 * s,
                dock_rect.y + 8.0 * s,
                11.0 * s,
                if session.is_capturing || session.is_editing {
                    (45, 52, 65, 255)
                } else {
                    (100, 116, 139, 255)
                },
            ),
        );
    });

    if session.is_editing && dock_rect.width >= 96.0 * s {
        let btn_w = (42.0 * s).min(dock_rect.width * 0.36);
        let btn_h = 18.0 * s;
        let btn_x = dock_rect.x + dock_rect.width - btn_w - 6.0 * s;
        let btn_y = dock_rect.y + 5.0 * s;
        let btn_rect = Rect::new(btn_x, btn_y, btn_w, btn_h);
        let mut pb = PathBuilder::new();
        add_round_rect(
            &mut pb,
            RoundRect {
                x: btn_x,
                y: btn_y,
                width: btn_w,
                height: btn_h,
                radius: 4.0 * s,
            },
        );
        if let Some(path) = pb.finish() {
            let mut paint = Paint::default();
            paint.set_color_rgba8(59, 130, 246, 230);
            pixmap.fill_path(
                &path,
                &paint,
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
        with_thread_local_text_renderer(|renderer| {
            renderer.draw_text_rgba(
                pixmap,
                tr("补录"),
                TextDrawOptions::new(
                    btn_x + 8.0 * s,
                    btn_y + 3.0 * s,
                    10.5 * s,
                    (255, 255, 255, 255),
                ),
            );
        });
        session.resume_button_rect = Some(btn_rect);
    } else {
        session.resume_button_rect = None;
    }

    render_dock_preview_content(pixmap, session, &dock_rect, s);
}

fn render_dock_preview_content(
    pixmap: &mut PixmapMut,
    session: &mut LongCaptureSession,
    dock_rect: &Rect,
    s: f32,
) {
    let preview_top = dock_rect.y + 28.0 * s;
    let gutter = 9.0 * s;
    let (avail_w, avail_h) = match session.direction {
        StitchDirection::Vertical => (
            (dock_rect.width - 16.0 * s - gutter).max(24.0 * s),
            (dock_rect.height - 36.0 * s).max(24.0 * s),
        ),
        StitchDirection::Horizontal => (
            (dock_rect.width - 16.0 * s).max(24.0 * s),
            (dock_rect.height - 36.0 * s - gutter).max(24.0 * s),
        ),
    };
    let preview_max_w = avail_w.round().max(16.0) as u32;
    let preview_max_h = avail_h.round().max(16.0) as u32;
    let min_cross = ((DEFAULT_MIN_THUMBNAIL_CROSS as f32) * s).round().max(24.0) as u32;

    let spec = session
        .stitcher
        .build_thumbnail_spec(preview_max_w, preview_max_h, min_cross);
    session.last_thumbnail_spec = Some(spec);

    let overview: ThumbnailOverview = if let Some(exact) = session
        .stitcher
        .cached_thumbnail_overview(preview_max_w, preview_max_h, min_cross)
    {
        exact.clone()
    } else if session.async_thumbnail_enabled {
        let generation = session.stitcher.generation();
        session.desired_thumbnail_spec = Some((generation, spec));
        if let Some(prev) = session.stitcher.stale_thumbnail_overview() {
            prev.clone()
        } else {
            session
                .stitcher
                .get_or_render_thumbnail_overview(preview_max_w, preview_max_h, min_cross)
                .clone()
        }
    } else {
        session
            .stitcher
            .get_or_render_thumbnail_overview(preview_max_w, preview_max_h, min_cross)
            .clone()
    };
    let (tw, th) = overview.image.dimensions();
    let thumb_x = dock_rect.x + 8.0 * s + ((preview_max_w.saturating_sub(tw)) as f32 / 2.0);
    let thumb_y = preview_top + ((preview_max_h.saturating_sub(th)) as f32 / 2.0);

    blit_rgba_to_pixmap(
        pixmap,
        &overview.image,
        thumb_x.round() as u32,
        thumb_y.round() as u32,
    );

    let thumb_rect = Rect::new(thumb_x, thumb_y, tw as f32, th as f32);
    session.thumbnail_content_rect = Some(thumb_rect);
    session.thumbnail_slice_start_ratio = overview.slice_start_ratio;
    if overview.is_cropped_window {
        let track_rect = match session.direction {
            StitchDirection::Vertical => Rect::new(
                dock_rect.x + dock_rect.width - 11.0 * s,
                preview_top,
                4.0 * s,
                preview_max_h as f32,
            ),
            StitchDirection::Horizontal => Rect::new(
                dock_rect.x + 8.0 * s,
                dock_rect.y + dock_rect.height - 11.0 * s,
                preview_max_w as f32,
                4.0 * s,
            ),
        };
        draw_overview_scrollbar(
            pixmap,
            &track_rect,
            OverviewOverlaySpec {
                direction: session.direction,
                overview: &overview,
                scale: s,
            },
        );
        draw_cut_edge_marker(
            pixmap,
            &thumb_rect,
            OverviewOverlaySpec {
                direction: session.direction,
                overview: &overview,
                scale: s,
            },
        );
    }

    draw_viewport_indicator(
        pixmap,
        session,
        &thumb_rect,
        OverviewOverlaySpec {
            direction: session.direction,
            overview: &overview,
            scale: s,
        },
    );
}

/// 计算方向自适应且避让边界的侧边预览面板矩形（全屏幕极限压缩时保证永不 panic）。
pub(crate) fn resolve_dock_rect(
    session: &LongCaptureSession,
    selection_rect: &Rect,
    screen_size: CanvasSize,
    s: f32,
) -> Rect {
    let margin = 8.0 * s;
    let gap = 12.0 * s;
    let screen_w = screen_size.width as f32;
    let screen_h = screen_size.height as f32;
    let max_avail_w = (screen_w - margin * 2.0).max(80.0 * s);
    let max_avail_h = (screen_h - margin * 2.0).max(80.0 * s);

    let (dock_w, dock_h) = match session.direction {
        StitchDirection::Vertical => (
            (152.0 * s).round().min(max_avail_w),
            (296.0 * s).round().min(max_avail_h),
        ),
        StitchDirection::Horizontal => (
            (296.0 * s).round().min(max_avail_w),
            (152.0 * s).round().min(max_avail_h),
        ),
    };

    let (raw_x, raw_y) = match session.direction {
        StitchDirection::Vertical => {
            let right_x = selection_rect.x + selection_rect.width + gap;
            let left_x = selection_rect.x - dock_w - gap;
            let x = if right_x + dock_w <= screen_w - margin {
                right_x
            } else if left_x >= margin {
                left_x
            } else {
                screen_w - dock_w - margin
            };
            (x, selection_rect.y)
        }
        StitchDirection::Horizontal => {
            let below_y = selection_rect.y + selection_rect.height + gap;
            let above_y = selection_rect.y - dock_h - gap;
            let x = selection_rect.x;
            let y = pick_horizontal_dock_y(
                Rect::new(x, 0.0, dock_w, dock_h),
                below_y,
                above_y,
                session.toolbar_rect,
                screen_h,
                margin,
            );
            (x, y)
        }
    };

    let max_x = (screen_w - dock_w - margin).max(margin);
    let max_y = (screen_h - dock_h - margin).max(margin);
    Rect {
        x: raw_x.clamp(margin, max_x),
        y: raw_y.clamp(margin, max_y),
        width: dock_w,
        height: dock_h,
    }
}

fn pick_horizontal_dock_y(
    dock_probe: Rect,
    below_y: f32,
    above_y: f32,
    toolbar_rect: Option<Rect>,
    screen_h: f32,
    margin: f32,
) -> f32 {
    let fits_below = below_y + dock_probe.height <= screen_h - margin;
    let fits_above = above_y >= margin;
    let overlaps = |y: f32| {
        toolbar_rect.is_some_and(|tb| {
            rects_overlap(
                Rect::new(dock_probe.x, y, dock_probe.width, dock_probe.height),
                tb,
            )
        })
    };

    if fits_below && !overlaps(below_y) {
        return below_y;
    }
    if fits_above && !overlaps(above_y) {
        return above_y;
    }
    if let Some(tb) = toolbar_rect {
        let after_tb = tb.y + tb.height + 6.0;
        if after_tb + dock_probe.height <= screen_h - margin {
            return after_tb;
        }
        let before_tb = tb.y - dock_probe.height - 6.0;
        if before_tb >= margin {
            return before_tb;
        }
    }
    if fits_below {
        below_y
    } else if fits_above {
        above_y
    } else {
        screen_h - dock_probe.height - margin
    }
}

fn rects_overlap(a: Rect, b: Rect) -> bool {
    a.x < b.x + b.width && a.x + a.width > b.x && a.y < b.y + b.height && a.y + a.height > b.y
}

fn draw_dock_card(pixmap: &mut PixmapMut, dock_rect: &Rect, s: f32) {
    let mut pb = PathBuilder::new();
    add_round_rect(
        &mut pb,
        RoundRect {
            x: dock_rect.x,
            y: dock_rect.y,
            width: dock_rect.width,
            height: dock_rect.height,
            radius: 8.0 * s,
        },
    );
    if let Some(path) = pb.finish() {
        let mut bg_paint = Paint::default();
        bg_paint.set_color_rgba8(255, 255, 255, 245);
        pixmap.fill_path(
            &path,
            &bg_paint,
            FillRule::Winding,
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
}

#[derive(Clone, Copy)]
struct OverviewOverlaySpec<'a> {
    direction: StitchDirection,
    overview: &'a ThumbnailOverview,
    scale: f32,
}

fn draw_overview_scrollbar(
    pixmap: &mut PixmapMut,
    track_rect: &Rect,
    spec: OverviewOverlaySpec<'_>,
) {
    let s = spec.scale;
    let mut track_pb = PathBuilder::new();
    add_round_rect(
        &mut track_pb,
        RoundRect {
            x: track_rect.x,
            y: track_rect.y,
            width: track_rect.width,
            height: track_rect.height,
            radius: 2.0 * s,
        },
    );
    if let Some(path) = track_pb.finish() {
        let mut paint = Paint::default();
        paint.set_color_rgba8(0, 0, 0, 24);
        pixmap.fill_path(
            &path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    let start = spec.overview.slice_start_ratio.clamp(0.0, 1.0);
    let end = spec.overview.slice_end_ratio.clamp(start, 1.0);
    let (thumb_rect, min_len) = match spec.direction {
        StitchDirection::Vertical => {
            let len = ((end - start) * track_rect.height).max(6.0 * s);
            let y = (track_rect.y + end * track_rect.height - len).max(track_rect.y);
            (Rect::new(track_rect.x, y, track_rect.width, len), len)
        }
        StitchDirection::Horizontal => {
            let len = ((end - start) * track_rect.width).max(6.0 * s);
            let x = (track_rect.x + end * track_rect.width - len).max(track_rect.x);
            (Rect::new(x, track_rect.y, len, track_rect.height), len)
        }
    };
    if min_len <= 0.0 {
        return;
    }

    let mut thumb_pb = PathBuilder::new();
    add_round_rect(
        &mut thumb_pb,
        RoundRect {
            x: thumb_rect.x,
            y: thumb_rect.y,
            width: thumb_rect.width,
            height: thumb_rect.height,
            radius: 2.0 * s,
        },
    );
    if let Some(path) = thumb_pb.finish() {
        let mut paint = Paint::default();
        paint.set_color_rgba8(59, 130, 246, 200);
        pixmap.fill_path(
            &path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

fn draw_cut_edge_marker(pixmap: &mut PixmapMut, thumb_rect: &Rect, spec: OverviewOverlaySpec<'_>) {
    let s = spec.scale;
    let has_head_cut = spec.overview.slice_start_ratio > 1e-4;
    let has_tail_cut = spec.overview.slice_end_ratio < 1.0 - 1e-4;
    if !has_head_cut && !has_tail_cut {
        return;
    }

    let mut pb = PathBuilder::new();
    let offset = 1.0 * s;
    match spec.direction {
        StitchDirection::Vertical => {
            if has_head_cut {
                let y = thumb_rect.y + offset;
                pb.move_to(thumb_rect.x, y);
                pb.line_to(thumb_rect.x + thumb_rect.width, y);
            }
            if has_tail_cut {
                let y = (thumb_rect.y + thumb_rect.height - offset).max(thumb_rect.y);
                pb.move_to(thumb_rect.x, y);
                pb.line_to(thumb_rect.x + thumb_rect.width, y);
            }
        }
        StitchDirection::Horizontal => {
            if has_head_cut {
                let x = thumb_rect.x + offset;
                pb.move_to(x, thumb_rect.y);
                pb.line_to(x, thumb_rect.y + thumb_rect.height);
            }
            if has_tail_cut {
                let x = (thumb_rect.x + thumb_rect.width - offset).max(thumb_rect.x);
                pb.move_to(x, thumb_rect.y);
                pb.line_to(x, thumb_rect.y + thumb_rect.height);
            }
        }
    }
    if let Some(path) = pb.finish() {
        let mut paint = Paint::default();
        paint.set_color_rgba8(59, 130, 246, 180);
        let stroke = Stroke {
            width: 1.5 * s,
            ..Default::default()
        };
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
}

fn draw_viewport_indicator(
    pixmap: &mut PixmapMut,
    session: &LongCaptureSession,
    thumb_rect: &Rect,
    spec: OverviewOverlaySpec<'_>,
) {
    let s = spec.scale;
    let overview = spec.overview;
    let vp = session.viewport_window();
    if vp.total <= 0.0 || thumb_rect.width <= 0.0 || thumb_rect.height <= 0.0 {
        return;
    }

    let full_start = vp.normalized_start();
    let full_end = (full_start + vp.normalized_size()).clamp(full_start, 1.0);
    // 当活跃视口完全落在极端长宽比截断概览窗之外时跳过绘制，避免在边缘生成幽灵窄条。
    if full_end <= overview.slice_start_ratio || full_start >= overview.slice_end_ratio {
        return;
    }
    let slice_span = (overview.slice_end_ratio - overview.slice_start_ratio).max(1e-4);
    let local_start = ((full_start - overview.slice_start_ratio) / slice_span).clamp(0.0, 1.0);
    let local_end = ((full_end - overview.slice_start_ratio) / slice_span).clamp(local_start, 1.0);
    let local_size = (local_end - local_start).max(0.05);

    let (vp_x, vp_y, vp_w, vp_h) = match session.direction {
        StitchDirection::Vertical => {
            let top = thumb_rect.y + local_start * thumb_rect.height;
            let height =
                (local_size * thumb_rect.height).min((thumb_rect.y + thumb_rect.height) - top);
            (thumb_rect.x, top, thumb_rect.width, height)
        }
        StitchDirection::Horizontal => {
            let left = thumb_rect.x + local_start * thumb_rect.width;
            let width =
                (local_size * thumb_rect.width).min((thumb_rect.x + thumb_rect.width) - left);
            (left, thumb_rect.y, width, thumb_rect.height)
        }
    };

    let (fill_rgba, stroke_rgba) = if !session.is_capturing {
        ((100, 116, 139, 45), (100, 116, 139, 220))
    } else if session.is_matched {
        ((34, 197, 94, 60), (34, 197, 94, 230))
    } else {
        ((234, 179, 8, 45), (234, 179, 8, 200))
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
        let mut vp_fill = Paint::default();
        vp_fill.set_color_rgba8(fill_rgba.0, fill_rgba.1, fill_rgba.2, fill_rgba.3);
        pixmap.fill_path(
            &vp_path,
            &vp_fill,
            FillRule::Winding,
            Transform::identity(),
            None,
        );

        let mut vp_stroke = Paint::default();
        vp_stroke.set_color_rgba8(stroke_rgba.0, stroke_rgba.1, stroke_rgba.2, stroke_rgba.3);
        let stroke = Stroke {
            width: 1.5 * s,
            ..Default::default()
        };
        pixmap.stroke_path(&vp_path, &vp_stroke, &stroke, Transform::identity(), None);
    }
}

/// 辅助函数：将直通 RGBA 图像预乘后贴入目标 PixmapMut 指定位置。
pub(crate) fn blit_rgba_to_pixmap(
    pixmap: &mut PixmapMut,
    src: &RgbaImage,
    dest_x: u32,
    dest_y: u32,
) {
    let (sw, sh) = src.dimensions();
    blit_straight_rgba_region_to_pixmap(
        src,
        pixmap,
        BlitRgbaRegion {
            src_x: 0,
            src_y: 0,
            dst_x: dest_x,
            dst_y: dest_y,
            width: sw,
            height: sh,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;
    use tiny_skia::Pixmap;

    #[test]
    fn test_dock_remains_visible_in_paused_and_editing_states() {
        let frame = RgbaImage::from_pixel(120, 160, Rgba([80, 120, 160, 255]));
        let mut session = LongCaptureSession::new(frame);
        let selection = Rect::new(40.0, 40.0, 120.0, 160.0);
        let screen = CanvasSize {
            width: 800,
            height: 600,
        };
        let mut pixmap = Pixmap::new(800, 600).expect("画布创建应成功");

        session.toggle_capture();
        assert!(!session.is_capturing);
        render_long_capture_dock(&mut pixmap.as_mut(), &mut session, &selection, screen, 1.0);
        assert!(
            session.thumbnail_rect.is_some(),
            "暂停状态下必须保留侧边长图预览窗口以便裁剪和审查"
        );
        assert!(session.resume_button_rect.is_none());

        session.enter_editing();
        render_long_capture_dock(&mut pixmap.as_mut(), &mut session, &selection, screen, 1.0);
        assert!(
            session.thumbnail_rect.is_some(),
            "进入编辑态后应保留侧边概览窗以支持快速视口定位与返回补录"
        );
        assert!(
            session.resume_button_rect.is_some(),
            "编辑态概览窗应渲染补录按钮"
        );
    }

    #[test]
    fn test_horizontal_dock_layout_uses_wide_card_and_clamps_tiny_screen() {
        let frame = RgbaImage::from_pixel(160, 120, Rgba([40, 60, 80, 255]));
        let mut session = LongCaptureSession::new(frame);
        session.toggle_direction();
        session.toolbar_rect = Some(Rect::new(100.0, 60.0, 320.0, 36.0));
        let selection = Rect::new(100.0, 108.0, 160.0, 120.0);

        let rect = resolve_dock_rect(
            &session,
            &selection,
            CanvasSize {
                width: 800,
                height: 600,
            },
            1.0,
        );
        assert!(rect.width > rect.height, "横向模式 Dock 应采用横向扁平卡片");
        assert!(
            !rects_overlap(rect, session.toolbar_rect.expect("工具栏矩形存在")),
            "横向 Dock 布局不应遮挡主控制栏"
        );

        let tiny = resolve_dock_rect(
            &session,
            &selection,
            CanvasSize {
                width: 60,
                height: 60,
            },
            1.0,
        );
        assert!(tiny.x >= 8.0 && tiny.y >= 8.0);
    }

    #[test]
    fn test_cut_edge_marker_tracks_truncated_end_and_viewport_hides_when_out_of_slice() {
        use snapin_render::{FrameShiftAnalysis, MatchResult};

        let thumb_rect = Rect::new(10.0, 10.0, 40.0, 100.0);
        let head_anchored_overview = ThumbnailOverview {
            image: RgbaImage::new(40, 100),
            slice_start_ratio: 0.0,
            slice_end_ratio: 0.4,
            is_cropped_window: true,
        };
        let mut pixmap = Pixmap::new(80, 120).expect("画布创建应成功");
        draw_cut_edge_marker(
            &mut pixmap.as_mut(),
            &thumb_rect,
            OverviewOverlaySpec {
                direction: StitchDirection::Vertical,
                overview: &head_anchored_overview,
                scale: 1.0,
            },
        );
        // 头部对齐展示时顶部完整，仅底部存在截断蓝线
        assert_eq!(pixmap.pixel(30, 11).expect("顶部像素存在").alpha(), 0);
        assert!(pixmap.pixel(30, 109).expect("底部像素存在").alpha() > 0);

        // 正向拼接一帧后 total=280, frame=160 -> viewport=[0.428.., 1.0]，与头部切片 [0.0, 0.4] 无交集
        let frame = RgbaImage::from_pixel(120, 160, Rgba([40, 60, 80, 255]));
        let mut session = LongCaptureSession::new(frame);
        let g = session.stitcher.generation();
        let next = RgbaImage::from_pixel(120, 160, Rgba([41, 61, 81, 255]));
        assert!(session
            .stitcher
            .apply_analyzed_shift(
                next,
                g,
                FrameShiftAnalysis::Matched(MatchResult {
                    offset: 120,
                    confidence: 0.95,
                    sticky_header_size: 0,
                    sticky_footer_size: 0,
                }),
            )
            .is_stitched());

        let mut pixmap2 = Pixmap::new(80, 120).expect("画布创建应成功");
        draw_viewport_indicator(
            &mut pixmap2.as_mut(),
            &session,
            &thumb_rect,
            OverviewOverlaySpec {
                direction: StitchDirection::Vertical,
                overview: &head_anchored_overview,
                scale: 1.0,
            },
        );
        assert!(
            pixmap2.data().iter().all(|&b| b == 0),
            "视口完全位于截断窗口外时不应绘制任何边缘幽灵框"
        );
    }
}
