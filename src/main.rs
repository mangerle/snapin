//! Snapin 应用程序入口点。
//!
//! 负责解析命令行参数并调度至守护进程服务或客户端 IPC 触发。

pub mod annotation;
pub mod config;
pub mod daemon;
pub mod error;
pub mod paster;
pub mod platform;
pub mod settings;
pub mod snipper;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use daemon::{send_command_to_daemon, DaemonCommand, IpcListener};
use paster::{
    render_color_swatch_card, render_simple_text_card, spawn_paster_window, try_parse_hex_color,
    PasterItem,
};
use platform::{
    capture_fullscreen_extension, capture_fullscreen_portal, exit_snapin_application,
    read_clipboard_image, read_clipboard_text, send_desktop_notification, sync_hotkeys_to_gnome,
};
use snipper::{run_snipper_session, SnipMode, SnipperOutcome};
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
    Snip {
        /// 截图并自动复制到剪贴板
        #[arg(long)]
        auto_copy: bool,
        /// 自定义截图（加载历史选区）
        #[arg(long)]
        custom: bool,
    },
    /// 触发贴图（由剪贴板或选区生成，默认绑定 F3）
    Paste,
    /// 批量显示/隐藏当前活动组所有贴图 (默认绑定 Ctrl+F3)
    Toggle,
    /// 切换贴图活动分组
    SwitchGroup {
        /// 目标分组 ID
        #[arg(default_value_t = 0)]
        id: usize,
    },
    /// 清空截屏与临时缓存历史
    ClearHistory,
    /// 完全退出 Snapin（停止守护服务、关闭贴图并收起托盘图标）
    Exit,
    /// 在前台启动常驻守护进程
    Daemon,
    /// 独立启动一个置顶贴图窗口展示指定图像文件
    PinFile {
        /// 贴图图像路径
        path: PathBuf,
        /// 初始 X 坐标
        #[arg(default_value_t = 200)]
        x: i32,
        /// 初始 Y 坐标
        #[arg(default_value_t = 200)]
        y: i32,
    },
    /// 打开 Snapin 首选项 / 设置页面
    Prefs,
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
            run_oneshot_paste().await?;
        }
        Some(Commands::PinFile { path, x, y }) => {
            run_pin_file(path, x, y)?;
        }
        Some(Commands::Prefs) => {
            open_preferences().await?;
        }
        Some(Commands::Toggle) => {
            let _ = send_command_to_daemon(&DaemonCommand::TogglePasters).await;
            let _ = std::process::Command::new("pkill")
                .args(["-USR1", "-f", "snapin pin-file"])
                .output();
        }
        Some(Commands::SwitchGroup { id }) => {
            let _ = send_command_to_daemon(&DaemonCommand::SwitchGroup(id)).await;
            let msg = format!("已切换至贴图分组: {id}");
            let _ = send_desktop_notification("Snapin 贴图组", &msg).await;
        }
        Some(Commands::ClearHistory) => {
            clear_snip_history().await?;
        }
        Some(Commands::Exit) => {
            exit_snapin_application();
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

/// 获取屏幕快照（优先扩展零白闪通道，自动优雅降级 Portal）
async fn acquire_screen_snapshot() -> Result<image::RgbaImage> {
    let t_start = std::time::Instant::now();
    let stamp = now_timestamp();
    let shm_dir = std::path::Path::new("/dev/shm");
    let target_dir = if shm_dir.exists() {
        shm_dir
    } else {
        std::path::Path::new("/tmp")
    };
    let target_path = target_dir.join(format!("snapin_screen_{stamp}.png"));
    let target_str = target_path.to_string_lossy().to_string();

    let t_dbus_start = std::time::Instant::now();
    if let Ok(true) = capture_fullscreen_extension(&target_str).await {
        let t_dbus_end = std::time::Instant::now();
        tracing::info!(
            "命中 GNOME 扩展原生零白闪截屏通道，D-Bus 耗时: {:.2}ms",
            (t_dbus_end - t_dbus_start).as_secs_f64() * 1000.0
        );

        let t_decode_start = std::time::Instant::now();
        if let Ok(bytes) = std::fs::read(&target_path) {
            let _ = std::fs::remove_file(&target_path);
            if let Ok(dynamic_img) = image::load_from_memory(&bytes) {
                let rgba = dynamic_img.into_rgba8();
                let t_decode_end = std::time::Instant::now();
                tracing::info!(
                    "PNG 读取并内存解码耗时: {:.2}ms, 原生全屏截屏总耗时: {:.2}ms",
                    (t_decode_end - t_decode_start).as_secs_f64() * 1000.0,
                    (t_decode_end - t_start).as_secs_f64() * 1000.0
                );
                return Ok(rgba);
            }
        }
    }

    tracing::info!(
        "当前走 XDG Desktop Portal 通道 (注销重新登录桌面后将自动激活 GNOME 原生零白闪通道)"
    );
    let rgba = capture_fullscreen_portal()
        .await
        .context("Portal 全屏截图捕获失败")?;
    Ok(rgba)
}

/// 执行单次截图流程
async fn run_oneshot_snip(cfg: config::Config, mode: SnipMode) -> Result<()> {
    let t0 = std::time::Instant::now();
    let img = acquire_screen_snapshot().await?;
    let t1 = std::time::Instant::now();
    tracing::info!(
        "快照准备就绪: {}x{}，耗时: {:.2}ms，正在瞬间呈现交互窗口...",
        img.width(),
        img.height(),
        (t1 - t0).as_secs_f64() * 1000.0
    );

    let cfg_clone = cfg.clone();
    let outcome = tokio::task::spawn_blocking(move || run_snipper_session(img, cfg_clone, mode))
        .await
        .context("截图任务调度异常")?
        .context("截图交互会话运行失败")?;

    if let Some(outcome) = outcome {
        match outcome {
            SnipperOutcome::Copied(_) => {
                if cfg.output.notify_on_copy {
                    send_desktop_notification("Snapin 截图", "选区图像已成功复制到剪贴板").await?;
                }
            }
            SnipperOutcome::Saved(path) => {
                if cfg.output.notify_on_save {
                    let msg = format!("选区图像已保存至: {}", path.display());
                    send_desktop_notification("Snapin 截图已保存", &msg).await?;
                }
            }
            SnipperOutcome::Pinned(item) => {
                let stamp = now_timestamp();
                let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
                    .map(PathBuf::from)
                    .unwrap_or_else(|_| std::env::temp_dir());
                let pin_path = runtime_dir.join(format!("snapin_pin_{stamp}.png"));
                let (px, py) = item.position;

                if let Some(img) = image::RgbaImage::from_raw(
                    item.pixmap.width(),
                    item.pixmap.height(),
                    item.pixmap.data().to_vec(),
                ) {
                    if img.save(&pin_path).is_ok() {
                        let current_exe =
                            std::env::current_exe().unwrap_or_else(|_| PathBuf::from("snapin"));
                        let _ = std::process::Command::new(current_exe)
                            .args([
                                "pin-file",
                                &pin_path.to_string_lossy(),
                                &px.to_string(),
                                &py.to_string(),
                            ])
                            .spawn();
                    }
                }
            }
            SnipperOutcome::Cancelled => {
                tracing::info!("用户已取消本次截图");
            }
        }
    }

    Ok(())
}

/// 独立启动贴图并挂载置顶窗口
fn run_pin_file(path: PathBuf, x: i32, y: i32) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }

    let img = image::open(&path).context("无法读取贴图临时文件")?;
    let rgba = img.to_rgba8();
    let mut pixmap = Pixmap::new(rgba.width(), rgba.height())
        .ok_or_else(|| anyhow::anyhow!("分配贴图 Pixmap 失败"))?;
    pixmap.data_mut().copy_from_slice(rgba.as_raw());

    let stamp = now_timestamp();
    let item = PasterItem::new(stamp, format!("pin_{stamp}"), pixmap, (x, y));

    // 运行贴图窗口直至用户关闭
    let res = spawn_paster_window(item);

    // 窗口退出后自动清理临时文件
    let _ = std::fs::remove_file(&path);

    res.context("贴图窗口运行异常")
}

