//! 长截图拼接管理器，负责双向增量扩展、固定双栏同步、局部浮层自愈、可逆裁剪与缩略图生成。

use super::canvas_ops::{
    append_forward_slice, axis_dims, prepend_backward_slice, rollback_head_preserving_initial,
    rollback_tail_preserving_initial, slice_along_axis,
};
use super::history::{concat_along_axis, CropEdge, CropHistory, CropHistoryEntry};
use super::matcher::{estimate_shift, frames_visually_unchanged};
use super::overlay::{heal_canvas_with_floating_mask, OverlayHealSpec};
use super::thumbnail::render_thumbnail_overview;
use super::types::{
    GrowthPolarity, MatchResult, StitchDirection, StitchFrameOutcome, StitchOptions,
    ThumbnailOverview, ViewportWindow, MIN_CROPPED_SPAN,
};
use image::RgbaImage;

/// 侧边缩略图缓存数据结构，保存最近渲染的高清概览及对应规格键。
#[derive(Debug, Clone)]
struct ThumbnailCache {
    overview: ThumbnailOverview,
    canvas_dim: (u32, u32),
    target_dim: (u32, u32),
    min_readable_cross: u32,
    anchor_at_head: bool,
}

/// 长截图拼接引擎。
///
/// 支持向下/向右追加与向上/向左前插双向生长极性，自动锁定初始扩展端，
/// 并在用户反向回滚回退至初始单帧大小时解锁以允许无缝换向扩展。
#[derive(Debug, Clone)]
pub struct LongCaptureStitcher {
    /// 拼接得到的全局超长位图画布
    canvas: RgbaImage,
    /// 当前激活生长端用于下一轮帧对齐的基准帧（正向为最末帧，前插为最首帧）
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
    /// 激活锚点端是否已被裁短至单帧尺度之下，用以保护后续不会出现断层拼接
    anchor_truncated_below_frame: bool,
    /// 画布最前端附着保留的固定页眉（或左侧边栏）厚度
    attached_header_size: u32,
    /// 画布最末端附着保留的固定页脚（或右侧边栏）厚度
    attached_footer_size: u32,
    /// 已经锁定的生长极性（Forward / Backward），None 表示仍在初始单帧待锁定状态
    growth_polarity: Option<GrowthPolarity>,
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
            anchor_truncated_below_frame: false,
            attached_header_size: 0,
            attached_footer_size: 0,
            growth_polarity: None,
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

    /// 查询当前锁定的双向拼接极性（`Some(1)` 表示尾部追加，`Some(-1)` 表示首部前插，`None` 表示未锁定）。
    pub const fn growth_polarity(&self) -> Option<i32> {
        match self.growth_polarity {
            Some(polarity) => Some(polarity.as_sign()),
            None => None,
        }
    }

    /// 显式解锁生长极性（通常由停止捕获或裁回初始尺寸触发）。
    pub fn reset_growth_polarity(&mut self) {
        self.growth_polarity = None;
    }

    /// 使用最近一帧重建拼接画布并切换主方向。
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

    /// 返回当前激活生长端用作下一轮对齐锚点的基准帧。
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
        if frame.dimensions() != self.initial_frame_size || self.anchor_truncated_below_frame {
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

        self.apply_matched_shift(frame, result)
    }

    fn apply_matched_shift(
        &mut self,
        frame: &RgbaImage,
        result: MatchResult,
    ) -> (MatchResult, StitchFrameOutcome) {
        let wants_forward = result.offset > 0;
        let is_growth = match self.growth_polarity {
            None => true,
            Some(GrowthPolarity::Forward) => wants_forward,
            Some(GrowthPolarity::Backward) => !wants_forward,
        };

        if is_growth {
            return if wants_forward {
                self.grow_forward(frame, result)
            } else {
                self.grow_backward(frame, result)
            };
        }

        if !self.options.auto_crop_reverse {
            return (result, StitchFrameOutcome::Ignored);
        }
        self.rollback_against_locked(frame, result)
    }

