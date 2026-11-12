//! 长截图拼接管理器，负责增量画布扩容、固定页脚同步、局部浮层自愈、可逆裁剪与缩略图生成。

use super::history::{concat_along_axis, CropEdge, CropHistory, CropHistoryEntry};
use super::matcher::{estimate_shift, frames_visually_unchanged};
use super::overlay::{heal_canvas_with_floating_mask, OverlayHealSpec};
use super::thumbnail::render_thumbnail_overview;
use super::types::{
    MatchResult, StitchDirection, StitchFrameOutcome, StitchOptions, ThumbnailOverview,
    ViewportWindow, MIN_CROPPED_SPAN,
};
use image::{GenericImage, GenericImageView, RgbaImage};

/// 侧边缩略图缓存数据结构，保存最近渲染的高清概览及对应规格键。
#[derive(Debug, Clone)]
struct ThumbnailCache {
    overview: ThumbnailOverview,
    canvas_dim: (u32, u32),
    target_dim: (u32, u32),
    min_readable_cross: u32,
}

/// 长截图拼接引擎。
///
/// 封装完整的画布状态、上一捕获参考帧、拼接方向、固定页脚状态、可逆裁剪历史与缩略图缓存。
#[derive(Debug, Clone)]
pub struct LongCaptureStitcher {
    /// 拼接得到的全局超长位图画布
    canvas: RgbaImage,
    /// 上一个成功拼入或末尾同步的基准帧
    last_frame: RgbaImage,
    /// 初始捕获首帧的原始尺寸 (宽, 高)
    initial_frame_size: (u32, u32),
    /// 当前拼接主方向
    direction: StitchDirection,
    /// 拼接算法配置项
    options: StitchOptions,
    /// 缓存的侧边缩略图及其映射信息
    thumbnail_cache: Option<ThumbnailCache>,
    /// 手动头/尾裁剪增量撤销栈
    crop_history: CropHistory,
    /// 尾部是否已被裁短至单帧尺度之下，用以保护后续不会出现断层拼接
    tail_truncated_below_frame: bool,
    /// 当前画布最末端附加保留的固定页脚（或右侧固定栏）厚度，供下一帧剥离替换正文
    attached_footer_size: u32,
}

impl LongCaptureStitcher {
    /// 基于初始捕获帧创建长截图拼接器
    pub fn new(first_frame: RgbaImage, direction: StitchDirection, options: StitchOptions) -> Self {
        let size = (first_frame.width(), first_frame.height());
        Self {
            canvas: first_frame.clone(),
            last_frame: first_frame,
            initial_frame_size: size,
            direction,
            options,
            thumbnail_cache: None,
            crop_history: CropHistory::default(),
            tail_truncated_below_frame: false,
            attached_footer_size: 0,
        }
    }

    /// 获取当前完整拼接长图的只读引用
    pub fn canvas(&self) -> &RgbaImage {
        &self.canvas
    }

    /// 提取并转移长图位图所有权
    pub fn into_canvas(self) -> RgbaImage {
        self.canvas
    }

    /// 获取拼接方向
    pub const fn direction(&self) -> StitchDirection {
        self.direction
    }

    /// 获取算法配置
    pub const fn options(&self) -> &StitchOptions {
        &self.options
    }

    /// 更新拼接配置
    pub fn set_options(&mut self, options: StitchOptions) {
        self.options = options;
    }

    /// 使用最近一帧重建拼接画布并切换主方向。
    ///
    /// 纵向与横向画布的宽高增长轴不同，不能直接复用已拼接的位图。切换方向时从
    /// 最近一帧重新开始，既能保证尺寸契约一致，也避免对超长画布做高成本旋转。
    /// 该操作会清空当前拼接进度，仅保留算法配置。
    pub fn restart_with_direction(&mut self, direction: StitchDirection) {
        let first_frame = self.last_frame.clone();
        let options = self.options;
        *self = Self::new(first_frame, direction, options);
    }

    /// 获取首帧初始尺寸 (宽, 高)
    pub const fn initial_frame_size(&self) -> (u32, u32) {
        self.initial_frame_size
    }

    /// 当前画布的总尺寸 (宽, 高)
    pub fn dimensions(&self) -> (u32, u32) {
        self.canvas.dimensions()
    }

    /// 返回上一帧成功拼入并用作下一轮对齐锚点的基准帧。
    pub fn last_frame(&self) -> &RgbaImage {
        &self.last_frame
    }

    /// 投喂最新捕获的屏幕帧进行位移分析与画布扩展（返回原始匹配诊断）。
    pub fn feed_frame(&mut self, frame: &RgbaImage) -> MatchResult {
        self.feed_frame_with_outcome(frame).0
    }

