//! Snapin 跨平台自更新系统封装模块。
//!
//! 基于 `shipup` 提供开箱即用的更新检测、下载验签、物理替换、崩溃回滚与偏好管理。

use shipup::{Update, UpdateEvent as ShipupEvent, Updater};
use std::time::Duration;
use thiserror::Error;

/// 自更新领域错误。
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum UpdaterError {
    /// 更新器构建、检查、下载或崩溃回滚失败
    #[error("自更新操作失败，原因: {reason}")]
    Operation {
        /// 可定位的中文错误描述
        reason: String,
    },
}

/// 构造带操作上下文的更新错误。
fn updater_error(context: &str, error: impl std::fmt::Display) -> UpdaterError {
    UpdaterError::Operation {
        reason: format!("{context}: {error}"),
    }
}

/// Snapin 官方发布包 Ed25519 签名验证公钥（Base64 编码）
pub const UPDATE_PUBLIC_KEY: &str = "qWJJRopD/biyFHEIKulKXEEk8XvDKWdL0y+xPNxip7w=";

/// Snapin 官方 GitHub Releases 最新版本清单端点
pub const DEFAULT_MANIFEST_URL: &str =
    "https://github.com/mangerle/snapin/releases/latest/download/latest.json";

/// 根据配置与编译期版本构建 `shipup::Updater` 实例
///
/// # Errors
/// 若版本号格式非法或更新器参数校验失败，返回 [`UpdaterError::Operation`]。
fn build_updater(custom_manifest_url: Option<&str>) -> Result<Updater, UpdaterError> {
    let manifest_url = custom_manifest_url
        .filter(|url| !url.trim().is_empty())
        .unwrap_or(DEFAULT_MANIFEST_URL);

    let allow_file = manifest_url.starts_with("file://");

    Updater::builder()
        .current_version(env!("CARGO_PKG_VERSION"))
        .map_err(|e| updater_error("解析当前客户端版本失败", e))?
        .manifest_url(manifest_url)
        .public_key(UPDATE_PUBLIC_KEY)
        .allow_file_protocol(allow_file)
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| updater_error("构建自更新引擎失败", e))
}

/// 异步检查是否有可用新版本
///
/// # Errors
/// 当网络超时、Manifest 解析异常或签名校验失败时返回错误。
pub async fn check_update_async(
    custom_manifest_url: Option<&str>,
) -> Result<Option<UpdateInfo>, UpdaterError> {
    let updater = build_updater(custom_manifest_url)?;
    updater
        .check_async()
        .await
        .map(|update| update.map(UpdateInfo::new))
        .map_err(|e| updater_error("异步检查更新失败", e))
}

/// 同步阻塞检查是否有可用新版本（适用于独立工作线程）
///
/// # Errors
///
/// 检查清单、网络请求、签名校验、安装或回滚失败时返回 [`UpdaterError`]。
pub fn check_update(custom_manifest_url: Option<&str>) -> Result<Option<UpdateInfo>, UpdaterError> {
    let updater = build_updater(custom_manifest_url)?;
    updater
        .check()
        .map(|update| update.map(UpdateInfo::new))
        .map_err(|e| updater_error("检查更新失败", e))
}

/// 已确认可用的新版本信息。
///
/// 对外只暴露版本、发布日期与更新日志，不把 `shipup::Update` 泄漏到应用层；
/// 下载与安装也必须经过本 crate 的统一入口。
#[derive(Debug, Clone)]
pub struct UpdateInfo {
    inner: Update,
}

impl UpdateInfo {
    fn new(inner: Update) -> Self {
        Self { inner }
    }

    /// 新版本号。
    pub fn version(&self) -> String {
        self.inner.version().to_string()
    }

    /// 更新日志。
    pub fn notes(&self) -> Option<String> {
        self.inner.notes().map(str::to_string)
    }

    /// 发布日期。
    pub fn pub_date(&self) -> Option<String> {
        self.inner.pub_date().map(str::to_string)
    }

    /// 异步下载并验签；回调只接收本 crate 的稳定事件类型。
    ///
    /// # Errors
    ///
    /// 网络失败、哈希或签名校验失败时返回 [`UpdaterError`]。
    pub async fn download_async<F>(&self, mut callback: F) -> Result<DownloadedUpdate, UpdaterError>
    where
        F: FnMut(UpdateEvent) + Send + 'static,
    {
        self.inner
            .download_async(|event| callback(map_event(event)))
            .await
            .map(DownloadedUpdate::new)
            .map_err(|error| updater_error("下载或验签更新包失败", error))
    }

