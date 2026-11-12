//! 长截图拼接管理器，负责双向增量扩展、固定双栏同步、局部浮层自愈、可逆裁剪与分块缩略图调度。

use super::canvas_ops::{
    append_forward_slice, exceeds_canvas_limits, plan_extend_span, prepend_backward_slice,
    rollback_head_preserving_initial, rollback_tail_preserving_initial, RollbackOutcome,
    RollbackSpec, SliceExtendOutcome, SliceExtendSpec,
};
use super::history::CropHistory;
use super::matcher::{estimate_shift, frames_visually_unchanged};
use super::overlay::{heal_canvas_with_floating_mask, OverlayHealSpec};
use super::paged_canvas::{PagedCanvas, PagedCanvasSnapshot};
use super::thumbnail::ThumbnailCache;
use super::types::{
    FrameShiftAnalysis, GrowthPolarity, MatchResult, StitchDirection, StitchFrameOutcome,
    StitchOptions, ViewportWindow,
};
use image::RgbaImage;
use std::sync::Arc;

/// 在无副作用工作线程执行的纯帧间位移分析：先执行静止采样剔除，再运行多层金字塔 ZNCC 位移估计。
pub fn analyze_frame_shift(
    anchor: &RgbaImage,
    candidate: &RgbaImage,
    direction: StitchDirection,
    options: &StitchOptions,
) -> FrameShiftAnalysis {
    if frames_visually_unchanged(anchor, candidate, direction, options) {
        return FrameShiftAnalysis::Unchanged;
    }
    let result = estimate_shift(anchor, candidate, direction, options);
    if result.is_matched(options.confidence_threshold) {
        FrameShiftAnalysis::Matched(result)
    } else {
        FrameShiftAnalysis::Unmatched {
            confidence: result.confidence,
        }
    }
}

/// 长截图拼接引擎。
///
/// 采用分段 [`PagedCanvas`] 存储以获得均摊 $O(1)$ 双向生长与可逆裁剪性能，
/// 并支持“后台分析位移 + 主线程轻量化提交 + 后台分块渲染缩略图”的零卡顿协同流水线。
#[derive(Debug, Clone)]
pub struct LongCaptureStitcher {
    /// 按主轴分块存储的长图画布
    pub(crate) canvas: PagedCanvas,
    /// 当前激活生长端用于下一轮帧对齐的共享只读基准帧
    pub(crate) last_frame: Arc<RgbaImage>,
    /// 单调递增的状态代际号，每次画布内容或对齐基准变化时加 1
    pub(crate) generation: u64,
    /// 初始捕获首帧的原始尺寸 (宽, 高)
    pub(crate) initial_frame_size: (u32, u32),
    /// 当前拼接主方向
    pub(crate) direction: StitchDirection,
    /// 拼接算法配置项
    options: StitchOptions,
    /// 缓存的侧边缩略图及其规格键
    pub(crate) thumbnail_cache: Option<ThumbnailCache>,
    /// 手动头/尾裁剪增量撤销栈
    pub(crate) crop_history: CropHistory,
    /// 激活锚点端是否已被裁短至单帧尺度之下，用以保护后续不会出现断层拼接
    pub(crate) anchor_truncated_below_frame: bool,
    /// 画布最前端附着保留的固定页眉（或左侧边栏）厚度
    pub(crate) attached_header_size: u32,
    /// 画布最末端附着保留的固定页脚（或右侧边栏）厚度
    pub(crate) attached_footer_size: u32,
    /// 已经锁定的生长极性（Forward / Backward），None 表示仍在初始单帧待锁定状态
    pub(crate) growth_polarity: Option<GrowthPolarity>,
}

