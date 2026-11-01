//! GNOME Shell 扩展部署模块。
//!
//! 在编译期内嵌 GNOME Shell 扩展全部资源文件，摆脱对当前工作目录的路径依赖，
//! 支持在任意系统目录下安全部署至用户扩展目录并调用系统启用命令。

use anyhow::{Context, Result};

const EXT_METADATA: &str = include_str!("../../../../extension/metadata.json");
const EXT_JAVASCRIPT: &str = include_str!("../../../../extension/extension.js");
const EXT_ICON_SVG: &[u8] = include_bytes!("../../../../assets/icons/snapin.svg");

/// 安装配套 GNOME Shell 扩展到用户目录并向系统请求启用
pub fn install_gnome_extension() -> Result<()> {
    let uuid = "snapin@wangle.local";
    let target_dir = snapin_core::AppPaths::gnome_extension_dir(uuid);

    let icons_dir = target_dir.join("icons");
    std::fs::create_dir_all(&icons_dir).context("创建 GNOME 扩展目标目录失败")?;

    // 写入内嵌的元数据与脚本文件
    std::fs::write(target_dir.join("metadata.json"), EXT_METADATA)
        .context("写入 GNOME 扩展 metadata.json 失败")?;
    std::fs::write(target_dir.join("extension.js"), EXT_JAVASCRIPT)
        .context("写入 GNOME 扩展 extension.js 失败")?;
    std::fs::write(icons_dir.join("snapin-tray.svg"), EXT_ICON_SVG)
        .context("写入 GNOME 扩展图标文件失败")?;

    tracing::info!("GNOME 扩展文件已成功部署至: {:?}", target_dir);

    // 请求 GNOME 启用该扩展（登录或重启 Shell 后将自动加载生效）
    let _ = std::process::Command::new("gnome-extensions")
        .args(["enable", uuid])
        .output();

    tracing::info!("已向 GNOME 发送扩展启用指令。");
    Ok(())
}