    /// 同步下载并验签；供设置中心阻塞线程池调用。
    ///
    /// # Errors
    ///
    /// 网络失败、哈希或签名校验失败时返回 [`UpdaterError`]。
    pub fn download_blocking<F>(&self, mut callback: F) -> Result<DownloadedUpdate, UpdaterError>
    where
        F: FnMut(UpdateEvent),
    {
        self.inner
            .download(|event| callback(map_event(event)))
            .map(DownloadedUpdate::new)
            .map_err(|error| updater_error("下载或验签更新包失败", error))
    }
}

/// 已下载并验签、等待安装的更新包。
#[derive(Debug, Clone)]
pub struct DownloadedUpdate {
    inner: shipup::DownloadedUpdate,
}

impl DownloadedUpdate {
    fn new(inner: shipup::DownloadedUpdate) -> Self {
        Self { inner }
    }

    /// 目标版本号。
    pub fn version(&self) -> String {
        self.inner.version().to_string()
    }

    /// 执行物理安装；回调接收统一的安装阶段事件。
    ///
    /// # Errors
    ///
    /// 归档解压、二进制替换或安装器派生失败时返回 [`UpdaterError`]。
    pub fn install<F>(&self, mut callback: F) -> Result<(), UpdaterError>
    where
        F: FnMut(UpdateEvent),
    {
        self.inner
            .install(|event| callback(map_event(event)))
            .map_err(|error| updater_error("应用更新包失败", error))
    }
}

/// 更新下载/安装阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum UpdateStage {
    CheckingManifest,
    DownloadStarted,
    VerifyingChecksum,
    VerifyingSignature,
    ExtractingArchive,
    Installing,
    ReadyToRestart,
    DeferredToReboot,
    Completed,
    Other,
}

/// 自更新进度事件。
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum UpdateEvent {
    /// 下载进度
    DownloadProgress {
        downloaded_bytes: u64,
        total_bytes: Option<u64>,
        percent: Option<f32>,
        speed_bytes_per_sec: Option<u64>,
    },
    /// 阶段切换
    Stage(UpdateStage),
    /// 失败重试中
    Retrying {
        attempt: u32,
        max_retries: u32,
        error: String,
    },
    /// 不可恢复失败
    Failed { reason: String },
}

fn map_event(event: ShipupEvent) -> UpdateEvent {
    match event {
        ShipupEvent::CheckingManifest => UpdateEvent::Stage(UpdateStage::CheckingManifest),
        ShipupEvent::DownloadStarted { .. } => UpdateEvent::Stage(UpdateStage::DownloadStarted),
        ShipupEvent::DownloadProgress {
            downloaded_bytes,
            total_bytes,
            percent,
            speed_bytes_per_sec,
            ..
        } => UpdateEvent::DownloadProgress {
            downloaded_bytes,
            total_bytes,
            percent,
            speed_bytes_per_sec,
        },
        ShipupEvent::VerifyingChecksum => UpdateEvent::Stage(UpdateStage::VerifyingChecksum),
        ShipupEvent::VerifyingSignature => UpdateEvent::Stage(UpdateStage::VerifyingSignature),
        ShipupEvent::ExtractingArchive => UpdateEvent::Stage(UpdateStage::ExtractingArchive),
        ShipupEvent::Installing => UpdateEvent::Stage(UpdateStage::Installing),
        ShipupEvent::ReadyToRestart => UpdateEvent::Stage(UpdateStage::ReadyToRestart),
        ShipupEvent::DeferredToReboot { .. } => UpdateEvent::Stage(UpdateStage::DeferredToReboot),
        ShipupEvent::Retrying {
            attempt,
            max_retries,
            error,
            ..
        } => UpdateEvent::Retrying {
            attempt,
            max_retries,
            error,
        },
        ShipupEvent::Failed { reason } => UpdateEvent::Failed { reason },
        ShipupEvent::Completed => UpdateEvent::Stage(UpdateStage::Completed),
    }
}

