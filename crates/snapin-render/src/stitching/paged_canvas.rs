//! 长截图按主轴分块页式画布与只读零拷贝快照。
//!
//! # 设计原理
//!
//! 传统连续 `RgbaImage` 存储会在每次滚动追加新条带时触发整张画布的重新分配与全量像素拷贝，
//! 导致总内存搬运复杂度达到 $O(N^2)$，且在捕获阶段就迫使缩略图与裁剪承担高昂的整图开销。
//! [`PagedCanvas`] 沿主拼接轴将数据拆分为上限 `CHUNK_MAIN_SPAN`（`512px`）的 `Arc<RgbaImage>` 条带队列：
//! - 前端前插与尾端追加仅合并进相邻未满条带或追加新块，均摊时间复杂度降至 $O(1)$；
//! - 各块以 `Arc` 共享，后台缩略图渲染快照仅需复制数十个引用计数（纳秒级零像素拷贝）；
//! - 仅当进入全图导出（复制、保存、贴图）请求连续位图引用时才通过 `OnceLock` 惰性拼装，并在变更时即刻失效。

use super::canvas_ops::{concat_imgs, copy_at_axis, copy_chunk_strip, merge_img_parts};
use super::types::StitchDirection;
use image::{GenericImageView, Rgba, RgbaImage};
use std::collections::VecDeque;
use std::sync::{Arc, OnceLock};

/// 单个条带块在主拼接轴方向上的最大长度（像素）。
pub(crate) const CHUNK_MAIN_SPAN: u32 = 512;

/// 长截图分块画布在某一代际下的不可变零拷贝快照，可供后台线程安全生成缩略图。
#[derive(Debug, Clone)]
pub struct PagedCanvasSnapshot {
    direction: StitchDirection,
    cross_span: u32,
    total_main: u32,
    chunks: Vec<Arc<RgbaImage>>,
    prefix_offsets: Vec<u32>,
}

impl PagedCanvasSnapshot {
    /// 返回快照的总宽高 `(width, height)`。
    pub const fn dimensions(&self) -> (u32, u32) {
        match self.direction {
            StitchDirection::Vertical => (self.cross_span, self.total_main),
            StitchDirection::Horizontal => (self.total_main, self.cross_span),
        }
    }

    /// 返回快照的拼接主方向。
    pub const fn direction(&self) -> StitchDirection {
        self.direction
    }

    /// 按主轴坐标快速二分定位所属分块及块内局部主轴偏移。
    pub(crate) fn locate_main(&self, main: u32) -> Option<(&RgbaImage, u32)> {
        if main >= self.total_main || self.chunks.is_empty() {
            return None;
        }
        let idx = self
            .prefix_offsets
            .partition_point(|&start| start <= main)
            .saturating_sub(1);
        let chunk = self.chunks.get(idx)?;
        let local_main = main.saturating_sub(self.prefix_offsets[idx]);
        Some((chunk.as_ref(), local_main))
    }

    /// 沿主拼接轴截取 `[start .. start + len]` 区间条带（仅触及相交分块）。
    pub(crate) fn slice_axis(&self, start: u32, len: u32) -> RgbaImage {
        let clamped_start = start.min(self.total_main);
        let actual_len = len.min(self.total_main.saturating_sub(clamped_start));
        let mut dst = alloc_axis_canvas(self.direction, actual_len, self.cross_span);
        if actual_len == 0 || self.cross_span == 0 {
            return dst;
        }

        let target_end = clamped_start + actual_len;
        let mut cursor = 0u32;
        for chunk in &self.chunks {
            let chunk_main = chunk_main_span(chunk, self.direction);
            let chunk_end = cursor + chunk_main;
            if chunk_end <= clamped_start {
                cursor = chunk_end;
                continue;
            }
            if cursor >= target_end {
                break;
            }
            let overlap_start = clamped_start.max(cursor);
            let overlap_end = target_end.min(chunk_end);
            let copy_len = overlap_end - overlap_start;
            let src_off = overlap_start - cursor;
            let dst_off = overlap_start - clamped_start;
            copy_chunk_strip(&mut dst, chunk, self.direction, src_off, dst_off, copy_len);
            cursor = chunk_end;
        }
        dst
    }

