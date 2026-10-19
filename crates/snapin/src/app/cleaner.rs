//! 截屏共享内存与历史临时缓存清理模块。

use anyhow::Result;
use snapin_platform::send_desktop_notification;
use std::path::PathBuf;

/// 清理所有截屏共享内存与历史选区缓存
pub async fn clear_snip_history() -> Result<()> {
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
