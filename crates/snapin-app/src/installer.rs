//! GNOME Shell 扩展部署模块。
//!
//! 在编译期内嵌 GNOME Shell 扩展全部资源文件，摆脱对当前工作目录的路径依赖，
//! 支持在任意系统目录下安全部署至用户扩展目录并调用系统启用命令。

#[cfg(target_os = "linux")]
use anyhow::Context;
use anyhow::Result;
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
const GNOME_EXTENSION_COMMAND_TIMEOUT: Duration = Duration::from_secs(10);

/// 安装配套 GNOME Shell 扩展到用户目录并向系统请求启用
pub fn install_gnome_extension() -> Result<()> {
    #[cfg(not(target_os = "linux"))]
    {
        tracing::info!("当前操作系统无需安装 GNOME Shell 扩展。");
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        let uuid = "snapin@wangle.local";
        let target_dir = snapin_foundation::AppPaths::gnome_extension_dir(uuid);

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

        // 请求 GNOME 启用该扩展（登录或重启 Shell 后将自动加载生效）
        let mut command = std::process::Command::new("gnome-extensions");
        command.args(["enable", uuid]);
        match output_with_timeout(command, GNOME_EXTENSION_COMMAND_TIMEOUT) {
            Ok(output) if output.status.success() => {
                tracing::info!("已向 GNOME 发送扩展启用指令。");
            }
            Ok(output) => {
                tracing::warn!(
                    "GNOME 扩展启用命令返回非零状态，状态: {}，原因: {}",
                    output.status,
                    String::from_utf8_lossy(&output.stderr).trim()
                );
            }
            Err(error) => {
                tracing::warn!("执行 GNOME 扩展启用命令失败: {error}");
            }
        }
        Ok(())
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
    fn test_write_embedded_file_atomically_replaces_existing_content() {
        let dir = tempfile::tempdir().expect("应可创建测试目录");
        let path = dir.path().join("extension.js");
        std::fs::write(&path, b"old").expect("应可写入旧内容");

        write_embedded_file(path.clone(), b"new").expect("嵌入式文件应可原子写入");

        assert_eq!(std::fs::read(path).expect("应可读取新内容"), b"new");
    }
}
