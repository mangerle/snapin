//! 截图会话媒体任务调度与结果回传。
//!
//! # 设计原理
//!
//! 截图窗口只负责生成“要处理哪一块图像”以及“结果如何呈现”，真正的 OCR、翻译、
//! 保存与剪贴板写入由应用层通过 [`MediaJobRunner`] 与 [`DesktopPort`] 完成。
//! 任务完成后经 `EventLoopProxy` 以用户事件回传窗口线程，避免在事件循环中执行
//! 阻塞推理或网络请求。
//!
//! 任务取消使用端口返回的不透明取消闭包，不把 Tokio 的 `AbortHandle` 泄漏到表现层，
//! 因此 UI 可以在没有异步运行时的单元测试环境中独立验证状态流转。

use super::state::SnipperState;
use super::types::SnipperOutcome;
use crate::ports::{JobCancel, JobId, MediaJob, MediaJobOutcome, MediaJobRunner, PortError};
use snapin_core::annotation::action::AnnotationShape;
use snapin_core::annotation::shapes::TranslationAction;
use std::sync::Arc;

/// 截图会话支持的后台任务类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnipperJobKind {
    Ocr,
    Translate,
    Save,
}

impl SnipperJobKind {
    /// 中文任务名称，用于界面提示
    pub fn label(self) -> &'static str {
        match self {
            Self::Ocr => "文字识别",
            Self::Translate => "原位翻译",
            Self::Save => "保存截图",
        }
    }
}

/// 截图任务结果回调类型。
///
/// 单次截图会话把结果投递到自己的 `EventLoopProxy`，常驻工作区则把结果投递到
/// 宿主事件循环；运行时因此不绑定具体事件类型，也不强制 UI 依赖 winit 代理。
pub type SnipperJobSink = Arc<dyn Fn(JobId, SnipperJobKind, MediaJobOutcome) + Send + Sync>;

/// 截图会话任务运行时：把结果回调与宿主媒体执行器组合在一起。
///
/// 每个截图会话独有一个实例，内部只持有 `Arc`，克隆代价极低。
#[derive(Clone)]
pub struct SnipperJobRuntime {
    runner: Arc<dyn MediaJobRunner>,
    sink: SnipperJobSink,
}

impl SnipperJobRuntime {
    /// 使用结果回调构建截图任务运行时
    pub fn new(runner: Arc<dyn MediaJobRunner>, sink: SnipperJobSink) -> Self {
        Self { runner, sink }
    }

    /// 提交任务并注册结果回传。
    ///
    /// # Errors
    ///
    /// 后台执行器停止、队列已满或任务参数非法时返回 [`PortError`]。
    pub fn submit(
        &self,
        kind: SnipperJobKind,
        job: MediaJob,
    ) -> Result<(JobId, JobCancel), PortError> {
        let sink = self.sink.clone();
        self.runner.submit(
            job,
            Box::new(move |job_id, outcome| {
                sink(job_id, kind, outcome);
            }),
        )
    }
}

