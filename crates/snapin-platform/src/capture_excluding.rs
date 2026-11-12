//! 排除指定覆盖层窗口的静默与选区局部抓屏驱动。
//!
//! # 设计原理
//!
//! 长截图连续滚动抓帧期间，全屏暗色遮罩与交互栏不能出现在捕获帧里。
//! Linux GNOME 扩展通道可在 Mutter 合成器内部按窗口标题临时设为透明一帧，并直接抓取
//! 对齐后的锁定选区边界，避免整屏 4K PNG 编码、拷贝与二次整屏裁剪；当遇到旧版扩展、
//! 异构缩放多屏或非标准小数缩放时自动无缝回退整屏排除捕获，兼顾极速与零回归。

#[cfg(not(target_os = "linux"))]
use crate::capture::capture_fullscreen_snapshot;
#[cfg(target_os = "linux")]
use crate::capture::{
    decode_extension_screenshot, read_extension_screenshot, TemporaryScreenshotFile,
};
use crate::error::PlatformError;
#[cfg(target_os = "linux")]
use image::GenericImageView;
use image::RgbaImage;

#[cfg(target_os = "linux")]
use crate::{
    capabilities::revalidate_gnome_extension_capability,
    extension_client::{
        capture_area_extension_excluding_title, capture_fullscreen_extension_excluding_title,
        ExtensionAreaCaptureRequest,
    },
};
#[cfg(target_os = "linux")]
use snapin_foundation::AppPaths;

/// 获取全屏快照，并在 Linux GNOME 扩展通道中排除指定标题的覆盖层窗口。
///
/// # Errors
///
/// 屏幕捕获、图像解码、尺寸校验或超时失败时返回 [`PlatformError`]。
pub async fn capture_fullscreen_snapshot_excluding_window(
    exclude_title: &str,
) -> Result<RgbaImage, PlatformError> {
    #[cfg(target_os = "linux")]
    {
        capture_linux_excluding_window(exclude_title).await
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = exclude_title;
        capture_fullscreen_snapshot(false).await
    }
}

/// 优先仅抓取指定物理选区（排除覆盖层窗口），不支持区域抓取时透明降级为全屏排除抓屏。
///
/// 调用方通过比对返回图像的宽高即可无感知区分“精确选区帧”与“降级全屏帧”。
///
/// # Errors
///
/// 屏幕捕获、图像解码、尺寸校验或超时失败时返回 [`PlatformError`]。
pub async fn capture_region_snapshot_excluding_window(
    exclude_title: &str,
    region: (u32, u32, u32, u32),
) -> Result<RgbaImage, PlatformError> {
    #[cfg(target_os = "linux")]
    {
        let caps = revalidate_gnome_extension_capability().await;
        if !caps.gnome_extension_active {
            return Err(PlatformError::Capture {
                message:
                    "Linux 录屏与长截图需要启用 GNOME 扩展 snapin@wangle，请重新安装或启用扩展"
                        .to_string(),
            });
        }
        if region.2 > 0 && region.3 > 0 {
            match capture_region_via_extension_excluding(exclude_title, region).await {
                Ok(Some(region_image)) => return Ok(region_image),
                Ok(None) => {
                    tracing::debug!("GNOME 扩展局部区域抓屏未命中精确对齐条件，回退全屏排除抓取");
                }
                Err(error) => {
                    tracing::debug!(
                        "GNOME 扩展局部区域抓屏未生效，回退全屏排除抓取，原因: {error}"
                    );
                }
            }
        }
        capture_via_extension_excluding(exclude_title)
            .await
            .map_err(|error| PlatformError::Capture {
                message: format!(
                    "Linux 录屏与长截图需要新版 GNOME 扩展 snapin@wangle，请更新扩展后重试，原因: {error}"
                ),
            })
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = region;
        capture_fullscreen_snapshot_excluding_window(exclude_title).await
    }
}

#[cfg(target_os = "linux")]
async fn capture_linux_excluding_window(exclude_title: &str) -> Result<RgbaImage, PlatformError> {
    let caps = revalidate_gnome_extension_capability().await;
    if !caps.gnome_extension_active {
        return Err(PlatformError::Capture {
            message: "Linux 录屏需要启用 GNOME 扩展 snapin@wangle，请重新安装或启用扩展"
                .to_string(),
        });
    }
    capture_via_extension_excluding(exclude_title)
        .await
        .map_err(|error| PlatformError::Capture {
            message: format!(
                "Linux 录屏需要新版 GNOME 扩展 snapin@wangle，请更新扩展后重试，原因: {error}"
            ),
        })
}

