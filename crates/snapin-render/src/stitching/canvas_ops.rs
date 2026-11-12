//! 长截图单轴画布低层切片、前插、追加与双向回滚辅助操作。
//!
//! 之所以将纯像素缓冲搬运从 `stitcher.rs` 剥离到此子模块，是因为双向拼接
//! （尾部追加/回滚与首部前插/回滚）均共享对称的轴向切片与页眉/页脚剥离重贴逻辑；
//! 集中管理可确保单文件紧凑可读并消除双向分支的重复样板代码。

use super::types::StitchDirection;
use image::{GenericImage, GenericImageView, RgbaImage};

/// 按拼接主方向返回 `(主轴长度, 交叉轴长度)`。
pub(crate) fn axis_dims(img: &RgbaImage, direction: StitchDirection) -> (u32, u32) {
    let (w, h) = img.dimensions();
    match direction {
        StitchDirection::Vertical => (h, w),
        StitchDirection::Horizontal => (w, h),
    }
}

/// 按 `(主轴长度, 交叉轴长度)` 分配透明底图。
pub(crate) fn alloc_axis_canvas(
    direction: StitchDirection,
    main_span: u32,
    cross_span: u32,
) -> RgbaImage {
    match direction {
        StitchDirection::Vertical => RgbaImage::new(cross_span, main_span),
        StitchDirection::Horizontal => RgbaImage::new(main_span, cross_span),
    }
}

/// 沿主拼接轴截取 `[start .. start + len]` 区间条带。
pub(crate) fn slice_along_axis(
    img: &RgbaImage,
    direction: StitchDirection,
    start: u32,
    len: u32,
) -> RgbaImage {
    match direction {
        StitchDirection::Vertical => img.view(0, start, img.width(), len).to_image(),
        StitchDirection::Horizontal => img.view(start, 0, len, img.height()).to_image(),
    }
}

/// 将条带拷贝至目标画布主轴的 `main_offset` 偏移位置。
pub(crate) fn copy_along_axis(
    dst: &mut RgbaImage,
    src: &RgbaImage,
    direction: StitchDirection,
    main_offset: u32,
) -> bool {
    let (x, y) = match direction {
        StitchDirection::Vertical => (0, main_offset),
        StitchDirection::Horizontal => (main_offset, 0),
    };
    dst.copy_from(src, x, y).is_ok()
}

/// 向画布尾部追加一截新增切片，并正确剥离旧页脚、粘贴新正文与重附单一页脚。
pub(crate) fn append_forward_slice(
    canvas: &mut RgbaImage,
    frame: &RgbaImage,
    direction: StitchDirection,
    step: u32,
    footer: u32,
    peel: u32,
) -> Option<u32> {
    let (old_main, cross) = axis_dims(canvas, direction);
    let frame_main = axis_dims(frame, direction).0;
    let safe_footer = footer.min(frame_main / 2);
    let valid_step = step.min(frame_main.saturating_sub(safe_footer));
    if valid_step == 0 {
        return None;
    }
    let safe_peel = if safe_footer > 0 {
        peel.min(old_main.saturating_sub(1))
    } else {
        0
    };
    let keep_main = old_main.saturating_sub(safe_peel);
    let new_main = keep_main
        .saturating_add(valid_step)
        .saturating_add(safe_footer);
    let mut new_canvas = alloc_axis_canvas(direction, new_main, cross);

    let head_keep = slice_along_axis(canvas, direction, 0, keep_main);
    let body_start = frame_main
        .saturating_sub(safe_footer)
        .saturating_sub(valid_step);
    let body_slice = slice_along_axis(frame, direction, body_start, valid_step);

    if !copy_along_axis(&mut new_canvas, &head_keep, direction, 0)
        || !copy_along_axis(&mut new_canvas, &body_slice, direction, keep_main)
    {
        return None;
    }
    if safe_footer > 0 {
        let footer_slice = slice_along_axis(
            frame,
            direction,
            frame_main.saturating_sub(safe_footer),
            safe_footer,
        );
        if !copy_along_axis(
            &mut new_canvas,
            &footer_slice,
            direction,
            keep_main + valid_step,
        ) {
            return None;
        }
    }
    *canvas = new_canvas;
    Some(safe_footer)
}

