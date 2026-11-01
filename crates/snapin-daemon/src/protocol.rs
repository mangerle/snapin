//! Unix Domain Socket 守护进程请求与响应协议模型。

use serde::{Deserialize, Serialize};
use snapin_core::SnipMode;
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
        active_group: usize,
        paster_count: usize,
    },
    /// 操作执行异常反馈
    Error(String),
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
    snapin_core::AppPaths::default_socket_path()
}