/// 打开 Snapin 现代化图形首选项与设置中心
async fn open_preferences() -> Result<()> {
    tracing::info!("正在启动 Snapin 现代化偏好设置中心...");
    settings::run_settings_gui().context("启动设置中心图形界面失败")?;
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
            spawn_paster_window(item).context("贴图窗口运行异常")?;
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
        spawn_paster_window(item).context("贴图窗口运行异常")?;
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

    // 自动确保 GNOME 快捷键注册与配套扩展原地热重载
    let _ = sync_hotkeys_to_gnome(&cfg.hotkeys);
    let _ = std::process::Command::new("gnome-extensions")
        .args(["disable", "snapin-helper@wangle.local"])
        .output();
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
    if cfg.general.notify_on_startup {
        let _ = send_desktop_notification(
            "Snapin 截图贴图",
            "服务已就绪：按 Ctrl+Alt+A 截图，按 F1 贴图",
        )
        .await;
    }

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
                let _ = std::process::Command::new("pkill")
                    .args(["-USR1", "-f", "snapin pin-file"])
                    .output();
            }
            DaemonCommand::SwitchGroup(id) => {
                tracing::info!("切换至贴图分组: {id}");
                let msg = format!("已切换至贴图分组: {id}");
                let _ = send_desktop_notification("Snapin 贴图组", &msg).await;
            }
            DaemonCommand::Ping => {
                tracing::debug!("心跳响应正常");
            }
        }
    }

    Ok(())
}

/// 清理所有截屏共享内存与历史选区缓存
async fn clear_snip_history() -> Result<()> {
    let mut count = 0;
    if let Ok(entries) = std::fs::read_dir("/dev/shm") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("snapin_screen_") {
                let _ = std::fs::remove_file(entry.path());
                count += 1;
            }
        }
    }
    let tmp_dir = std::env::temp_dir();
    if let Ok(entries) = std::fs::read_dir(&tmp_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("snapin_") {
                let _ = std::fs::remove_file(entry.path());
                count += 1;
            }
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        let cache_file = PathBuf::from(home).join(".cache/snapin/last_selection.txt");
        if cache_file.exists() {
            let _ = std::fs::remove_file(cache_file);
            count += 1;
        }
    }
    send_desktop_notification(
        "Snapin 截屏历史已清空",
        &format!("已成功清理 {count} 项临时快照与选区缓存"),
    )
    .await?;
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

        // 原地热重载扩展 (先 disable 再 enable，无需登出即可立即刷新托盘菜单)
        let _ = std::process::Command::new("gnome-extensions")
            .args(["disable", uuid])
            .output();
        let _ = std::process::Command::new("gnome-extensions")
            .args(["enable", uuid])
            .output();

        println!("已向 GNOME 发送扩展热重载启用指令。");
    } else {
        println!("未在当前目录找到 extension/ 源码文件！");
    }

    Ok(())
}
