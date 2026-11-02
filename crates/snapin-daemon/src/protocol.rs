//! Unix Domain Socket 守护进程请求与响应协议模型。

use serde::{Deserialize, Serialize};
use snapin_foundation::{PostCaptureAction, SnipMode};
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::oneshot;

/// 最大允许的 IPC 数据包尺寸（1MB，满足指令、状态快照与安全物理路径传输，防范异常内存滥用）
pub const MAX_IPC_FRAME_SIZE: usize = 1024 * 1024;

/// IPC 通信读写超时阈值（2 秒，避免套接字挂起导致的协程泄漏）
pub const IPC_TIMEOUT: Duration = Duration::from_secs(2);

/// IPC 支持的守护进程请求协议
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DaemonRequest {
    /// 重新加载配置文件并热更新快捷键与运行时参数
    ReloadConfig,
    /// 触发贴图（由剪贴板生成）
    Paste,
    /// 在常驻 GUI 事件循环中打开截图/取色覆盖层
    StartSnip {
        /// 触发模式
        mode: SnipMode,
        /// 截图前延时秒数
        delay_seconds: u32,
        /// 首次锁定选区后自动执行的动作
        #[serde(default)]
        post_action: Option<PostCaptureAction>,
    },
    /// 客户端直接向常驻守护进程投递像素数据创建贴图窗口（通过内存文件系统 Raw 路径零膨胀传输）
    CreatePaster {
        width: u32,
        height: u32,
        x: i32,
        y: i32,
        title: String,
        raw_path: String,
    },
    /// 批量隐藏或显示当前活动组所有贴图
    TogglePasters,
    /// 切换活动贴图分组（若为 None 则循环切换至下一个分组）
    SwitchGroup(Option<usize>),
    /// 显示当前活动分组全部贴图
    OpenWorkspace,
    /// 清理截屏历史临时文件与选区缓存
    ClearHistory,
    /// 请求守护进程优雅平滑停机
    Shutdown,
    /// 心跳与存活检测
    Ping,
    /// 查询守护进程当前运行状态
    GetStatus,
}

/// 守护进程向 IPC 客户端返回的强类型响应协议
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DaemonResponse {
    /// 操作成功完成
    Ok,
    /// 心跳存活确认
    Pong,
    /// 状态查询结果快照
    Status {
        #[serde(default)]
        active_group: usize,
        #[serde(default)]
        paster_count: usize,
    },
    /// 操作执行异常反馈
    Error(DaemonFailure),
}

/// IPC 业务失败类别。
///
/// 客户端可能据此决定重试、提示用户修正输入或直接降级；`message` 只承担展示
/// 与排障职责，不再承担程序化分支语义。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum DaemonErrorCode {
    /// 未分类的服务端异常
    Internal,
    /// 工作区或执行器暂时不可用
    Unavailable,
    /// 操作等待超时
    Timeout,
    /// 请求参数或状态非法
    InvalidInput,
    /// 当前版本尚未支持的能力
    Unsupported,
    /// 本地持久化或文件操作失败
    Io,
    /// 并发资源已满
    Busy,
    /// 旧版客户端收到无法识别的新错误类别
    #[serde(other)]
    Unknown,
}

/// IPC 业务失败载荷。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DaemonFailure {
    /// 强类型错误类别
    pub code: DaemonErrorCode,
    /// 可直接展示的中文错误描述
    pub message: String,
}

impl DaemonResponse {
    /// 构造带明确类别的业务失败响应。
    pub fn error(code: DaemonErrorCode, message: impl Into<String>) -> Self {
        Self::Error(DaemonFailure {
            code,
            message: message.into(),
        })
    }
}

/// IPC 调度通道载荷：包含请求、响应回执与客户端超时取消信号。
///
/// 连接处理器在等待业务响应超过 [`IPC_TIMEOUT`] 后会触发 `cancelled`，
/// 业务执行方据此在下一个阶段边界停止工作，避免客户端已经收到超时错误、
/// 服务端却仍然执行非幂等副作用。
pub struct IpcInvocation {
    pub request: DaemonRequest,
    pub responder: oneshot::Sender<DaemonResponse>,
    pub cancelled: tokio_util::sync::CancellationToken,
}

/// 获取本用户的默认 IPC Socket 路径（由 AppPaths 统一纳管）
pub fn get_socket_path() -> PathBuf {
    snapin_foundation::AppPaths::default_socket_path()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 错误类别必须独立于展示文案完成序列化往返。
    #[test]
    fn test_daemon_error_roundtrip_preserves_machine_code() {
        let response = DaemonResponse::error(DaemonErrorCode::Timeout, "业务处理超时");
        let encoded = serde_json::to_string(&response).expect("错误响应应可序列化");
        let decoded: DaemonResponse = serde_json::from_str(&encoded).expect("错误响应应可反序列化");
        assert_eq!(decoded, response);
    }

    /// 新版本新增错误类别时，旧客户端必须落为 Unknown 而不是整帧反序列化失败。
    #[test]
    fn test_unknown_error_code_falls_back_to_unknown() {
        let code: DaemonErrorCode =
            serde_json::from_str("\"future_error\"").expect("未知类别应可兜底");
        assert_eq!(code, DaemonErrorCode::Unknown);
    }
}
