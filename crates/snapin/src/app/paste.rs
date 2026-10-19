//! 贴图生成与窗口展示模块。

use anyhow::{Context, Result};
use snapin_platform::{
    generate_unique_id, read_clipboard_image, read_clipboard_text, send_desktop_notification,
};
use snapin_ui::paster::{
    render_color_swatch_card, render_simple_text_card, spawn_paster_window, try_parse_hex_color,
    PasterItem,
};
use std::path::PathBuf;
use tiny_skia::Pixmap;

/// 检查系统剪贴板并就地生成贴图
pub async fn run_oneshot_paste() -> Result<()> {
    tracing::info!("正在检查剪贴板内容以生成贴图...");

    if let Ok(Some(img)) = read_clipboard_image() {
        if let Some(mut pixmap) = Pixmap::new(img.width(), img.height()) {
            pixmap.data_mut().copy_from_slice(img.as_raw());
            let uid = generate_unique_id();
            let item = PasterItem::new(uid, format!("clip_img_{uid}"), pixmap, (200, 200));
            spawn_paster_window(item).context("贴图窗口运行异常")?;
            return Ok(());
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
        spawn_paster_window(item).context("贴图窗口运行异常")?;
        return Ok(());
    }

    send_desktop_notification("Snapin 贴图提示", "当前剪贴板为空，无法生成贴图").await?;
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
    let item = PasterItem::new(uid, format!("pin_{uid}"), pixmap, (x, y));

    // 运行贴图窗口直至用户关闭
    let res = spawn_paster_window(item);

    // 窗口退出后自动清理临时文件（如果是 .raw 运行时文件）
    if path.extension().and_then(|e| e.to_str()) == Some("raw") {
        let _ = std::fs::remove_file(&path);
    }

    res.context("贴图窗口运行异常")
}