/// 处理后台任务结果，驱动截图会话状态流转。
pub fn handle_job_outcome(
    state: &mut SnipperState,
    job_id: JobId,
    kind: SnipperJobKind,
    outcome: MediaJobOutcome,
) {
    if state.pending_job_id != Some(job_id) {
        tracing::debug!("丢弃过期的截图媒体任务结果: {job_id:?}");
        return;
    }
    state.pending_job = None;
    state.pending_job_id = None;
    state.pending_job_cancel = None;

    match outcome {
        MediaJobOutcome::Ocr { text } => {
            let text = text.trim();
            if text.is_empty() {
                state.set_status_message("未识别到文字");
                return;
            }
            match state.desktop.write_text_clipboard(text) {
                Ok(()) => {
                    // 识别结果只写入剪贴板，不结束截图会话：用户常常要在同一张图上继续
                    // 标注或再识别另一块区域，直接关闭会让人来不及确认结果。
                    state.set_status_message(format!(
                        "已复制识别文字（{} 字符）",
                        text.chars().count()
                    ));
                }
                Err(error) => {
                    tracing::warn!("复制 OCR 文本至系统剪贴板失败: {error}");
                    state.set_status_message(format!("复制识别文字失败: {error}"));
                }
            }
        }
        MediaJobOutcome::Translated { items } => {
            if items.is_empty() {
                state.set_status_message("未获得有效翻译结果");
                return;
            }
            state
                .annotations
                .push(AnnotationShape::Translation(TranslationAction::new(items)));
            state.toolbar.active_tool = None;
            state.toolbar.sub_rect = None;
            state.clear_status_message();
            tracing::info!("原位翻译完成，已成功压入标注动作栈");
        }
        MediaJobOutcome::Saved { path } => {
            state.outcome = Some(SnipperOutcome::Saved(path));
        }
        MediaJobOutcome::SaveCancelled => {
            state.set_status_message("已取消保存");
        }
        MediaJobOutcome::Failed { message } => {
            tracing::warn!("{}后台任务失败: {message}", kind.label());
            state.set_status_message(format!("{}失败: {message}", kind.label()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;
    use snapin_core::annotation::shapes::TranslationShapeItem;
    use snapin_core::config::CoverMode;
    use std::path::PathBuf;

    fn translation_item() -> TranslationShapeItem {
        TranslationShapeItem {
            rect: (10.0, 10.0, 40.0, 20.0),
            original: "Hello".to_string(),
            translated: "你好".to_string(),
            bg_color: (255, 255, 255, 255),
            text_color: (0, 0, 0, 255),
            font_size: 14.0,
            cover_mode: CoverMode::SolidColor,
        }
    }

    /// 让状态接受一个测试任务 ID，模拟真实提交成功后的待处理态。
    fn accept_job(state: &mut SnipperState) -> JobId {
        let job_id = JobId::new(1);
        state.pending_job_id = Some(job_id);
        job_id
    }

    #[test]
    fn test_handle_empty_ocr_outcome_keeps_session_open() {
        let mut state = SnipperState::new();
        let job_id = accept_job(&mut state);
        handle_job_outcome(
            &mut state,
            job_id,
            SnipperJobKind::Ocr,
            MediaJobOutcome::Ocr {
                text: String::new(),
            },
        );
        assert!(state.outcome.is_none());
        assert_eq!(state.status_message.as_deref(), Some("未识别到文字"));
    }

    #[test]
    fn test_handle_translate_outcome_pushes_annotation() {
        let mut state = SnipperState::new();
        let job_id = accept_job(&mut state);
        handle_job_outcome(
            &mut state,
            job_id,
            SnipperJobKind::Translate,
            MediaJobOutcome::Translated {
                items: vec![translation_item()],
            },
        );
        assert!(state.annotations.can_undo());
        assert!(state.status_message.is_none());
    }

    #[test]
    fn test_handle_saved_outcome_sets_result() {
        let mut state = SnipperState::new();
        let job_id = accept_job(&mut state);
        let path = PathBuf::from("/tmp/snapin_test.png");
        handle_job_outcome(
            &mut state,
            job_id,
            SnipperJobKind::Save,
            MediaJobOutcome::Saved { path: path.clone() },
        );
        match state.outcome {
            Some(SnipperOutcome::Saved(saved_path)) => assert_eq!(saved_path, path),
            _ => panic!("保存成功必须返回 Saved 结果"),
        }
    }

    #[test]
    fn test_handle_save_cancelled_keeps_session_open() {
        let mut state = SnipperState::new();
        let job_id = accept_job(&mut state);
        handle_job_outcome(
            &mut state,
            job_id,
            SnipperJobKind::Save,
            MediaJobOutcome::SaveCancelled,
        );
        assert!(state.outcome.is_none(), "取消保存不得结束截图会话");
        assert_eq!(state.status_message.as_deref(), Some("已取消保存"));
    }

    #[test]
    fn test_handle_job_failure_sets_status() {
        let mut state = SnipperState::new();
        let job_id = accept_job(&mut state);
        handle_job_outcome(
            &mut state,
            job_id,
            SnipperJobKind::Translate,
            MediaJobOutcome::Failed {
                message: "网络超时".to_string(),
            },
        );
        assert!(state
            .status_message
            .as_deref()
            .unwrap_or_default()
            .contains("网络超时"));
    }

    /// OCR 成功后应保留截图会话并给出结果提示，而不是立刻关闭窗口
    #[test]
    fn test_ocr_success_keeps_session_open() {
        let mut state = SnipperState::new();
        let job_id = accept_job(&mut state);
        handle_job_outcome(
            &mut state,
            job_id,
            SnipperJobKind::Ocr,
            MediaJobOutcome::Ocr {
                text: "识别出来的一段文字".to_string(),
            },
        );

        assert!(state.outcome.is_none(), "OCR 成功不得直接把会话置为结束态");
        let message = state.status_message.as_deref().unwrap_or_default();
        assert!(
            message.contains("已复制识别文字"),
            "应提示识别结果已复制，实际: {message}"
        );
    }

    /// 测试用图像必须与真实任务使用相同的 RGBA 布局
    #[test]
    fn test_media_job_accepts_rgba_image() {
        let image = image::RgbaImage::from_pixel(2, 2, Rgba([255, 0, 0, 255]));
        let job = MediaJob::Ocr { image };
        assert!(matches!(job, MediaJob::Ocr { .. }));
    }
}

#[cfg(test)]
mod stale_tests {
    use super::*;
    use crate::ports::JobId;

    /// 旧任务的迟到回调不得清空当前任务或改写会话状态。
    #[test]
    fn test_stale_job_outcome_is_ignored() {
        let mut state = SnipperState::new();
        state.pending_job = Some(SnipperJobKind::Save);
        state.pending_job_id = Some(JobId::new(2));

        handle_job_outcome(
            &mut state,
            JobId::new(1),
            SnipperJobKind::Save,
            MediaJobOutcome::Failed {
                message: "旧任务失败".to_string(),
            },
        );

        assert_eq!(state.pending_job_id, Some(JobId::new(2)));
        assert!(state.outcome.is_none());
        assert!(state.status_message.is_none());
    }
}
