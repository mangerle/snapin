//! `SnipperView` 长截图异步抓取、后台匹配提交与缩略图后台预渲染流水线扩展。

use super::jobs::{AnalyzeFrameTask, LongCaptureAsyncEvent, ThumbnailRenderTask};
use super::state::frame_sync::{AnalyzedLongFrame, PlannedLongFrame};
use super::state::CapturedFrameEffect;
use super::view::SnipperView;
use crate::ports::PortError;
use crate::snipper::selection::SelectionState;
use image::{GenericImageView, RgbaImage};
use snapin_render::analyze_frame_shift;

impl SnipperView {
    /// 处理异步长截图抓屏结果：立即恢复覆盖层可见性，并把繁重的裁剪与匹配分析卸载到后台线程。
    ///
    /// 抓屏期间覆盖层被隐藏以确保捕获到底层应用；无论成功、失败还是结果已过期，
    /// 都必须第一时间恢复窗口可见，再非阻塞投递匹配任务。
    pub fn handle_screen_capture_finished(
        &mut self,
        request_id: u64,
        result: Result<RgbaImage, PortError>,
    ) {
        self.window.set_visible(true);
        self.window.focus_window();
        match self
            .state
            .plan_long_frame_analysis(&self.bg_image, request_id, result)
        {
            PlannedLongFrame::Completed(CapturedFrameEffect::Stale) => return,
            PlannedLongFrame::Completed(_) => {}
            PlannedLongFrame::Dispatch(task) => {
                if let Err(fallback) = self.long_capture_worker.schedule_frame_analysis(task) {
                    self.apply_sync_fallback_analysis(*fallback);
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
        let (x, y, w, h) = task.selection_bounds;
        let selection_frame = task.full_frame.view(x, y, w, h).to_image();
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
                full_frame: task.full_frame,
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
        self.window.request_redraw();
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
