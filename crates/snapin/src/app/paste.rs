//! 贴图生成与窗口展示模块。

use anyhow::{Context, Result};
use snapin_platform::{
    generate_secure_temp_path, generate_unique_id, read_clipboard_image, read_clipboard_text,
    send_desktop_notification,
};
use snapin_ui::paster::{
    render_color_swatch_card, render_simple_text_card, spawn_paster_window, try_parse_hex_color,
    PasterItem,
};
use std::path::PathBuf;
use tiny_skia::Pixmap;

use snapin_ui::paster::PasterWorkspaceProxy;

/// 从剪贴板提取图像或文本生成 PasterItem 数据项
pub fn create_paster_item_from_clipboard() -> Result<Option<PasterItem>> {
    if let Ok(Some(img)) = read_clipboard_image() {
        if let Some(mut pixmap) = Pixmap::new(img.width(), img.height()) {
            pixmap.data_mut().copy_from_slice(img.as_raw());
            let uid = generate_unique_id();
            let item = PasterItem::new(uid, format!("clip_img_{uid}"), pixmap, (200, 200));
            return Ok(Some(item));
        }
    }

    if let Ok(Some(text)) = read_clipboard_text() {
        let uid = generate_unique_id();
        let pixmap = if let Some((r, g, b)) = try_parse_hex_color(&text) {
            render_color_swatch_card(r, g, b).context("生成色彩卡片失败")?
        } else {
            let lines: Vec<&str> = text.lines().collect();
            render_simple_text_card(&lines).context("生成文本便签卡片失败")?
        };

        let item = PasterItem::new(uid, format!("clip_note_{uid}"), pixmap, (250, 250));
        return Ok(Some(item));
    }

    Ok(None)
}

/// 检查系统剪贴板并将新贴图直接挂载至集中工作区
pub async fn paste_clipboard_to_workspace(proxy: &PasterWorkspaceProxy) -> Result<()> {
    tracing::info!("正在检查剪贴板内容以生成贴图...");
    if let Some(item) = create_paster_item_from_clipboard()? {
        proxy
            .open_paster(item)
            .context("向集中工作区投递贴图失败")?;
    } else {
        send_desktop_notification("Snapin 贴图提示", "当前剪贴板为空，无法生成贴图").await?;
    }
    Ok(())
}

/// 检查系统剪贴板并就地生成独立贴图（单次会话降级模式）
pub async fn run_oneshot_paste() -> Result<()> {
    tracing::info!("正在检查剪贴板内容以生成独立贴图...");
    if let Some(item) = create_paster_item_from_clipboard()? {
        tokio::task::spawn_blocking(move || spawn_paster_window(item))
            .await
            .context("贴图任务调度失败")?
            .context("贴图窗口运行异常")?;
    } else {
        send_desktop_notification("Snapin 贴图提示", "当前剪贴板为空，无法生成贴图").await?;
    }
    Ok(())
}

/// 独立启动贴图并挂载置顶窗口
pub fn run_pin_file(path: PathBuf, x: i32, y: i32) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }

    let pixmap = if path.extension().and_then(|e| e.to_str()) == Some("raw") {
        // 读取原生二进制像素格式，零解压计算开销
        let bytes = std::fs::read(&path).context("读取贴图原生像素文件失败")?;
        if bytes.len() < 8 {
            anyhow::bail!("贴图原生数据损坏");
        }
        let width = u32::from_be_bytes(bytes[0..4].try_into()?);
        let height = u32::from_be_bytes(bytes[4..8].try_into()?);
        let expected_len = 8 + (width as usize) * (height as usize) * 4;
        if bytes.len() != expected_len {
            anyhow::bail!("贴图像素尺寸与数据长度不一致");
        }
        let mut p =
            Pixmap::new(width, height).ok_or_else(|| anyhow::anyhow!("分配贴图 Pixmap 失败"))?;
        p.data_mut().copy_from_slice(&bytes[8..]);
        p
    } else {
        // 兼容外部常规图像文件（PNG / JPEG）
        let img = image::open(&path).context("无法读取外部贴图图像文件")?;
        let rgba = img.to_rgba8();
        let mut p = Pixmap::new(rgba.width(), rgba.height())
            .ok_or_else(|| anyhow::anyhow!("分配贴图 Pixmap 失败"))?;
        p.data_mut().copy_from_slice(rgba.as_raw());
        p
    };

    let uid = generate_unique_id();
    let title = format!("pin_{uid}");
    let width = pixmap.width();
    let height = pixmap.height();
    let raw_temp = generate_secure_temp_path("paster_pin", "raw");
    if std::fs::write(&raw_temp, pixmap.data()).is_ok() {
        let req = snapin_daemon::DaemonCommand::CreatePaster {
            width,
            height,
            x,
            y,
            title: title.clone(),
            raw_path: raw_temp.to_string_lossy().to_string(),
        };

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        if let Ok(true) = rt.block_on(snapin_daemon::send_command_to_daemon(&req)) {
            tracing::info!("外部图片已通过零序列化膨胀安全路径委托常驻集中工作区展示");
            if path.extension().and_then(|e| e.to_str()) == Some("raw") {
                let _ = std::fs::remove_file(&path);
            }
            return Ok(());
        }
        let _ = std::fs::remove_file(&raw_temp);
    }

    let item = PasterItem::new(uid, title, pixmap, (x, y));
    let res = spawn_paster_window(item);

    if path.extension().and_then(|e| e.to_str()) == Some("raw") {
        let _ = std::fs::remove_file(&path);
    }

    res.context("贴图窗口运行异常")
}
