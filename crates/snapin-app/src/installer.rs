//! GNOME Shell 扩展部署模块。
//!
//! 在编译期内嵌 GNOME Shell 扩展全部资源文件，摆脱对当前工作目录的路径依赖，
//! 支持在任意系统目录下安全部署至用户扩展目录并调用系统启用命令。

use anyhow::Result;
#[cfg(target_os = "linux")]
use anyhow::{bail, Context};
#[cfg(target_os = "linux")]
use snapin_platform::output_with_timeout;
#[cfg(target_os = "linux")]
use std::time::Duration;

#[cfg(target_os = "linux")]
const EXT_METADATA: &str = include_str!("../../../extension/metadata.json");
#[cfg(target_os = "linux")]
const EXT_JAVASCRIPT: &str = include_str!("../../../extension/extension.js");
#[cfg(target_os = "linux")]
const EXT_ICON_SVG: &[u8] = include_bytes!("../../../assets/icons/snapin.svg");
#[cfg(target_os = "linux")]
const GNOME_EXTENSION_UUID: &str = "snapin@wangle";
#[cfg(target_os = "linux")]
const LEGACY_GNOME_EXTENSION_UUIDS: &[&str] = &[
    "snapin@wangle.local",
    "snapin-helper@wangle.local",
    "snapin-helper@wangle",
    "snapin-recordtest@wangle",
];
#[cfg(target_os = "linux")]
const GNOME_EXTENSION_COMMAND_TIMEOUT: Duration = Duration::from_secs(10);
#[cfg(target_os = "linux")]
const GNOME_EXTENSION_ACTIVE_WAIT: Duration = Duration::from_secs(15);
#[cfg(target_os = "linux")]
const GNOME_EXTENSION_POLL_INTERVAL: Duration = Duration::from_millis(150);

/// 安装配套 GNOME Shell 扩展到用户目录并重新加载。
///
/// 先原子替换磁盘文件，再确保正式 UUID 处于启用状态。GNOME Shell 会缓存已经加载
/// 的 ESM 模块，因此从历史 UUID 首次迁移或扩展接口升级后，仍需注销并重新登录一次。
/// 使用稳定 UUID 可以保证后续启动始终加载同一目录，避免旧扩展继续遮蔽新版本。
///
/// # Errors
///
/// 扩展目录创建、内嵌资源写入、原子替换或启用命令执行失败时返回错误。
pub fn install_gnome_extension() -> Result<()> {
    #[cfg(not(target_os = "linux"))]
    {
        tracing::info!("当前操作系统无需安装 GNOME Shell 扩展。");
        Ok(())
    }

    #[cfg(target_os = "linux")]
    {
        let target_dir = snapin_foundation::AppPaths::gnome_extension_dir(GNOME_EXTENSION_UUID);
        let icons_dir = target_dir.join("icons");
        std::fs::create_dir_all(&icons_dir).context("创建 GNOME 扩展目标目录失败")?;

        // 以同目录临时文件原子替换，避免 Shell 在升级过程中读到半截脚本。
        write_embedded_file(target_dir.join("metadata.json"), EXT_METADATA.as_bytes())
            .context("写入 GNOME 扩展 metadata.json 失败")?;
        write_embedded_file(target_dir.join("extension.js"), EXT_JAVASCRIPT.as_bytes())
            .context("写入 GNOME 扩展 extension.js 失败")?;
        write_embedded_file(icons_dir.join("snapin-tray.svg"), EXT_ICON_SVG)
            .context("写入 GNOME 扩展图标文件失败")?;

        tracing::info!("GNOME 扩展文件已成功部署至: {:?}", target_dir);
        reload_gnome_extension(GNOME_EXTENSION_UUID)?;
        wait_for_gnome_extension_active(GNOME_EXTENSION_UUID)?;
        cleanup_legacy_extensions();
        Ok(())
    }
}

/// 确保扩展处于启用状态；Shell 已缓存模块时会在下一次登录加载新文件。
#[cfg(target_os = "linux")]
fn reload_gnome_extension(uuid: &str) -> Result<()> {
    // 扩展可能本来就未启用；停用失败不应阻止后续启用。
    if let Err(error) = set_gnome_extension_enabled(uuid, false) {
        tracing::debug!("停用 GNOME 扩展时未取得成功状态，继续尝试启用: {error}");
    }
    set_gnome_extension_enabled(uuid, true)?;
    tracing::info!("GNOME 扩展已启用，UUID: {uuid}；若接口尚未更新，请注销并重新登录");
    Ok(())
}

