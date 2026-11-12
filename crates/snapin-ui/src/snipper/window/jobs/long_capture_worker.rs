//! 长截图后台分析与缩略图降采样工作线程。
//!
//! # 设计原理
//!
//! 长截图抓屏返回的全屏图像裁剪、多层高斯金字塔 ZNCC 位移匹配与超长缩略图双线性降采样
//! 均属于计算密集型操作，若放在 Winit GUI 主线程执行会直接阻塞用户输入与动画渲染。
//! 本模块采用职责分离的独立有界后台线程（杜绝无界通道内存失控，并消除缩略图对帧分析的队头阻塞）：
//! - **帧分析线程**（`sync_channel(4)`）：按捕获序无副作用执行位移匹配，通过 `expected_generation` 防呆；
//! - **缩略图线程**（`sync_channel(2)` + 最新单槽合并）：在独立线程中降采样预览图，绝不阻塞高优位移分析。

use image::{GenericImageView, RgbaImage};
use snapin_render::{
    analyze_frame_shift, render_thumbnail_overview, FrameShiftAnalysis, PagedCanvasSnapshot,
    StitchDirection, StitchOptions, ThumbnailOverview, ThumbnailRenderSpec,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use tracing::warn;

/// 后台长截图工作线程向 UI 事件循环投递的异步结果。
pub enum LongCaptureAsyncEvent {
    /// 后台已完成选区裁剪与帧间位移分析，交由主线程快速提交分块画布。
    FrameAnalyzed {
        /// 对应的抓屏请求标识
        request_id: u64,
        /// 提交分析时对应的拼接器代际号
        expected_generation: u64,
        /// 仅全屏降级抓屏时保留的整屏图像；选区局部抓屏路径下为 `None`（免整屏拷帧）
        full_frame: Option<RgbaImage>,
        /// 与当前锁定选区等大的局部帧
        selection_frame: RgbaImage,
        /// 帧间匹配结论
        analysis: FrameShiftAnalysis,
    },
    /// 后台已完成分块画布缩略图概览降采样。
    ThumbnailReady {
        /// 生成该缩略图时对应的拼接器代际号
        generation: u64,
        /// 生成该缩略图时的画布总宽高
        canvas_dim: (u32, u32),
        /// 渲染使用的规格键
        spec: ThumbnailRenderSpec,
        /// 渲染完成的高可读缩略图概览
        overview: ThumbnailOverview,
    },
}

/// 长截图后台工作结果回传回调。
pub type LongCaptureEventSink = Arc<dyn Fn(LongCaptureAsyncEvent) + Send + Sync>;

/// 投递给后台线程的一帧位移分析请求。
#[derive(Debug)]
pub struct AnalyzeFrameTask {
    /// 抓屏请求标识
    pub request_id: u64,
    /// 提交分析时对应的拼接器代际号
    pub expected_generation: u64,
    /// 上一锚点帧的共享只读引用
    pub anchor_frame: Arc<RgbaImage>,
    /// 本次抓屏返回的完整屏幕图像
    pub full_frame: RgbaImage,
    /// 当前锁定选区边界 `(x, y, width, height)`
    pub selection_bounds: (u32, u32, u32, u32),
    /// 当前拼接方向
    pub direction: StitchDirection,
    /// 当前拼接算法参数
    pub options: StitchOptions,
}

/// 投递给后台线程的缩略图降采样请求。
pub struct ThumbnailRenderTask {
    /// 目标拼接器代际号
    pub generation: u64,
    /// 零拷贝分块画布快照
    pub snapshot: PagedCanvasSnapshot,
    /// 渲染规格参数
    pub spec: ThumbnailRenderSpec,
}

/// 长截图专用有界后台双线程控制器，退出作用域时依赖 RAII 自动平滑停止。
pub struct LongCaptureWorker {
    analyze_sender: Option<SyncSender<Box<AnalyzeFrameTask>>>,
    thumb_sender: Option<SyncSender<()>>,
    latest_thumb: Arc<Mutex<Option<ThumbnailRenderTask>>>,
    thumb_queued: Arc<AtomicBool>,
    analyze_handle: Option<JoinHandle<()>>,
    thumb_handle: Option<JoinHandle<()>>,
}

impl LongCaptureWorker {
    /// 启动长截图专用后台分析线程与缩略图降采样线程，并绑定 UI 事件唤醒回传通道。
    pub fn new(sink: LongCaptureEventSink) -> Self {
        let (analyze_sender, analyze_receiver) = mpsc::sync_channel::<Box<AnalyzeFrameTask>>(4);
        let (thumb_sender, thumb_receiver) = mpsc::sync_channel::<()>(2);
        let latest_thumb = Arc::new(Mutex::new(None::<ThumbnailRenderTask>));
        let thumb_queued = Arc::new(AtomicBool::new(false));
        let thumb_slot = latest_thumb.clone();
        let queued_flag = thumb_queued.clone();

        let analyze_sink = sink.clone();
        let analyze_handle = thread::Builder::new()
            .name("snapin-long-analyze".to_string())
            .spawn(move || {
                for task in analyze_receiver {
                    run_analyze_task(*task, &analyze_sink);
                }
            })
            .ok();

        let thumb_handle = thread::Builder::new()
            .name("snapin-long-thumb".to_string())
            .spawn(move || {
                for () in thumb_receiver {
                    run_latest_thumbnail_task(&thumb_slot, &queued_flag, &sink);
                }
            })
            .ok();

        Self {
            analyze_sender: Some(analyze_sender),
            thumb_sender: Some(thumb_sender),
            latest_thumb,
            thumb_queued,
            analyze_handle,
            thumb_handle,
        }
    }

    /// 非阻塞提交抓屏裁剪与位移分析任务；工作线程不可用或通道已满时返还原任务以便调用方兜底处理。
    pub fn schedule_frame_analysis(
        &self,
        task: AnalyzeFrameTask,
    ) -> Result<(), Box<AnalyzeFrameTask>> {
        let Some(sender) = &self.analyze_sender else {
            return Err(Box::new(task));
        };
        match sender.try_send(Box::new(task)) {
            Ok(()) => Ok(()),
            Err(mpsc::TrySendError::Full(task)) | Err(mpsc::TrySendError::Disconnected(task)) => {
                Err(task)
            }
        }
    }

    /// 以最新覆盖合并语义非阻塞调度缩略图渲染，自动丢弃被超越的过期中间任务。
    pub fn schedule_thumbnail_render(&self, task: ThumbnailRenderTask) {
        let Some(sender) = &self.thumb_sender else {
            return;
        };
        {
            let mut slot = lock_thumb_slot(&self.latest_thumb);
            *slot = Some(task);
        }
        if !self.thumb_queued.swap(true, Ordering::AcqRel) && sender.try_send(()).is_err() {
            self.thumb_queued.store(false, Ordering::Release);
        }
    }
}

impl Drop for LongCaptureWorker {
    fn drop(&mut self) {
        drop(self.analyze_sender.take());
        drop(self.thumb_sender.take());
        if let Some(handle) = self.analyze_handle.take() {
            if handle.join().is_err() {
                warn!("长截图后台位移分析线程异常退出");
            }
        }
        if let Some(handle) = self.thumb_handle.take() {
            if handle.join().is_err() {
                warn!("长截图后台缩略图渲染线程异常退出");
            }
        }
    }
}

/// 若抓屏帧已精确等于选区尺寸则零拷贝直通，否则裁出锁定选区并保留原整屏帧供局部刷新替换。
pub(crate) fn split_captured_frame_and_selection(
    frame: RgbaImage,
    bounds: (u32, u32, u32, u32),
) -> (Option<RgbaImage>, RgbaImage) {
    let (x, y, width, height) = bounds;
    if frame.dimensions() == (width, height) {
        (None, frame)
    } else {
        let selection_frame = frame.view(x, y, width, height).to_image();
        (Some(frame), selection_frame)
    }
}

fn run_analyze_task(task: AnalyzeFrameTask, sink: &LongCaptureEventSink) {
    let (_, _, width, height) = task.selection_bounds;
    if width == 0 || height == 0 {
        return;
    }
    let (full_frame, selection_frame) =
        split_captured_frame_and_selection(task.full_frame, task.selection_bounds);
    let analysis = analyze_frame_shift(
        &task.anchor_frame,
        &selection_frame,
        task.direction,
        &task.options,
    );
    sink(LongCaptureAsyncEvent::FrameAnalyzed {
        request_id: task.request_id,
        expected_generation: task.expected_generation,
        full_frame,
        selection_frame,
        analysis,
    });
}

fn lock_thumb_slot(
    thumb_slot: &Arc<Mutex<Option<ThumbnailRenderTask>>>,
) -> MutexGuard<'_, Option<ThumbnailRenderTask>> {
    thumb_slot.lock().unwrap_or_else(|poisoned| {
        warn!("后台缩略图任务槽互斥锁处于中毒状态，已记录异常并接管清理陈旧槽位");
        let mut guard = poisoned.into_inner();
        *guard = None;
        guard
    })
}

