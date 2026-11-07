//! Snapin 应用层：截图、贴图、守护进程与系统集成的用例编排。
//!
//! 本 crate 位于表现层（`snapin-ui`）与基础设施层（`snapin-platform`、
//! `snapin-service`）之间，负责读取配置、创建后台执行器、装配桌面端口，
//! 并驱动截图/贴图窗口生命周期。二进制入口只保留命令行解析与装配。
//!
//! 依赖方向约束：
//! - `snapin-ui` 不再依赖 platform/service，只通过 `snapin_ui::ports` 请求宿主能力；
//! - 平台与服务错误在端口或任务结果边界转换为可展示描述；
//! - 应用层不直接渲染任何像素，窗口呈现仍由 `snapin-ui` 负责。

mod cleaner;
mod cli;
mod clipboard_history;
mod config;
mod daemon;
mod daemon_client;
mod desktop;
mod installer;
mod lifecycle;
mod media;
mod paste;
mod placement;
mod recording;
mod services;
mod snip;
mod update;
mod workspace_state;

pub use cleaner::clear_snip_history;
pub use cli::{
    initialize_platform_capabilities, is_daemon_online, run_application_command,
    sync_system_hotkeys, ApplicationCommand, SnipOptions,
};
pub use daemon::run_daemon_process;
pub use daemon_client::ensure_daemon_running;
pub use installer::install_gnome_extension;
pub use lifecycle::exit_snapin_application;
pub use paste::run_pin_file;
pub use update::run_cli_update;
