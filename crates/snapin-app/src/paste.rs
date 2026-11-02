//! 贴图生成与窗口展示模块。

use anyhow::{bail, Context, Result};
use snapin_foundation::AppPaths;
use snapin_platform::{read_clipboard_image, read_clipboard_text, send_desktop_notification};
use snapin_ui::paster::{
    render_color_swatch_card, render_simple_text_card, try_parse_hex_color, PasterItem,
    PasterWorkspaceProxy,
};
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use tiny_skia::Pixmap;

/// 外部贴图源文件允许读取的最大字节数。
const MAX_PASTER_IMAGE_FILE_BYTES: usize = 64 * 1024 * 1024;
/// 外部贴图解码后的最大宽度与高度。
const MAX_PASTER_IMAGE_SIDE: u32 = 16_384;

/// 从剪贴板提取图像或文本生成 PasterItem 数据项
pub fn create_paster_item_from_clipboard() -> Result<Option<PasterItem>> {
    match read_clipboard_image() {
        Ok(Some(img)) => {
            let mut pixmap = Pixmap::new(img.width(), img.height()).ok_or_else(|| {
                anyhow::anyhow!(
                    "分配剪贴板图像缓冲区失败，尺寸: {}x{}",
                    img.width(),
                    img.height()
                )
            })?;
            write_rgba_into(&mut pixmap, img.as_raw())?;
            let uid = AppPaths::generate_unique_id();
            let item = PasterItem::new(uid, format!("clip_img_{uid}"), pixmap, (200, 200));
            return Ok(Some(item));
        }
        Ok(None) => {}
        Err(error) => return Err(error).context("读取系统剪贴板图像失败"),
    }

    match read_clipboard_text() {
        Ok(Some(text)) => {
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
        Ok(None) => {}
        Err(error) => return Err(error).context("读取系统剪贴板文本失败"),
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
        if let Err(error) = send_desktop_notification(
            snapin_foundation::i18n::tr("Snapin 贴图提示"),
            snapin_foundation::i18n::tr("当前剪贴板为空，无法生成贴图"),
        )
        .await
        {
            tracing::debug!("发送剪贴板为空提示失败或系统通知不可用: {error}");
        }
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
        let max_bytes = snapin_storage::MAX_RAW_PIXELS
            .saturating_mul(4)
            .saturating_add(8);
        if metadata.len() > max_bytes {
            anyhow::bail!("贴图原生像素文件过大: {} 字节", metadata.len());
        }
        let mut file = std::fs::File::open(path).context("打开贴图原生像素文件失败")?;
        let mut header = [0u8; 8];
        file.read_exact(&mut header)
            .context("读取贴图原生像素头部失败")?;
        let width = u32::from_be_bytes(
            header[0..4]
                .try_into()
                .map_err(|_| anyhow::anyhow!("贴图原生像素宽度头部长度异常"))?,
        );
        let height = u32::from_be_bytes(
            header[4..8]
                .try_into()
                .map_err(|_| anyhow::anyhow!("贴图原生像素高度头部长度异常"))?,
        );
        let pixel_bytes = snapin_storage::checked_rgba_byte_len(width, height)
            .ok_or_else(|| anyhow::anyhow!("贴图像素尺寸非法或超过上限"))?;
        let expected_len = 8usize
            .checked_add(pixel_bytes)
            .ok_or_else(|| anyhow::anyhow!("贴图像素长度溢出"))?;
        let actual_len = usize::try_from(metadata.len())
            .map_err(|_| anyhow::anyhow!("贴图原生像素文件长度超出平台寻址范围"))?;
        if actual_len != expected_len {
            anyhow::bail!(
                "贴图像素尺寸与数据长度不一致，宽度: {width}，高度: {height}，实际长度: {actual_len}"
            );
        }
        let mut pixmap =
            Pixmap::new(width, height).ok_or_else(|| anyhow::anyhow!("分配贴图 Pixmap 失败"))?;
        file.read_exact(pixmap.data_mut())
            .context("读取贴图像素数据失败")?;
        return Ok(pixmap);
    }

    let bytes = snapin_foundation::read_file_limited(path, MAX_PASTER_IMAGE_FILE_BYTES)
        .context("读取外部贴图图像文件失败")?;
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .context("无法识别外部贴图图像格式")?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_PASTER_IMAGE_SIDE);
    limits.max_image_height = Some(MAX_PASTER_IMAGE_SIDE);
    limits.max_alloc = Some(snapin_storage::MAX_RAW_PIXELS.saturating_mul(4));
    reader.limits(limits);
    let img = reader.decode().context("无法解码外部贴图图像")?;
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

/// 通过常驻守护进程展示外部图像文件。
///
/// 图像解码与原生像素写入在阻塞线程池完成；守护进程通过受控运行时目录接收
/// 一次性像素文件，避免把整幅图像序列化进 IPC 帧。用户提供的源文件始终只读，
/// 只有本次新建的临时像素文件会在移交后回收。
///
/// # Errors
///
/// 目标文件不存在、图像解码失败、守护进程无法启动或拒绝创建贴图时返回错误。
pub async fn run_pin_file(path: PathBuf, x: i32, y: i32) -> Result<()> {
    if !path.exists() {
        tracing::warn!("贴图目标文件不存在，已忽略本次请求: {:?}", path);
        return Ok(());
    }
    crate::daemon_client::ensure_daemon_running().await?;

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

    let raw_temp_for_write = raw_temp.clone();
    let written = tokio::task::spawn_blocking(move || {
        AppPaths::write_secure_file(&raw_temp_for_write, pixmap.data()).is_ok()
    })
    .await
    .context("贴图临时像素写入任务调度失败")?;
    if !written {
        bail!("贴图像素临时文件写入失败，请检查运行时目录权限");
    }

    let request = snapin_daemon::DaemonRequest::CreatePaster {
        width,
        height,
        x,
        y,
        title,
        raw_path: raw_temp.to_string_lossy().to_string(),
    };
    if !matches!(
        snapin_daemon::send_command_to_daemon(&request).await,
        Ok(true)
    ) {
        let _ = tokio::fs::remove_file(&raw_temp).await;
        bail!("守护进程拒绝创建贴图，临时像素文件已清理");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    #[test]
    fn test_load_pixmap_rejects_oversized_dimensions() {
        let dir = tempfile::tempdir().expect("应可创建测试目录");
        let path = dir.path().join("too-tall.png");
        RgbaImage::from_pixel(1, MAX_PASTER_IMAGE_SIDE + 1, Rgba([0, 0, 0, 255]))
            .save(&path)
            .expect("应可写入测试图像");

        assert!(
            load_pixmap_from_file(&path).is_err(),
            "超大尺寸图像必须被拒绝"
        );
    }

    #[test]
    fn test_load_pixmap_rejects_oversized_file_before_decode() {
        let dir = tempfile::tempdir().expect("应可创建测试目录");
        let path = dir.path().join("too-large.png");
        let file = std::fs::File::create(&path).expect("应可创建测试文件");
        file.set_len((MAX_PASTER_IMAGE_FILE_BYTES as u64).saturating_add(1))
            .expect("应可扩展测试文件");

        assert!(
            load_pixmap_from_file(&path).is_err(),
            "超大源文件必须被拒绝"
        );
    }
}