fn run_latest_thumbnail_task(
    thumb_slot: &Arc<Mutex<Option<ThumbnailRenderTask>>>,
    queued_flag: &Arc<AtomicBool>,
    sink: &LongCaptureEventSink,
) {
    loop {
        queued_flag.store(false, Ordering::Release);
        let next = {
            let mut slot = lock_thumb_slot(thumb_slot);
            slot.take()
        };
        let Some(task) = next else {
            break;
        };
        let canvas_dim = task.snapshot.dimensions();
        let overview = render_thumbnail_overview(&task.snapshot, task.spec);
        sink(LongCaptureAsyncEvent::ThumbnailReady {
            generation: task.generation,
            canvas_dim,
            spec: task.spec,
            overview,
        });
        if !queued_flag.load(Ordering::Acquire) {
            let has_more = lock_thumb_slot(thumb_slot).is_some();
            if !has_more {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;
    use snapin_render::LongCaptureStitcher;

    fn textured_strip(width: u32, height: u32, offset_y: u32) -> RgbaImage {
        let mut img = RgbaImage::new(width, height);
        for y in 0..height {
            let eff_y = y + offset_y;
            let band = (eff_y / 10) as u8;
            for x in 0..width {
                let r = (band.wrapping_mul(37).wrapping_add((x % 29) as u8)) % 230 + 15;
                let g = ((eff_y * 11 + x * 5) % 210) as u8 + 20;
                let b = (((eff_y * 7) ^ (x * 13)) % 210) as u8 + 20;
                img.put_pixel(x, y, Rgba([r, g, b, 255]));
            }
        }
        img
    }

    #[test]
    fn test_worker_executes_analysis_and_coalesces_thumbnail_off_gui_thread() {
        let (tx, rx) = mpsc::sync_channel::<LongCaptureAsyncEvent>(8);
        let worker = LongCaptureWorker::new(Arc::new(move |ev| {
            let _ = tx.send(ev);
        }));

        let first = textured_strip(160, 180, 0);
        let mut stitcher =
            LongCaptureStitcher::new(first, StitchDirection::Vertical, StitchOptions::default());
        let next_full = textured_strip(220, 240, 40);

        worker
            .schedule_frame_analysis(AnalyzeFrameTask {
                request_id: 7,
                expected_generation: stitcher.generation(),
                anchor_frame: stitcher.last_frame_arc(),
                full_frame: next_full,
                selection_bounds: (0, 0, 160, 180),
                direction: stitcher.direction(),
                options: *stitcher.options(),
            })
            .expect("后台提交分析任务应成功");

        let analyzed = rx.recv().expect("应收到后台分析结果");
        let LongCaptureAsyncEvent::FrameAnalyzed {
            request_id,
            expected_generation,
            selection_frame,
            analysis,
            ..
        } = analyzed
        else {
            panic!("预期 FrameAnalyzed 事件");
        };
        assert_eq!(request_id, 7);
        assert!(stitcher
            .apply_analyzed_shift(selection_frame, expected_generation, analysis)
            .is_stitched());

        let spec = stitcher.build_thumbnail_spec(80, 120, 48);
        worker.schedule_thumbnail_render(ThumbnailRenderTask {
            generation: stitcher.generation(),
            snapshot: stitcher.thumbnail_snapshot(),
            spec,
        });
        let thumb_ev = rx.recv().expect("应收到后台缩略图结果");
        assert!(matches!(
            thumb_ev,
            LongCaptureAsyncEvent::ThumbnailReady { canvas_dim, .. } if canvas_dim == stitcher.dimensions()
        ));
    }

    #[test]
    fn test_split_captured_frame_skips_full_copy_for_region_sized_input() {
        let region_only = textured_strip(160, 180, 32);
        let (full, sel) = split_captured_frame_and_selection(region_only, (40, 60, 160, 180));
        assert!(full.is_none());
        assert_eq!(sel.dimensions(), (160, 180));
    }
}
