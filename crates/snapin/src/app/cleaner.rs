//! 截屏共享内存与历史临时缓存清理模块。

use anyhow::Result;
use snapin_platform::{clear_selection_cache, get_secure_runtime_dir, send_desktop_notification};

/// 清理本用户所有截屏共享内存与历史选区缓存
pub async fn clear_snip_history() -> Result<()> {
    let mut count = 0;

    // 1. 清理当前用户安全运行时目录（严禁误删正在运行的 IPC 通信套接字）
    let runtime_dir = get_secure_runtime_dir();
    let socket_path = snapin_core::AppPaths::default_socket_path();
    if let Ok(entries) = std::fs::read_dir(&runtime_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p != socket_path && std::fs::remove_file(&p).is_ok() {
                count += 1;
            }
        }
    }

    // 2. 清理历史选区记忆缓存文件 (由 save_helper 统一负责路径与格式)
    count += clear_selection_cache();

    send_desktop_notification(
        "Snapin 截屏历史已清空",
        &format!("已成功清理 {count} 项临时快照与选区缓存"),
    )
    .await?;
    Ok(())
}
