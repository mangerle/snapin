//! 排除指定覆盖层窗口的静默与选区局部抓屏驱动。
//!
//! # 设计原理
//!
//! - **Linux GNOME 扩展**：优先调用 `CaptureAreaExcludingTitle` 在 Mutter 合成器内按窗口标题
//!   透明隐藏并直接编码对齐后的锁定选区 PNG 至运行时 `tmpfs`；旧扩展或非均匀多屏自动回退全屏；
//! - **Linux 非 GNOME 降级**：覆盖层已按 `long_capture_requires_window_hide() == true` 隐藏，
//!   Wayland wlroots/Sway/Hyprland 优先走 `grim` 选区 stdout 管道直出，X11 尝试 `maim` 选区直出，
//!   其余环境回退非交互式 XDG Portal 并本地裁切选区；
//! - **Windows / macOS**：按虚拟桌面物理坐标锁定与选区相交的物理显示器，直接调用
//!   `xcap::Monitor::capture_region` 仅抓取选区局部显存/表面（macOS Retina 自动换算点/物理比例），
//!   规避每滚动一帧都全量抓取多块 4K 显示器的开销。

#[cfg(any(target_os = "windows", target_os = "macos"))]
use crate::capture::{
    capture_fullscreen_snapshot, capture_monitor_physical_geometry, DesktopBounds, CAPTURE_TIMEOUT,
};
#[cfg(target_os = "linux")]
use crate::capture::{
    decode_extension_screenshot, read_extension_screenshot, TemporaryScreenshotFile,
};
#[cfg(target_os = "linux")]
use crate::command::output_with_timeout;
use crate::error::PlatformError;
#[cfg(any(target_os = "linux", target_os = "macos", test))]
use image::GenericImageView;
use image::RgbaImage;

#[cfg(target_os = "linux")]
use crate::{
    capabilities::revalidate_gnome_extension_capability,
    extension_client::{
        capture_area_extension_excluding_title, capture_fullscreen_extension_excluding_title,
        ExtensionAreaCaptureRequest,
    },
    portal::capture_fullscreen_portal,
};
#[cfg(target_os = "linux")]
use snapin_foundation::AppPaths;
#[cfg(target_os = "linux")]
use std::time::Duration;

/// 外部选区抓屏工具（`grim` / `maim`）的超时上限。
#[cfg(target_os = "linux")]
const CLI_REGION_CAPTURE_TIMEOUT: Duration = Duration::from_secs(3);

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

/// 优先仅抓取指定物理选区（排除覆盖层窗口），不支持区域抓取时透明降级为全屏抓屏。
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
        capture_linux_region_excluding_window(exclude_title, region).await
    }

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        let _ = exclude_title;
        if region.2 > 0 && region.3 > 0 {
            match tokio::time::timeout(
                CAPTURE_TIMEOUT,
                tokio::task::spawn_blocking(move || capture_xcap_region(region)),
            )
            .await
            {
                Ok(Ok(Ok(Some(img)))) => return Ok(img),
                Ok(Ok(Ok(None))) => {
                    tracing::debug!("xcap 选区区域抓取未匹配单屏/多屏交集条件，回退全屏抓取");
                }
                Ok(Ok(Err(err))) => {
                    tracing::debug!("xcap 选区局部抓屏失败，回退全屏抓取，原因: {err}");
                }
                Ok(Err(join_err)) => {
                    tracing::debug!("xcap 选区抓屏线程异常，回退全屏抓取: {join_err}");
                }
                Err(_) => {
                    tracing::debug!("xcap 选区局部抓屏超时，回退全屏抓取");
                }
            }
        }
        capture_fullscreen_snapshot(false).await
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        let _ = (exclude_title, region);
        Err(PlatformError::Unsupported {
            capability: "长截图抓屏".to_string(),
        })
    }
}

#[cfg(target_os = "linux")]
async fn capture_linux_region_excluding_window(
    exclude_title: &str,
    region: (u32, u32, u32, u32),
) -> Result<RgbaImage, PlatformError> {
    let caps = revalidate_gnome_extension_capability().await;
    if caps.gnome_extension_active {
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
        return capture_via_extension_excluding(exclude_title)
            .await
            .map_err(|error| PlatformError::Capture {
                message: format!(
                    "Linux 录屏与长截图需要新版 GNOME 扩展 snapin@wangle，请更新扩展后重试，原因: {error}"
                ),
            });
    }

    capture_linux_non_gnome_fallback(caps.is_wayland, &caps.desktop_env, Some(region)).await
}

