//! 截屏共享内存与历史临时缓存清理模块。
//!
//! 运行时目录同时存放 IPC 套接字、单实例锁与贴图临时像素文件。清理逻辑只删除
//! 本应用创建且已超过安全年龄的 `paster_pin_*.raw` 临时文件，绝不能按目录全删，
//! 否则会误删正在处理的像素文件，甚至删除守护进程持有的单实例锁文件。

use anyhow::{Context, Result};
use snapin_core::AppPaths;
use snapin_platform::send_desktop_notification;
use snapin_storage::clear_selection_cache;
use std::path::Path;
use std::time::{Duration, SystemTime};

/// 临时像素文件至少保留的年龄；正在传输的贴图文件绝不会被立即清理。
const TEMP_FILE_MIN_AGE: Duration = Duration::from_secs(10 * 60);
/// 贴图临时像素文件前缀，与 `AppPaths::generate_temp_path("paster_pin", "raw")` 对应。
const PASTER_TEMP_PREFIX: &str = "paster_pin_";

/// 清理本用户所有截屏共享内存与历史选区缓存。
///
/// # Errors
///
/// 运行时目录读取、临时文件删除或选区缓存清理失败时返回错误。
pub async fn clear_snip_history() -> Result<()> {
    let runtime_dir = AppPaths::runtime_dir();
    let socket_path = AppPaths::default_socket_path();
    let count = tokio::task::spawn_blocking(move || {
        cleanup_runtime_dir(&runtime_dir, &socket_path, TEMP_FILE_MIN_AGE)
    })
    .await
    .context("截屏历史清理任务调度失败")??;

    let cache_count = tokio::task::spawn_blocking(clear_selection_cache)
        .await
        .context("选区缓存清理任务调度失败")?;

    // 清理已经完成，通知失败属于非致命提示，不能把成功清理误报为失败。
    if let Err(error) = send_desktop_notification(
        "Snapin 截屏历史已清空",
        &format!("已成功清理 {count} 项临时快照与 {cache_count} 项选区缓存"),
    )
    .await
    {
        tracing::debug!("发送截屏历史清理通知失败或系统通知不可用: {error}");
    }
    Ok(())
}

/// 清理运行时目录中过期的贴图临时像素文件，并返回删除数量。
fn cleanup_runtime_dir(
    runtime_dir: &Path,
    socket_path: &Path,
    min_age: Duration,
) -> std::io::Result<usize> {
    let entries = match std::fs::read_dir(runtime_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error),
    };
    let now = SystemTime::now();
    let mut removed = 0usize;

    for entry in entries.flatten() {
        let path = entry.path();
        if path == socket_path || !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !name.starts_with(PASTER_TEMP_PREFIX)
            || path.extension().and_then(|ext| ext.to_str()) != Some("raw")
        {
            continue;
        }
        let is_stale = entry
            .metadata()
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age >= min_age);
        if is_stale && std::fs::remove_file(&path).is_ok() {
            removed = removed.saturating_add(1);
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// 清理只能命中已知前缀的过期临时像素文件，套接字、锁与未知文件必须保留。
    #[test]
    fn test_cleanup_runtime_dir_only_removes_stale_paster_temp_files() {
        let dir = tempfile::tempdir().expect("应可创建测试目录");
        let socket = dir.path().join("snapin.sock");
        fs::write(&socket, b"socket").expect("应可写入模拟套接字");
        let lock = dir.path().join("snapin.lock");
        fs::write(&lock, b"lock").expect("应可写入模拟锁");
        let unknown = dir.path().join("user-data.bin");
        fs::write(&unknown, b"keep").expect("应可写入未知文件");
        let temp = dir.path().join("paster_pin_1_2.raw");
        fs::write(&temp, b"pixels").expect("应可写入临时像素文件");

        let removed =
            cleanup_runtime_dir(dir.path(), &socket, Duration::ZERO).expect("清理任务不应失败");

        assert_eq!(removed, 1);
        assert!(!temp.exists());
        assert!(socket.exists());
        assert!(lock.exists());
        assert!(unknown.exists());
    }

    /// 未达到年龄阈值的临时文件必须保留，避免与正在进行的贴图传输竞态。
    #[test]
    fn test_cleanup_runtime_dir_keeps_fresh_temp_files() {
        let dir = tempfile::tempdir().expect("应可创建测试目录");
        let socket = dir.path().join("snapin.sock");
        let temp = dir.path().join("paster_pin_1_2.raw");
        fs::write(&temp, b"pixels").expect("应可写入临时像素文件");

        let removed = cleanup_runtime_dir(dir.path(), &socket, Duration::from_secs(3600))
            .expect("清理任务不应失败");

        assert_eq!(removed, 0);
        assert!(temp.exists());
    }
}
