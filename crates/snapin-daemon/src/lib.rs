//! 单实例守护进程管理与 Unix Domain Socket 进程间通信模块。

pub mod socket;

pub use socket::{
    get_socket_path, read_framed_message, send_command_to_daemon, send_request_to_daemon,
    write_framed_message, DaemonCommand, DaemonRequest, DaemonResponse, IpcInvocation, IpcListener,
};