    /// 投喂最新捕获的屏幕帧并返回强类型状态转移结果，明确区分拼接、静止、失配与忽略。
    pub fn feed_frame_outcome(&mut self, frame: &RgbaImage) -> StitchFrameOutcome {
        self.feed_frame_with_outcome(frame).1
    }

    fn feed_frame_with_outcome(&mut self, frame: &RgbaImage) -> (MatchResult, StitchFrameOutcome) {
        if frame.dimensions() != self.initial_frame_size || self.tail_truncated_below_frame {
            return (MatchResult::idle(), StitchFrameOutcome::Ignored);
        }
        if frames_visually_unchanged(&self.last_frame, frame, self.direction, &self.options) {
            return (MatchResult::idle(), StitchFrameOutcome::Unchanged);
        }

        let result = estimate_shift(&self.last_frame, frame, self.direction, &self.options);
        if !result.is_matched(self.options.confidence_threshold) {
            return (
                result,
                StitchFrameOutcome::Unmatched {
                    confidence: result.confidence,
                },
            );
        }

        if result.offset > 0 {
            let step = result.offset as u32;
            let frame_main = self.main_axis_spans().1;
            let footer = result.sticky_footer_size.min(frame_main / 2);
            let peel = if self.attached_footer_size > 0 {
                self.attached_footer_size
            } else {
                footer
            };
            heal_canvas_with_floating_mask(
                &mut self.canvas,
                &OverlayHealSpec {
                    prev: &self.last_frame,
                    curr: frame,
                    direction: self.direction,
                    step,
                    header: result.sticky_header_size,
                    footer,
                    existing_canvas_footer: peel,
                },
            );
            if !self.append_forward_slice(frame, step, footer, peel) {
                return (MatchResult::idle(), StitchFrameOutcome::Ignored);
            }
            self.last_frame = frame.clone();
            self.thumbnail_cache = None;
            self.crop_history.clear();
            return (
                result,
                StitchFrameOutcome::Stitched {
                    offset: result.offset,
                    confidence: result.confidence,
                    sticky_header_size: result.sticky_header_size,
                    sticky_footer_size: result.sticky_footer_size,
                },
            );
        }

        if result.offset < 0 && self.options.auto_crop_reverse {
            let rollback_pixels = (-result.offset) as u32;
            self.rollback_tail_preserving_initial(
                frame,
                rollback_pixels,
                result.sticky_footer_size,
            );
            self.last_frame = frame.clone();
            self.thumbnail_cache = None;
            self.crop_history.clear();
            return (
                result,
                StitchFrameOutcome::Stitched {
                    offset: result.offset,
                    confidence: result.confidence,
                    sticky_header_size: result.sticky_header_size,
                    sticky_footer_size: result.sticky_footer_size,
                },
            );
        }

        (result, StitchFrameOutcome::Ignored)
    }