    /// 读取全图坐标 `(x, y)` 处的 RGBA 像素。
    pub(crate) fn get_pixel_xy(&self, x: u32, y: u32) -> [u8; 4] {
        let (main, cross) = match self.direction {
            StitchDirection::Vertical => (y, x),
            StitchDirection::Horizontal => (x, y),
        };
        let Some((chunk, local_main)) = self.locate_main(main) else {
            return [0, 0, 0, 0];
        };
        let (cx, cy) = match self.direction {
            StitchDirection::Vertical => (cross, local_main),
            StitchDirection::Horizontal => (local_main, cross),
        };
        if cx < chunk.width() && cy < chunk.height() {
            chunk.get_pixel(cx, cy).0
        } else {
            [0, 0, 0, 0]
        }
    }
}

/// 沿主拼接轴分块存储的分段画布，兼顾均摊 $O(1)$ 追加/前插/裁剪与零冗余连续视图导出。
#[derive(Debug)]
pub(crate) struct PagedCanvas {
    direction: StitchDirection,
    cross_span: u32,
    total_main: u32,
    chunks: VecDeque<Arc<RgbaImage>>,
    materialized: OnceLock<RgbaImage>,
}

impl Clone for PagedCanvas {
    fn clone(&self) -> Self {
        Self {
            direction: self.direction,
            cross_span: self.cross_span,
            total_main: self.total_main,
            chunks: self.chunks.clone(),
            materialized: OnceLock::new(),
        }
    }
}

impl PagedCanvas {
    /// 由初始首帧构造分块画布，超长首帧会自动拆分为不超过 `CHUNK_MAIN_SPAN` 的规范条带。
    pub(crate) fn new(first_frame: RgbaImage, direction: StitchDirection) -> Self {
        let (total_main, cross_span) = img_axis_dims(&first_frame, direction);
        let chunks: VecDeque<Arc<RgbaImage>> = split_into_chunks(first_frame, direction)
            .into_iter()
            .map(Arc::new)
            .collect();
        Self {
            direction,
            cross_span,
            total_main,
            chunks,
            materialized: OnceLock::new(),
        }
    }

    /// 获取画布总宽高 `(width, height)`（$O(1)$ 无内存分配）。
    pub(crate) const fn dimensions(&self) -> (u32, u32) {
        match self.direction {
            StitchDirection::Vertical => (self.cross_span, self.total_main),
            StitchDirection::Horizontal => (self.total_main, self.cross_span),
        }
    }

    /// 获取主轴当前总长度（像素）。
    pub(crate) const fn main_span(&self) -> u32 {
        self.total_main
    }

    /// 获取交叉轴像素宽度/高度。
    pub(crate) const fn cross_span(&self) -> u32 {
        self.cross_span
    }

    /// 获取与当前分块完全一致的零拷贝不可变快照。
    pub(crate) fn snapshot(&self) -> PagedCanvasSnapshot {
        let mut prefix_offsets = Vec::with_capacity(self.chunks.len());
        let mut offset = 0u32;
        for chunk in &self.chunks {
            prefix_offsets.push(offset);
            offset = offset.saturating_add(chunk_main_span(chunk, self.direction));
        }
        PagedCanvasSnapshot {
            direction: self.direction,
            cross_span: self.cross_span,
            total_main: self.total_main,
            chunks: self.chunks.iter().cloned().collect(),
            prefix_offsets,
        }
    }

    /// 惰性拼装并返回完整连续位图引用；捕获滚动阶段不会触发，仅编辑/导出调用。
    pub(crate) fn as_contiguous(&self) -> &RgbaImage {
        self.materialized.get_or_init(|| self.assemble_contiguous())
    }

