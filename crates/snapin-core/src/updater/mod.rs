//! Snapin 跨平台自更新系统封装模块。
//!
//! 基于 `shipup` 提供开箱即用的更新检测、下载验签、物理替换、崩溃回滚与偏好管理。

use crate::error::CoreError;
use shipup::{Update, Updater};
use std::time::Duration;

/// Snapin 官方发布包 Ed25519 签名验证公钥（Base64 编码）
pub const UPDATE_PUBLIC_KEY: &str = "qWJJRopD/biyFHEIKulKXEEk8XvDKWdL0y+xPNxip7w=";

/// Snapin 官方 GitHub Releases 最新版本清单端点
pub const DEFAULT_MANIFEST_URL: &str =
    "https://github.com/mangerle/snapin/releases/latest/download/latest.json";

/// 更新信息视图模型，供 GUI 与 CLI 呈现更新摘要
#[derive(Debug, Clone)]
pub struct UpdateInfo {
    /// 新版本号（例如 "1.2.0"）
    pub version: String,
    /// 版本更新说明（Markdown 或纯文本变更日志）
    pub release_notes: String,
    /// 发布时间戳文本（可选）
    pub pub_date: Option<String>,
}

impl From<&Update> for UpdateInfo {
    fn from(update: &Update) -> Self {
        Self {
            version: update.version().to_string(),
            release_notes: update.notes().unwrap_or("暂无详细更新日志。").to_string(),
            pub_date: update.pub_date().map(|d| d.to_string()),
        }
    }
}

/// 下载进度上下文快照
#[derive(Debug, Clone)]
pub struct DownloadProgress {
    /// 下载百分比进度（0.0 ~ 100.0）
    pub percent: Option<f32>,
    /// 瞬时下载速率（字节/秒）
    pub speed_bytes_per_sec: Option<u64>,
    /// 已下载字节数
    pub downloaded_bytes: u64,
    /// 总字节数（若服务端支持 Content-Length）
    pub total_bytes: Option<u64>,
}

/// 根据配置与编译期版本构建 `shipup::Updater` 实例
///
/// # Errors
/// 若版本号格式非法或更新器参数校验失败，返回 [`CoreError::UpdateError`]。
pub fn build_updater(custom_manifest_url: Option<&str>) -> Result<Updater, CoreError> {
    let manifest_url = custom_manifest_url
        .filter(|url| !url.trim().is_empty())
        .unwrap_or(DEFAULT_MANIFEST_URL);

    Updater::builder()
        .current_version(env!("CARGO_PKG_VERSION"))
        .map_err(|e| CoreError::UpdateError(format!("解析当前客户端版本失败: {e}")))?
        .manifest_url(manifest_url)
        .public_key(UPDATE_PUBLIC_KEY)
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| CoreError::UpdateError(format!("构建自更新引擎失败: {e}")))
}

/// 异步检查是否有可用新版本
///
/// # Errors
/// 当网络超时、Manifest 解析异常或签名校验失败时返回错误。
pub async fn check_update_async(
    custom_manifest_url: Option<&str>,
) -> Result<Option<Update>, CoreError> {
    let updater = build_updater(custom_manifest_url)?;
    updater
        .check_async()
        .await
        .map_err(|e| CoreError::UpdateError(format!("异步检查更新失败: {e}")))
}

/// 同步阻塞检查是否有可用新版本（适用于独立工作线程）
pub fn check_update(custom_manifest_url: Option<&str>) -> Result<Option<Update>, CoreError> {
    let updater = build_updater(custom_manifest_url)?;
    updater
        .check()
        .map_err(|e| CoreError::UpdateError(format!("检查更新失败: {e}")))
}

/// 客户端启动入口的崩溃自检与自动回滚
///
/// # 机制说明
/// 在应用 `main` 首行执行。若新版本由于打包动态库缺失或严重 bug 导致连续启动崩溃达到 2 次，
/// 会自动用上一稳定版本的备份覆盖回滚，防止客户端陷入不可恢复的闪退死锁。
pub fn check_and_recover_on_startup() -> Result<(), CoreError> {
    shipup::check_and_recover_current(2)
        .map_err(|e| CoreError::UpdateError(format!("启动自检与崩溃回滚执行失败: {e}")))?;
    Ok(())
}

/// 客户端主流程平稳就绪后确认升级成功闭环
///
/// # 机制说明
/// 在守护进程或主界面成功渲染就绪后调用，重置崩溃计数器并安全清理旧版本备份。
pub fn confirm_update_success() -> Result<(), CoreError> {
    shipup::confirm_update_success()
        .map_err(|e| CoreError::UpdateError(format!("确认升级状态闭环失败: {e}")))?;
    Ok(())
}
