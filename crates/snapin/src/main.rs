//! Snapin 应用程序入口点。
//!
//! 所有 GUI 会话统一由常驻守护进程的主线程事件循环托管；命令行进程只负责解析参数、
//! 确保守护进程在线并投递强类型 IPC 指令。设置中心作为独立 GUI 进程直接在主线程运行，
//! 避免任何进程内创建第二个 winit 事件循环。

// 在 Windows 发布构建中隐藏控制台黑框窗口，在 Debug 模式下保留以供输出日志调试。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

pub mod cli;
mod instance_lock;

use anyhow::{Context, Result};
use clap::Parser;
use cli::{Cli, CommandRoute, Commands};
use instance_lock::InstanceLock;
use snapin_app::{
    initialize_platform_capabilities, install_gnome_extension, is_daemon_online,
    run_application_command, run_daemon_process, sync_system_hotkeys,
};
use snapin_config::config::{self, Config};
use snapin_settings::{run_clipboard_history_panel, run_settings_gui};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// 安装全局日志订阅器。
///
/// 级别解析次序为 `RUST_LOG` 环境变量 > 配置文件 `general.log_level` > `info`，
/// 使临时排障无需改动用户配置即可覆盖落盘的偏好。
fn init_tracing(config_level: &str) {
    // arboard 在 Wayland 合成器不支持 data-control 时会主动降级到 X11，
    // 这是受支持的兼容路径而非应用故障；仅保留其 error 级别，避免正常回退污染告警日志。
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("{config_level},arboard=error")));
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
    let online = runtime.block_on(is_daemon_online());
    runtime.shutdown_timeout(std::time::Duration::from_secs(1));
    Ok(if online {
        Commands::Prefs
    } else {
        Commands::Daemon
    })
}

/// 将异步子命令交给应用层执行。
fn run_application(
    runtime: &tokio::runtime::Runtime,
    command: Commands,
    config: Option<Config>,
) -> Result<()> {
    let operation = command
        .into_application_command()
        .context("异步子命令收到了错误的调度路径")?;
    runtime.block_on(run_application_command(operation, config))
}

/// 在独立设置中心进程内初始化平台能力探测。
fn run_preferences() -> Result<()> {
    let runtime = cli_runtime()?;
    runtime.block_on(initialize_platform_capabilities());
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

    // 3. 自更新健康自检只在长驻守护进程入口执行。
    //
    // `snapin snip/paste/pin-file` 是热键高频拉起的短命令，进程正常退出也不会确认
    // 升级成功；若在 CLI 入口累加未确认启动计数，更新后的正常使用会被误判为连续崩溃
    // 并回滚刚安装的健康版本。守护进程启动平稳后会调用确认接口闭环。
    match command.route() {
        CommandRoute::Daemon => {
            let config = config.context("守护进程启动缺少用户配置")?;
            run_daemon_process(config)
        }
        CommandRoute::Preferences => {
            let Some(lock) = InstanceLock::try_acquire("settings")? else {
                tracing::info!("Snapin 设置中心已在运行，忽略重复启动");
                return Ok(());
            };
            tracing::debug!("已获取设置中心单实例锁: {}", lock.path().display());
            run_preferences()
        }
        CommandRoute::ClipboardPanel => {
            let Some(lock) = InstanceLock::try_acquire("clipboard-panel")? else {
                tracing::info!("Snapin 剪贴板历史面板已在运行，忽略重复启动");
                return Ok(());
            };
            tracing::debug!("已获取剪贴板历史面板单实例锁: {}", lock.path().display());
            run_clipboard_history_panel().context("启动剪贴板历史面板失败")
        }
        CommandRoute::InstallExtension => install_gnome_extension(),
        CommandRoute::SyncHotkeys => {
            let config = config.context("快捷键同步流程缺少用户配置")?;
            sync_system_hotkeys(&config)?;
            tracing::info!("成功将快捷键同步注册至 Ubuntu GNOME 系统！");
            Ok(())
        }
        CommandRoute::Async => {
            let runtime = cli_runtime()?;
            run_application(&runtime, command, config)
        }
    }
}
