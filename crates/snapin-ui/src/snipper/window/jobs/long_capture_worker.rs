//! 长截图后台分析与缩略图降采样工作线程。
//!
//! # 设计原理
//!
//! 长截图抓屏返回的全屏图像裁剪、多层高斯金字塔 ZNCC 位移匹配与超长缩略图双线性降采样
//! 均属于计算密集型操作，若放在 Winit GUI 主线程执行会直接阻塞用户输入与动画渲染。
//! 本模块使用单一有界同步通道（`sync_channel(4)`，杜绝无界通道内存失控）驱动后台专用线程：
//! - 帧匹配任务按捕获序在后台无副作用完成，通过状态代际号 `expected_generation` 防呆；
//! - 缩略图任务通过最新单槽合并替换实现背压折半覆盖（连续快速滚动期间自动跳过中间过期帧）。

use image::{GenericImageView, RgbaImage};
use snapin_render::{
    analyze_frame_shift, render_thumbnail_overview, FrameShiftAnalysis, PagedCanvasSnapshot,
    StitchDirection, StitchOptions, ThumbnailOverview, ThumbnailRenderSpec,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

/// 后台长截图工作线程向 UI 事件循环投递的异步结果。
pub enum LongCaptureAsyncEvent {
    /// 后台已完成整屏裁剪与帧间位移分析，交由主线程快速提交分块画布。
    FrameAnalyzed {
        /// 对应的抓屏请求标识
        request_id: u64,
        /// 提交分析时对应的拼接器代际号
        expected_generation: u64,
        /// 原始全屏抓屏图像（匹配成功时晋升为新底图）
        full_frame: RgbaImage,
        /// 已裁剪出的选区局部帧
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

enum WorkerMsg {
    Analyze(Box<AnalyzeFrameTask>),
    ThumbnailTick,
}

/// 长截图专用有界后台计算线程控制器，退出作用域时依赖 RAII 自动平滑停止。
pub struct LongCaptureWorker {
    sender: Option<SyncSender<WorkerMsg>>,
    latest_thumb: Arc<Mutex<Option<ThumbnailRenderTask>>>,
    thumb_queued: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl LongCaptureWorker {
    /// 启动长截图专用后台工作线程，并绑定 UI 事件唤醒回传通道。
    pub fn new(sink: LongCaptureEventSink) -> Self {
        let (sender, receiver) = mpsc::sync_channel::<WorkerMsg>(4);
        let latest_thumb = Arc::new(Mutex::new(None::<ThumbnailRenderTask>));
        let thumb_queued = Arc::new(AtomicBool::new(false));
        let thumb_slot = latest_thumb.clone();
        let queued_flag = thumb_queued.clone();

        let builder = thread::Builder::new().name("snapin-long-capture".to_string());
        let handle = builder
            .spawn(move || {
                for msg in receiver {
                    match msg {
                        WorkerMsg::Analyze(task) => run_analyze_task(*task, &sink),
                        WorkerMsg::ThumbnailTick => {
                            run_latest_thumbnail_task(&thumb_slot, &queued_flag, &sink);
                        }
                    }
                }
            })
            .ok();

        Self {
            sender: Some(sender),
            latest_thumb,
            thumb_queued,
            handle,
        }
    }

    /// 非阻塞提交抓屏裁剪与位移分析任务；工作线程不可用或通道已满时返还原任务以便调用方兜底处理。
    pub fn schedule_frame_analysis(
        &self,
        task: AnalyzeFrameTask,
    ) -> Result<(), Box<AnalyzeFrameTask>> {
        let Some(sender) = &self.sender else {
            return Err(Box::new(task));
        };
        match sender.try_send(WorkerMsg::Analyze(Box::new(task))) {
            Ok(()) => Ok(()),
            Err(mpsc::TrySendError::Full(WorkerMsg::Analyze(task)))
            | Err(mpsc::TrySendError::Disconnected(WorkerMsg::Analyze(task))) => Err(task),
            Err(_) => unreachable!("分析派发通道只可能回传 Analyze 消息"),
        }
    }

    /// 以最新覆盖合并语义非阻塞调度缩略图渲染，自动丢弃被超越的过期中间任务。
    pub fn schedule_thumbnail_render(&self, task: ThumbnailRenderTask) {
        let Some(sender) = &self.sender else {
            return;
        };
        {
            let mut slot = self
                .latest_thumb
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            *slot = Some(task);
        }
        if !self.thumb_queued.swap(true, Ordering::AcqRel)
            && sender.try_send(WorkerMsg::ThumbnailTick).is_err()
        {
            self.thumb_queued.store(false, Ordering::Release);
        }
    }
}

impl Drop for LongCaptureWorker {
    fn drop(&mut self) {
        drop(self.sender.take());
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn run_analyze_task(task: AnalyzeFrameTask, sink: &LongCaptureEventSink) {
    let (x, y, width, height) = task.selection_bounds;
    if width == 0 || height == 0 {
        return;
    }
    let selection_frame = task.full_frame.view(x, y, width, height).to_image();
    let analysis = analyze_frame_shift(
        &task.anchor_frame,
        &selection_frame,
        task.direction,
        &task.options,
    );
    sink(LongCaptureAsyncEvent::FrameAnalyzed {
        request_id: task.request_id,
        expected_generation: task.expected_generation,
        full_frame: task.full_frame,
        selection_frame,
        analysis,
    });
}

fn run_latest_thumbnail_task(
    thumb_slot: &Arc<Mutex<Option<ThumbnailRenderTask>>>,
    queued_flag: &Arc<AtomicBool>,
    sink: &LongCaptureEventSink,
) {
    loop {
        queued_flag.store(false, Ordering::Release);
        let next = {
            let mut slot = thumb_slot
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
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
            let has_more = thumb_slot
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .is_some();
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
}
