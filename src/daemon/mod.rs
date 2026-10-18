//! 单实例守护进程管理与 Unix Domain Socket 进程间通信模块。

pub mod socket;

pub use socket::{get_socket_path, send_command_to_daemon, DaemonCommand, IpcListener};
