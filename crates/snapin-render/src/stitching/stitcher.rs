//! 长截图拼接管理器，负责增量画布扩容、反向回滚裁剪、首尾手动裁剪与缩略图生成。

use super::matcher::{estimate_shift, frames_visually_unchanged};
use super::types::{
    MatchResult, StitchDirection, StitchFrameOutcome, StitchOptions, ViewportWindow,
};
use image::{imageops, GenericImage, GenericImageView, RgbaImage};

/// 侧边缩略图缓存数据结构，保存已生成的缩略图及其对应的画布尺寸与目标尺寸。
#[derive(Debug, Clone)]
struct ThumbnailCache {
    image: RgbaImage,
    canvas_dim: (u32, u32),
    target_dim: (u32, u32),
}

/// 长截图拼接引擎。
///
/// 封装完整的画布状态、上一捕获参考帧、拼接方向以及缩略图缓存。
#[derive(Debug, Clone)]
pub struct LongCaptureStitcher {
    /// 拼接得到的全局超长位图画布
    canvas: RgbaImage,
    /// 上一个成功拼入画布的基准帧
    last_frame: RgbaImage,
    /// 初始捕获首帧的原始尺寸 (宽, 高)
    initial_frame_size: (u32, u32),
    /// 当前拼接主方向
    direction: StitchDirection,
    /// 拼接算法配置项
    options: StitchOptions,
    /// 缓存的侧边缩略图及其对应的画布尺寸与目标尺寸
    thumbnail_cache: Option<ThumbnailCache>,
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
        if frame.dimensions() != self.initial_frame_size {
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
            // 反向回滚：自动从画布末端裁除回滚的多余行
            let rollback_pixels = (-result.offset) as u32;
            self.crop_tail(rollback_pixels);
            self.last_frame = frame.clone();
            self.thumbnail_cache = None;
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

    /// 手动或自动裁切画布首部（上裁剪）
    pub fn crop_head(&mut self, pixels: u32) {
        let (width, height) = self.canvas.dimensions();
        let max_crop = match self.direction {
            StitchDirection::Vertical => {
                let min_height = self.initial_frame_size.1.min(height);
                height.saturating_sub(min_height)
            }
            StitchDirection::Horizontal => {
                let min_width = self.initial_frame_size.0.min(width);
                width.saturating_sub(min_width)
            }
        };
        let crop = pixels.min(max_crop);
        if crop == 0 {
            return;
        }

        self.canvas = match self.direction {
            StitchDirection::Vertical => self
                .canvas
                .view(0, crop, width, height.saturating_sub(crop))
                .to_image(),
            StitchDirection::Horizontal => self
                .canvas
                .view(crop, 0, width.saturating_sub(crop), height)
                .to_image(),
        };
        self.thumbnail_cache = None;
    }

    /// 手动或自动裁切画布尾部（下裁剪）
    pub fn crop_tail(&mut self, pixels: u32) {
        let (width, height) = self.canvas.dimensions();
        let max_crop = match self.direction {
            StitchDirection::Vertical => {
                let min_height = self.initial_frame_size.1.min(height);
                height.saturating_sub(min_height)
            }
            StitchDirection::Horizontal => {
                let min_width = self.initial_frame_size.0.min(width);
                width.saturating_sub(min_width)
            }
        };
        let crop = pixels.min(max_crop);
        if crop == 0 {
            return;
        }

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
        self.thumbnail_cache = None;
    }

    /// 计算当前捕获视窗在长图画布上的几何映射区间（用于在侧边栏绘制绿色视口方框）
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

    /// 获取或按需生成长图的缩略图（带尺寸缓存）
    pub fn get_or_render_thumbnail(&mut self, max_w: u32, max_h: u32) -> &RgbaImage {
        let canvas_dim = self.canvas.dimensions();
        let target_dim = (max_w, max_h);

        let need_rebuild = match &self.thumbnail_cache {
            Some(cached) => cached.canvas_dim != canvas_dim || cached.target_dim != target_dim,
            None => true,
        };

        if need_rebuild {
            let (cw, ch) = canvas_dim;
            let scale_x = max_w as f32 / cw as f32;
            let scale_y = max_h as f32 / ch as f32;
            let scale = scale_x.min(scale_y).min(1.0);

            let thumb_w = ((cw as f32 * scale).round() as u32).max(1);
            let thumb_h = ((ch as f32 * scale).round() as u32).max(1);

            let thumbnail = imageops::resize(
                &self.canvas,
                thumb_w,
                thumb_h,
                imageops::FilterType::Nearest,
            );
            self.thumbnail_cache = Some(ThumbnailCache {
                image: thumbnail,
                canvas_dim,
                target_dim,
            });
        }

        match &self.thumbnail_cache {
            Some(cache) => &cache.image,
            None => &self.canvas,
        }
    }
}
