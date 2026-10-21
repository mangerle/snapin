//! Snapin 应用程序入口点。
//!
//! 负责解析命令行参数，并按子命令的真实需要最小化地初始化日志、配置与平台能力，
//! 最后调度至守护进程服务或客户端 IPC 触发。

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
use snapin_core::config::{self, Config};
use snapin_daemon::{send_command_to_daemon, DaemonRequest};
use snapin_platform::{init_platform_capabilities, sync_hotkeys_to_gnome};
use snapin_settings::run_settings_gui;
use snapin_ui::snipper::SnipMode;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// 安装全局日志订阅器。
///
/// 级别解析次序为 `RUST_LOG` 环境变量 > 配置文件 `general.log_level` > `info`，
/// 使临时排障无需改动用户配置即可覆盖落盘的偏好。
fn init_tracing(config_level: &str) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(config_level));
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();
}

/// 按子命令的真实需要取出用户配置。
///
/// 设计原理：配置读盘发生在本函数之前（日志级别依赖它），但只有少数子命令真正消费配置字段。
/// 对 `snip`、`sync-hotkeys`、`daemon` 而言配置缺失即无法正确工作，必须致命报错；
/// 其余命令（如 `toggle`、`exit`）只是短生命周期的 IPC 客户端，配置文件损坏时降级为忽略并继续，
/// 避免一处 TOML 语法错误把与偏好无关的运维命令一并拖死。
fn config_for(command: &Commands, loaded: Result<Config>) -> Result<Option<Config>> {
    if command.needs_config() {
        return loaded.map(Some).context("加载或初始化 Snapin 配置文件失败");
    }

    if let Err(error) = loaded {
        tracing::warn!(
            "配置文件读取失败，子命令 [{command:?}] 不依赖用户配置，已忽略并继续，原因: {error}"
        );
    }
    Ok(None)
}

#[tokio::main]
async fn main() -> Result<()> {
    // 1. 最先解析命令行：--help 与 --version 等纯 CLI 路径应立即结束，不产生读盘与 D-Bus 副作用
    let Cli { command } = Cli::parse();

    // 2. 预读配置以还原用户的日志分级偏好，随后校验各子命令对配置的强依赖
    let loaded_config = config::load_or_create_config(None).map_err(anyhow::Error::new);
    init_tracing(
        loaded_config
            .as_ref()
            .map_or("info", |cfg| cfg.general.log_level.as_str()),
    );
    let config = config_for(&command, loaded_config)?;

    // 3. 仅截图与偏好设置需要 GNOME 特权扩展的在线状态，其余命令不再发起 D-Bus 探测往返
    if command.needs_capabilities() {
        init_platform_capabilities().await;
    }

    match command {
        Commands::Snip {
            auto_copy,
            custom,
            translate,
        } => {
            tracing::info!("正在启动区域截图流程...");
            let mode = if translate {
                SnipMode::Translate
            } else if auto_copy {
                SnipMode::AutoCopy
            } else if custom {
                SnipMode::Custom
            } else {
                SnipMode::Normal
            };
            let cfg = config.context("区域截图流程缺少用户配置")?;
            run_oneshot_snip(cfg, mode).await?;
        }
        Commands::Paste => {
            tracing::info!("正在检查剪贴板生成贴图...");
            if let Ok(true) = send_command_to_daemon(&DaemonRequest::Paste).await {
                tracing::info!("已通过轻量 IPC 委托常驻守护服务完成贴图");
            } else {
                tracing::info!("守护服务未运行，降级为本地直接执行贴图");
                run_oneshot_paste().await?;
            }
        }
        Commands::PinFile { path, x, y } => {
            run_pin_file(path, x, y).await?;
        }
        Commands::Prefs => {
            tracing::info!("正在启动 Snapin 现代化偏好设置中心...");
            run_settings_gui().context("启动设置中心图形界面失败")?;
        }
        Commands::Toggle => {
            if let Ok(true) = send_command_to_daemon(&DaemonRequest::TogglePasters).await {
                tracing::info!("已通过 IPC 请求守护进程批量切换贴图显隐状态");
            } else {
                tracing::info!("未检测到常驻守护服务，无活动贴图需要切换");
            }
        }
        Commands::SwitchGroup { id } => {
            if let Ok(true) = send_command_to_daemon(&DaemonRequest::SwitchGroup(id)).await {
                tracing::info!("已通过 IPC 请求守护进程切换贴图分组至: {id:?}");
            } else {
                tracing::info!("未检测到常驻守护服务，无活动贴图分组需要切换");
            }
        }
        Commands::ClearHistory => {
            clear_snip_history().await?;
        }
        Commands::Exit => {
            exit_snapin_application().await?;
        }
        Commands::InstallExtension => {
            install_gnome_extension()?;
        }
        Commands::SyncHotkeys => {
            let cfg = config.context("快捷键同步流程缺少用户配置")?;
            sync_hotkeys_to_gnome(&cfg.hotkeys).context("同步快捷键至 GNOME 失败")?;
            tracing::info!("成功将快捷键同步注册至 Ubuntu GNOME 系统！");
        }
        Commands::Daemon => {
            let cfg = config.context("守护进程启动缺少用户配置")?;
            run_daemon_server(cfg).await?;
        }
    }

    Ok(())
}
