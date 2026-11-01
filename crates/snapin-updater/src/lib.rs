//! Snapin 跨平台自更新系统封装模块。
//!
//! 基于 `shipup` 提供开箱即用的更新检测、下载验签、物理替换、崩溃回滚与偏好管理。

use shipup::{Update, Updater};
use std::time::Duration;
use thiserror::Error;

/// 自更新领域错误。
#[derive(Debug, Error)]
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
) -> Result<Option<Update>, UpdaterError> {
    let updater = build_updater(custom_manifest_url)?;
    updater
        .check_async()
        .await
        .map_err(|e| updater_error("异步检查更新失败", e))
}

/// 同步阻塞检查是否有可用新版本（适用于独立工作线程）
pub fn check_update(custom_manifest_url: Option<&str>) -> Result<Option<Update>, UpdaterError> {
    let updater = build_updater(custom_manifest_url)?;
    updater
        .check()
        .map_err(|e| updater_error("检查更新失败", e))
}

/// 客户端启动入口的崩溃自检与自动回滚
///
/// # 机制说明
/// 在应用 `main` 首行执行。若新版本由于打包动态库缺失或严重 bug 导致连续启动崩溃达到 2 次，
/// 会自动用上一稳定版本的备份覆盖回滚，防止客户端陷入不可恢复的闪退死锁。
pub fn check_and_recover_on_startup() -> Result<(), UpdaterError> {
    shipup::check_and_recover_current(2)
        .map_err(|e| updater_error("启动自检与崩溃回滚执行失败", e))?;
    Ok(())
}

/// 客户端主流程平稳就绪后确认升级成功闭环
///
/// # 机制说明
/// 在守护进程或主界面成功渲染就绪后调用，重置崩溃计数器并安全清理旧版本备份。
pub fn confirm_update_success() -> Result<(), UpdaterError> {
    shipup::confirm_update_success().map_err(|e| updater_error("确认升级状态闭环失败", e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