#[cfg(target_os = "linux")]
async fn capture_via_extension_excluding(exclude_title: &str) -> Result<RgbaImage, PlatformError> {
    let target_path =
        AppPaths::generate_temp_path("screen", "png").map_err(|source| PlatformError::Io {
            path: AppPaths::runtime_dir().display().to_string(),
            source,
        })?;
    let _cleanup = TemporaryScreenshotFile::new(target_path.clone());
    let target_str = target_path.to_string_lossy().to_string();
    let success =
        capture_fullscreen_extension_excluding_title(&target_str, exclude_title, false).await?;
    if !success {
        return Err(PlatformError::Capture {
            message: format!("GNOME 扩展排除覆盖层截屏失败，目标路径: {target_str}"),
        });
    }
    let file_bytes = read_extension_screenshot(target_path, target_str).await?;
    decode_extension_screenshot(file_bytes).await
}

#[cfg(target_os = "linux")]
async fn capture_region_via_extension_excluding(
    exclude_title: &str,
    region: (u32, u32, u32, u32),
) -> Result<Option<RgbaImage>, PlatformError> {
    let target_path =
        AppPaths::generate_temp_path("screen", "png").map_err(|source| PlatformError::Io {
            path: AppPaths::runtime_dir().display().to_string(),
            source,
        })?;
    let _cleanup = TemporaryScreenshotFile::new(target_path.clone());
    let target_str = target_path.to_string_lossy().to_string();
    let Some(offset) = capture_area_extension_excluding_title(ExtensionAreaCaptureRequest {
        target_path: &target_str,
        exclude_title,
        bounds: region,
        include_cursor: false,
    })
    .await?
    else {
        return Ok(None);
    };
    let file_bytes = read_extension_screenshot(target_path, target_str).await?;
    let decoded = decode_extension_screenshot(file_bytes).await?;
    Ok(extract_exact_region_from_captured(
        decoded,
        offset,
        (region.2, region.3),
    ))
}

/// 从扩展外扩对齐抓取的 PNG 中裁剪回精确物理尺寸；若尺寸不足则返回 `None` 触发全屏降级。
#[cfg(any(target_os = "linux", test))]
pub(crate) fn extract_exact_region_from_captured(
    decoded: RgbaImage,
    offset: (u32, u32),
    target_size: (u32, u32),
) -> Option<RgbaImage> {
    #[cfg(not(target_os = "linux"))]
    use image::GenericImageView;
    let (offset_x, offset_y) = offset;
    let (target_w, target_h) = target_size;
    if target_w == 0 || target_h == 0 {
        return None;
    }
    if offset_x == 0 && offset_y == 0 && decoded.dimensions() == (target_w, target_h) {
        return Some(decoded);
    }
    let end_x = offset_x.checked_add(target_w)?;
    let end_y = offset_y.checked_add(target_h)?;
    if end_x > decoded.width() || end_y > decoded.height() {
        tracing::debug!(
            "局部抓取 PNG 尺寸({}x{})无法覆盖目标裁剪区({}, {}, {}, {})，回退全屏抓取",
            decoded.width(),
            decoded.height(),
            offset_x,
            offset_y,
            target_w,
            target_h
        );
        return None;
    }
    Some(
        decoded
            .view(offset_x, offset_y, target_w, target_h)
            .to_image(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn test_extract_exact_region_passthrough_without_copy_when_exact_match() {
        let mut img = RgbaImage::new(120, 80);
        img.put_pixel(11, 22, Rgba([10, 20, 30, 255]));
        let extracted =
            extract_exact_region_from_captured(img, (0, 0), (120, 80)).expect("应零拷贝直接返回");
        assert_eq!(extracted.dimensions(), (120, 80));
        assert_eq!(*extracted.get_pixel(11, 22), Rgba([10, 20, 30, 255]));
    }

    #[test]
    fn test_extract_exact_region_crops_padded_alignment_border() {
        let mut padded = RgbaImage::from_pixel(128, 90, Rgba([0, 0, 0, 255]));
        padded.put_pixel(3, 5, Rgba([200, 100, 50, 255]));
        let extracted = extract_exact_region_from_captured(padded, (3, 5), (120, 80))
            .expect("应按对齐偏移精准裁出选区");
        assert_eq!(extracted.dimensions(), (120, 80));
        assert_eq!(*extracted.get_pixel(0, 0), Rgba([200, 100, 50, 255]));
    }

    #[test]
    fn test_extract_exact_region_rejects_undersized_or_overflow_bounds() {
        let small = RgbaImage::new(100, 80);
        assert!(extract_exact_region_from_captured(small, (2, 0), (100, 80)).is_none());
    }
}
