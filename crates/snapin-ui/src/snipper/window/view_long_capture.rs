//! `SnipperView` 长截图异步抓取、后台匹配提交与缩略图后台预渲染流水线扩展。

use super::event_dispatcher::dispatch_pending_long_capture_frame;
use super::jobs::{
    split_captured_frame_and_selection, AnalyzeFrameTask, LongCaptureAsyncEvent,
    ScheduleAnalysisError, ThumbnailRenderTask,
};
use super::state::frame_sync::{AnalyzedLongFrame, PlannedLongFrame};
use super::state::CapturedFrameEffect;
use super::view::SnipperView;
use crate::ports::PortError;
use crate::snipper::selection::SelectionState;
use image::RgbaImage;
use snapin_foundation::i18n::tr;
use snapin_render::analyze_frame_shift;
use std::time::Instant;
use tracing::{debug, warn};

impl SnipperView {
    /// 处理异步长截图抓屏结果：立即恢复覆盖层交互，并把繁重的裁剪与匹配分析卸载到后台线程。
    ///
    /// 抓屏期间覆盖层处于指针穿透或隐藏状态；无论成功、失败还是结果已过期，
    /// 都必须第一时间恢复覆盖层指针命中与必要可见性，再非阻塞投递匹配任务。
    pub fn handle_screen_capture_finished(
        &mut self,
        request_id: u64,
        result: Result<RgbaImage, PortError>,
    ) {
        if let Err(error) = self.window.set_cursor_hittest(true) {
            debug!("恢复覆盖层指针命中失败: {error}");
        }
        if !self.window.is_visible().unwrap_or(false) {
            self.window.set_visible(true);
        }
        // 录制进行中严禁每帧抢回系统焦点，防止底层目标窗口频繁失焦导致下拉列表关闭或滚轮响应中断；
        // 仅当捕获已暂停或退出长截图模式时才把焦点还给覆盖层以恢复快捷键控制。
        let still_capturing = self
            .state
            .long_capture
            .as_ref()
            .is_some_and(|session| session.is_capturing);
        if !still_capturing {
            self.window.focus_window();
        }
        match self
            .state
            .plan_long_frame_analysis(&self.bg_image, request_id, result)
        {
            PlannedLongFrame::Completed(CapturedFrameEffect::Stale) => {
                self.window.request_redraw();
                return;
            }
            PlannedLongFrame::Completed(_) => {}
            PlannedLongFrame::Dispatch(task) => {
                match self.long_capture_worker.schedule_frame_analysis(task) {
                    Ok(()) => self.window.request_redraw(),
                    Err(ScheduleAnalysisError::Full(dropped_task)) => {
                        self.state
                            .jobs
                            .finish_screen_capture(dropped_task.request_id);
                        if let Some(session) = self.state.long_capture.as_mut() {
                            session.record_analysis_backpressure(Instant::now());
                        }
                        warn!(
                            "长截图后台分析队列已满，已丢弃过载帧并进入背压冷却（请求序号: {}）",
                            dropped_task.request_id
                        );
                        self.state
                            .set_status_message(tr("处理队列繁忙，已限流等待对齐"));
                        self.window.request_redraw();
                    }
                    Err(ScheduleAnalysisError::Disconnected(fallback)) => {
                        warn!(
                            "长截图后台分析通道不可用，降级为界面线程同步对齐（请求序号: {}）",
                            fallback.request_id
                        );
                        self.apply_sync_fallback_analysis(*fallback);
                    }
                }
                return;
            }
        }
        self.window.request_redraw();
    }

