//! 截图会话媒体任务调度与结果回传。
//!
//! # 设计原理
//!
//! 截图窗口只负责生成“要处理哪一块图像”以及“结果如何呈现”，真正的 OCR、翻译、
//! 保存与剪贴板写入由应用层通过 [`MediaJobRunner`] 与
//! [`DesktopPort`](crate::ports::DesktopPort) 完成。
//! 任务完成后经 `EventLoopProxy` 以用户事件回传窗口线程，避免在事件循环中执行
//! 阻塞推理或网络请求。
//!
//! 任务取消使用端口返回的不透明取消闭包，不把 Tokio 的 `AbortHandle` 泄漏到表现层，
//! 因此 UI 可以在没有异步运行时的单元测试环境中独立验证状态流转。

use crate::ports::{JobCancel, JobId, MediaJob, MediaJobOutcome, MediaJobRunner, PortError};
use std::sync::Arc;

/// 截图会话支持的后台任务类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnipperJobKind {
    Ocr,
    Redact,
    Qr,
    Translate,
    Save,
    Export,
    Beautify,
}

impl SnipperJobKind {
    /// 中文任务名称，用于界面提示
    pub fn label(self) -> &'static str {
        match self {
            Self::Ocr => "文字识别",
            Self::Redact => "智能打码",
            Self::Qr => "二维码识别",
            Self::Translate => "原位翻译",
            Self::Save => "保存截图",
            Self::Export => "导出 OCR 结果",
            Self::Beautify => "美化导出",
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

mod outcome;
pub use outcome::handle_job_outcome;