    fn grow_forward(
        &mut self,
        frame: &RgbaImage,
        result: MatchResult,
    ) -> (MatchResult, StitchFrameOutcome) {
        let step = result.offset as u32;
        let frame_main = self.main_axis_spans().1;
        let footer = result.sticky_footer_size.min(frame_main / 2);
        let peel = if self.attached_footer_size > 0 {
            self.attached_footer_size
        } else {
            footer
        };
        let spec = OverlayHealSpec {
            prev: &self.last_frame,
            curr: frame,
            direction: self.direction,
            step,
            header: result.sticky_header_size,
            footer,
            existing_canvas_header: self.attached_header_size,
            existing_canvas_footer: peel,
            is_prepend: false,
        };
        heal_canvas_with_floating_mask(&mut self.canvas, &spec);
        let Some(new_footer) =
            append_forward_slice(&mut self.canvas, frame, self.direction, step, footer, peel)
        else {
            return (MatchResult::idle(), StitchFrameOutcome::Ignored);
        };
        self.attached_footer_size = new_footer;
        self.growth_polarity = Some(GrowthPolarity::Forward);
        self.commit_matched_frame(frame, result)
    }

    fn grow_backward(
        &mut self,
        frame: &RgbaImage,
        result: MatchResult,
    ) -> (MatchResult, StitchFrameOutcome) {
        let step = (-result.offset) as u32;
        let frame_main = self.main_axis_spans().1;
        let header = result.sticky_header_size.min(frame_main / 2);
        let peel = if self.attached_header_size > 0 {
            self.attached_header_size
        } else {
            header
        };
        let spec = OverlayHealSpec {
            prev: &self.last_frame,
            curr: frame,
            direction: self.direction,
            step,
            header,
            footer: result.sticky_footer_size,
            existing_canvas_header: peel,
            existing_canvas_footer: self.attached_footer_size,
            is_prepend: true,
        };
        heal_canvas_with_floating_mask(&mut self.canvas, &spec);
        let Some(new_header) =
            prepend_backward_slice(&mut self.canvas, frame, self.direction, step, header, peel)
        else {
            return (MatchResult::idle(), StitchFrameOutcome::Ignored);
        };
        self.attached_header_size = new_header;
        self.growth_polarity = Some(GrowthPolarity::Backward);
        self.commit_matched_frame(frame, result)
    }

    fn rollback_against_locked(
        &mut self,
        frame: &RgbaImage,
        result: MatchResult,
    ) -> (MatchResult, StitchFrameOutcome) {
        let pixels = result.offset.unsigned_abs();
        match self.growth_polarity {
            Some(GrowthPolarity::Forward) => {
                self.attached_footer_size = rollback_tail_preserving_initial(
                    &mut self.canvas,
                    frame,
                    self.direction,
                    pixels,
                    result.sticky_footer_size,
                    self.attached_footer_size,
                );
            }
            Some(GrowthPolarity::Backward) => {
                self.attached_header_size = rollback_head_preserving_initial(
                    &mut self.canvas,
                    frame,
                    self.direction,
                    pixels,
                    result.sticky_header_size,
                    self.attached_header_size,
                );
            }
            None => return (result, StitchFrameOutcome::Ignored),
        }
        let (span, frame_span) = self.main_axis_spans();
        if span <= frame_span {
            self.growth_polarity = None;
        }
        self.commit_matched_frame(frame, result)
    }

    fn commit_matched_frame(
        &mut self,
        frame: &RgbaImage,
        result: MatchResult,
    ) -> (MatchResult, StitchFrameOutcome) {
        self.last_frame = frame.clone();
        self.thumbnail_cache = None;
        self.crop_history.clear();
        (
            result,
            StitchFrameOutcome::Stitched {
                offset: result.offset,
                confidence: result.confidence,
                sticky_header_size: result.sticky_header_size,
                sticky_footer_size: result.sticky_footer_size,
            },
        )
    }

    /// 手动裁切画布首部（上裁剪 / 左裁剪），返回实际裁去的像素长度。
    pub fn crop_head(&mut self, pixels: u32) -> u32 {
        let crop = self.clamp_manual_crop(pixels);
        if crop == 0 {
            return 0;
        }
        let span = axis_dims(&self.canvas, self.direction).0;
        let removed_strip = slice_along_axis(&self.canvas, self.direction, 0, crop);
        let remaining = slice_along_axis(
            &self.canvas,
            self.direction,
            crop,
            span.saturating_sub(crop),
        );
        self.push_crop_history(CropEdge::Head, removed_strip);
        self.canvas = remaining;
        self.attached_header_size = self.attached_header_size.saturating_sub(crop);
        self.refresh_polarity_and_anchor_after_crop();
        self.thumbnail_cache = None;
        crop
    }