    /// 提取完整连续位图的所有权，尽量复用已有惰性缓存或唯一分块以避免多余拷贝。
    pub(crate) fn into_contiguous(mut self) -> RgbaImage {
        if let Some(cached) = self.materialized.take() {
            return cached;
        }
        if self.chunks.len() == 1 {
            if let Some(chunk) = self.chunks.pop_front() {
                return Arc::unwrap_or_clone(chunk);
            }
        }
        self.assemble_contiguous()
    }

    fn assemble_contiguous(&self) -> RgbaImage {
        let mut out = alloc_axis_canvas(self.direction, self.total_main, self.cross_span);
        let mut cursor = 0u32;
        for chunk in &self.chunks {
            let main = chunk_main_span(chunk, self.direction);
            let _ = copy_at_axis(&mut out, chunk, self.direction, cursor);
            cursor = cursor.saturating_add(main);
        }
        out
    }

    /// 沿主拼接轴截取 `[start .. start + len]` 区间并组合成独立位图。
    pub(crate) fn slice_axis(&self, start: u32, len: u32) -> RgbaImage {
        self.snapshot().slice_axis(start, len)
    }

    /// 提取画布矩形区域 `[x..x+width, y..y+height]`，用于编辑视口局部呈现，避免整图物化。
    pub(crate) fn copy_region(&self, x: u32, y: u32, width: u32, height: u32) -> RgbaImage {
        let (cw, ch) = self.dimensions();
        let sx = x.min(cw);
        let sy = y.min(ch);
        let rw = width.min(cw.saturating_sub(sx));
        let rh = height.min(ch.saturating_sub(sy));
        if rw == 0 || rh == 0 {
            return RgbaImage::new(0, 0);
        }
        match self.direction {
            StitchDirection::Vertical => {
                let strip = self.slice_axis(sy, rh);
                if sx == 0 && rw == cw {
                    strip
                } else {
                    strip.view(sx, 0, rw, rh).to_image()
                }
            }
            StitchDirection::Horizontal => {
                let strip = self.slice_axis(sx, rw);
                if sy == 0 && rh == ch {
                    strip
                } else {
                    strip.view(0, sy, rw, rh).to_image()
                }
            }
        }
    }

    /// 向画布末端均摊 $O(1)$ 追加一段等副轴位图（自动合并小条带并拆大条带）。
    pub(crate) fn append_contiguous(&mut self, img: &RgbaImage) {
        let add_main = img_axis_dims(img, self.direction).0;
        if add_main == 0 {
            return;
        }
        self.materialized.take();
        if let Some(last) = self.chunks.back_mut() {
            let last_main = chunk_main_span(last, self.direction);
            if last_main.saturating_add(add_main) <= CHUNK_MAIN_SPAN {
                let combined = concat_imgs(last.as_ref(), img, self.direction);
                *last = Arc::new(combined);
                self.total_main = self.total_main.saturating_add(add_main);
                return;
            }
        }
        for piece in split_into_chunks_ref(img, self.direction) {
            self.chunks.push_back(Arc::new(piece));
        }
        self.total_main = self.total_main.saturating_add(add_main);
    }

    /// 向画布首端均摊 $O(1)$ 前插一段等副轴位图（自动合并小条带并拆大条带）。
    pub(crate) fn prepend_contiguous(&mut self, img: &RgbaImage) {
        let add_main = img_axis_dims(img, self.direction).0;
        if add_main == 0 {
            return;
        }
        self.materialized.take();
        if let Some(first) = self.chunks.front_mut() {
            let first_main = chunk_main_span(first, self.direction);
            if first_main.saturating_add(add_main) <= CHUNK_MAIN_SPAN {
                let combined = concat_imgs(img, first.as_ref(), self.direction);
                *first = Arc::new(combined);
                self.total_main = self.total_main.saturating_add(add_main);
                return;
            }
        }
        let pieces = split_into_chunks_ref(img, self.direction);
        for piece in pieces.into_iter().rev() {
            self.chunks.push_front(Arc::new(piece));
        }
        self.total_main = self.total_main.saturating_add(add_main);
    }

