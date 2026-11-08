//! xcap 原生帧采集、选区裁剪与流式编码循环。

#[cfg(any(target_os = "windows", target_os = "macos"))]
use super::preview::{make_preview_frame, PreviewCollector};
#[cfg(any(target_os = "windows", target_os = "macos"))]
use super::recording_artifact;
#[cfg(any(target_os = "windows", target_os = "macos"))]
use super::timer::ActiveRecordingTimer;
use super::RecordingControl;
#[cfg(any(target_os = "windows", target_os = "macos"))]
use super::{create_encoder, EncoderDimensions};
#[cfg(any(target_os = "windows", target_os = "macos", test))]
use image::RgbaImage;
#[cfg(any(target_os = "windows", target_os = "macos"))]
use snapin_ports::RecordingArtifact;
use snapin_ports::{RecordingEvent, RecordingEventSink, RecordingOptions};
use std::path::PathBuf;
#[cfg(any(target_os = "windows", target_os = "macos"))]
use std::sync::atomic::Ordering;
use std::sync::Arc;
#[cfg(any(target_os = "windows", target_os = "macos"))]
use std::time::{Duration, Instant};
#[cfg(any(target_os = "windows", target_os = "macos"))]
use xcap::Monitor;

/// 一次录屏采集任务所需的共享参数。
pub(super) struct CaptureRequest {
    /// 录制区域、帧率与格式。
    pub(super) options: RecordingOptions,
    /// 已创建的目标文件。
    pub(super) file: std::fs::File,
    /// 最终输出路径。
    pub(super) path: PathBuf,
    /// 状态事件投递器。
    pub(super) sink: RecordingEventSink,
    /// 暂停、停止与取消控制。
    pub(super) control: Arc<RecordingControl>,
}

/// 单次录制最多等待的停止排空时间，防止驱动不关闭帧通道时阻塞收尾。
#[cfg(any(target_os = "windows", target_os = "macos"))]
const STOP_DRAIN_TIMEOUT: Duration = Duration::from_secs(1);
/// 进度事件最小推送间隔，兼顾界面秒表流畅度与事件循环负载。
#[cfg(any(target_os = "windows", target_os = "macos"))]
const PROGRESS_INTERVAL: Duration = Duration::from_millis(500);

