//! 长截图异步抓屏预处理规划、后台分析提交与状态提示收口。
//!
//! 将抓屏帧校验、后台分析任务打包以及最终 `StitchFrameOutcome` 的底图/状态收敛逻辑
//! 集中管理，使同步单测入口 `apply_captured_long_frame` 与真实异步流水线共享同一套状态语义。

use super::{AutoScrollStopReason, CapturedFrameEffect, SnipperState};
use crate::ports::PortError;
use crate::snipper::selection::SelectionState;
use crate::snipper::window::jobs::AnalyzeFrameTask;
use image::{imageops, RgbaImage};
use snapin_foundation::{i18n::tr, trf};
use snapin_render::{FrameShiftAnalysis, StitchFrameOutcome};
use tracing::{debug, warn};

/// 后台分析任务完成回传给主状态机的打包载荷。
pub(crate) struct AnalyzedLongFrame {
    pub(crate) request_id: u64,
    pub(crate) expected_generation: u64,
    pub(crate) full_frame: Option<RgbaImage>,
    pub(crate) selection_frame: RgbaImage,
    pub(crate) analysis: FrameShiftAnalysis,
}

/// 在后台调度分析前完成的前置校验结果。
pub(crate) enum PlannedLongFrame {
    /// 帧已陈旧、已暂停、失败或分辨率失配，已在前置校验阶段完成收口，无需继续派发后台任务
    Completed(CapturedFrameEffect),
    /// 通过校验，可立即非阻塞投递到后台工作线程完成选区裁剪与位移估计
    Dispatch(AnalyzeFrameTask),
}

impl SnipperState {
    /// 校验异步抓屏回调并将其封装为无需持有 `&SnipperState` 的后台分析任务。
    pub(crate) fn plan_long_frame_analysis(
        &mut self,
        bg_image: &RgbaImage,
        request_id: u64,
        result: Result<RgbaImage, PortError>,
    ) -> PlannedLongFrame {
        if !self.jobs.is_active_screen_capture(request_id) {
            return PlannedLongFrame::Completed(CapturedFrameEffect::Stale);
        }
        let frame = match result {
            Ok(frame) => frame,
            Err(error) => {
                self.jobs.finish_screen_capture(request_id);
                if let Some(session) = self.long_capture.as_mut() {
                    session.stop_auto_scroll();
                }
                warn!("长截图抓屏失败，请求序号: {request_id}，原因: {error}");
                self.set_status_message(trf!(
                    "长截图抓屏失败: {}",
                    "Long capture failed: {}",
                    error
                ));
                return PlannedLongFrame::Completed(CapturedFrameEffect::NoBgUpdate);
            }
        };
        let Some(bounds) = self.locked_selection_bounds(bg_image.width(), bg_image.height()) else {
            self.jobs.finish_screen_capture(request_id);
            return PlannedLongFrame::Completed(CapturedFrameEffect::NoBgUpdate);
        };
        if !self.is_valid_long_capture_dimensions(bg_image, &frame, (bounds.2, bounds.3)) {
            self.jobs.finish_screen_capture(request_id);
            return PlannedLongFrame::Completed(CapturedFrameEffect::NoBgUpdate);
        }
        let Some(session) = self.long_capture.as_ref() else {
            self.jobs.finish_screen_capture(request_id);
            return PlannedLongFrame::Completed(CapturedFrameEffect::NoBgUpdate);
        };
        if (bounds.2, bounds.3) != session.stitcher.initial_frame_size() {
            self.jobs.finish_screen_capture(request_id);
            return PlannedLongFrame::Completed(CapturedFrameEffect::NoBgUpdate);
        }

        PlannedLongFrame::Dispatch(AnalyzeFrameTask {
            request_id,
            expected_generation: session.stitcher.generation(),
            anchor_frame: session.stitcher.last_frame_arc(),
            full_frame: frame,
            selection_bounds: bounds,
            direction: session.stitcher.direction(),
            options: *session.stitcher.options(),
        })
    }

