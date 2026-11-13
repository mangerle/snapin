//! 长截图单轴画布低层切片、前插、追加、双向回滚及容量边界辅助操作。
//!
//! # 设计原理
//!
//! 依托 [`PagedCanvas`] 的分块头/尾弹出（`pop_head` / `pop_tail`）与块末尾合并追加/前插，
//! 滚动扩展与回滚彻底摆脱整图重分配与全量像素拷贝，单步耗时保持均摊 $O(1)$；
//! 同时在扩展写入前前置校验像素总量与单轴长度上限，阻止恶意或无限页面耗尽内存。

use super::paged_canvas::{alloc_axis_canvas, img_axis_dims, slice_img_axis, PagedCanvas};
use super::types::{StitchDirection, StitchOptions};
use image::RgbaImage;

/// 前向追加或反向前插的规划状态。
pub(crate) enum SliceExtendOutcome {
    /// 已将新正文切片与最新固定栏无损接入画布，返回最新附着的固定栏厚度
    Applied(u32),
    /// 扩展后画布像素或单轴尺寸将超越配置上限，已保持原画布零改动
    LimitReached,
    /// 当前步长退化为零，未修改画布
    Ignored,
}

/// 正向追加或反向前插的参数封装（收敛参数个数 <= 4）。
pub(crate) struct SliceExtendSpec<'a> {
    pub(crate) frame: &'a RgbaImage,
    pub(crate) direction: StitchDirection,
    pub(crate) step: u32,
    pub(crate) band_size: u32,
    pub(crate) peel_size: u32,
    pub(crate) options: &'a StitchOptions,
}

/// 双向自动回滚的参数封装。
pub(crate) struct RollbackSpec<'a> {
    pub(crate) frame: &'a RgbaImage,
    pub(crate) direction: StitchDirection,
    pub(crate) pixels: u32,
    pub(crate) band_size: u32,
    pub(crate) attached_band_size: u32,
}

/// 校验扩展后画布是否超出最大单轴长度或总像素上限（0 代表关闭对应限制）。
pub(crate) fn exceeds_canvas_limits(
    main_span: u32,
    cross_span: u32,
    options: &StitchOptions,
) -> bool {
    if options.max_canvas_extent > 0 && main_span > options.max_canvas_extent {
        return true;
    }
    if options.max_canvas_pixels > 0 {
        let total = u64::from(main_span).saturating_mul(u64::from(cross_span));
        if total > options.max_canvas_pixels {
            return true;
        }
    }
    false
}

/// 规划本次扩展后的实际有效步长、待剥离旧栏厚度、最新栏厚度以及预计总主轴跨度。
pub(crate) fn plan_extend_span(
    canvas: &PagedCanvas,
    spec: &SliceExtendSpec<'_>,
) -> Option<(u32, u32, u32, u32)> {
    let old_main = canvas.main_span();
    let frame_main = img_axis_dims(spec.frame, spec.direction).0;
    let safe_band = spec.band_size.min(frame_main / 2);
    let valid_step = spec.step.min(frame_main.saturating_sub(safe_band));
    if valid_step == 0 {
        return None;
    }
    let safe_peel = if safe_band > 0 {
        spec.peel_size.min(old_main.saturating_sub(1))
    } else {
        0
    };
    // 防止新帧固定栏吞入过多正文留白导致正向/反向切片起点反超终点，保证画布增量严格等于 valid_step
    let effective_band = safe_band.min(valid_step.saturating_add(safe_peel));
    let projected = old_main.saturating_add(valid_step);
    Some((valid_step, safe_peel, effective_band, projected))
}

