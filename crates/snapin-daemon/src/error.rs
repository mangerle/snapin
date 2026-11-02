//! Snapin 守护进程与 IPC 进程间通信领域错误模型。
//!
//! 封装 Unix Domain Socket 套接字创建、监听、连接超时与强类型分帧编解码异常。

use crate::protocol::DaemonErrorCode;
use thiserror::Error;

/// 守护进程与 IPC 领域错误枚举
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum DaemonError {
    /// Unix Domain Socket 套接字通信或绑定异常
    #[error("单实例 IPC 套接字通信异常，套接字路径: {socket_path}，原因: {message}")]
    Socket {
        socket_path: String,
        message: String,
    },

    /// 进程间通信分帧或协议格式异常
    #[error("IPC 通信协议或帧格式异常: {0}")]
    Protocol(String),

    /// IPC 分帧读写过程中的套接字 I/O 异常。
    ///
    /// `stage` 显式标注失败环节（读取长度前缀、写入响应帧等）。同一条 socket 上的失败
    /// 在原先的 `#[from] io::Error` 笼统转换下无法区分现场，改为阶段化描述后可直接在日志里定位。
    #[error("IPC 套接字{stage}失败，原因: {source}")]
    Io {
        stage: &'static str,
        #[source]
        source: std::io::Error,
    },

    /// 消息载荷序列化/反序列化失败
    ///
    /// 不把第三方 `serde_json::Error` 直接挂在公共枚举上，跨边界时只保留阶段
    /// 与中文描述，避免依赖升级改变对外错误契约。
    #[error("IPC 消息 JSON 编解码失败，阶段: {stage}，原因: {message}")]
    Json {
        stage: &'static str,
        message: String,
    },

    /// 守护进程业务处理返回失败
    #[error("守护进程处理指令失败，类别: {code:?}，原因: {message}")]
    Business {
        /// 强类型业务失败类别
        code: DaemonErrorCode,
        /// 中文失败原因
        message: String,
    },
}
