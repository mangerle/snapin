//! 独立 GUI 进程的单实例锁。
//!
//! 设置中心与剪贴板面板各自是独立进程；重复启动会并发生成配置、更新包与面板状态。
//! 锁文件位于 0700 运行时目录，进程存活期间持有排他锁，退出时由 RAII 自动释放。

use anyhow::{Context, Result};
use snapin_foundation::AppPaths;
use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};

/// 进程生命周期内持有的单实例锁。
pub(crate) struct InstanceLock {
    path: PathBuf,
    _file: File,
}

impl InstanceLock {
    /// 尝试获取指定名称的锁；已有活进程持锁时返回 `None`。
    pub(crate) fn try_acquire(name: &str) -> Result<Option<Self>> {
        let path = AppPaths::ensure_runtime_dir().join(format!("{name}.lock"));
        Self::try_acquire_at(path)
    }

    fn try_acquire_at(path: PathBuf) -> Result<Option<Self>> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("创建单实例锁目录失败: {}", parent.display()))?;
        }
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&path)
            .with_context(|| format!("打开单实例锁文件失败: {}", path.display()))?;
        match file.try_lock() {
            Ok(()) => Ok(Some(Self { path, _file: file })),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(error)) => {
                Err(error).with_context(|| format!("获取单实例锁失败: {}", path.display()))
            }
        }
    }

    /// 锁文件路径，供诊断日志使用。
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instance_lock_rejects_second_owner() {
        let dir = tempfile::tempdir().expect("应可创建测试目录");
        let path = dir.path().join("settings.lock");
        let first = InstanceLock::try_acquire_at(path.clone())
            .expect("首次加锁应成功")
            .expect("首次加锁应获得锁");

        assert!(InstanceLock::try_acquire_at(path)
            .expect("第二次尝试不应报错")
            .is_none());
        drop(first);
    }
}
