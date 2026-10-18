//! Snapin 应用程序入口点。
//!
//! 负责解析命令行参数并调度至守护进程服务或客户端 IPC 触发。

pub mod annotation;
pub mod config;
pub mod daemon;
pub mod error;
pub mod paster;
pub mod platform;
pub mod snipper;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use daemon::{send_command_to_daemon, DaemonCommand, IpcListener};
use platform::{capture_fullscreen_portal, send_desktop_notification, sync_hotkeys_to_gnome};
use std::path::PathBuf;
use tokio::sync::mpsc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// 命令行参数结构体
#[derive(Parser, Debug)]
#[command(
    name = "snapin",
    author = "wangle",
    version = "0.1.0",
    about = "专为 Linux Wayland (Ubuntu 26 GNOME) 打造的纯 Rust 截图与贴图工具"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

/// 支持的子命令定义
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// 触发区域截图流水线 (默认绑定 F1)
    Snip,
    /// 触发贴图（由剪贴板或选区生成，默认绑定 F3）
    Paste,
    /// 批量显示/隐藏当前活动组所有贴图 (默认绑定 Ctrl+F3)
    Toggle,
    /// 切换贴图活动分组
    SwitchGroup {
        /// 目标分组 ID
        id: usize,
    },
    /// 在前台启动常驻守护进程
    Daemon,
    /// 部署配套 GNOME Shell 扩展以获得置顶特权
    InstallExtension,
    /// 向系统 GNOME 快捷键中心同步绑定默认热键
    SyncHotkeys,
}

#[tokio::main]
async fn main() -> Result<()> {
    // 1. 初始化结构化日志
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new("info"))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = Cli::parse();

    // 2. 加载或初始化配置
    let cfg = config::load_or_create_config(None).context("加载或初始化 Snapin 配置文件失败")?;

    match cli.command {
        Some(Commands::Snip) => {
            // 首先尝试通过 IPC 向已运行的守护进程发送指令
            let delivered = send_command_to_daemon(&DaemonCommand::Snip)
                .await
                .context("向守护进程发送截图指令失败")?;

            if !delivered {
                tracing::info!("守护进程未在运行，正在直接执行一次性截屏流程...");
                run_oneshot_snip().await?;
            }
        }
        Some(Commands::Paste) => {
            let delivered = send_command_to_daemon(&DaemonCommand::Paste)
                .await
                .context("向守护进程发送贴图指令失败")?;

            if !delivered {
                tracing::info!("守护进程未在运行，正在检查剪贴板直接生成贴图...");
                send_desktop_notification("Snapin 提示", "未检测到常驻守护进程，已触发贴图请求")
                    .await?;
            }
        }
        Some(Commands::Toggle) => {
            let _ = send_command_to_daemon(&DaemonCommand::TogglePasters).await;
        }
        Some(Commands::SwitchGroup { id }) => {
            let _ = send_command_to_daemon(&DaemonCommand::SwitchGroup(id)).await;
        }
        Some(Commands::InstallExtension) => {
            install_gnome_extension()?;
        }
        Some(Commands::SyncHotkeys) => {
            sync_hotkeys_to_gnome(&cfg.hotkeys).context("同步快捷键至 GNOME 失败")?;
            println!("成功将快捷键同步注册至 Ubuntu GNOME 系统！");
        }
        Some(Commands::Daemon) | None => {
            run_daemon_server(cfg).await?;
        }
    }

    Ok(())
}

/// 执行单次截图流程
async fn run_oneshot_snip() -> Result<()> {
    let img = capture_fullscreen_portal()
        .await
        .context("Portal 全屏截图捕获失败")?;

    let msg = format!(
        "已成功捕获屏幕快照，物理尺寸: {}x{}",
        img.width(),
        img.height()
    );
    send_desktop_notification("Snapin 截图就绪", &msg).await?;
    Ok(())
}

/// 运行后台守护服务
async fn run_daemon_server(cfg: config::Config) -> Result<()> {
    tracing::info!("Snapin 守护服务启动中...");

    // 自动确保 GNOME 快捷键注册
    let _ = sync_hotkeys_to_gnome(&cfg.hotkeys);

    // 绑定 IPC 监听器
    let listener = IpcListener::bind(None).context("绑定 IPC 监听套接字失败")?;

    // 创建有界命令通道 (容量 32)
    let (tx, mut rx) = mpsc::channel::<DaemonCommand>(32);

    // 在后台协程中运行套接字循环
    tokio::spawn(async move {
        listener.run_loop(tx).await;
    });

    tracing::info!("Snapin 守护服务已进入就绪状态，等待热键指令...");

    // 主服务事件消费循环
    while let Some(cmd) = rx.recv().await {
        tracing::info!("守护进程接收到控制指令: {:?}", cmd);
        match cmd {
            DaemonCommand::Snip => {
                if let Err(e) = run_oneshot_snip().await {
                    tracing::error!("执行截图流程失败: {e}");
                }
            }
            DaemonCommand::Paste => {
                tracing::info!("执行贴图处理流程");
            }
            DaemonCommand::TogglePasters => {
                tracing::info!("批量切换贴图显示状态");
            }
            DaemonCommand::SwitchGroup(id) => {
                tracing::info!("切换至贴图分组: {id}");
            }
            DaemonCommand::Ping => {
                tracing::debug!("心跳响应正常");
            }
        }
    }

    Ok(())
}

/// 安装配套 GNOME Shell 扩展
fn install_gnome_extension() -> Result<()> {
    let home = std::env::var("HOME").context("未找到 HOME 环境变量")?;
    let uuid = "snapin-helper@wangle.local";
    let target_dir = PathBuf::from(home)
        .join(".local/share/gnome-shell/extensions")
        .join(uuid);

    std::fs::create_dir_all(&target_dir).context("创建 GNOME 扩展目标目录失败")?;

    let current_dir = std::env::current_dir()?;
    let ext_source_dir = current_dir.join("extension");

    let meta_file = ext_source_dir.join("metadata.json");
    let js_file = ext_source_dir.join("extension.js");

    if meta_file.exists() && js_file.exists() {
        std::fs::copy(&meta_file, target_dir.join("metadata.json"))?;
        std::fs::copy(&js_file, target_dir.join("extension.js"))?;
        println!("GNOME 扩展文件已部署至: {target_dir:?}");

        // 尝试激活扩展
        let _ = std::process::Command::new("gnome-extensions")
            .args(["enable", uuid])
            .output();

        println!("已向 GNOME 发送扩展启用指令。");
    } else {
        println!("未在当前目录找到 extension/ 源码文件！");
    }

    Ok(())
}