    /// 手动裁切画布尾部（下裁剪 / 右裁剪），返回实际裁去的像素长度。
    pub fn crop_tail(&mut self, pixels: u32) -> u32 {
        let crop = self.clamp_manual_crop(pixels);
        if crop == 0 {
            return 0;
        }
        let span = axis_dims(&self.canvas, self.direction).0;
        let new_span = span.saturating_sub(crop);
        let removed_strip = slice_along_axis(&self.canvas, self.direction, new_span, crop);
        let remaining = slice_along_axis(&self.canvas, self.direction, 0, new_span);
        self.push_crop_history(CropEdge::Tail, removed_strip);
        self.canvas = remaining;
        self.attached_footer_size = self.attached_footer_size.saturating_sub(crop);
        self.refresh_polarity_and_anchor_after_crop();
        self.thumbnail_cache = None;
        crop
    }

    fn push_crop_history(&mut self, edge: CropEdge, removed_strip: RgbaImage) {
        self.crop_history.push(CropHistoryEntry {
            edge,
            removed_strip,
            prev_last_frame: self.last_frame.clone(),
            prev_anchor_truncated_below_frame: self.anchor_truncated_below_frame,
            prev_attached_header_size: self.attached_header_size,
            prev_attached_footer_size: self.attached_footer_size,
            prev_growth_polarity: self.growth_polarity,
        });
    }

    /// 当前是否存在可撤销的手动头/尾裁剪记录。
    pub const fn can_undo_crop(&self) -> bool {
        !self.crop_history.is_empty()
    }

    /// 激活对齐端是否已被手动裁短至单帧尺寸以下（此时禁止继续直接拼接后续帧）。
    pub const fn is_anchor_truncated_below_frame(&self) -> bool {
        self.anchor_truncated_below_frame
    }

    /// 兼容既有命名：等价于 [`Self::is_anchor_truncated_below_frame`]。
    pub const fn is_tail_truncated_below_frame(&self) -> bool {
        self.anchor_truncated_below_frame
    }

    /// 撤销最近一次手动头/尾裁剪并无损恢复画布、极性与锚点基准帧。
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
        self.anchor_truncated_below_frame = entry.prev_anchor_truncated_below_frame;
        self.attached_header_size = entry.prev_attached_header_size;
        self.attached_footer_size = entry.prev_attached_footer_size;
        self.growth_polarity = entry.prev_growth_polarity;
        self.thumbnail_cache = None;
        true
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

    fn refresh_polarity_and_anchor_after_crop(&mut self) {
        let (span, frame_span) = self.main_axis_spans();
        if span <= frame_span {
            self.growth_polarity = None;
        }
        if span < frame_span || frame_span == 0 {
            self.anchor_truncated_below_frame = true;
            return;
        }
        let anchor_start = if self.growth_polarity == Some(GrowthPolarity::Backward) {
            0
        } else {
            span.saturating_sub(frame_span)
        };
        self.last_frame = slice_along_axis(&self.canvas, self.direction, anchor_start, frame_span);
        self.anchor_truncated_below_frame = false;
    }

    /// 计算当前捕获视窗在长图画布上的几何映射区间（正向停留在末端，前插停留在首端）。
    pub fn current_viewport_window(&self) -> ViewportWindow {
        let (width, height) = self.canvas.dimensions();
        let (frame_span, total_span) = match self.direction {
            StitchDirection::Vertical => (self.initial_frame_size.1 as f32, height as f32),
            StitchDirection::Horizontal => (self.initial_frame_size.0 as f32, width as f32),
        };
        let start = if self.growth_polarity == Some(GrowthPolarity::Backward) {
            0.0
        } else {
            (total_span - frame_span).max(0.0)
        };

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
        let anchor_at_head = self.growth_polarity == Some(GrowthPolarity::Backward);
        let need_rebuild = self.thumbnail_cache.as_ref().is_none_or(|cached| {
            cached.canvas_dim != canvas_dim
                || cached.target_dim != target_dim
                || cached.min_readable_cross != min_readable_cross
                || cached.anchor_at_head != anchor_at_head
        });
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
                anchor_at_head,
            ),
            canvas_dim,
            target_dim,
            min_readable_cross,
            anchor_at_head,
        });
        &cache.overview
    }

    /// 获取或按需生成整图等比缩放的缩略图位图（带尺寸缓存）。
    pub fn get_or_render_thumbnail(&mut self, max_w: u32, max_h: u32) -> &RgbaImage {
        &self.get_or_render_thumbnail_overview(max_w, max_h, 0).image
    }
}