    /// 从画布末端移除 `count` 像素并返回被剥离的片段；仅修改触及到的末尾分块。
    pub(crate) fn pop_tail(&mut self, count: u32) -> Option<RgbaImage> {
        let actual = count.min(self.total_main);
        if actual == 0 {
            return None;
        }
        self.materialized.take();
        let mut remaining = actual;
        let mut removed_parts: Vec<RgbaImage> = Vec::new();
        while remaining > 0 {
            let Some(tail_arc) = self.chunks.pop_back() else {
                break;
            };
            let tail_main = chunk_main_span(&tail_arc, self.direction);
            if tail_main <= remaining {
                removed_parts.push(Arc::unwrap_or_clone(tail_arc));
                remaining -= tail_main;
            } else {
                let keep_main = tail_main - remaining;
                let kept = slice_img_axis(&tail_arc, self.direction, 0, keep_main);
                let cut = slice_img_axis(&tail_arc, self.direction, keep_main, remaining);
                self.chunks.push_back(Arc::new(kept));
                removed_parts.push(cut);
                remaining = 0;
            }
        }
        self.total_main = self.total_main.saturating_sub(actual);
        removed_parts.reverse();
        Some(merge_img_parts(
            &removed_parts,
            self.direction,
            actual,
            self.cross_span,
        ))
    }

    /// 从画布首端移除 `count` 像素并返回被剥离的片段；仅修改触及到的首部分块。
    pub(crate) fn pop_head(&mut self, count: u32) -> Option<RgbaImage> {
        let actual = count.min(self.total_main);
        if actual == 0 {
            return None;
        }
        self.materialized.take();
        let mut remaining = actual;
        let mut removed_parts: Vec<RgbaImage> = Vec::new();
        while remaining > 0 {
            let Some(head_arc) = self.chunks.pop_front() else {
                break;
            };
            let head_main = chunk_main_span(&head_arc, self.direction);
            if head_main <= remaining {
                removed_parts.push(Arc::unwrap_or_clone(head_arc));
                remaining -= head_main;
            } else {
                let cut = slice_img_axis(&head_arc, self.direction, 0, remaining);
                let kept =
                    slice_img_axis(&head_arc, self.direction, remaining, head_main - remaining);
                self.chunks.push_front(Arc::new(kept));
                removed_parts.push(cut);
                remaining = 0;
            }
        }
        self.total_main = self.total_main.saturating_sub(actual);
        Some(merge_img_parts(
            &removed_parts,
            self.direction,
            actual,
            self.cross_span,
        ))
    }

    /// 创建面向连续或局部单调主轴写入的写游标，缓存分片定位以消除逐像素扫描与反复失效缓存。
    pub(crate) fn write_cursor(&mut self) -> PagedCanvasWriteCursor<'_> {
        PagedCanvasWriteCursor {
            canvas: self,
            chunk_idx: 0,
            chunk_start: 0,
            chunk_end: 0,
            initialized: false,
            cache_invalidated: false,
        }
    }
}

/// 面向连续或单调主轴写入的分片写游标。
///
/// 采用局部游标缓存上次命中的分片索引与起止区间，并在首次实际写入行时惰性触发
/// `materialized` 缓存失效与 `Arc::make_mut` 写时复制，避免空扫描或逐像素从头查表；
/// 代价是依赖调用方主轴访问具备一定局部性，极端随机跳跃时会按分片粒度线性步进。
pub(crate) struct PagedCanvasWriteCursor<'a> {
    canvas: &'a mut PagedCanvas,
    chunk_idx: usize,
    chunk_start: u32,
    chunk_end: u32,
    initialized: bool,
    cache_invalidated: bool,
}

