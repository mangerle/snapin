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
    capture_fullscreen_extension, capture_fullscreen_portal, read_clipboard_image,
    read_clipboard_text, send_desktop_notification, sync_hotkeys_to_gnome,
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
        Some(Commands::Snip) => {
            tracing::info!("正在启动区域截图流程...");
            run_oneshot_snip().await?;
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
            "GNOME 扩展 D-Bus 截屏耗时: {:.2}ms",
            (t_dbus_end - t_dbus_start).as_secs_f64() * 1000.0
        );

        let t_decode_start = std::time::Instant::now();
        if let Ok(bytes) = std::fs::read(&target_path) {
            let _ = std::fs::remove_file(&target_path);
            if let Ok(dynamic_img) = image::load_from_memory(&bytes) {
                let rgba = dynamic_img.into_rgba8();
                let t_decode_end = std::time::Instant::now();
                tracing::info!(
                    "PNG 读取并内存解码耗时: {:.2}ms, 总截屏耗时: {:.2}ms",
                    (t_decode_end - t_decode_start).as_secs_f64() * 1000.0,
                    (t_decode_end - t_start).as_secs_f64() * 1000.0
                );
                return Ok(rgba);
            }
        }
    }

    tracing::info!("通过 XDG Desktop Portal 捕获屏幕快照...");
    let rgba = capture_fullscreen_portal()
        .await
        .context("Portal 全屏截图捕获失败")?;
    Ok(rgba)
}

/// 执行单次截图流程
async fn run_oneshot_snip() -> Result<()> {
    let t0 = std::time::Instant::now();
    let img = acquire_screen_snapshot().await?;
    let t1 = std::time::Instant::now();
    tracing::info!(
        "快照准备就绪: {}x{}，耗时: {:.2}ms，正在启动交互窗口...",
        img.width(),
        img.height(),
        (t1 - t0).as_secs_f64() * 1000.0
    );

    let outcome = tokio::task::spawn_blocking(move || run_snipper_session(img))
        .await
        .context("截图任务调度异常")?
        .context("截图交互会话运行失败")?;

    if let Some(outcome) = outcome {
        let cfg = config::load_or_create_config(None).unwrap_or_default();
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

/// 打开 Snapin 图形化首选项与设置页面
async fn open_preferences() -> Result<()> {
    let cfg_path = config::get_default_config_path()?;
    tracing::info!("正在打开首选项配置: {:?}", cfg_path);

    let output = std::process::Command::new("zenity")
        .args([
            "--info",
            "--title=Snapin 首选项与设置",
            "--window-icon=snapin",
            "--width=440",
            "--ok-label=打开配置文件",
            "--extra-button=同步系统快捷键",
            "--text=<big><b>Snapin 原生截图与贴图设置</b></big>\n\n<b>全局热键:</b>\n  • 区域截图: <b>F1</b>\n  • 固定贴图: <b>F3</b>\n  • 批量显隐: <b>Ctrl + F3</b>\n\n<b>贴图窗口快捷操作:</b>\n  • 复制图像: <b>C</b>\n  • 另存文件: <b>S</b>\n  • 顺时针旋转 90°: <b>3</b>\n  • 水平翻转: <b>H</b>\n  • 垂直翻转: <b>V</b>\n  • 调节透明度: <b>1 / 2</b>\n  • 缩放贴图: <b>鼠标滚轮</b>\n  • 关闭贴图: <b>双击左键 / Esc</b>\n\n<b>配置文件路径:</b>\n<i>~/.config/snapin/config.toml</i>",
        ])
        .output();

    if let Ok(out) = output {
        let resp = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if resp == "同步系统快捷键" {
            let cfg = config::load_or_create_config(None).unwrap_or_default();
            let _ = sync_hotkeys_to_gnome(&cfg.hotkeys);
            send_desktop_notification("Snapin 设置", "快捷键已成功重新同步至系统").await?;
            return Ok(());
        }
    }

    // 若用户点击打开配置文件或直接回车确认，拉起文本编辑器
    let _ = std::process::Command::new("gnome-text-editor")
        .arg(&cfg_path)
        .spawn()
        .or_else(|_| {
            std::process::Command::new("xdg-open")
                .arg(&cfg_path)
                .spawn()
        });

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
        let _ = send_desktop_notification("Snapin 截图贴图", "服务已就绪：按 F1 截图，按 F3 贴图")
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
