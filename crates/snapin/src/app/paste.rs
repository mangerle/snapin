//! 贴图生成与窗口展示模块。

use anyhow::{Context, Result};
use snapin_core::AppPaths;
use snapin_platform::{read_clipboard_image, read_clipboard_text, send_desktop_notification};
use snapin_ui::paster::{
    render_color_swatch_card, render_simple_text_card, spawn_paster_window, try_parse_hex_color,
    PasterItem,
};
use std::path::{Path, PathBuf};
use tiny_skia::Pixmap;

use snapin_ui::paster::PasterWorkspaceProxy;

/// 从剪贴板提取图像或文本生成 PasterItem 数据项
pub fn create_paster_item_from_clipboard() -> Result<Option<PasterItem>> {
    if let Ok(Some(img)) = read_clipboard_image() {
        if let Some(mut pixmap) = Pixmap::new(img.width(), img.height()) {
            pixmap.data_mut().copy_from_slice(img.as_raw());
            let uid = AppPaths::generate_unique_id();
            let item = PasterItem::new(uid, format!("clip_img_{uid}"), pixmap, (200, 200));
            return Ok(Some(item));
        }
    }

    if let Ok(Some(text)) = read_clipboard_text() {
        let uid = AppPaths::generate_unique_id();
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
    let item = tokio::task::spawn_blocking(create_paster_item_from_clipboard)
        .await
        .context("剪贴板解析任务调度失败")??;

    if let Some(item) = item {
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
    let item = tokio::task::spawn_blocking(create_paster_item_from_clipboard)
        .await
        .context("剪贴板解析任务调度失败")??;

    if let Some(item) = item {
        tokio::task::spawn_blocking(move || spawn_paster_window(item))
            .await
            .context("贴图任务调度失败")?
            .context("贴图窗口运行异常")?;
    } else {
        send_desktop_notification("Snapin 贴图提示", "当前剪贴板为空，无法生成贴图").await?;
    }
    Ok(())
}

/// 从磁盘读取并解码目标图像为 tiny-skia 位图（同步阻塞实现，仅供阻塞线程池调用）。
///
/// `raw` 后缀视为运行时共享内存中的原生像素容器，采用 `[4 字节宽][4 字节高][RGBA 数据]`
/// 大端布局零解压直读；其余后缀走图像库解码。两条路径都包含磁盘 IO 与像素级 CPU 计算，
/// 因此严禁在异步运行时工作线程上直接调用。
fn load_pixmap_from_file(path: &Path) -> Result<Pixmap> {
    if path.extension().and_then(|ext| ext.to_str()) == Some("raw") {
        let metadata = std::fs::metadata(path).context("读取贴图原生像素文件元数据失败")?;
        let max_bytes = snapin_platform::MAX_RAW_PIXELS
            .saturating_mul(4)
            .saturating_add(8);
        if metadata.len() > max_bytes {
            anyhow::bail!("贴图原生像素文件过大: {} 字节", metadata.len());
        }
        let bytes = std::fs::read(path).context("读取贴图原生像素文件失败")?;
        if bytes.len() < 8 {
            anyhow::bail!("贴图原生像素数据损坏，长度: {}", bytes.len());
        }
        let width = u32::from_be_bytes(
            bytes[0..4]
                .try_into()
                .map_err(|_| anyhow::anyhow!("贴图原生像素宽度头部长度异常"))?,
        );
        let height = u32::from_be_bytes(
            bytes[4..8]
                .try_into()
                .map_err(|_| anyhow::anyhow!("贴图原生像素高度头部长度异常"))?,
        );
        let pixel_bytes = snapin_platform::checked_rgba_byte_len(width, height)
            .ok_or_else(|| anyhow::anyhow!("贴图像素尺寸非法或超过上限"))?;
        let expected_len = 8usize
            .checked_add(pixel_bytes)
            .ok_or_else(|| anyhow::anyhow!("贴图像素长度溢出"))?;
        if bytes.len() != expected_len {
            anyhow::bail!(
                "贴图像素尺寸与数据长度不一致，宽度: {width}，高度: {height}，实际长度: {}",
                bytes.len()
            );
        }
        let mut pixmap =
            Pixmap::new(width, height).ok_or_else(|| anyhow::anyhow!("分配贴图 Pixmap 失败"))?;
        write_rgba_into(&mut pixmap, &bytes[8..])?;
        return Ok(pixmap);
    }

    let img = image::open(path).context("无法读取外部贴图图像文件")?;
    let rgba = img.to_rgba8();
    let (width, height) = (rgba.width(), rgba.height());
    let mut pixmap =
        Pixmap::new(width, height).ok_or_else(|| anyhow::anyhow!("分配贴图 Pixmap 失败"))?;
    write_rgba_into(&mut pixmap, rgba.as_raw())?;
    Ok(pixmap)
}

/// 将源 RGBA 像素写入位图缓冲区，长度不一致时报错而非触发越界 panic。
///
/// tiny-skia 缓冲区按 `宽 x 高 x 4` 精确分配，理论上与解码结果等长；此处显式校验是为了
/// 在图像库返回异常尺寸（例如截断的 PNG）时给出可定位的中文错误，而不是留下 panic 现场。
fn write_rgba_into(pixmap: &mut Pixmap, src: &[u8]) -> Result<()> {
    let dst = pixmap.data_mut();
    if dst.len() != src.len() {
        anyhow::bail!(
            "贴图像素长度与目标缓冲区不匹配，目标字节: {}，源字节: {}",
            dst.len(),
            src.len()
        );
    }
    dst.copy_from_slice(src);
    Ok(())
}

/// 独立启动贴图并挂载置顶窗口
pub async fn run_pin_file(path: PathBuf, x: i32, y: i32) -> Result<()> {
    if !path.exists() {
        tracing::warn!("贴图目标文件不存在，已忽略本次请求: {:?}", path);
        return Ok(());
    }
    let is_raw_source = path.extension().and_then(|ext| ext.to_str()) == Some("raw");

    let load_path = path.clone();
    let pixmap = tokio::task::spawn_blocking(move || load_pixmap_from_file(&load_path))
        .await
        .context("贴图解码任务调度失败")?
        .context("贴图解码失败")?;

    let uid = AppPaths::generate_unique_id();
    let title = format!("pin_{uid}");
    let width = pixmap.width();
    let height = pixmap.height();
    let raw_temp = AppPaths::generate_temp_path("paster_pin", "raw");

    // 临时原生像素落盘同为阻塞 IO，连同位图与路径一起移交阻塞线程处理后再取回，
    // 既避免整幅像素的额外复制，也保留后续清理所需的临时文件路径
    let (written, pixmap, raw_temp) = tokio::task::spawn_blocking(move || {
        let written = snapin_core::AppPaths::write_secure_file(&raw_temp, pixmap.data()).is_ok();
        (written, pixmap, raw_temp)
    })
    .await
    .context("贴图临时像素写入任务调度失败")?;

    let delivered = if written {
        let req = snapin_daemon::DaemonRequest::CreatePaster {
            width,
            height,
            x,
            y,
            title: title.clone(),
            raw_path: raw_temp.to_string_lossy().to_string(),
        };
        match snapin_daemon::send_command_to_daemon(&req).await {
            Ok(true) => {
                tracing::info!("外部图片已通过零序列化膨胀安全路径委托常驻集中工作区展示");
                true
            }
            _ => {
                let _ = tokio::fs::remove_file(&raw_temp).await;
                false
            }
        }
    } else {
        tracing::warn!("贴图临时像素文件写入失败，降级为本地独立贴图窗口");
        false
    };

    if !delivered {
        let item = PasterItem::new(uid, title, pixmap, (x, y));
        tokio::task::spawn_blocking(move || spawn_paster_window(item))
            .await
            .context("贴图任务调度失败")?
            .context("贴图窗口运行异常")?;
    }

    // 原生像素源文件是一次性中转载体，无论移交成功与否都必须回收，防止共享内存目录堆积
    if is_raw_source {
        let _ = tokio::fs::remove_file(&path).await;
    }

    Ok(())
}