/// 向画布首部前插一截上方/左侧新增切片，同时剥离旧首部固定栏并重附到全图最前端。
pub(crate) fn prepend_backward_slice(
    canvas: &mut RgbaImage,
    frame: &RgbaImage,
    direction: StitchDirection,
    step: u32,
    header: u32,
    peel: u32,
) -> Option<u32> {
    let (old_main, cross) = axis_dims(canvas, direction);
    let frame_main = axis_dims(frame, direction).0;
    let safe_header = header.min(frame_main / 2);
    let valid_step = step.min(frame_main.saturating_sub(safe_header));
    if valid_step == 0 {
        return None;
    }
    let safe_peel = if safe_header > 0 {
        peel.min(old_main.saturating_sub(1))
    } else {
        0
    };
    let keep_main = old_main.saturating_sub(safe_peel);
    let new_main = safe_header
        .saturating_add(valid_step)
        .saturating_add(keep_main);
    let mut new_canvas = alloc_axis_canvas(direction, new_main, cross);

    if safe_header > 0 {
        let header_slice = slice_along_axis(frame, direction, 0, safe_header);
        if !copy_along_axis(&mut new_canvas, &header_slice, direction, 0) {
            return None;
        }
    }
    let body_slice = slice_along_axis(frame, direction, safe_header, valid_step);
    let tail_keep = slice_along_axis(canvas, direction, safe_peel, keep_main);
    if !copy_along_axis(&mut new_canvas, &body_slice, direction, safe_header)
        || !copy_along_axis(
            &mut new_canvas,
            &tail_keep,
            direction,
            safe_header + valid_step,
        )
    {
        return None;
    }
    *canvas = new_canvas;
    Some(safe_header)
}

/// 正向扩展后的反向回滚：从画布尾部裁去回滚像素并恢复页脚至新末端，至少保留单帧长度。
pub(crate) fn rollback_tail_preserving_initial(
    canvas: &mut RgbaImage,
    frame: &RgbaImage,
    direction: StitchDirection,
    pixels: u32,
    footer: u32,
    attached_footer_size: u32,
) -> u32 {
    let (old_main, cross) = axis_dims(canvas, direction);
    let frame_main = axis_dims(frame, direction).0;
    let safe_footer = footer.min(frame_main / 2);
    let safe_peel = if safe_footer > 0 {
        attached_footer_size.min(old_main.saturating_sub(1))
    } else {
        0
    };
    let body_main = old_main.saturating_sub(safe_peel);
    let min_body = frame_main.saturating_sub(safe_footer).min(body_main);
    let crop = pixels.min(body_main.saturating_sub(min_body));
    let keep_body = body_main.saturating_sub(crop);
    let new_main = keep_body.saturating_add(safe_footer);
    if crop == 0 && safe_peel == safe_footer {
        return safe_footer;
    }

    let mut new_canvas = alloc_axis_canvas(direction, new_main, cross);
    let body_keep = slice_along_axis(canvas, direction, 0, keep_body);
    if !copy_along_axis(&mut new_canvas, &body_keep, direction, 0) {
        return attached_footer_size;
    }
    if safe_footer > 0 {
        let footer_slice = slice_along_axis(
            frame,
            direction,
            frame_main.saturating_sub(safe_footer),
            safe_footer,
        );
        if !copy_along_axis(&mut new_canvas, &footer_slice, direction, keep_body) {
            return attached_footer_size;
        }
    }
    *canvas = new_canvas;
    safe_footer
}

/// 反向前插后的回滚：从画布首部裁去正向回滚的像素并重附最新页眉至新首端，至少保留单帧长度。
pub(crate) fn rollback_head_preserving_initial(
    canvas: &mut RgbaImage,
    frame: &RgbaImage,
    direction: StitchDirection,
    pixels: u32,
    header: u32,
    attached_header_size: u32,
) -> u32 {
    let (old_main, cross) = axis_dims(canvas, direction);
    let frame_main = axis_dims(frame, direction).0;
    let safe_header = header.min(frame_main / 2);
    let safe_peel = if safe_header > 0 {
        attached_header_size.min(old_main.saturating_sub(1))
    } else {
        0
    };
    let body_main = old_main.saturating_sub(safe_peel);
    let min_body = frame_main.saturating_sub(safe_header).min(body_main);
    let crop = pixels.min(body_main.saturating_sub(min_body));
    let keep_start = safe_peel.saturating_add(crop);
    let keep_len = old_main.saturating_sub(keep_start);
    let new_main = safe_header.saturating_add(keep_len);
    if crop == 0 && safe_peel == safe_header {
        return safe_header;
    }

    let mut new_canvas = alloc_axis_canvas(direction, new_main, cross);
    if safe_header > 0 {
        let header_slice = slice_along_axis(frame, direction, 0, safe_header);
        if !copy_along_axis(&mut new_canvas, &header_slice, direction, 0) {
            return attached_header_size;
        }
    }
    let body_keep = slice_along_axis(canvas, direction, keep_start, keep_len);
    if !copy_along_axis(&mut new_canvas, &body_keep, direction, safe_header) {
        return attached_header_size;
    }
    *canvas = new_canvas;
    safe_header
}