#[cfg(target_os = "linux")]
async fn capture_linux_excluding_window(exclude_title: &str) -> Result<RgbaImage, PlatformError> {
    let caps = revalidate_gnome_extension_capability().await;
    if caps.gnome_extension_active {
        return capture_via_extension_excluding(exclude_title)
            .await
            .map_err(|error| PlatformError::Capture {
                message: format!(
                    "Linux 录屏需要新版 GNOME 扩展 snapin@wangle，请更新扩展后重试，原因: {error}"
                ),
            });
    }

    capture_linux_non_gnome_fallback(caps.is_wayland, &caps.desktop_env, None).await
}

/// 在未启用 GNOME 扩展的 X11 或非 GNOME Wayland 桌面执行隐藏窗口后的静音抓屏兜底。
#[cfg(target_os = "linux")]
async fn capture_linux_non_gnome_fallback(
    is_wayland: bool,
    desktop_env: &str,
    region: Option<(u32, u32, u32, u32)>,
) -> Result<RgbaImage, PlatformError> {
    if is_wayland && is_gnome_desktop(desktop_env) {
        return Err(PlatformError::Capture {
            message: "GNOME Wayland 长截图需要启用扩展 snapin@wangle 以避免每帧白闪".to_string(),
        });
    }

    if let Some((x, y, width, height)) = region.filter(|&(_, _, w, h)| w > 0 && h > 0) {
        let cli_bytes = tokio::task::spawn_blocking(move || {
            try_capture_linux_cli_region(is_wayland, (x, y, width, height))
        })
        .await
        .ok()
        .flatten();
        if let Some(png_bytes) = cli_bytes {
            if let Ok(image) = decode_extension_screenshot(png_bytes).await {
                if image.dimensions() == (width, height) {
                    return Ok(image);
                }
            }
        }
    }

    let full = capture_fullscreen_portal().await?;
    if let Some((x, y, width, height)) = region.filter(|&(_, _, w, h)| w > 0 && h > 0) {
        if let Some(cropped) =
            extract_exact_region_from_captured(full.clone(), (x, y), (width, height))
        {
            return Ok(cropped);
        }
    }
    Ok(full)
}

#[cfg(any(target_os = "linux", test))]
fn is_gnome_desktop(desktop_env: &str) -> bool {
    let lower = desktop_env.to_ascii_lowercase();
    lower.contains("gnome") || lower.contains("ubuntu")
}

