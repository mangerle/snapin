//! Snapin 应用程序入口点。
//!
//! 所有 GUI 会话统一由常驻守护进程的主线程事件循环托管；命令行进程只负责解析参数、
//! 确保守护进程在线并投递强类型 IPC 指令。设置中心作为独立 GUI 进程直接在主线程运行，
//! 避免任何进程内创建第二个 winit 事件循环。

// 在 Windows 发布构建中隐藏控制台黑框窗口，在 Debug 模式下保留以供输出日志调试。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

pub mod cli;

use anyhow::{bail, Context, Result};
use clap::Parser;
use cli::{Cli, Commands};
use snapin_app::{
    clear_snip_history, ensure_daemon_running, exit_snapin_application, install_gnome_extension,
    run_cli_update, run_daemon_process, run_pin_file,
};
use snapin_core::config::{self, Config};
use snapin_core::SnipMode;
use snapin_daemon::{send_command_to_daemon, DaemonRequest};
use snapin_platform::{init_platform_capabilities, sync_hotkeys_to_gnome};
use snapin_settings::run_settings_gui;
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
/// 守护进程与快捷键同步强依赖配置；其余命令是短生命周期 IPC 客户端或不消费偏好字段，
/// 配置文件损坏时降级为忽略并继续，避免一处 TOML 语法错误把无关运维命令一并拖死。
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

/// 构建命令行进程使用的短生命周期 Tokio 运行时。
fn cli_runtime() -> Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("snapin-cli")
        .build()
        .context("创建命令行异步运行时失败")
}

/// 解析无子命令启动意图：守护进程在线时打开设置中心，否则启动常驻服务。
fn resolve_bare_invocation() -> Result<Commands> {
    let runtime = cli_runtime()?;
    let online = matches!(
        runtime.block_on(send_command_to_daemon(&DaemonRequest::Ping)),
        Ok(true)
    );
    runtime.shutdown_timeout(std::time::Duration::from_secs(1));
    Ok(if online {
        Commands::Prefs
    } else {
        Commands::Daemon
    })
}

/// 执行不依赖 GUI 主线程的 IPC 与运维命令。
fn run_async_command(
    runtime: &tokio::runtime::Runtime,
    command: Commands,
    config: Option<Config>,
) -> Result<()> {
    runtime.block_on(async move {
        match command {
            Commands::Snip {
                auto_copy,
                custom,
                translate,
                fullscreen,
                delay,
            } => {
                tracing::info!("正在请求常驻守护服务启动截图覆盖层...");
                let mode = if fullscreen {
                    SnipMode::Fullscreen
                } else if translate {
                    SnipMode::Translate
                } else if auto_copy {
                    SnipMode::AutoCopy
                } else if custom {
                    SnipMode::Custom
                } else {
                    SnipMode::Normal
                };
                ensure_daemon_running().await?;
                send_or_fail(DaemonRequest::StartSnip {
                    mode,
                    delay_seconds: delay,
                })
                .await
            }
            Commands::PickColor => {
                tracing::info!("正在请求常驻守护服务启动取色覆盖层...");
                ensure_daemon_running().await?;
                send_or_fail(DaemonRequest::StartSnip {
                    mode: SnipMode::PickColor,
                    delay_seconds: 0,
                })
                .await
            }
            Commands::Paste => {
                tracing::info!("正在请求常驻守护服务检查剪贴板生成贴图...");
                ensure_daemon_running().await?;
                send_or_fail(DaemonRequest::Paste).await
            }
            Commands::PinFile { path, x, y } => run_pin_file(path, x, y).await,
            Commands::Toggle => {
                if !send_or_absent(DaemonRequest::TogglePasters).await? {
                    tracing::info!("未检测到常驻守护服务，无活动贴图需要切换");
                }
                Ok(())
            }
            Commands::SwitchGroup { id } => {
                if let Some(group) = id {
                    if !snapin_ui::paster::is_valid_group_id(group) {
                        bail!(
                            "贴图分组 ID 越界: {group}，有效范围: 0..={}",
                            snapin_ui::paster::TOTAL_PIN_GROUPS.saturating_sub(1)
                        );
                    }
                }
                if !send_or_absent(DaemonRequest::SwitchGroup(id)).await? {
                    tracing::info!("未检测到常驻守护服务，无活动贴图分组需要切换");
                }
                Ok(())
            }
            Commands::ClearHistory => {
                if !send_or_absent(DaemonRequest::ClearHistory).await? {
                    clear_snip_history().await?;
                }
                Ok(())
            }
            Commands::Exit => exit_snapin_application().await,
            Commands::Update { check } => run_cli_update(config, check).await,
            Commands::Prefs
            | Commands::Daemon
            | Commands::InstallExtension
            | Commands::SyncHotkeys => {
                bail!("子命令必须由主线程同步流程处理，收到了错误的调度路径")
            }
        }
    })
}

/// 发送请求并要求守护进程明确成功。
async fn send_or_fail(request: DaemonRequest) -> Result<()> {
    match send_command_to_daemon(&request).await {
        Ok(true) => {
            tracing::info!("已通过 IPC 委托常驻守护服务处理请求");
            Ok(())
        }
        Ok(false) => bail!("常驻守护服务未响应请求"),
        Err(error) => Err(error).context("常驻守护服务处理请求失败"),
    }
}

/// 发送请求；守护进程缺席时返回 `false` 而不是错误。
async fn send_or_absent(request: DaemonRequest) -> Result<bool> {
    match send_command_to_daemon(&request).await {
        Ok(handled) => Ok(handled),
        Err(error) => Err(error).context("常驻守护服务处理请求失败"),
    }
}

/// 在独立设置中心进程内初始化平台能力探测。
fn run_preferences() -> Result<()> {
    let runtime = cli_runtime()?;
    runtime.block_on(init_platform_capabilities());
    runtime.shutdown_timeout(std::time::Duration::from_secs(1));
    tracing::info!("正在启动 Snapin 现代化偏好设置中心...");
    run_settings_gui().context("启动设置中心图形界面失败")
}

fn main() -> Result<()> {
    // 1. 最先解析命令行：--help 与 --version 等纯 CLI 路径立即结束，不产生副作用
    let Cli { command } = Cli::parse();
    let command = match command {
        Some(command) => command,
        None => resolve_bare_invocation()?,
    };

    // 2. 预读配置以还原日志级别，并校验强依赖配置的子命令
    let loaded_config = config::load_or_create_config(None).map_err(anyhow::Error::new);
    init_tracing(
        loaded_config
            .as_ref()
            .map_or("info", |cfg| cfg.general.log_level.as_str()),
    );
    let config = config_for(&command, loaded_config)?;

    // 3. 自更新崩溃自愈必须在进入 GUI/服务主流程之前完成
    if let Err(error) = snapin_updater::check_and_recover_on_startup() {
        tracing::warn!("自更新启动自检失败，继续启动: {error}");
    }

    match command {
        Commands::Daemon => {
            let config = config.context("守护进程启动缺少用户配置")?;
            run_daemon_process(config)
        }
        Commands::Prefs => run_preferences(),
        Commands::InstallExtension => install_gnome_extension(),
        Commands::SyncHotkeys => {
            let config = config.context("快捷键同步流程缺少用户配置")?;
            sync_hotkeys_to_gnome(&config.hotkeys).context("同步快捷键至 GNOME 失败")?;
            tracing::info!("成功将快捷键同步注册至 Ubuntu GNOME 系统！");
            Ok(())
        }
        other => {
            let runtime = cli_runtime()?;
            run_async_command(&runtime, other, config)
        }
    }
}
