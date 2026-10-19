//! 截屏共享内存与历史临时缓存清理模块。

use anyhow::Result;
use snapin_platform::{get_secure_runtime_dir, send_desktop_notification};
use std::path::PathBuf;

/// 清理本用户所有截屏共享内存与历史选区缓存
pub async fn clear_snip_history() -> Result<()> {
    let mut count = 0;

    // 1. 清理当前用户安全运行时目录
    let runtime_dir = get_secure_runtime_dir();
    if let Ok(entries) = std::fs::read_dir(&runtime_dir) {
        for entry in entries.flatten() {
            if std::fs::remove_file(entry.path()).is_ok() {
                count += 1;
            }
        }
    }

    // 2. 清理历史选区记忆缓存文件
    if let Ok(home) = std::env::var("HOME") {
        let cache_file = PathBuf::from(home).join(".cache/snapin/last_selection.txt");
        if cache_file.exists() && std::fs::remove_file(cache_file).is_ok() {
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