    /// 在 GUI 主线程提交后台匹配分析结果，并在发生位移时更新冻结底图与提示文案。
    pub(crate) fn apply_analyzed_long_frame(
        &mut self,
        bg_image: &mut RgbaImage,
        analyzed: AnalyzedLongFrame,
    ) -> CapturedFrameEffect {
        if !self.jobs.finish_screen_capture(analyzed.request_id) {
            return CapturedFrameEffect::Stale;
        }
        let Some(bounds) = self.locked_selection_bounds(bg_image.width(), bg_image.height()) else {
            return CapturedFrameEffect::NoBgUpdate;
        };
        let frame_for_check = analyzed
            .full_frame
            .as_ref()
            .unwrap_or(&analyzed.selection_frame);
        if !self.is_valid_long_capture_dimensions(bg_image, frame_for_check, (bounds.2, bounds.3)) {
            return CapturedFrameEffect::NoBgUpdate;
        }
        let Some(session) = self.long_capture.as_mut() else {
            return CapturedFrameEffect::NoBgUpdate;
        };
        let outcome = session.apply_analyzed_shift(
            analyzed.selection_frame,
            analyzed.expected_generation,
            analyzed.analysis,
        );
        let region_patch = if matches!(outcome, StitchFrameOutcome::Stitched { .. })
            && analyzed.full_frame.is_none()
        {
            Some((bounds.0, bounds.1, session.stitcher.last_frame_arc()))
        } else {
            None
        };
        if let Some((bx, by, patch)) = region_patch {
            imageops::replace(bg_image, patch.as_ref(), i64::from(bx), i64::from(by));
        }
        self.finish_stitch_outcome(bg_image, analyzed.full_frame, outcome)
    }

    /// 检查抓屏帧与会话激活态及屏幕/选区尺寸的一致性；同时兼容全屏帧与选区局部帧。
    pub(super) fn is_valid_long_capture_dimensions(
        &mut self,
        bg_image: &RgbaImage,
        frame: &RgbaImage,
        selection_size: (u32, u32),
    ) -> bool {
        if !self
            .long_capture
            .as_ref()
            .is_some_and(|session| session.is_capturing)
        {
            return false;
        }
        let dims = frame.dimensions();
        if dims != bg_image.dimensions() && dims != selection_size {
            warn!(
                "长截图抓屏分辨率与当前画布/选区均不一致，抓屏={}x{}，画布={}x{}，选区={}x{}，已保留原底图",
                frame.width(),
                frame.height(),
                bg_image.width(),
                bg_image.height(),
                selection_size.0,
                selection_size.1,
            );
            self.set_status_message(tr("抓屏分辨率与当前画布不一致，已保留原底图"));
            return false;
        }
        true
    }

    pub(super) fn locked_selection_bounds(
        &self,
        max_w: u32,
        max_h: u32,
    ) -> Option<(u32, u32, u32, u32)> {
        let SelectionState::Locked { rect, .. } = self.viewport.selection else {
            return None;
        };
        let (_, _, w, h) = rect.to_pixel_bounds(max_w, max_h);
        if w == 0 || h == 0 {
            return None;
        }
        Some(rect.to_pixel_bounds(max_w, max_h))
    }