/// 均摊 $O(1)$ 向画布尾部追加一截新增切片，剥离旧页脚并重附单一页脚。
pub(crate) fn append_forward_slice(
    canvas: &mut PagedCanvas,
    spec: &SliceExtendSpec<'_>,
) -> SliceExtendOutcome {
    let Some((valid_step, safe_peel, safe_footer, projected_main)) = plan_extend_span(canvas, spec)
    else {
        return SliceExtendOutcome::Ignored;
    };
    if exceeds_canvas_limits(projected_main, canvas.cross_span(), spec.options) {
        return SliceExtendOutcome::LimitReached;
    }

    let frame_main = img_axis_dims(spec.frame, spec.direction).0;
    if safe_peel > 0 {
        let _ = canvas.pop_tail(safe_peel);
    }
    // 旧画布正文截止于上一帧的 frame_main - safe_peel，在当前帧中上移了 valid_step
    let body_start = frame_main
        .saturating_sub(safe_peel)
        .saturating_sub(valid_step);
    let body_end = frame_main.saturating_sub(safe_footer);
    let body_len = body_end.saturating_sub(body_start);
    if body_len > 0 {
        let body_slice = slice_img_axis(spec.frame, spec.direction, body_start, body_len);
        canvas.append_contiguous(&body_slice);
    }
    if safe_footer > 0 {
        let footer_slice = slice_img_axis(spec.frame, spec.direction, body_end, safe_footer);
        canvas.append_contiguous(&footer_slice);
    }
    SliceExtendOutcome::Applied(safe_footer)
}

/// 均摊 $O(1)$ 向画布首部前插一截上方/左侧新增切片，同时剥离并重附唯一首部固定栏。
pub(crate) fn prepend_backward_slice(
    canvas: &mut PagedCanvas,
    spec: &SliceExtendSpec<'_>,
) -> SliceExtendOutcome {
    let Some((valid_step, safe_peel, safe_header, projected_main)) = plan_extend_span(canvas, spec)
    else {
        return SliceExtendOutcome::Ignored;
    };
    if exceeds_canvas_limits(projected_main, canvas.cross_span(), spec.options) {
        return SliceExtendOutcome::LimitReached;
    }

    if safe_peel > 0 {
        let _ = canvas.pop_head(safe_peel);
    }
    // 旧画布正文起始于上一帧的 safe_peel，在当前帧中下移了 valid_step
    let body_end = safe_peel.saturating_add(valid_step);
    let body_len = body_end.saturating_sub(safe_header);
    if body_len > 0 {
        let body_slice = slice_img_axis(spec.frame, spec.direction, safe_header, body_len);
        canvas.prepend_contiguous(&body_slice);
    }
    if safe_header > 0 {
        let header_slice = slice_img_axis(spec.frame, spec.direction, 0, safe_header);
        canvas.prepend_contiguous(&header_slice);
    }
    SliceExtendOutcome::Applied(safe_header)
}

/// 双向自动回滚的执行结果。
pub(crate) enum RollbackOutcome {
    /// 实际裁除了回退内容并重附最新固定栏
    Applied {
        /// 重附后的固定栏厚度（像素）
        band_size: u32,
        /// 回滚位移超出可裁上限并贴近初始单帧边界被夹断
        clamped_to_initial: bool,
    },
    /// 当前已无回滚裁剪余量，未对画布做任何修改
    NoOp,
}

/// 正向扩展后的反向回滚：直接弹出尾部回滚块并重贴最新页脚，至少保留单帧长度。
pub(crate) fn rollback_tail_preserving_initial(
    canvas: &mut PagedCanvas,
    spec: &RollbackSpec<'_>,
) -> RollbackOutcome {
    let old_main = canvas.main_span();
    let frame_main = img_axis_dims(spec.frame, spec.direction).0;
    let safe_footer = spec.band_size.min(frame_main / 2);
    let safe_peel = if safe_footer > 0 {
        spec.attached_band_size.min(old_main.saturating_sub(1))
    } else {
        0
    };
    let available_after_band = old_main
        .saturating_sub(safe_peel)
        .saturating_add(safe_footer);
    let crop = spec
        .pixels
        .min(available_after_band.saturating_sub(frame_main));
    if crop == 0 {
        return RollbackOutcome::NoOp;
    }

    let _ = canvas.pop_tail(safe_peel.saturating_add(crop));
    if safe_footer > 0 {
        let footer_slice = slice_img_axis(
            spec.frame,
            spec.direction,
            frame_main.saturating_sub(safe_footer),
            safe_footer,
        );
        canvas.append_contiguous(&footer_slice);
    }
    RollbackOutcome::Applied {
        band_size: safe_footer,
        clamped_to_initial: crop < spec.pixels,
    }
}