/// 客户端启动入口的崩溃自检与自动回滚
///
/// # 机制说明
/// 在应用 `main` 首行执行。若新版本由于打包动态库缺失或严重 bug 导致连续启动崩溃达到 2 次，
/// 会自动用上一稳定版本的备份覆盖回滚，防止客户端陷入不可恢复的闪退死锁。
///
/// # Errors
///
/// 检查清单、网络请求、签名校验、安装或回滚失败时返回 [`UpdaterError`]。
pub fn check_and_recover_on_startup() -> Result<(), UpdaterError> {
    shipup::check_and_recover_current(2)
        .map_err(|e| updater_error("启动自检与崩溃回滚执行失败", e))?;
    Ok(())
}

/// 客户端主流程平稳就绪后确认升级成功闭环
///
/// # 机制说明
/// 在守护进程或主界面成功渲染就绪后调用，重置崩溃计数器并安全清理旧版本备份。
///
/// # Errors
///
/// 检查清单、网络请求、签名校验、安装或回滚失败时返回 [`UpdaterError`]。
pub fn confirm_update_success() -> Result<(), UpdaterError> {
    shipup::confirm_update_success().map_err(|e| updater_error("确认升级状态闭环失败", e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 外部事件必须映射为稳定的本 crate 事件，供应用与设置中心共用。
    #[test]
    fn test_shipup_event_mapping_is_stable() {
        let progress = map_event(ShipupEvent::DownloadProgress {
            downloaded_bytes: 10,
            total_bytes: Some(100),
            percent: Some(10.0),
            speed_bytes_per_sec: Some(1024),
            eta: None,
        });
        assert_eq!(
            progress,
            UpdateEvent::DownloadProgress {
                downloaded_bytes: 10,
                total_bytes: Some(100),
                percent: Some(10.0),
                speed_bytes_per_sec: Some(1024),
            }
        );

        let retry = map_event(ShipupEvent::Retrying {
            attempt: 1,
            max_retries: 3,
            delay: Duration::from_millis(100),
            error: "网络抖动".to_string(),
        });
        assert!(matches!(retry, UpdateEvent::Retrying { attempt: 1, .. }));
        assert!(matches!(
            map_event(ShipupEvent::VerifyingSignature),
            UpdateEvent::Stage(UpdateStage::VerifyingSignature)
        ));
    }

    /// 测试默认配置下 Updater 能够成功构建且参数合法
    #[test]
    fn test_build_updater_default() {
        let updater = build_updater(None);
        assert!(updater.is_ok(), "默认更新器应当构建成功");
    }

    /// 测试传递自定义 Manifest URL 时更新器能够成功构建
    #[test]
    fn test_build_updater_custom_url() {
        let custom = "https://example.com/mirror/latest.json";
        let updater = build_updater(Some(custom));
        assert!(updater.is_ok(), "自定义端点更新器应当构建成功");
    }

    /// 测试空字符串自定义端点能够安全回退至官方默认 URL
    #[test]
    fn test_build_updater_fallback_on_empty() {
        let updater = build_updater(Some("   "));
        assert!(updater.is_ok(), "空字符串应当安全回退到默认 URL 并构建成功");
    }

    /// 测试启动崩溃自检与确认闭环可以安全幂等调用
    #[test]
    fn test_crash_recovery_and_confirm_lifecycle() {
        let recover_res = check_and_recover_on_startup();
        assert!(recover_res.is_ok(), "首次正常启动自检应当顺利通过");

        let confirm_res = confirm_update_success();
        assert!(confirm_res.is_ok(), "确认升级状态闭环应当顺利通过");
    }

    /// 测试未经合法 Ed25519 签名的伪造清单会被更新器强行安全阻断
    #[test]
    fn test_unsigned_manifest_rejected_by_signature_guard() {
        let temp_dir = tempfile::tempdir().expect("创建临时目录失败");
        let manifest_path = temp_dir.path().join("latest.json");
        std::fs::write(&manifest_path, r#"{"version":"9.9.9","packages":{}}"#)
            .expect("写入临时清单失败");

        let file_url = format!("file://{}", manifest_path.to_string_lossy());
        let check_res = check_update(Some(&file_url));
        assert!(
            check_res.is_err(),
            "缺少合法 Ed25519 签名的清单必须被更新器拦截拒绝"
        );
    }
}