    /// 接收长截图后台工作线程完成的位移匹配或缩略图结果，并刷新选区底图与重绘。
    pub fn handle_long_capture_event(&mut self, event: LongCaptureAsyncEvent) {
        match event {
            LongCaptureAsyncEvent::FrameAnalyzed {
                request_id,
                expected_generation,
                full_frame,
                selection_frame,
                analysis,
            } => {
                let effect = self.state.apply_analyzed_long_frame(
                    &mut self.bg_image,
                    AnalyzedLongFrame {
                        request_id,
                        expected_generation,
                        full_frame,
                        selection_frame,
                        analysis,
                    },
                );
                self.finish_analyzed_effect(effect);
            }
            LongCaptureAsyncEvent::ThumbnailReady {
                generation,
                canvas_dim,
                spec,
                overview,
            } => {
                if generation >= self.last_installed_thumb_gen {
                    if let Some(session) = self.state.long_capture.as_mut() {
                        session
                            .stitcher
                            .install_thumbnail_cache(canvas_dim, spec, overview);
                        self.last_installed_thumb_gen = generation;
                        self.window.request_redraw();
                    }
                }
            }
        }
    }

    fn apply_sync_fallback_analysis(&mut self, task: AnalyzeFrameTask) {
        let (full_frame, selection_frame) =
            split_captured_frame_and_selection(task.full_frame, task.selection_bounds);
        let analysis = analyze_frame_shift(
            &task.anchor_frame,
            &selection_frame,
            task.direction,
            &task.options,
        );
        let effect = self.state.apply_analyzed_long_frame(
            &mut self.bg_image,
            AnalyzedLongFrame {
                request_id: task.request_id,
                expected_generation: task.expected_generation,
                full_frame,
                selection_frame,
                analysis,
            },
        );
        self.finish_analyzed_effect(effect);
    }

    fn finish_analyzed_effect(&mut self, effect: CapturedFrameEffect) {
        match effect {
            CapturedFrameEffect::Stale => return,
            CapturedFrameEffect::NoBgUpdate => {}
            CapturedFrameEffect::BgUpdated => {
                let res = if let SelectionState::Locked { rect, .. } = self.state.viewport.selection
                {
                    self.presenter
                        .update_background_region(&self.bg_image, &rect, true)
                } else {
                    self.presenter.replace_background(&self.bg_image, true)
                };
                if let Err(error) = res {
                    tracing::warn!("局部更新长截图实时底图失败: {error}");
                }
                if let Some(session) = self.state.long_capture.as_mut() {
                    if let Some(spec) = session.last_thumbnail_spec {
                        let generation = session.stitcher.generation();
                        session.desired_thumbnail_spec = Some((generation, spec));
                    }
                }
                self.schedule_desired_long_capture_thumbnail();
            }
        }
        // 前帧分析刚落盘，若用户在这期间持续滑动留下了累积滚轮增量则立刻无缝续拍
        dispatch_pending_long_capture_frame(
            &self.window,
            &mut self.state,
            self.bg_image.dimensions(),
        );
        self.window.request_redraw();
    }

    /// 推进长截图自动滚到底节拍；到达预定空窗间隔后触发下一轮滚动抓屏。
    pub fn tick_long_capture_auto_scroll(&mut self, now: std::time::Instant) -> bool {
        let due = self
            .state
            .long_capture
            .as_ref()
            .and_then(|session| session.auto_scroll_deadline())
            .is_some_and(|deadline| now >= deadline);
        if !due {
            return false;
        }
        dispatch_pending_long_capture_frame(
            &self.window,
            &mut self.state,
            self.bg_image.dimensions(),
        )
    }

    pub(super) fn schedule_desired_long_capture_thumbnail(&mut self) {
        let Some(session) = self.state.long_capture.as_mut() else {
            return;
        };
        let Some((generation, spec)) = session.desired_thumbnail_spec.take() else {
            return;
        };
        if self.last_scheduled_thumb == Some((generation, spec)) {
            return;
        }
        self.last_scheduled_thumb = Some((generation, spec));
        self.long_capture_worker
            .schedule_thumbnail_render(ThumbnailRenderTask {
                generation,
                snapshot: session.stitcher.thumbnail_snapshot(),
                spec,
            });
    }
}
