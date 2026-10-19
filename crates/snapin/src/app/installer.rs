//! GNOME Shell 扩展部署与热重载模块。

use anyhow::{Context, Result};
use std::path::PathBuf;

/// 安装配套 GNOME Shell 扩展
pub fn install_gnome_extension() -> Result<()> {
    let home = std::env::var("HOME").context("未找到 HOME 环境变量")?;
    let uuid = "snapin@wangle.local";
    let target_dir = PathBuf::from(home)
        .join(".local/share/gnome-shell/extensions")
        .join(uuid);

    std::fs::create_dir_all(&target_dir).context("创建 GNOME 扩展目标目录失败")?;

    let current_dir = std::env::current_dir()?;
    let ext_source_dir = current_dir.join("extension");

    let meta_file = ext_source_dir.join("metadata.json");
    let js_file = ext_source_dir.join("extension.js");

    if meta_file.exists() && js_file.exists() {
        std::fs::copy(&meta_file, target_dir.join("metadata.json"))?;
        std::fs::copy(&js_file, target_dir.join("extension.js"))?;
        tracing::info!("GNOME 扩展文件已部署至: {:?}", target_dir);

        // 原地热重载扩展 (先 disable 再 enable，无需登出即可立即刷新托盘菜单)
        let _ = std::process::Command::new("gnome-extensions")
            .args(["disable", uuid])
            .output();
        let _ = std::process::Command::new("gnome-extensions")
            .args(["enable", uuid])
            .output();

        tracing::info!("已向 GNOME 发送扩展热重载启用指令。");
    } else {
        tracing::warn!("未在当前目录找到 extension/ 源码文件！");
    }

    Ok(())
}