    /// 剥离画布末端此前附着的固定页脚后，先贴入页脚正上方新滚出的正文，再将最新页脚挂在末尾。
    fn append_forward_slice(
        &mut self,
        frame: &RgbaImage,
        step: u32,
        footer: u32,
        peel: u32,
    ) -> bool {
        let (old_main, cross) = self.axis_canvas_dims();
        let frame_main = self.main_axis_spans().1;
        let safe_footer = footer.min(frame_main / 2);
        let valid_step = step.min(frame_main.saturating_sub(safe_footer));
        if valid_step == 0 {
            return false;
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
        let mut new_canvas = self.alloc_axis_canvas(new_main, cross);

        let head_keep = self.slice_along_axis(&self.canvas, 0, keep_main);
        let body_start = frame_main
            .saturating_sub(safe_footer)
            .saturating_sub(valid_step);
        let body_slice = self.slice_along_axis(frame, body_start, valid_step);

        if !self.copy_along_axis(&mut new_canvas, &head_keep, 0)
            || !self.copy_along_axis(&mut new_canvas, &body_slice, keep_main)
        {
            return false;
        }
        if safe_footer > 0 {
            let footer_slice =
                self.slice_along_axis(frame, frame_main.saturating_sub(safe_footer), safe_footer);
            if !self.copy_along_axis(&mut new_canvas, &footer_slice, keep_main + valid_step) {
                return false;
            }
        }
        self.canvas = new_canvas;
        self.attached_footer_size = safe_footer;
        true
    }

    /// 反向滚动自动回滚时兼顾剥离、裁除与重附页脚，保留至少初始单帧跨度且不写入手动撤回栈。
    fn rollback_tail_preserving_initial(&mut self, frame: &RgbaImage, pixels: u32, footer: u32) {
        let (old_main, cross) = self.axis_canvas_dims();
        let frame_main = self.main_axis_spans().1;
        let safe_footer = footer.min(frame_main / 2);
        let safe_peel = if safe_footer > 0 {
            self.attached_footer_size.min(old_main.saturating_sub(1))
        } else {
            0
        };
        let body_main = old_main.saturating_sub(safe_peel);
        let min_body = frame_main.saturating_sub(safe_footer).min(body_main);
        let crop = pixels.min(body_main.saturating_sub(min_body));
        let keep_body = body_main.saturating_sub(crop);
        let new_main = keep_body.saturating_add(safe_footer);
        if crop == 0 && safe_peel == safe_footer {
            return;
        }

        let mut new_canvas = self.alloc_axis_canvas(new_main, cross);
        let body_keep = self.slice_along_axis(&self.canvas, 0, keep_body);
        if !self.copy_along_axis(&mut new_canvas, &body_keep, 0) {
            return;
        }
        if safe_footer > 0 {
            let footer_slice =
                self.slice_along_axis(frame, frame_main.saturating_sub(safe_footer), safe_footer);
            if !self.copy_along_axis(&mut new_canvas, &footer_slice, keep_body) {
                return;
            }
        }
        self.canvas = new_canvas;
        self.attached_footer_size = safe_footer;
    }

    /// 手动裁切画布首部（上裁剪 / 左裁剪），返回实际裁去的像素长度。
    ///
    /// 允许突破 `initial_frame_size` 深入裁掉首屏自带的冗余页眉或侧边边距，
    /// 并同步压入增量撤回栈以支持无损撤销；头部裁切不改动尾部基准帧，可无缝续录。
    pub fn crop_head(&mut self, pixels: u32) -> u32 {
        let crop = self.clamp_manual_crop(pixels);
        if crop == 0 {
            return 0;
        }
        let span = self.axis_canvas_dims().0;
        let removed_strip = self.slice_along_axis(&self.canvas, 0, crop);
        let remaining = self.slice_along_axis(&self.canvas, crop, span.saturating_sub(crop));
        self.crop_history.push(CropHistoryEntry {
            edge: CropEdge::Head,
            removed_strip,
            prev_last_frame: self.last_frame.clone(),
            prev_tail_truncated_below_frame: self.tail_truncated_below_frame,
            prev_attached_footer_size: self.attached_footer_size,
        });
        self.canvas = remaining;
        self.thumbnail_cache = None;
        crop
    }

    /// 手动裁切画布尾部（下裁剪 / 右裁剪），返回实际裁去的像素长度。
    ///
    /// 允许裁短至最小保底尺度 [`MIN_CROPPED_SPAN`]，并压入增量撤回栈：
    /// - 裁后长度仍 `>= initial_frame_size` 时，立即从画布新末端同步更新 `last_frame`，
    ///   保证继续滚动或切换方向时以最新结尾像素精准对齐；
    /// - 裁后长度进入单帧内部时标记 `tail_truncated_below_frame`，防止后续帧产生断层误拼。
    pub fn crop_tail(&mut self, pixels: u32) -> u32 {
        let crop = self.clamp_manual_crop(pixels);
        if crop == 0 {
            return 0;
        }
        let span = self.axis_canvas_dims().0;
        let new_span = span.saturating_sub(crop);
        let removed_strip = self.slice_along_axis(&self.canvas, new_span, crop);
        let remaining = self.slice_along_axis(&self.canvas, 0, new_span);
        self.crop_history.push(CropHistoryEntry {
            edge: CropEdge::Tail,
            removed_strip,
            prev_last_frame: self.last_frame.clone(),
            prev_tail_truncated_below_frame: self.tail_truncated_below_frame,
            prev_attached_footer_size: self.attached_footer_size,
        });
        self.canvas = remaining;
        self.attached_footer_size = self.attached_footer_size.saturating_sub(crop);
        self.sync_last_frame_from_tail();
        self.thumbnail_cache = None;
        crop
    }

    /// 当前是否存在可撤销的手动头/尾裁剪记录。
    pub const fn can_undo_crop(&self) -> bool {
        !self.crop_history.is_empty()
    }

    /// 尾部是否已被手动裁短至单帧尺寸以下（此时禁止继续直接拼接后续帧）。
    pub const fn is_tail_truncated_below_frame(&self) -> bool {
        self.tail_truncated_below_frame
    }

    /// 撤销最近一次手动头/尾裁剪并无损恢复画布与末尾基准帧。
    pub fn undo_crop(&mut self) -> bool {
        let Some(entry) = self.crop_history.pop() else {
            return false;
        };
        let rebuilt = match entry.edge {
            CropEdge::Head => concat_along_axis(&entry.removed_strip, &self.canvas, self.direction),
            CropEdge::Tail => concat_along_axis(&self.canvas, &entry.removed_strip, self.direction),
        };
        let Some(canvas) = rebuilt else {
            return false;
        };
        self.canvas = canvas;
        self.last_frame = entry.prev_last_frame;
        self.tail_truncated_below_frame = entry.prev_tail_truncated_below_frame;
        self.attached_footer_size = entry.prev_attached_footer_size;
        self.thumbnail_cache = None;
        true
    }

    fn axis_canvas_dims(&self) -> (u32, u32) {
        let (w, h) = self.canvas.dimensions();
        match self.direction {
            StitchDirection::Vertical => (h, w),
            StitchDirection::Horizontal => (w, h),
        }
    }

    fn alloc_axis_canvas(&self, main_span: u32, cross_span: u32) -> RgbaImage {
        match self.direction {
            StitchDirection::Vertical => RgbaImage::new(cross_span, main_span),
            StitchDirection::Horizontal => RgbaImage::new(main_span, cross_span),
        }
    }

    fn slice_along_axis(&self, img: &RgbaImage, start: u32, len: u32) -> RgbaImage {
        match self.direction {
            StitchDirection::Vertical => img.view(0, start, img.width(), len).to_image(),
            StitchDirection::Horizontal => img.view(start, 0, len, img.height()).to_image(),
        }
    }

    fn copy_along_axis(&self, dst: &mut RgbaImage, src: &RgbaImage, main_offset: u32) -> bool {
        let (x, y) = match self.direction {
            StitchDirection::Vertical => (0, main_offset),
            StitchDirection::Horizontal => (main_offset, 0),
        };
        dst.copy_from(src, x, y).is_ok()
    }

    fn clamp_manual_crop(&self, pixels: u32) -> u32 {
        let (span, frame_span) = self.main_axis_spans();
        let min_span = MIN_CROPPED_SPAN.min(frame_span).min(span);
        pixels.min(span.saturating_sub(min_span))
    }

    fn main_axis_spans(&self) -> (u32, u32) {
        let (width, height) = self.canvas.dimensions();
        match self.direction {
            StitchDirection::Vertical => (height, self.initial_frame_size.1),
            StitchDirection::Horizontal => (width, self.initial_frame_size.0),
        }
    }

    fn sync_last_frame_from_tail(&mut self) {
        let (span, frame_span) = self.main_axis_spans();
        if span < frame_span || frame_span == 0 {
            self.tail_truncated_below_frame = true;
            return;
        }
        self.last_frame =
            self.slice_along_axis(&self.canvas, span.saturating_sub(frame_span), frame_span);
        self.tail_truncated_below_frame = false;
    }

    /// 计算当前捕获视窗在长图画布上的几何映射区间（用于在侧边栏绘制视口方框）
    pub fn current_viewport_window(&self) -> ViewportWindow {
        let (width, height) = self.canvas.dimensions();
        let (frame_span, total_span) = match self.direction {
            StitchDirection::Vertical => (self.initial_frame_size.1 as f32, height as f32),
            StitchDirection::Horizontal => (self.initial_frame_size.0 as f32, width as f32),
        };
        let start = (total_span - frame_span).max(0.0);

        ViewportWindow {
            start,
            size: frame_span.min(total_span),
            total: total_span,
        }
    }

    /// 获取或按需生成带局部窗口比例映射的高可读缩略图概览（带规格缓存）。
    pub fn get_or_render_thumbnail_overview(
        &mut self,
        max_w: u32,
        max_h: u32,
        min_readable_cross: u32,
    ) -> &ThumbnailOverview {
        let canvas_dim = self.canvas.dimensions();
        let target_dim = (max_w, max_h);
        let need_rebuild = match &self.thumbnail_cache {
            Some(cached) => {
                cached.canvas_dim != canvas_dim
                    || cached.target_dim != target_dim
                    || cached.min_readable_cross != min_readable_cross
            }
            None => true,
        };
        if need_rebuild {
            self.thumbnail_cache = None;
        }
        let cache = self.thumbnail_cache.get_or_insert_with(|| ThumbnailCache {
            overview: render_thumbnail_overview(
                &self.canvas,
                self.direction,
                max_w,
                max_h,
                min_readable_cross,
            ),
            canvas_dim,
            target_dim,
            min_readable_cross,
        });
        &cache.overview
    }

    /// 获取或按需生成整图等比缩放的缩略图位图（带尺寸缓存）。
    pub fn get_or_render_thumbnail(&mut self, max_w: u32, max_h: u32) -> &RgbaImage {
        &self.get_or_render_thumbnail_overview(max_w, max_h, 0).image
    }
}
