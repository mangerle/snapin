//! 守护进程 IPC 指令处理器。
//!
//! 每个指令返回强类型响应，耗时文件与配置操作统一调度到阻塞线程池，
//! 避免阻塞异步主循环。

use super::super::paste::paste_clipboard_to_workspace;
use snapin_core::config;
use snapin_core::AppPaths;
use snapin_daemon::{DaemonCommand, DaemonResponse};
use snapin_platform::{send_desktop_notification, spawn_detached_system_command};
use snapin_ui::paster::{PasterItem, PasterWorkspaceProxy};
use std::path::PathBuf;
use std::time::Duration;
use tiny_skia::Pixmap;
use tokio::time::timeout;

const STATUS_QUERY_TIMEOUT: Duration = Duration::from_secs(2);

/// 处理单条守护进程指令
pub async fn handle_command(
    command: DaemonCommand,
    workspace: &PasterWorkspaceProxy,
) -> DaemonResponse {
    match command {
        DaemonCommand::ReloadConfig => reload_config(workspace).await,
        DaemonCommand::Snip => {
            let current_exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("snapin"));
            spawn_detached_system_command(&current_exe.to_string_lossy(), &["snip"]);
            DaemonResponse::Ok
        }
        DaemonCommand::Paste => {
            if let Err(error) = paste_clipboard_to_workspace(workspace).await {
                tracing::error!("守护服务执行剪贴板贴图失败: {error}");
                DaemonResponse::Error(format!("剪贴板贴图失败: {error}"))
            } else {
                DaemonResponse::Ok
            }
        }
        DaemonCommand::CreatePaster {
            width,
            height,
            x,
            y,
            title,
            raw_path,
        } => create_paster(workspace, width, height, x, y, title, raw_path).await,
        DaemonCommand::TogglePasters => {
            if let Err(error) = workspace.toggle_visibility() {
                DaemonResponse::Error(format!("切换贴图显隐失败: {error}"))
            } else {
                DaemonResponse::Ok
            }
        }
        DaemonCommand::SwitchGroup(target_id) => {
            if let Err(error) = workspace.switch_group(target_id) {
                return DaemonResponse::Error(format!("切换贴图分组失败: {error}"));
            }
            let _ = send_desktop_notification("Snapin 贴图组", "已切换贴图活动分组").await;
            DaemonResponse::Ok
        }
        DaemonCommand::Shutdown => DaemonResponse::Ok,
        DaemonCommand::Ping => DaemonResponse::Pong,
        DaemonCommand::GetStatus => {
            match timeout(STATUS_QUERY_TIMEOUT, workspace.get_status()).await {
                Ok(Ok(status)) => DaemonResponse::Status {
                    active_group: status.active_group,
                    paster_count: status.paster_count,
                },
                Ok(Err(error)) => DaemonResponse::Error(format!("查询贴图状态失败: {error}")),
                Err(_) => DaemonResponse::Error("查询贴图状态超时".to_string()),
            }
        }
    }
}

async fn reload_config(workspace: &PasterWorkspaceProxy) -> DaemonResponse {
    let loaded = tokio::task::spawn_blocking(|| config::load_or_create_config(None)).await;

    let new_config = match loaded {
        Ok(Ok(config)) => config,
        Ok(Err(error)) => {
            tracing::error!("重新加载配置文件失败: {error}");
            return DaemonResponse::Error(format!("重新加载配置文件失败: {error}"));
        }
        Err(error) => {
            tracing::error!("配置加载任务异常: {error}");
            return DaemonResponse::Error(format!("配置加载任务异常: {error}"));
        }
    };

    let hotkeys = new_config.hotkeys.clone();
    let hotkey_result =
        tokio::task::spawn_blocking(move || snapin_platform::sync_hotkeys_to_gnome(&hotkeys)).await;
    match hotkey_result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => tracing::warn!("热重载配置时同步系统快捷键失败: {error}"),
        Err(error) => tracing::warn!("快捷键同步任务异常: {error}"),
    }

    match workspace.reload_config(new_config) {
        Ok(()) => {
            tracing::info!("守护进程已成功热重载配置并同步至贴图工作区与系统快捷键");
            DaemonResponse::Ok
        }
        Err(error) => {
            tracing::warn!("热重载配置时同步贴图工作区失败: {error}");
            DaemonResponse::Error(format!("同步贴图工作区失败: {error}"))
        }
    }
}

async fn create_paster(
    workspace: &PasterWorkspaceProxy,
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    title: String,
    raw_path: String,
) -> DaemonResponse {
    let file_path = PathBuf::from(&raw_path);
    let runtime_dir = AppPaths::runtime_dir();
    let runtime_dir = runtime_dir
        .canonicalize()
        .unwrap_or_else(|_| runtime_dir.clone());
    let is_safe = tokio::task::spawn_blocking({
        let file_path = file_path.clone();
        move || {
            file_path
                .canonicalize()
                .map(|path| path.starts_with(&runtime_dir))
                .unwrap_or(false)
        }
    })
    .await
    .unwrap_or(false);

    if !is_safe {
        tracing::warn!("拒绝接收不受信任目录下的贴图临时文件: {:?}", file_path);
        return DaemonResponse::Error("非法贴图临时物理路径".to_string());
    }

    let Some(expected_len) = snapin_platform::checked_rgba_byte_len(width, height) else {
        return DaemonResponse::Error("贴图像素尺寸非法或超过上限".to_string());
    };

    let metadata_matches = tokio::fs::metadata(&file_path)
        .await
        .map(|metadata| metadata.len() == expected_len as u64)
        .unwrap_or(false);
    if !metadata_matches {
        return DaemonResponse::Error("像素数据长度与图像分辨率不匹配".to_string());
    }

    let bytes = match tokio::fs::read(&file_path).await {
        Ok(bytes) => bytes,
        Err(error) => {
            tracing::warn!("读取贴图临时文件失败: {error}");
            return DaemonResponse::Error(format!("读取贴图临时文件失败: {error}"));
        }
    };
    let _ = tokio::fs::remove_file(&file_path).await;

    if bytes.len() != expected_len {
        return DaemonResponse::Error("像素数据长度与图像分辨率不匹配".to_string());
    }

    let Some(mut pixmap) = Pixmap::new(width, height) else {
        return DaemonResponse::Error("分配像素缓冲区失败".to_string());
    };
    pixmap.data_mut().copy_from_slice(&bytes);

    let uid = AppPaths::generate_unique_id();
    let item = PasterItem::new(uid, title, pixmap, (x, y));
    match workspace.open_paster(item) {
        Ok(()) => DaemonResponse::Ok,
        Err(error) => {
            tracing::error!("在集中工作区中打开贴图失败: {error}");
            DaemonResponse::Error(format!("打开贴图失败: {error}"))
        }
    }
}
