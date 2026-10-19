//! Snapin 常驻后台守护进程主服务循环模块。

use super::paste::paste_clipboard_to_workspace;
use anyhow::{Context, Result};
use snapin_core::config::Config;
use snapin_daemon::{
    send_command_to_daemon, DaemonCommand, DaemonResponse, IpcInvocation, IpcListener,
};
use snapin_platform::{
    enable_gnome_extension, generate_unique_id, send_desktop_notification,
    spawn_detached_system_command, sync_hotkeys_to_gnome,
};
use snapin_ui::paster::{spawn_paster_workspace_thread, PasterItem};
use std::path::PathBuf;
use tiny_skia::Pixmap;
use tokio::sync::mpsc;

/// 运行后台守护服务
pub async fn run_daemon_server(cfg: Config) -> Result<()> {
    // 1. 若已有守护服务在运行，保持幂等性静默退出，不做多余改变
    if let Ok(true) = send_command_to_daemon(&DaemonCommand::Ping).await {
        tracing::info!("Snapin 守护服务已在后台运行中，无需重复启动。");
        enable_gnome_extension();
        return Ok(());
    }

    tracing::info!("Snapin 守护服务启动中...");

    // 自动确保 GNOME 快捷键注册与配套扩展原地热重载
    if let Err(e) = sync_hotkeys_to_gnome(&cfg.hotkeys) {
        tracing::warn!("启动守护服务时向系统注册快捷键失败: {e}");
    }
    enable_gnome_extension();

    // 2. 启动贴图集中工作区（常驻单一 GUI 事件循环独立线程）
    let workspace_proxy =
        spawn_paster_workspace_thread(false).context("初始化并启动集中贴图工作区失败")?;

    // 3. 初始化并启动 IPC 监听服务
    let listener = IpcListener::bind(None)
        .await
        .context("绑定 IPC 监听套接字失败")?;
    let (tx, mut rx) = mpsc::channel::<IpcInvocation>(32);

    let listener_handle = tokio::spawn(async move {
        listener.run_loop(tx).await;
    });

    tracing::info!("Snapin 守护服务已就绪，等待热键指令...");
    if cfg.general.notify_on_startup {
        if let Err(e) = send_desktop_notification(
            "Snapin 截图贴图",
            "服务已就绪：按 Ctrl+Alt+A 截图，按 F1 贴图",
        )
        .await
        {
            tracing::debug!("发送启动就绪通知失败或系统通知不可用: {e}");
        }
    }

    // 4. IPC 指令调度主循环：将贴图与分组操作直接在单一工作区内微秒级分发
    while let Some(inv) = rx.recv().await {
        tracing::info!("守护进程接收到控制指令: {:?}", inv.request);
        let resp = match inv.request {
            DaemonCommand::Snip => {
                let current_exe =
                    std::env::current_exe().unwrap_or_else(|_| PathBuf::from("snapin"));
                spawn_detached_system_command(&current_exe.to_string_lossy(), &["snip"]);
                DaemonResponse::Ok
            }
            DaemonCommand::Paste => {
                let proxy = workspace_proxy.clone();
                tokio::spawn(async move {
                    if let Err(e) = paste_clipboard_to_workspace(&proxy).await {
                        tracing::error!("守护服务执行剪贴板贴图失败: {e}");
                    }
                });
                DaemonResponse::Ok
            }
            DaemonCommand::CreatePaster {
                width,
                height,
                x,
                y,
                title,
                raw_path,
            } => {
                let file_path = PathBuf::from(&raw_path);
                let runtime_dir = snapin_platform::get_secure_runtime_dir();
                // 路径防穿越安全校验：确保临时文件位于受控的安全运行时目录
                let is_safe = file_path
                    .canonicalize()
                    .map(|p| p.starts_with(&runtime_dir))
                    .unwrap_or(false);

                if !is_safe {
                    tracing::warn!("拒绝接收不受信任目录下的贴图临时文件: {:?}", file_path);
                    DaemonResponse::Error("非法贴图临时物理路径".to_string())
                } else if let Ok(bytes) = std::fs::read(&file_path) {
                    let _ = std::fs::remove_file(&file_path);
                    let expected_len = (width as usize) * (height as usize) * 4;
                    if bytes.len() == expected_len {
                        if let Some(mut pixmap) = Pixmap::new(width, height) {
                            pixmap.data_mut().copy_from_slice(&bytes);
                            let uid = generate_unique_id();
                            let item = PasterItem::new(uid, title, pixmap, (x, y));
                            if let Err(e) = workspace_proxy.open_paster(item) {
                                tracing::error!("在集中工作区中打开贴图失败: {e}");
                                DaemonResponse::Error(format!("打开贴图失败: {e}"))
                            } else {
                                DaemonResponse::Ok
                            }
                        } else {
                            DaemonResponse::Error("分配像素缓冲区失败".to_string())
                        }
                    } else {
                        DaemonResponse::Error("像素数据长度与图像分辨率不匹配".to_string())
                    }
                } else {
                    DaemonResponse::Error("读取贴图临时文件失败".to_string())
                }
            }
            DaemonCommand::TogglePasters => {
                if let Err(e) = workspace_proxy.toggle_visibility() {
                    DaemonResponse::Error(format!("切换贴图显隐失败: {e}"))
                } else {
                    DaemonResponse::Ok
                }
            }
            DaemonCommand::SwitchGroup(target_id) => {
                if let Err(e) = workspace_proxy.switch_group(target_id) {
                    DaemonResponse::Error(format!("切换贴图分组失败: {e}"))
                } else {
                    let _ = send_desktop_notification("Snapin 贴图组", "已切换贴图活动分组").await;
                    DaemonResponse::Ok
                }
            }
            DaemonCommand::Shutdown => {
                tracing::info!("接收到平滑停机指令，退出守护进程事件循环");
                let _ = workspace_proxy.shutdown();
                let _ = inv.responder.send(DaemonResponse::Ok);
                break;
            }
            DaemonCommand::Ping => {
                tracing::debug!("心跳响应正常");
                DaemonResponse::Pong
            }
            DaemonCommand::GetStatus => {
                if let Ok(status) = workspace_proxy.get_status().await {
                    DaemonResponse::Status {
                        active_group: status.active_group,
                        paster_count: status.paster_count,
                    }
                } else {
                    DaemonResponse::Status {
                        active_group: 0,
                        paster_count: 0,
                    }
                }
            }
        };
        let _ = inv.responder.send(resp);
    }

    // 平滑退出：中止后台 IPC 监听协程
    listener_handle.abort();
    let _ = listener_handle.await;

    Ok(())
}