impl LongCaptureStitcher {
    /// 基于初始捕获帧创建长截图拼接器
    pub fn new(first_frame: RgbaImage, direction: StitchDirection, options: StitchOptions) -> Self {
        let size = (first_frame.width(), first_frame.height());
        let anchor = Arc::new(first_frame.clone());
        Self {
            canvas: PagedCanvas::new(first_frame, direction),
            last_frame: anchor,
            generation: 1,
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

    /// 获取当前完整拼接长图的只读引用（首次调用按需惰性装配分块，常规录屏/滚动阶段零触发）
    pub fn canvas(&self) -> &RgbaImage {
        self.canvas.as_contiguous()
    }

    /// 提取并转移完整长图位图所有权
    pub fn into_canvas(self) -> RgbaImage {
        self.canvas.into_contiguous()
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

    /// 获取当前画布与对齐基准的单调代际号。
    pub const fn generation(&self) -> u64 {
        self.generation
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
        let next_gen = self.generation.wrapping_add(1);
        let first_frame = (*self.last_frame).clone();
        let options = self.options;
        *self = Self::new(first_frame, direction, options);
        self.generation = next_gen;
    }

    /// 获取首帧初始尺寸 (宽, 高)
    pub const fn initial_frame_size(&self) -> (u32, u32) {
        self.initial_frame_size
    }

    /// 当前画布的总尺寸 (宽, 高)（$O(1)$ 直读，不触及整图物化）。
    pub const fn dimensions(&self) -> (u32, u32) {
        self.canvas.dimensions()
    }

    /// 返回当前激活生长端用作下一轮对齐锚点的基准帧。
    pub fn last_frame(&self) -> &RgbaImage {
        self.last_frame.as_ref()
    }

    /// 返回对齐基准帧的零拷贝共享句柄，用于投递到后台分析线程。
    pub fn last_frame_arc(&self) -> Arc<RgbaImage> {
        self.last_frame.clone()
    }

    /// 生成分块画布在当前代际下的零拷贝只读快照，用于后台生成侧边缩略图。
    pub fn thumbnail_snapshot(&self) -> PagedCanvasSnapshot {
        self.canvas.snapshot()
    }

    /// 提取长图画布上的局部矩形区域，供编辑视口直接渲染而无需整图物化。
    pub fn slice_region(&self, x: u32, y: u32, width: u32, height: u32) -> RgbaImage {
        self.canvas.copy_region(x, y, width, height)
    }

    /// 投喂最新捕获的屏幕帧进行位移分析与画布扩展（同步兼容入口，返回原始匹配诊断）。
    pub fn feed_frame(&mut self, frame: &RgbaImage) -> MatchResult {
        self.feed_frame_with_outcome(frame).0
    }

    /// 投喂最新捕获的屏幕帧并返回强类型状态转移结果，明确区分拼接、静止、失配、触达上限与忽略。
    pub fn feed_frame_outcome(&mut self, frame: &RgbaImage) -> StitchFrameOutcome {
        self.feed_frame_with_outcome(frame).1
    }

    /// 提交后台线程已完成匹配的位移结果到分块画布；若代际已变化或帧尺寸失效则安全忽略。
    pub fn apply_analyzed_shift(
        &mut self,
        frame: RgbaImage,
        expected_generation: u64,
        analysis: FrameShiftAnalysis,
    ) -> StitchFrameOutcome {
        if self.generation != expected_generation
            || frame.dimensions() != self.initial_frame_size
            || self.anchor_truncated_below_frame
        {
            return StitchFrameOutcome::Ignored;
        }
        match analysis {
            FrameShiftAnalysis::Unchanged => StitchFrameOutcome::Unchanged,
            FrameShiftAnalysis::Unmatched { confidence } => {
                StitchFrameOutcome::Unmatched { confidence }
            }
            FrameShiftAnalysis::Matched(result) => self.apply_matched_shift(&frame, result).1,
        }
    }

    fn feed_frame_with_outcome(&mut self, frame: &RgbaImage) -> (MatchResult, StitchFrameOutcome) {
        if frame.dimensions() != self.initial_frame_size || self.anchor_truncated_below_frame {
            return (MatchResult::idle(), StitchFrameOutcome::Ignored);
        }
        match analyze_frame_shift(&self.last_frame, frame, self.direction, &self.options) {
            FrameShiftAnalysis::Unchanged => (MatchResult::idle(), StitchFrameOutcome::Unchanged),
            FrameShiftAnalysis::Unmatched { confidence } => (
                MatchResult {
                    offset: 0,
                    confidence,
                    sticky_header_size: 0,
                    sticky_footer_size: 0,
                },
                StitchFrameOutcome::Unmatched { confidence },
            ),
            FrameShiftAnalysis::Matched(result) => self.apply_matched_shift(frame, result),
        }
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
        let extend_spec = SliceExtendSpec {
            frame,
            direction: self.direction,
            step,
            band_size: footer,
            peel_size: peel,
            options: &self.options,
        };
        let Some((_, _, _, projected_main)) = plan_extend_span(&self.canvas, &extend_spec) else {
            return (MatchResult::idle(), StitchFrameOutcome::Ignored);
        };
        if exceeds_canvas_limits(projected_main, self.canvas.cross_span(), &self.options) {
            return (result, StitchFrameOutcome::LimitReached);
        }

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
        match append_forward_slice(&mut self.canvas, &extend_spec) {
            SliceExtendOutcome::Applied(new_footer) => {
                self.attached_footer_size = new_footer;
                self.growth_polarity = Some(GrowthPolarity::Forward);
                self.commit_matched_frame(frame, result)
            }
            SliceExtendOutcome::LimitReached => (result, StitchFrameOutcome::LimitReached),
            SliceExtendOutcome::Ignored => (MatchResult::idle(), StitchFrameOutcome::Ignored),
        }
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
        let extend_spec = SliceExtendSpec {
            frame,
            direction: self.direction,
            step,
            band_size: header,
            peel_size: peel,
            options: &self.options,
        };
        let Some((_, _, _, projected_main)) = plan_extend_span(&self.canvas, &extend_spec) else {
            return (MatchResult::idle(), StitchFrameOutcome::Ignored);
        };
        if exceeds_canvas_limits(projected_main, self.canvas.cross_span(), &self.options) {
            return (result, StitchFrameOutcome::LimitReached);
        }

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
        match prepend_backward_slice(&mut self.canvas, &extend_spec) {
            SliceExtendOutcome::Applied(new_header) => {
                self.attached_header_size = new_header;
                self.growth_polarity = Some(GrowthPolarity::Backward);
                self.commit_matched_frame(frame, result)
            }
            SliceExtendOutcome::LimitReached => (result, StitchFrameOutcome::LimitReached),
            SliceExtendOutcome::Ignored => (MatchResult::idle(), StitchFrameOutcome::Ignored),
        }
    }

    fn rollback_against_locked(
        &mut self,
        frame: &RgbaImage,
        result: MatchResult,
    ) -> (MatchResult, StitchFrameOutcome) {
        let pixels = result.offset.unsigned_abs();
        let outcome = match self.growth_polarity {
            Some(GrowthPolarity::Forward) => {
                let res = rollback_tail_preserving_initial(
                    &mut self.canvas,
                    &RollbackSpec {
                        frame,
                        direction: self.direction,
                        pixels,
                        band_size: result.sticky_footer_size,
                        attached_band_size: self.attached_footer_size,
                    },
                );
                if let RollbackOutcome::Applied { band_size, .. } = res {
                    self.attached_footer_size = band_size;
                }
                res
            }
            Some(GrowthPolarity::Backward) => {
                let res = rollback_head_preserving_initial(
                    &mut self.canvas,
                    &RollbackSpec {
                        frame,
                        direction: self.direction,
                        pixels,
                        band_size: result.sticky_header_size,
                        attached_band_size: self.attached_header_size,
                    },
                );
                if let RollbackOutcome::Applied { band_size, .. } = res {
                    self.attached_header_size = band_size;
                }
                res
            }
            None => return (result, StitchFrameOutcome::Ignored),
        };
        let RollbackOutcome::Applied {
            clamped_to_initial, ..
        } = outcome
        else {
            // 未发生任何实质裁剪时严禁更新锚点帧或自增代际，防止参考帧与画布末端失步
            return (result, StitchFrameOutcome::Ignored);
        };
        let (span, frame_span) = self.main_axis_spans();
        if span <= frame_span {
            self.growth_polarity = None;
        }
        if clamped_to_initial {
            let anchor_start = if self.growth_polarity == Some(GrowthPolarity::Backward) {
                0
            } else {
                span.saturating_sub(frame_span)
            };
            let synced_anchor = self.canvas.slice_axis(anchor_start, frame_span);
            return self.commit_matched_frame(&synced_anchor, result);
        }
        self.commit_matched_frame(frame, result)
    }

    fn commit_matched_frame(
        &mut self,
        frame: &RgbaImage,
        result: MatchResult,
    ) -> (MatchResult, StitchFrameOutcome) {
        self.last_frame = Arc::new(frame.clone());
        self.generation = self.generation.wrapping_add(1);
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
}
