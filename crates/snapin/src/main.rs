//! Snapin 应用程序入口点。
//!
//! 负责解析命令行参数并调度至守护进程服务或客户端 IPC 触发。

pub mod app;
pub mod cli;

use anyhow::{Context, Result};
use app::cleaner::clear_snip_history;
use app::daemon::run_daemon_server;
use app::installer::install_gnome_extension;
use app::lifecycle::exit_snapin_application;
use app::paste::{run_oneshot_paste, run_pin_file};
use app::snip::run_oneshot_snip;
use clap::Parser;
use cli::{Cli, Commands};
use snapin_core::config;
use snapin_daemon::{send_command_to_daemon, DaemonRequest};
use snapin_platform::{init_platform_capabilities, sync_hotkeys_to_gnome};
use snapin_settings::run_settings_gui;
use snapin_ui::snipper::SnipMode;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    // 1. 预读应用配置文件（以便依据用户偏好设置日志分级）
    let cfg_result = config::load_or_create_config(None);
    let default_level = cfg_result
        .as_ref()
        .map(|c| c.general.log_level.as_str())
        .unwrap_or("info");

    // 2. 初始化结构化日志（优先支持 RUST_LOG 环境变量覆盖，其次尊重用户配置文件的 log_level）
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(default_level));
    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer())
        .init();

    // 3. 初始化底层平台能力探测（探测 Wayland、GNOME Mutter 扩展等环境并全局缓存）
    init_platform_capabilities().await;

    let cli = Cli::parse();

    // 5. 校验配置加载结果，若配置文件损坏则附带上下文优雅报错
    let cfg = cfg_result.context("加载或初始化 Snapin 配置文件失败")?;

    match cli.command {
        Some(Commands::Snip { auto_copy, custom }) => {
            tracing::info!("正在启动区域截图流程...");
            let mode = if auto_copy {
                SnipMode::AutoCopy
            } else if custom {
                SnipMode::Custom
            } else {
                SnipMode::Normal
            };
            run_oneshot_snip(cfg, mode).await?;
        }
        Some(Commands::Paste) => {
            tracing::info!("正在检查剪贴板生成贴图...");
            if let Ok(true) = send_command_to_daemon(&DaemonRequest::Paste).await {
                tracing::info!("已通过轻量 IPC 委托常驻守护服务完成贴图");
            } else {
                tracing::info!("守护服务未运行，降级为本地直接执行贴图");
                run_oneshot_paste().await?;
            }
        }
        Some(Commands::PinFile { path, x, y }) => {
            run_pin_file(path, x, y).await?;
        }
        Some(Commands::Prefs) => {
            tracing::info!("正在启动 Snapin 现代化偏好设置中心...");
            run_settings_gui().context("启动设置中心图形界面失败")?;
        }
        Some(Commands::Toggle) => {
            if let Ok(true) = send_command_to_daemon(&DaemonRequest::TogglePasters).await {
                tracing::info!("已通过 IPC 请求守护进程批量切换贴图显隐状态");
            } else {
                tracing::info!("未检测到常驻守护服务，无活动贴图需要切换");
            }
        }
        Some(Commands::SwitchGroup { id }) => {
            if let Ok(true) = send_command_to_daemon(&DaemonRequest::SwitchGroup(id)).await {
                tracing::info!("已通过 IPC 请求守护进程切换贴图分组至: {id:?}");
            } else {
                tracing::info!("未检测到常驻守护服务，无活动贴图分组需要切换");
            }
        }
        Some(Commands::ClearHistory) => {
            clear_snip_history().await?;
        }
        Some(Commands::Exit) => {
            exit_snapin_application().await?;
        }
        Some(Commands::InstallExtension) => {
            install_gnome_extension()?;
        }
        Some(Commands::SyncHotkeys) => {
            sync_hotkeys_to_gnome(&cfg.hotkeys).context("同步快捷键至 GNOME 失败")?;
            tracing::info!("成功将快捷键同步注册至 Ubuntu GNOME 系统！");
        }
        Some(Commands::Daemon) | None => {
            run_daemon_server(cfg).await?;
        }
    }

    Ok(())
}