impl<'a> PagedCanvasWriteCursor<'a> {
    /// 定位主轴坐标 `main` 所在的可变分片及分片内偏移，并将物化缓存标为失效。
    pub(crate) fn locate_row_mut(
        &mut self,
        main: u32,
    ) -> Option<(&mut RgbaImage, u32, StitchDirection)> {
        if main >= self.canvas.total_main || self.canvas.chunks.is_empty() {
            return None;
        }
        let direction = self.canvas.direction;
        if !self.initialized {
            self.seed_position(main, direction);
        }
        while main < self.chunk_start && self.chunk_idx > 0 {
            self.chunk_idx -= 1;
            let span = chunk_main_span(&self.canvas.chunks[self.chunk_idx], direction);
            self.chunk_end = self.chunk_start;
            self.chunk_start = self.chunk_end.saturating_sub(span);
        }
        while main >= self.chunk_end && self.chunk_idx + 1 < self.canvas.chunks.len() {
            self.chunk_idx += 1;
            let span = chunk_main_span(&self.canvas.chunks[self.chunk_idx], direction);
            self.chunk_start = self.chunk_end;
            self.chunk_end = self.chunk_start.saturating_add(span);
        }
        if main < self.chunk_start || main >= self.chunk_end {
            return None;
        }
        if !self.cache_invalidated {
            self.canvas.materialized.take();
            self.cache_invalidated = true;
        }
        let local_main = main - self.chunk_start;
        let chunk = Arc::make_mut(&mut self.canvas.chunks[self.chunk_idx]);
        Some((chunk, local_main, direction))
    }

    fn seed_position(&mut self, main: u32, direction: StitchDirection) {
        if main < self.canvas.total_main / 2 {
            self.chunk_idx = 0;
            self.chunk_start = 0;
            self.chunk_end = chunk_main_span(&self.canvas.chunks[0], direction);
        } else {
            let last_idx = self.canvas.chunks.len() - 1;
            let span = chunk_main_span(&self.canvas.chunks[last_idx], direction);
            self.chunk_idx = last_idx;
            self.chunk_end = self.canvas.total_main;
            self.chunk_start = self.chunk_end.saturating_sub(span);
        }
        self.initialized = true;
    }
}

pub(crate) fn img_axis_dims(img: &RgbaImage, direction: StitchDirection) -> (u32, u32) {
    match direction {
        StitchDirection::Vertical => (img.height(), img.width()),
        StitchDirection::Horizontal => (img.width(), img.height()),
    }
}

fn chunk_main_span(chunk: &RgbaImage, direction: StitchDirection) -> u32 {
    img_axis_dims(chunk, direction).0
}

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

pub(crate) fn slice_img_axis(
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

pub(crate) fn put_axis_pixel(
    img: &mut RgbaImage,
    main: u32,
    cross: u32,
    direction: StitchDirection,
    rgba: [u8; 4],
) {
    let (x, y) = match direction {
        StitchDirection::Vertical => (cross, main),
        StitchDirection::Horizontal => (main, cross),
    };
    img.put_pixel(x, y, Rgba(rgba));
}

fn split_into_chunks(img: RgbaImage, direction: StitchDirection) -> Vec<RgbaImage> {
    let main = img_axis_dims(&img, direction).0;
    if main == 0 || main <= CHUNK_MAIN_SPAN {
        return vec![img];
    }
    split_into_chunks_ref(&img, direction)
}

fn split_into_chunks_ref(img: &RgbaImage, direction: StitchDirection) -> Vec<RgbaImage> {
    let main = img_axis_dims(img, direction).0;
    let mut out = Vec::with_capacity((main / CHUNK_MAIN_SPAN + 1) as usize);
    let mut start = 0u32;
    while start < main {
        let len = CHUNK_MAIN_SPAN.min(main - start);
        out.push(slice_img_axis(img, direction, start, len));
        start += len;
    }
    out
}