/// 反向前插后的回滚：直接弹出首部回滚块并重贴最新页眉，至少保留单帧长度。
pub(crate) fn rollback_head_preserving_initial(
    canvas: &mut PagedCanvas,
    spec: &RollbackSpec<'_>,
) -> RollbackOutcome {
    let old_main = canvas.main_span();
    let frame_main = img_axis_dims(spec.frame, spec.direction).0;
    let safe_header = spec.band_size.min(frame_main / 2);
    let safe_peel = if safe_header > 0 {
        spec.attached_band_size.min(old_main.saturating_sub(1))
    } else {
        0
    };
    let available_after_band = old_main
        .saturating_sub(safe_peel)
        .saturating_add(safe_header);
    let crop = spec
        .pixels
        .min(available_after_band.saturating_sub(frame_main));
    if crop == 0 {
        return RollbackOutcome::NoOp;
    }

    let _ = canvas.pop_head(safe_peel.saturating_add(crop));
    if safe_header > 0 {
        let header_slice = slice_img_axis(spec.frame, spec.direction, 0, safe_header);
        canvas.prepend_contiguous(&header_slice);
    }
    RollbackOutcome::Applied {
        band_size: safe_header,
        clamped_to_initial: crop < spec.pixels,
    }
}

/// 从主轴像素集合并拼接成多段小条带所需的底层搬运函数
pub(crate) fn copy_at_axis(
    dst: &mut RgbaImage,
    src: &RgbaImage,
    direction: StitchDirection,
    main_offset: u32,
) -> bool {
    let (x, y) = match direction {
        StitchDirection::Vertical => (0, main_offset),
        StitchDirection::Horizontal => (main_offset, 0),
    };
    image::GenericImage::copy_from(dst, src, x, y).is_ok()
}

pub(crate) fn copy_chunk_strip(
    dst: &mut RgbaImage,
    chunk: &RgbaImage,
    direction: StitchDirection,
    src_off: u32,
    dst_off: u32,
    len: u32,
) {
    match direction {
        StitchDirection::Vertical => {
            let view = image::GenericImageView::view(chunk, 0, src_off, chunk.width(), len);
            let _ = image::GenericImage::copy_from(dst, &*view, 0, dst_off);
        }
        StitchDirection::Horizontal => {
            let view = image::GenericImageView::view(chunk, src_off, 0, len, chunk.height());
            let _ = image::GenericImage::copy_from(dst, &*view, dst_off, 0);
        }
    }
}

pub(crate) fn concat_imgs(
    first: &RgbaImage,
    second: &RgbaImage,
    direction: StitchDirection,
) -> RgbaImage {
    let (m1, c1) = img_axis_dims(first, direction);
    let m2 = img_axis_dims(second, direction).0;
    let mut out = alloc_axis_canvas(direction, m1.saturating_add(m2), c1);
    let _ = copy_at_axis(&mut out, first, direction, 0);
    let _ = copy_at_axis(&mut out, second, direction, m1);
    out
}

pub(crate) fn merge_img_parts(
    parts: &[RgbaImage],
    direction: StitchDirection,
    total_main: u32,
    cross_span: u32,
) -> RgbaImage {
    if parts.len() == 1 {
        return parts[0].clone();
    }
    let mut out = alloc_axis_canvas(direction, total_main, cross_span);
    let mut cursor = 0u32;
    for part in parts {
        let m = img_axis_dims(part, direction).0;
        let _ = copy_at_axis(&mut out, part, direction, cursor);
        cursor = cursor.saturating_add(m);
    }
    out
}