    /// 按统一状态转移表完成 `StitchFrameOutcome` 到冻结底图与用户提示的收敛。
    pub(super) fn finish_stitch_outcome(
        &mut self,
        bg_image: &mut RgbaImage,
        full_frame: Option<RgbaImage>,
        outcome: StitchFrameOutcome,
    ) -> CapturedFrameEffect {
        let stop_reason = self.long_capture.as_mut().and_then(
            crate::snipper::window::state::LongCaptureSession::take_last_auto_stop_reason,
        );
        let is_auto = self
            .long_capture
            .as_ref()
            .is_some_and(crate::snipper::window::state::LongCaptureSession::is_auto_scrolling);
        let recording_hint = if is_auto {
            tr("自动滚到底录制中：再次点击自动滚动按钮或滚动鼠标可随时停止")
        } else {
            tr("长截图录制中：在选区内滚动鼠标自动拼接")
        };
        let effect = match outcome {
            StitchFrameOutcome::Stitched { .. } => {
                if let Some(frame) = full_frame {
                    *bg_image = frame;
                }
                if self.session.status_message.as_deref() != Some(recording_hint) {
                    self.set_status_message(recording_hint);
                }
                CapturedFrameEffect::BgUpdated
            }
            StitchFrameOutcome::Unchanged => {
                if self.session.status_message.as_deref() != Some(recording_hint) {
                    self.set_status_message(recording_hint);
                }
                CapturedFrameEffect::NoBgUpdate
            }
            StitchFrameOutcome::Unmatched { confidence } => {
                debug!(confidence, "长截图新帧未能匹配当前画布");
                self.set_status_message(trf!(
                    "未能匹配新画面（重叠度 {:.0}%）：请减慢滚动速度或回退重叠内容",
                    "Failed to match new frame (overlap {:.0}%): scroll slower or scroll back to overlap",
                    confidence * 100.0
                ));
                CapturedFrameEffect::NoBgUpdate
            }
            StitchFrameOutcome::LimitReached => {
                warn!("长截图画布已达到最大安全像素或长度上限，已停止继续扩展");
                self.set_status_message(tr("长截图画布已达最大安全上限，已暂停追加后续内容"));
                CapturedFrameEffect::NoBgUpdate
            }
            StitchFrameOutcome::Ignored => CapturedFrameEffect::NoBgUpdate,
        };
        match stop_reason {
            Some(AutoScrollStopReason::ReachedEnd) => {
                self.set_status_message(tr("已自动滚动到底部并停止"));
            }
            Some(AutoScrollStopReason::UnmatchedStalled) => {
                self.set_status_message(tr("自动滚动连续未能匹配新画面，已安全停止"));
            }
            Some(AutoScrollStopReason::CanvasLimitReached) | None => {}
        }
        effect
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snipper::selection::Rect;
    use crate::snipper::window::state::LongCaptureSession;
    use image::Rgba;
    use snapin_render::{analyze_frame_shift, StitchOptions};

    fn textured_screen(width: u32, height: u32, scroll_y: u32) -> RgbaImage {
        let mut img = RgbaImage::from_pixel(width, height, Rgba([20, 20, 20, 255]));
        for y in 20..180 {
            let doc_y = y + scroll_y;
            let band = (doc_y / 12) as u8;
            for x in 20..180 {
                let r = band.wrapping_mul(41).wrapping_add((x % 31) as u8) % 230 + 15;
                let g = ((doc_y * 13 + x * 5) % 210) as u8 + 20;
                let b = (((doc_y * 7) ^ (x * 17)) % 210) as u8 + 20;
                img.put_pixel(x, y, Rgba([r, g, b, 255]));
            }
        }
        img
    }

    #[test]
    fn test_plan_and_apply_analyzed_long_frame_end_to_end() {
        let mut state = SnipperState::new();
        let rect = Rect::new(20.0, 20.0, 160.0, 160.0);
        state.viewport.selection = SelectionState::Locked {
            rect,
            active_drag: None,
        };
        let mut bg = textured_screen(220, 220, 0);
        let init_crop = image::GenericImageView::view(&bg, 20, 20, 160, 160).to_image();
        state.long_capture = Some(LongCaptureSession::new(init_crop));

        let req_id = state.jobs.begin_screen_capture().expect("应成功登记请求");
        let next_screen = textured_screen(220, 220, 48);
        let PlannedLongFrame::Dispatch(task) =
            state.plan_long_frame_analysis(&bg, req_id, Ok(next_screen.clone()))
        else {
            panic!("合法抓屏帧应规划为后台分析任务");
        };
        assert!(state.jobs.has_pending_screen_capture());

        let (sx, sy, sw, sh) = task.selection_bounds;
        let sel = image::GenericImageView::view(&task.full_frame, sx, sy, sw, sh).to_image();
        let analysis = analyze_frame_shift(&task.anchor_frame, &sel, task.direction, &task.options);
        let effect = state.apply_analyzed_long_frame(
            &mut bg,
            AnalyzedLongFrame {
                request_id: task.request_id,
                expected_generation: task.expected_generation,
                full_frame: Some(task.full_frame),
                selection_frame: sel,
                analysis,
            },
        );
        assert_eq!(effect, CapturedFrameEffect::BgUpdated);
        assert!(!state.jobs.has_pending_screen_capture());
        assert_eq!(bg, next_screen);
        let dims = state
            .long_capture
            .as_ref()
            .expect("会话应在")
            .stitcher
            .dimensions();
        assert!(dims.1 > 160);
    }

    #[test]
    fn test_limit_reached_outcome_sets_status_without_overwriting_bg() {
        let mut state = SnipperState::new();
        let rect = Rect::new(20.0, 20.0, 160.0, 160.0);
        state.viewport.selection = SelectionState::Locked {
            rect,
            active_drag: None,
        };
        let mut bg = textured_screen(220, 220, 0);
        let orig_bg = bg.clone();
        let init_crop = image::GenericImageView::view(&bg, 20, 20, 160, 160).to_image();
        let mut session = LongCaptureSession::new(init_crop);
        let opts = StitchOptions {
            max_canvas_extent: 170,
            ..Default::default()
        };
        session.stitcher.set_options(opts);
        state.long_capture = Some(session);

        let effect = state.finish_stitch_outcome(
            &mut bg,
            Some(textured_screen(220, 220, 48)),
            StitchFrameOutcome::LimitReached,
        );
        assert_eq!(effect, CapturedFrameEffect::NoBgUpdate);
        assert_eq!(bg, orig_bg);
        assert!(state
            .session
            .status_message
            .as_deref()
            .is_some_and(|msg| msg.contains("上限")));
    }

    #[test]
    fn test_region_sized_frame_patches_selection_without_touching_surroundings() {
        let mut state = SnipperState::new();
        let rect = Rect::new(20.0, 20.0, 160.0, 160.0);
        state.viewport.selection = SelectionState::Locked {
            rect,
            active_drag: None,
        };
        let mut bg = textured_screen(220, 220, 0);
        let init_crop = image::GenericImageView::view(&bg, 20, 20, 160, 160).to_image();
        state.long_capture = Some(LongCaptureSession::new(init_crop));

        let next_full = textured_screen(220, 220, 48);
        let region_only = image::GenericImageView::view(&next_full, 20, 20, 160, 160).to_image();
        let req_id = state.jobs.begin_screen_capture().expect("应成功登记请求");
        let PlannedLongFrame::Dispatch(task) =
            state.plan_long_frame_analysis(&bg, req_id, Ok(region_only.clone()))
        else {
            panic!("局部选区尺寸帧应合法通过前置校验");
        };
        assert_eq!(task.full_frame.dimensions(), (160, 160));

        let analysis = analyze_frame_shift(
            &task.anchor_frame,
            &region_only,
            task.direction,
            &task.options,
        );
        let effect = state.apply_analyzed_long_frame(
            &mut bg,
            AnalyzedLongFrame {
                request_id: task.request_id,
                expected_generation: task.expected_generation,
                full_frame: None,
                selection_frame: region_only.clone(),
                analysis,
            },
        );
        assert_eq!(effect, CapturedFrameEffect::BgUpdated);
        assert_eq!(
            image::GenericImageView::view(&bg, 20, 20, 160, 160).to_image(),
            region_only
        );
        assert_eq!(*bg.get_pixel(5, 5), Rgba([20, 20, 20, 255]));
    }
}
