//! 长截图拼接管理器，负责增量画布扩容、反向回滚裁剪、首尾手动裁剪与缩略图生成。

use super::matcher::{estimate_shift, frames_visually_unchanged};
use super::thumbnail::render_thumbnail_overview;
use super::types::{
    MatchResult, StitchDirection, StitchFrameOutcome, StitchOptions, ThumbnailOverview,
    ViewportWindow, MIN_CROPPED_SPAN,
};
use image::{GenericImage, GenericImageView, RgbaImage};

/// 单次会话最多保留的手动裁剪撤销步数，防止频繁重录或连续裁剪长期占用堆内存。
const MAX_CROP_HISTORY: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CropEdge {
    Head,
    Tail,
}

#[derive(Debug, Clone)]
struct CropHistoryEntry {
    edge: CropEdge,
    /// 仅缓存被裁掉的窄边缘条带而非整张超长画布，兼顾多步撤回与低内存占用。
    removed_strip: RgbaImage,
    prev_last_frame: RgbaImage,
    prev_tail_truncated_below_frame: bool,
}

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
/// 封装完整的画布状态、上一捕获参考帧、拼接方向、可逆裁剪历史与缩略图缓存。
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
    crop_history: Vec<CropHistoryEntry>,
    /// 尾部是否已被裁短至单帧尺度之下，用以保护后续不会出现断层拼接
    tail_truncated_below_frame: bool,
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
            crop_history: Vec::new(),
            tail_truncated_below_frame: false,
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
            // 正向滚动：将新出现的尾部像素行拼接到画布末端
            if !self.append_forward_slice(frame, result.offset as u32) {
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
                },
            );
        }

        if result.offset < 0 && self.options.auto_crop_reverse {
            // 反向回滚：自动从画布末端裁除回滚的多余行，保留至少初始单帧跨度且不写入手动撤回栈
            let rollback_pixels = (-result.offset) as u32;
            self.truncate_tail_preserving_initial(rollback_pixels);
            self.last_frame = frame.clone();
            self.thumbnail_cache = None;
            self.crop_history.clear();
            return (
                result,
                StitchFrameOutcome::Stitched {
                    offset: result.offset,
                    confidence: result.confidence,
                    sticky_header_size: result.sticky_header_size,
                },
            );
        }

        // 未开启反向自动裁剪或位移为 0：画布与基准帧均不改变
        (result, StitchFrameOutcome::Ignored)
    }

    /// 将正向滚动产生的新切片追加到当前长图画布末端
    fn append_forward_slice(&mut self, frame: &RgbaImage, step: u32) -> bool {
        match self.direction {
            StitchDirection::Vertical => {
                let (width, old_height) = self.canvas.dimensions();
                let frame_height = frame.height();
                let valid_step = step.min(frame_height);
                let new_height = old_height.saturating_add(valid_step);
                let mut new_canvas = RgbaImage::new(width, new_height);
                if new_canvas.copy_from(&self.canvas, 0, 0).is_err() {
                    return false;
                }
                let slice_y = frame_height.saturating_sub(valid_step);
                let slice = frame.view(0, slice_y, width, valid_step).to_image();
                if new_canvas.copy_from(&slice, 0, old_height).is_err() {
                    return false;
                }
                self.canvas = new_canvas;
            }
            StitchDirection::Horizontal => {
                let (old_width, height) = self.canvas.dimensions();
                let frame_width = frame.width();
                let valid_step = step.min(frame_width);
                let new_width = old_width.saturating_add(valid_step);
                let mut new_canvas = RgbaImage::new(new_width, height);
                if new_canvas.copy_from(&self.canvas, 0, 0).is_err() {
                    return false;
                }
                let slice_x = frame_width.saturating_sub(valid_step);
                let slice = frame.view(slice_x, 0, valid_step, height).to_image();
                if new_canvas.copy_from(&slice, old_width, 0).is_err() {
                    return false;
                }
                self.canvas = new_canvas;
            }
        }
        true
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
        let (width, height) = self.canvas.dimensions();
        let (removed_strip, remaining) = match self.direction {
            StitchDirection::Vertical => (
                self.canvas.view(0, 0, width, crop).to_image(),
                self.canvas
                    .view(0, crop, width, height.saturating_sub(crop))
                    .to_image(),
            ),
            StitchDirection::Horizontal => (
                self.canvas.view(0, 0, crop, height).to_image(),
                self.canvas
                    .view(crop, 0, width.saturating_sub(crop), height)
                    .to_image(),
            ),
        };
        let prev_last_frame = self.last_frame.clone();
        let prev_truncated = self.tail_truncated_below_frame;
        self.push_crop_history(
            CropEdge::Head,
            removed_strip,
            prev_last_frame,
            prev_truncated,
        );
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
        let (width, height) = self.canvas.dimensions();
        let (removed_strip, remaining) = match self.direction {
            StitchDirection::Vertical => {
                let new_h = height.saturating_sub(crop);
                (
                    self.canvas.view(0, new_h, width, crop).to_image(),
                    self.canvas.view(0, 0, width, new_h).to_image(),
                )
            }
            StitchDirection::Horizontal => {
                let new_w = width.saturating_sub(crop);
                (
                    self.canvas.view(new_w, 0, crop, height).to_image(),
                    self.canvas.view(0, 0, new_w, height).to_image(),
                )
            }
        };
        let prev_last_frame = self.last_frame.clone();
        let prev_truncated = self.tail_truncated_below_frame;
        self.push_crop_history(
            CropEdge::Tail,
            removed_strip,
            prev_last_frame,
            prev_truncated,
        );
        self.canvas = remaining;
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
        self.thumbnail_cache = None;
        true
    }

    /// 反向滚动自动回滚时仅裁掉不超过初始单帧跨度的尾部多余内容，不写入手动撤回栈。
    fn truncate_tail_preserving_initial(&mut self, pixels: u32) {
        let (span, frame_span) = self.main_axis_spans();
        let min_span = frame_span.min(span);
        let crop = pixels.min(span.saturating_sub(min_span));
        if crop == 0 {
            return;
        }
        let (width, height) = self.canvas.dimensions();
        self.canvas = match self.direction {
            StitchDirection::Vertical => self
                .canvas
                .view(0, 0, width, height.saturating_sub(crop))
                .to_image(),
            StitchDirection::Horizontal => self
                .canvas
                .view(0, 0, width.saturating_sub(crop), height)
                .to_image(),
        };
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

    fn push_crop_history(
        &mut self,
        edge: CropEdge,
        removed_strip: RgbaImage,
        prev_last_frame: RgbaImage,
        prev_tail_truncated_below_frame: bool,
    ) {
        if self.crop_history.len() >= MAX_CROP_HISTORY {
            self.crop_history.remove(0);
        }
        self.crop_history.push(CropHistoryEntry {
            edge,
            removed_strip,
            prev_last_frame,
            prev_tail_truncated_below_frame,
        });
    }

    fn sync_last_frame_from_tail(&mut self) {
        let (span, frame_span) = self.main_axis_spans();
        if span < frame_span || frame_span == 0 {
            self.tail_truncated_below_frame = true;
            return;
        }
        let (width, height) = self.canvas.dimensions();
        self.last_frame = match self.direction {
            StitchDirection::Vertical => self
                .canvas
                .view(0, height - frame_span, width, frame_span)
                .to_image(),
            StitchDirection::Horizontal => self
                .canvas
                .view(width - frame_span, 0, frame_span, height)
                .to_image(),
        };
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
            let overview = render_thumbnail_overview(
                &self.canvas,
                self.direction,
                max_w,
                max_h,
                min_readable_cross,
            );
            self.thumbnail_cache = Some(ThumbnailCache {
                overview,
                canvas_dim,
                target_dim,
                min_readable_cross,
            });
        }

        &self
            .thumbnail_cache
            .as_ref()
            .expect("缩略图缓存已即时构建完毕")
            .overview
    }

    /// 获取或按需生成整图等比缩放的缩略图位图（带尺寸缓存）。
    pub fn get_or_render_thumbnail(&mut self, max_w: u32, max_h: u32) -> &RgbaImage {
        &self.get_or_render_thumbnail_overview(max_w, max_h, 0).image
    }
}

/// 沿主拼接轴将前后两幅等副轴位图无损拼接为一幅画布，用于撤销头部或尾部手动裁剪。
fn concat_along_axis(
    first: &RgbaImage,
    second: &RgbaImage,
    direction: StitchDirection,
) -> Option<RgbaImage> {
    let (w1, h1) = first.dimensions();
    let (w2, h2) = second.dimensions();
    let mut combined = match direction {
        StitchDirection::Vertical => RgbaImage::new(w1.max(w2), h1.saturating_add(h2)),
        StitchDirection::Horizontal => RgbaImage::new(w1.saturating_add(w2), h1.max(h2)),
    };
    combined.copy_from(first, 0, 0).ok()?;
    let (dx, dy) = match direction {
        StitchDirection::Vertical => (0, h1),
        StitchDirection::Horizontal => (w1, 0),
    };
    combined.copy_from(second, dx, dy).ok()?;
    Some(combined)
}