/// 等待 GNOME Shell 完成扩展加载，避免登录阶段守护进程先行时停留在 INACTIVE。
#[cfg(target_os = "linux")]
fn wait_for_gnome_extension_active(uuid: &str) -> Result<()> {
    let deadline = std::time::Instant::now() + GNOME_EXTENSION_ACTIVE_WAIT;
    let mut attempt = 0_u32;
    while std::time::Instant::now() < deadline {
        if gnome_extension_is_active(uuid) {
            tracing::info!("GNOME 扩展已进入 ACTIVE 状态，UUID: {uuid}");
            return Ok(());
        }
        // Shell 尚未完成启动时首次 enable 可能只写入启用状态；定期重试启用可在
        // 扩展管理器就绪后完成真实加载，而不必要求用户手动再执行一次命令。
        if attempt % 10 == 9 {
            let _ = set_gnome_extension_enabled(uuid, true);
        }
        attempt = attempt.saturating_add(1);
        std::thread::sleep(GNOME_EXTENSION_POLL_INTERVAL);
    }
    bail!("等待 GNOME 扩展进入 ACTIVE 状态超时，UUID: {uuid}");
}

/// 查询扩展是否已经由 Shell 真正加载。
#[cfg(target_os = "linux")]
fn gnome_extension_is_active(uuid: &str) -> bool {
    let mut command = std::process::Command::new("gnome-extensions");
    command.env("LC_ALL", "C").args(["info", uuid]);
    matches!(
        output_with_timeout(command, GNOME_EXTENSION_COMMAND_TIMEOUT),
        Ok(output)
            if output.status.success()
                && String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .any(|line| line.trim() == "State: ACTIVE")
    )
}

/// 调用 `gnome-extensions` 切换扩展状态。
#[cfg(target_os = "linux")]
fn set_gnome_extension_enabled(uuid: &str, enabled: bool) -> Result<()> {
    let action = if enabled { "enable" } else { "disable" };
    let mut command = std::process::Command::new("gnome-extensions");
    command.env("LC_ALL", "C").args([action, uuid]);
    let output = output_with_timeout(command, GNOME_EXTENSION_COMMAND_TIMEOUT)
        .with_context(|| format!("执行 GNOME 扩展 {action} 命令失败"))?;
    if !output.status.success() {
        bail!(
            "GNOME 扩展 {action} 命令失败，UUID: {uuid}，状态: {}，原因: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

/// 清理曾经使用过的扩展 UUID，避免旧 D-Bus 接口继续遮蔽正式扩展。
#[cfg(target_os = "linux")]
fn cleanup_legacy_extensions() {
    for uuid in LEGACY_GNOME_EXTENSION_UUIDS {
        if let Err(error) = set_gnome_extension_enabled(uuid, false) {
            tracing::debug!("停用历史 GNOME 扩展时未取得成功状态，UUID: {uuid}，原因: {error}");
        }
        let path = snapin_foundation::AppPaths::gnome_extension_dir(uuid);
        if path.exists() {
            if let Err(error) = std::fs::remove_dir_all(&path) {
                tracing::warn!(
                    "清理历史 GNOME 扩展目录失败，路径: {}，原因: {error}",
                    path.display()
                );
            } else {
                tracing::info!("已清理历史 GNOME 扩展目录: {}", path.display());
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn write_embedded_file(target: std::path::PathBuf, bytes: &[u8]) -> Result<()> {
    let parent = target.parent().context("GNOME 扩展目标文件缺少父目录")?;
    let temp = parent.join(format!(
        ".extension.{}.{}.tmp",
        std::process::id(),
        snapin_foundation::AppPaths::generate_unique_id()
    ));
    snapin_foundation::AppPaths::write_secure_file(&temp, bytes)
        .with_context(|| format!("写入 GNOME 扩展临时文件失败: {}", temp.display()))?;
    if let Err(error) = snapin_foundation::AppPaths::replace_file_atomically(&temp, &target) {
        let _ = std::fs::remove_file(&temp);
        return Err(error)
            .with_context(|| format!("替换 GNOME 扩展文件失败: {}", target.display()));
    }
    Ok(())
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn test_embedded_extension_contract_uses_stable_uuid_and_exclusion_method() {
        assert!(EXT_METADATA.contains("\"uuid\": \"snapin@wangle\""));
        assert!(EXT_JAVASCRIPT.contains("CaptureFullscreenExcludingTitle"));
        assert!(EXT_JAVASCRIPT.contains("StartRecordingStream"));
        assert!(EXT_JAVASCRIPT.contains("SimulateScroll"));
    }

    #[test]
    fn test_write_embedded_file_atomically_replaces_existing_content() {
        let dir = tempfile::tempdir().expect("应可创建测试目录");
        let path = dir.path().join("extension.js");
        std::fs::write(&path, b"old").expect("应可写入旧内容");

        write_embedded_file(path.clone(), b"new").expect("嵌入式文件应可原子写入");

        assert_eq!(std::fs::read(path).expect("应可读取新内容"), b"new");
    }
}