#[cfg(target_os = "linux")]
fn try_capture_linux_cli_region(is_wayland: bool, region: (u32, u32, u32, u32)) -> Option<Vec<u8>> {
    use std::process::Command;

    let (x, y, width, height) = region;
    if is_wayland {
        let mut grim = Command::new("grim");
        grim.args(["-g", &format!("{x},{y} {width}x{height}"), "-"]);
        if let Ok(output) = output_with_timeout(grim, CLI_REGION_CAPTURE_TIMEOUT) {
            if output.status.success() && !output.stdout.is_empty() {
                return Some(output.stdout);
            }
        }
        return None;
    }

    let mut maim = Command::new("maim");
    maim.args(["-g", &format!("{width}x{height}+{x}+{y}")]);
    if let Ok(output) = output_with_timeout(maim, CLI_REGION_CAPTURE_TIMEOUT) {
        if output.status.success() && !output.stdout.is_empty() {
            return Some(output.stdout);
        }
    }
    None
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

/// 在 Windows / macOS 上按虚拟桌面物理坐标系仅抓取与选区相交的显示器局部像素。
#[cfg(any(target_os = "windows", target_os = "macos"))]
fn capture_xcap_region(region: (u32, u32, u32, u32)) -> Result<Option<RgbaImage>, PlatformError> {
    let monitors = xcap::Monitor::all().map_err(|error| PlatformError::Capture {
        message: format!("获取物理显示器列表失败: {error}"),
    })?;
    if monitors.is_empty() {
        return Ok(None);
    }

    let mut bounds = DesktopBounds::default();
    let mut areas = Vec::with_capacity(monitors.len());
    for monitor in &monitors {
        let area = capture_monitor_physical_geometry(monitor)?;
        bounds.include(area)?;
        areas.push(area);
    }
    let origin = bounds.origin()?;
    let target = (
        i64::from(origin.0) + i64::from(region.0),
        i64::from(origin.1) + i64::from(region.1),
        region.2,
        region.3,
    );

    let mut hits = Vec::with_capacity(monitors.len());
    for (monitor, area) in monitors.iter().zip(areas.iter()) {
        if let Some(intersection) = compute_monitor_region_intersection(
            (
                i64::from(area.x),
                i64::from(area.y),
                area.width,
                area.height,
            ),
            target,
        ) {
            hits.push((monitor, intersection));
        }
    }
    if hits.is_empty() {
        return Ok(None);
    }
    if hits.len() == 1 && hits[0].1.dest_size == (region.2, region.3) {
        return capture_single_monitor_subregion(hits[0].0, hits[0].1);
    }

    let mut canvas = RgbaImage::new(region.2, region.3);
    for (monitor, part) in hits {
        let Some(tile) = capture_single_monitor_subregion(monitor, part)? else {
            return Ok(None);
        };
        image::imageops::overlay(
            &mut canvas,
            &tile,
            i64::from(part.dest_offset.0),
            i64::from(part.dest_offset.1),
        );
    }
    Ok(Some(canvas))
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn capture_single_monitor_subregion(
    monitor: &xcap::Monitor,
    part: MonitorRegionSlice,
) -> Result<Option<RgbaImage>, PlatformError> {
    let (lx, ly) = part.local_origin;
    let (lw, lh) = part.dest_size;

    #[cfg(target_os = "windows")]
    {
        let mut image =
            monitor
                .capture_region(lx, ly, lw, lh)
                .map_err(|error| PlatformError::Capture {
                    message: format!("Windows 局部选区抓屏失败: {error}"),
                })?;
        crate::capture::normalize_windows_capture_alpha(&mut image);
        Ok((image.dimensions() == (lw, lh)).then_some(image))
    }

    #[cfg(target_os = "macos")]
    {
        let scale = f64::from(monitor.scale_factor().unwrap_or(1.0).max(1.0));
        let pt_max_w = monitor.width().unwrap_or(0);
        let pt_max_h = monitor.height().unwrap_or(0);
        let Some((pt_x, pt_y, pt_w, pt_h, dx, dy)) =
            map_physical_region_to_scaled_points((lx, ly, lw, lh), scale, (pt_max_w, pt_max_h))
        else {
            return Ok(None);
        };
        let image = monitor
            .capture_region(pt_x, pt_y, pt_w, pt_h)
            .map_err(|error| PlatformError::Capture {
                message: format!("macOS 局部选区抓屏失败: {error}"),
            })?;
        Ok(extract_exact_region_from_captured(
            image,
            (dx, dy),
            (lw, lh),
        ))
    }
}

/// 单个显示器局部抓屏交集映射结果。
#[cfg(any(target_os = "windows", target_os = "macos", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MonitorRegionSlice {
    /// 显示器内部物理左上角 `(lx, ly)`
    pub(crate) local_origin: (u32, u32),
    /// 写入目标画布的左上角偏移 `(dx, dy)`
    pub(crate) dest_offset: (u32, u32),
    /// 交集矩形宽高 `(w, h)`
    pub(crate) dest_size: (u32, u32),
}

/// 计算物理显示器与目标物理选区的交集切片；无重叠返回 `None`。
#[cfg(any(target_os = "windows", target_os = "macos", test))]
pub(crate) fn compute_monitor_region_intersection(
    monitor: (i64, i64, u32, u32),
    target: (i64, i64, u32, u32),
) -> Option<MonitorRegionSlice> {
    let (mx, my, mw, mh) = monitor;
    let (tx, ty, tw, th) = target;
    let left = mx.max(tx);
    let top = my.max(ty);
    let right = (mx + i64::from(mw)).min(tx + i64::from(tw));
    let bottom = (my + i64::from(mh)).min(ty + i64::from(th));
    if right <= left || bottom <= top {
        return None;
    }
    Some(MonitorRegionSlice {
        local_origin: (
            u32::try_from(left - mx).ok()?,
            u32::try_from(top - my).ok()?,
        ),
        dest_offset: (
            u32::try_from(left - tx).ok()?,
            u32::try_from(top - ty).ok()?,
        ),
        dest_size: (
            u32::try_from(right - left).ok()?,
            u32::try_from(bottom - top).ok()?,
        ),
    })
}

/// 将显示器内局部物理矩形向外取整映射到 macOS 逻辑点区域，并返回返回图内部精确裁剪偏移。
#[cfg(any(target_os = "macos", test))]
pub(crate) fn map_physical_region_to_scaled_points(
    local_phys: (u32, u32, u32, u32),
    scale: f64,
    monitor_point_size: (u32, u32),
) -> Option<(u32, u32, u32, u32, u32, u32)> {
    let (lx, ly, lw, lh) = local_phys;
    let (max_pt_w, max_pt_h) = monitor_point_size;
    if !scale.is_finite() || scale <= 0.0 || lw == 0 || lh == 0 || max_pt_w == 0 || max_pt_h == 0 {
        return None;
    }
    let pt_x = ((f64::from(lx) / scale).floor() as u32).min(max_pt_w.saturating_sub(1));
    let pt_y = ((f64::from(ly) / scale).floor() as u32).min(max_pt_h.saturating_sub(1));
    let pt_right = ((f64::from(lx + lw) / scale).ceil() as u32).min(max_pt_w);
    let pt_bottom = ((f64::from(ly + lh) / scale).ceil() as u32).min(max_pt_h);
    let pt_w = pt_right.checked_sub(pt_x).filter(|&v| v > 0)?;
    let pt_h = pt_bottom.checked_sub(pt_y).filter(|&v| v > 0)?;
    let dx = lx.saturating_sub((f64::from(pt_x) * scale).round() as u32);
    let dy = ly.saturating_sub((f64::from(pt_y) * scale).round() as u32);
    Some((pt_x, pt_y, pt_w, pt_h, dx, dy))
}

/// 从外扩对齐抓取的结果图中裁剪回精确物理尺寸；尺寸不足则返回 `None` 触发全屏降级。
#[cfg(any(target_os = "linux", target_os = "macos", test))]
pub(crate) fn extract_exact_region_from_captured(
    decoded: RgbaImage,
    offset: (u32, u32),
    target_size: (u32, u32),
) -> Option<RgbaImage> {
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
            "局部抓取图像尺寸({}x{})无法覆盖目标裁剪区({}, {}, {}, {})，回退全屏抓取",
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

    #[test]
    fn test_compute_monitor_region_intersection_handles_negative_and_spanning_monitors() {
        let mon_left = (-1920, 0, 1920, 1080);
        let mon_right = (0, 0, 2560, 1440);
        let spanning = (-100, 200, 300, 400);

        assert_eq!(
            compute_monitor_region_intersection(mon_left, spanning),
            Some(MonitorRegionSlice {
                local_origin: (1820, 200),
                dest_offset: (0, 0),
                dest_size: (100, 400),
            })
        );
        assert_eq!(
            compute_monitor_region_intersection(mon_right, spanning),
            Some(MonitorRegionSlice {
                local_origin: (0, 200),
                dest_offset: (100, 0),
                dest_size: (200, 400),
            })
        );
    }

    #[test]
    fn test_map_physical_region_to_scaled_points_handles_retina_odd_bounds() {
        // Retina 2x 下 (101+301=402) 整除边界为 [50..201] => 151pt(302px) 恰好完整覆盖 [1..302]
        assert_eq!(
            map_physical_region_to_scaled_points((101, 203, 301, 401), 2.0, (1440, 900)),
            Some((50, 101, 151, 201, 1, 1))
        );
        // 奇数起点 + 偶数宽高导致右/下边界落在半像素处，应向上多囊括 1pt
        assert_eq!(
            map_physical_region_to_scaled_points((101, 203, 300, 400), 2.0, (1440, 900)),
            Some((50, 101, 151, 201, 1, 1))
        );
    }

    #[test]
    fn test_is_gnome_desktop_matches_gnome_variants() {
        assert!(is_gnome_desktop("ubuntu:GNOME"));
        assert!(is_gnome_desktop("GNOME"));
        assert!(!is_gnome_desktop("KDE"));
        assert!(!is_gnome_desktop("sway"));
    }
}
