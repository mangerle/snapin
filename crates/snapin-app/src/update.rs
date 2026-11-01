//! Snapin CLI 在线自更新子命令执行模块。

use anyhow::{Context, Result};
use shipup::{Update, UpdateEvent};
use snapin_core::config::Config;

/// 执行 CLI 更新流程
pub async fn run_cli_update(config: Option<Config>, check_only: bool) -> Result<()> {
    let custom_url = config.and_then(|c| c.general.custom_update_url);
    println!("正在检查 Snapin 最新版本...");

    let update = snapin_updater::check_update_async(custom_url.as_deref())
        .await
        .context("检查更新失败")?;

    let Some(update) = update else {
        println!(
            "当前已是最新版本 (v{})，无需更新。",
            env!("CARGO_PKG_VERSION")
        );
        return Ok(());
    };

    print_update_info(&update);

    if check_only {
        return Ok(());
    }

    println!("\n开始下载新版本更新包...");
    let downloaded = download_with_progress(update).await?;

    println!("\n正在应用物理安装...");
    downloaded
        .install(|event| {
            tracing::info!("更新包物理落地: {event:?}");
        })
        .context("执行物理安装失败")?;

    println!("Snapin 已成功升级至最新版本！请重启守护进程生效。");
    Ok(())
}

/// 打印新版本摘要信息
fn print_update_info(update: &Update) {
    println!("\n发现可用新版本: v{}", update.version());
    if let Some(date) = update.pub_date() {
        println!("发布时间: {date}");
    }
    if let Some(notes) = update.notes() {
        println!("\n更新日志:\n{notes}");
    }
}

/// 执行带控制台进度的异步下载与验签
async fn download_with_progress(update: Update) -> Result<shipup::DownloadedUpdate> {
    update
        .download_async(|event| match event {
            UpdateEvent::DownloadProgress {
                percent,
                speed_bytes_per_sec,
                ..
            } => {
                let speed_str = match speed_bytes_per_sec {
                    Some(s) if s >= 1024 * 1024 => {
                        format!("{:.2} MB/s", s as f64 / (1024.0 * 1024.0))
                    }
                    Some(s) if s >= 1024 => format!("{:.1} KB/s", s as f64 / 1024.0),
                    Some(s) => format!("{s} B/s"),
                    None => "-- B/s".to_string(),
                };
                let p = percent.unwrap_or(0.0);
                print!("\r下载进度: {p:.1}%  速率: {speed_str}      ");
                let _ = std::io::Write::flush(&mut std::io::stdout());
            }
            UpdateEvent::VerifyingSignature => {
                println!("\n正在验证 Ed25519 数字签名与 SHA-256 哈希完整性...");
            }
            _ => {}
        })
        .await
        .context("下载或验签更新包失败")
}
