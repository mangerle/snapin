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
use paster::{
    render_color_swatch_card, render_simple_text_card, spawn_paster_window, try_parse_hex_color,
    PasterItem,
};
use platform::{
    capture_fullscreen_portal, read_clipboard_image, read_clipboard_text,
    send_desktop_notification, sync_hotkeys_to_gnome,
};
use snipper::{run_snipper_session, SnipperOutcome};
use std::path::PathBuf;
use tiny_skia::Pixmap;
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
            tracing::info!("正在启动区域截图流程...");
            run_oneshot_snip().await?;
        }
        Some(Commands::Paste) => {
            let delivered = send_command_to_daemon(&DaemonCommand::Paste)
                .await
                .context("向守护进程发送贴图指令失败")?;

            if !delivered {
                tracing::info!("守护进程未在运行，正在直接检查剪贴板生成贴图...");
                run_oneshot_paste().await?;
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
    tracing::info!("正在通过 Portal 捕获当前屏幕快照...");
    let img = capture_fullscreen_portal()
        .await
        .context("Portal 全屏截图捕获失败")?;

    tracing::info!(
        "快照尺寸: {}x{}，正在启动交互窗口...",
        img.width(),
        img.height()
    );

    let outcome = tokio::task::spawn_blocking(move || run_snipper_session(img.to_rgba8()))
        .await
        .context("截图任务调度异常")?
        .context("截图交互会话运行失败")?;

    if let Some(outcome) = outcome {
        match outcome {
            SnipperOutcome::Copied(_) => {
                send_desktop_notification("Snapin 截图", "选区图像已成功复制到剪贴板").await?;
            }
            SnipperOutcome::Saved(path) => {
                let msg = format!("选区图像已保存至: {}", path.display());
                send_desktop_notification("Snapin 截图已保存", &msg).await?;
            }
            SnipperOutcome::Pinned(item) => {
                if let Err(e) = spawn_paster_window(item) {
                    tracing::error!("贴图窗口运行异常: {e}");
                }
            }
            SnipperOutcome::Cancelled => {
                tracing::info!("用户已取消本次截图");
            }
        }
    }

    Ok(())
}

/// 检查系统剪贴板并就地生成贴图
async fn run_oneshot_paste() -> Result<()> {
    tracing::info!("正在检查剪贴板内容以生成贴图...");

    if let Ok(Some(img)) = read_clipboard_image() {
        if let Some(mut pixmap) = Pixmap::new(img.width(), img.height()) {
            pixmap.data_mut().copy_from_slice(img.as_raw());
            let stamp = now_timestamp();
            let item = PasterItem::new(stamp, format!("clip_img_{stamp}"), pixmap, (200, 200));
            std::thread::spawn(move || {
                let _ = spawn_paster_window(item);
            });
            return Ok(());
        }
    }

    if let Ok(Some(text)) = read_clipboard_text() {
        let stamp = now_timestamp();
        let pixmap = if let Some((r, g, b)) = try_parse_hex_color(&text) {
            render_color_swatch_card(r, g, b).context("生成色彩卡片失败")?
        } else {
            let lines: Vec<&str> = text.lines().collect();
            render_simple_text_card(&lines).context("生成文本便签卡片失败")?
        };

        let item = PasterItem::new(stamp, format!("clip_note_{stamp}"), pixmap, (250, 250));
        std::thread::spawn(move || {
            let _ = spawn_paster_window(item);
        });
        return Ok(());
    }

    send_desktop_notification("Snapin 贴图提示", "当前剪贴板为空，无法生成贴图").await?;
    Ok(())
}

/// 获取当前秒级时间戳
fn now_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 运行后台守护服务
async fn run_daemon_server(cfg: config::Config) -> Result<()> {
    tracing::info!("Snapin 守护服务启动中...");

    // 自动确保 GNOME 快捷键注册与配套扩展激活
    let _ = sync_hotkeys_to_gnome(&cfg.hotkeys);
    let _ = std::process::Command::new("gnome-extensions")
        .args(["enable", "snapin-helper@wangle.local"])
        .output();

    let listener = IpcListener::bind(None)
        .await
        .context("绑定 IPC 监听套接字失败")?;
    let (tx, mut rx) = mpsc::channel::<DaemonCommand>(32);

    tokio::spawn(async move {
        listener.run_loop(tx).await;
    });

    tracing::info!("Snapin 守护服务已就绪，等待热键指令...");
    let _ =
        send_desktop_notification("Snapin 截图贴图", "服务已就绪：按 F1 截图，按 F3 贴图").await;

    while let Some(cmd) = rx.recv().await {
        tracing::info!("守护进程接收到控制指令: {:?}", cmd);
        match cmd {
            DaemonCommand::Snip => {
                let current_exe =
                    std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("snapin"));
                if let Err(e) = std::process::Command::new(current_exe).arg("snip").spawn() {
                    tracing::error!("调起截图前台子进程失败: {e}");
                }
            }
            DaemonCommand::Paste => {
                if let Err(e) = run_oneshot_paste().await {
                    tracing::error!("执行贴图流程失败: {e}");
                }
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