/// Windows 与 macOS 原生帧流录制循环；函数返回时采集器和编码器均已释放。
#[cfg(any(target_os = "windows", target_os = "macos"))]
pub(super) fn capture_native_and_encode(
    request: CaptureRequest,
) -> Result<RecordingArtifact, String> {
    let CaptureRequest {
        options,
        file,
        path,
        sink,
        control,
    } = request;
    let monitor = select_monitor(options)?;
    let local = region_in_monitor(options, &monitor)?;
    let mut encoder = create_encoder(
        options.format,
        file,
        EncoderDimensions {
            width: options.region.width,
            height: options.region.height,
            fps: options.fps,
        },
    )?;
    let (recorder, receiver) = monitor
        .monitor
        .video_recorder()
        .map_err(|error| format!("初始化屏幕帧采集器失败: {error}"))?;
    recorder
        .start()
        .map_err(|error| format!("启动屏幕帧采集失败: {error}"))?;

    let started_at = Instant::now();
    sink(RecordingEvent::Started);
    emit_degradation_warnings(options, &sink);

    let frame_interval = Duration::from_secs_f64(1.0 / f64::from(options.fps.max(1)));
    let nominal_frame_ms = (1000_u32 / options.fps.max(1)).max(1);
    let mut last_frame_at: Option<Instant> = None;
    let mut active_timer = ActiveRecordingTimer::new(started_at);
    let mut last_progress_at = started_at;
    let mut frame_count = 0_u64;
    let mut preview = PreviewCollector::new();
    let mut result = Ok(());

    loop {
        if control.cancelled.load(Ordering::Acquire) {
            active_timer.pause(control.stop_requested_at().unwrap_or_else(Instant::now));
            result = Ok(());
            break;
        }
        if control.stop_requested.load(Ordering::Acquire) {
            active_timer.pause(control.stop_requested_at().unwrap_or_else(Instant::now));
            break;
        }

        let frame = match receiver.recv_timeout(Duration::from_millis(25)) {
            Ok(frame) => frame,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                result = Err("屏幕采集通道意外关闭".to_string());
                break;
            }
        };
        if control.paused.load(Ordering::Acquire) {
            active_timer.pause(control.pause_requested_at().unwrap_or_else(Instant::now));
            continue;
        }

        let now = Instant::now();
        active_timer.resume(now);
        if last_frame_at.is_some_and(|previous| now.duration_since(previous) < frame_interval) {
            continue;
        }
        let cropped = crop_frame(CropRequest {
            raw: &frame.raw,
            frame_size: (frame.width, frame.height),
            origin: local,
            size: (options.region.width, options.region.height),
        })?;
        let mut cropped = cropped;
        apply_overlay(&mut cropped, &control);
        let elapsed = active_timer.elapsed(now);
        if preview.sample_due(elapsed) {
            preview.record(make_preview_frame(&cropped), elapsed);
        }
        let frame_duration_ms = active_timer.frame_duration_ms(now, nominal_frame_ms);
        if let Err(message) = encoder.write_frame(&cropped, frame_duration_ms) {
            result = Err(message);
            break;
        }
        frame_count = frame_count.saturating_add(1);
        last_frame_at = Some(now);

        let progress_now = Instant::now();
        if progress_now.duration_since(last_progress_at) >= PROGRESS_INTERVAL {
            sink(RecordingEvent::Progress {
                elapsed: active_timer.elapsed(progress_now),
                frames: frame_count,
            });
            last_progress_at = progress_now;
        }
    }

    if let Err(error) = recorder.stop() {
        if result.is_ok() {
            result = Err(format!("停止屏幕帧采集失败: {error}"));
        }
    }
    drain_stopped_frames(&receiver);
    let final_duration = active_timer.elapsed(Instant::now());

    let cancelled = control.cancelled.load(Ordering::Acquire);
    if cancelled {
        drop(encoder);
        remove_partial_file(&path);
        return Ok(recording_artifact(path, final_duration, preview));
    }
    if let Err(message) = result {
        drop(encoder);
        remove_partial_file(&path);
        return Err(message);
    }
    if frame_count == 0 {
        drop(encoder);
        remove_partial_file(&path);
        return Err("没有采集到任何视频帧".to_string());
    }
    encoder.finish()?;
    Ok(recording_artifact(path, final_duration, preview))
}

/// 录制目标显示器及其虚拟桌面左上角原点。
#[cfg(any(target_os = "windows", target_os = "macos"))]
struct MonitorTarget {
    monitor: Monitor,
    desktop_x: i32,
    desktop_y: i32,
}

/// 选择选区中心所在的显示器。
///
/// 截图底图坐标从虚拟桌面联合边界归一化为 `(0, 0)`，而 xcap 显示器坐标保留
/// 全局有符号原点。这里先重建全局坐标，再按选区中心命中显示器，避免副屏位于
/// 主屏左侧或上方时录错屏幕。
#[cfg(any(target_os = "windows", target_os = "macos"))]
fn select_monitor(options: RecordingOptions) -> Result<MonitorTarget, String> {
    let monitors = Monitor::all().map_err(|error| format!("获取显示器列表失败: {error}"))?;
    let desktop_x = monitors
        .iter()
        .map(|monitor| {
            monitor
                .x()
                .map_err(|error| format!("读取显示器 X 坐标失败: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .min()
        .ok_or_else(|| "未检测到可用显示器".to_string())?;
    let desktop_y = monitors
        .iter()
        .map(|monitor| {
            monitor
                .y()
                .map_err(|error| format!("读取显示器 Y 坐标失败: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .min()
        .ok_or_else(|| "未检测到可用显示器".to_string())?;
    let center_x = options.region.x.saturating_add(options.region.width / 2);
    let center_y = options.region.y.saturating_add(options.region.height / 2);
    let global_x = i64::from(desktop_x) + i64::from(center_x);
    let global_y = i64::from(desktop_y) + i64::from(center_y);
    for monitor in monitors {
        let x = i64::from(
            monitor
                .x()
                .map_err(|error| format!("读取显示器 X 坐标失败: {error}"))?,
        );
        let y = i64::from(
            monitor
                .y()
                .map_err(|error| format!("读取显示器 Y 坐标失败: {error}"))?,
        );
        let width = i64::from(
            monitor
                .width()
                .map_err(|error| format!("读取显示器宽度失败: {error}"))?,
        );
        let height = i64::from(
            monitor
                .height()
                .map_err(|error| format!("读取显示器高度失败: {error}"))?,
        );
        if global_x >= x && global_x < x + width && global_y >= y && global_y < y + height {
            return Ok(MonitorTarget {
                monitor,
                desktop_x,
                desktop_y,
            });
        }
    }
    Err("录制区域中心不在任何显示器范围内".to_string())
}

/// 计算选区在显示器帧中的局部像素偏移并校验边界。
#[cfg(any(target_os = "windows", target_os = "macos"))]
fn region_in_monitor(
    options: RecordingOptions,
    target: &MonitorTarget,
) -> Result<(u32, u32), String> {
    let monitor = &target.monitor;
    let monitor_x = monitor
        .x()
        .map_err(|error| format!("读取显示器 X 坐标失败: {error}"))?;
    let monitor_y = monitor
        .y()
        .map_err(|error| format!("读取显示器 Y 坐标失败: {error}"))?;
    let monitor_width = monitor
        .width()
        .map_err(|error| format!("读取显示器宽度失败: {error}"))?;
    let monitor_height = monitor
        .height()
        .map_err(|error| format!("读取显示器高度失败: {error}"))?;
    let local_x = i64::from(target.desktop_x) + i64::from(options.region.x) - i64::from(monitor_x);
    let local_y = i64::from(target.desktop_y) + i64::from(options.region.y) - i64::from(monitor_y);
    let right = local_x + i64::from(options.region.width);
    let bottom = local_y + i64::from(options.region.height);
    if local_x < 0
        || local_y < 0
        || right > i64::from(monitor_width)
        || bottom > i64::from(monitor_height)
    {
        return Err(format!(
            "录制区域超出显示器范围，选区: ({}, {}) {}x{}，显示器: ({monitor_x}, {monitor_y}) {monitor_width}x{monitor_height}",
            options.region.x,
            options.region.y,
            options.region.width,
            options.region.height
        ));
    }
    let local_x = u32::try_from(local_x).map_err(|_| "录制区域局部 X 坐标非法".to_string())?;
    let local_y = u32::try_from(local_y).map_err(|_| "录制区域局部 Y 坐标非法".to_string())?;
    Ok((local_x, local_y))
}

/// 单帧裁剪参数。
#[cfg(any(target_os = "windows", target_os = "macos", test))]
struct CropRequest<'a> {
    raw: &'a [u8],
    frame_size: (u32, u32),
    origin: (u32, u32),
    size: (u32, u32),
}

/// 从整屏帧中裁剪 RGBA 选区，预分配目标缓冲区并逐行复制。
#[cfg(any(target_os = "windows", target_os = "macos", test))]
fn crop_frame(request: CropRequest<'_>) -> Result<RgbaImage, String> {
    let CropRequest {
        raw,
        frame_size: (width, height),
        origin: (x, y),
        size: (crop_width, crop_height),
    } = request;
    let frame_stride = usize::try_from(width)
        .ok()
        .and_then(|width| width.checked_mul(4))
        .ok_or_else(|| "屏幕帧行宽溢出".to_string())?;
    if raw.len() != frame_stride.saturating_mul(height as usize) {
        return Err(format!(
            "屏幕帧缓冲区长度异常，尺寸: {width}x{height}，实际: {} 字节",
            raw.len()
        ));
    }
    if x.saturating_add(crop_width) > width || y.saturating_add(crop_height) > height {
        return Err("裁剪区域超出屏幕帧边界".to_string());
    }
    let row_bytes = usize::try_from(crop_width)
        .ok()
        .and_then(|width| width.checked_mul(4))
        .ok_or_else(|| "录制帧行宽溢出".to_string())?;
    let capacity = row_bytes
        .checked_mul(crop_height as usize)
        .ok_or_else(|| "录制帧缓冲区尺寸溢出".to_string())?;
    let mut output = Vec::with_capacity(capacity);
    for row in y..y.saturating_add(crop_height) {
        let start = row as usize * frame_stride + x as usize * 4;
        let end = start + row_bytes;
        output.extend_from_slice(&raw[start..end]);
    }
    RgbaImage::from_raw(crop_width, crop_height, output)
        .ok_or_else(|| "构建裁剪后录制帧失败".to_string())
}

/// 排空停止命令之后已经进入通道的尾部帧。
#[cfg(any(target_os = "windows", target_os = "macos"))]
fn drain_stopped_frames(receiver: &std::sync::mpsc::Receiver<xcap::Frame>) {
    let deadline = Instant::now() + STOP_DRAIN_TIMEOUT;
    while Instant::now() < deadline {
        match receiver.recv_timeout(Duration::from_millis(50)) {
            Ok(_) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

/// 将 UI 标注透明层叠加到当前采集帧。
#[cfg(any(target_os = "windows", target_os = "macos", test))]
fn apply_overlay(frame: &mut RgbaImage, control: &RecordingControl) {
    let overlay = control
        .overlay
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(overlay) = overlay.as_ref() {
        if overlay.dimensions() == frame.dimensions() {
            image::imageops::overlay(frame, overlay, 0, 0);
        }
    }
}

pub(super) fn remove_partial_file(path: &std::path::Path) {
    if let Err(error) = std::fs::remove_file(path) {
        if error.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(
                "清理未完成录屏文件失败，路径: {}，原因: {error}",
                path.display()
            );
        }
    }
}

/// 明确告知用户当前原生采集通道的能力降级，避免开关看似生效但实际静默无效。
pub(super) fn emit_degradation_warnings(options: RecordingOptions, sink: &RecordingEventSink) {
    if options.include_system_audio {
        sink(RecordingEvent::Warning {
            message: "当前系统声音采集尚未接入，本次将录制无声视频".to_string(),
        });
    }
    if options.include_cursor || options.highlight_cursor {
        sink(RecordingEvent::Warning {
            message: "当前采集通道不保证包含鼠标指针，鼠标高亮将不写入视频".to_string(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{apply_overlay, crop_frame, CropRequest, RecordingControl};
    use image::{Rgba, RgbaImage};
    use std::sync::atomic::Ordering;

    #[test]
    fn test_crop_frame_extracts_requested_pixels() {
        let raw = vec![
            1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255, 10, 11, 12, 255, 13, 14, 15, 255, 16, 17, 18,
            255, 19, 20, 21, 255, 22, 23, 24, 255,
        ];
        let cropped = crop_frame(CropRequest {
            raw: &raw,
            frame_size: (4, 2),
            origin: (1, 0),
            size: (2, 2),
        })
        .expect("裁剪应成功");
        assert_eq!(cropped.dimensions(), (2, 2));
        assert_eq!(
            cropped.as_raw(),
            &[4, 5, 6, 255, 7, 8, 9, 255, 16, 17, 18, 255, 19, 20, 21, 255]
        );
    }

    #[test]
    fn test_apply_overlay_blends_transparent_annotation_layer() {
        let control = RecordingControl::default();
        *control
            .overlay
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) =
            Some(RgbaImage::from_pixel(2, 2, Rgba([255, 0, 0, 255])));
        let mut frame = RgbaImage::from_pixel(2, 2, Rgba([0, 0, 255, 255]));
        apply_overlay(&mut frame, &control);
        assert_eq!(frame.get_pixel(0, 0), &Rgba([255, 0, 0, 255]));
        assert!(!control.cancelled.load(Ordering::Acquire));
    }
}
