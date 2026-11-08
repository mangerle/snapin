//! Linux Mutter ScreenCast 连续帧采集。

use super::capture::{emit_degradation_warnings, CaptureRequest};
use super::preview::{make_preview_frame, PreviewCollector};
use super::timer::ActiveRecordingTimer;
use super::{create_encoder, recording_artifact, EncoderDimensions};
use snapin_ports::{RecordingArtifact, RecordingEvent};
use snapin_runtime::TaskRuntime;
use std::collections::VecDeque;
use std::process::Stdio;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;
use tokio::process::{Child, ChildStdout, Command};
use tokio::task::JoinHandle;

const FRAME_READ_TIMEOUT: Duration = Duration::from_secs(8);
const PROGRESS_INTERVAL: Duration = Duration::from_millis(500);
const STDERR_LIMIT_BYTES: usize = 8192;

/// 将 Mutter 返回的 PipeWire 节点 ID 解析为 GStreamer 所需的 object.serial。
async fn resolve_pipewire_serial(node_id: u32) -> Result<u32, String> {
    let node = node_id.to_string();
    let mut last_error = format!("PipeWire 节点 {node_id} 缺少 object.serial 属性");
    for _ in 0..30 {
        let output = tokio::time::timeout(
            Duration::from_secs(2),
            Command::new("pw-cli")
                .args(["info", &node])
                .kill_on_drop(true)
                .output(),
        )
        .await
        .map_err(|_| format!("查询 PipeWire 节点 {node_id} 超时"))?
        .map_err(|error| format!("执行 pw-cli 查询 PipeWire 节点失败: {error}"))?;
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Some(serial) = stdout.lines().find_map(|line| {
                let (_, value) = line.split_once("object.serial = \"")?;
                value.split('"').next()?.parse::<u32>().ok()
            }) {
                return Ok(serial);
            }
        } else {
            last_error = format!(
                "pw-cli 查询 PipeWire 节点 {node_id} 失败: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(last_error)
}

/// 连续 PipeWire 录屏帧源。
struct GstCapture {
    child: Child,
    stdout: ChildStdout,
    stderr_task: Option<JoinHandle<Vec<u8>>>,
}

impl GstCapture {
    async fn start(options: snapin_ports::RecordingOptions) -> Result<Self, String> {
        let node_id = snapin_platform::start_recording_stream(
            options.region.x,
            options.region.y,
            options.region.width,
            options.region.height,
            options.include_cursor,
        )
        .await
        .map_err(|error| format!("创建 Mutter 连续录屏流失败: {error}"))?;
        let target_serial = match resolve_pipewire_serial(node_id).await {
            Ok(serial) => serial,
            Err(message) => {
                let _ = snapin_platform::stop_recording_stream().await;
                return Err(message);
            }
        };
        // Mutter 在发出节点号后才完成 PipeWire 参数协商；过早连接会一直等待首帧。
        tokio::time::sleep(Duration::from_millis(300)).await;
        let width = options.region.width.to_string();
        let height = options.region.height.to_string();
        let framerate = format!("{}/1", options.fps);
        let mut child = match Command::new("gst-launch-1.0")
            .arg("-q")
            .arg("pipewiresrc")
            .arg(format!("target-object={target_serial}"))
            .arg("do-timestamp=true")
            .arg(format!(
                "keepalive-time={}",
                (1000 / options.fps.max(1)).max(1)
            ))
            .arg("!")
            .arg("videoconvert")
            .arg("!")
            .arg("videoscale")
            .arg("!")
            .arg("videorate")
            .arg("!")
            .arg(format!(
                "video/x-raw,format=RGBA,width={width},height={height},framerate={framerate}"
            ))
            .arg("!")
            .arg("fdsink")
            .arg("fd=1")
            .arg("sync=false")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
        {
            Ok(child) => child,
            Err(error) => {
                let _ = snapin_platform::stop_recording_stream().await;
                return Err(format!("启动 PipeWire 帧读取进程失败: {error}"));
            }
        };
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "PipeWire 帧读取进程缺少标准输出".to_string())?;
        let stderr_task = child.stderr.take().map(drain_stderr);
        Ok(Self {
            child,
            stdout,
            stderr_task,
        })
    }

    async fn stop(&mut self) -> String {
        if let Err(error) = self.child.start_kill() {
            tracing::debug!("尝试结束 PipeWire 帧读取进程失败: {error}");
        }
        let _ = self.child.wait().await;
        let stderr = match self.stderr_task.take() {
            Some(task) => task.await.unwrap_or_default(),
            None => Vec::new(),
        };
        if let Err(error) = snapin_platform::stop_recording_stream().await {
            tracing::debug!("停止 Mutter 录屏流失败: {error}");
        }
        String::from_utf8_lossy(&stderr).trim().to_string()
    }

    fn take_stderr_task(&mut self) -> Option<JoinHandle<Vec<u8>>> {
        self.stderr_task.take()
    }

    async fn read_frame(&mut self, frame: &mut [u8]) -> Result<bool, String> {
        let mut filled = 0;
        while filled < frame.len() {
            if self.child.try_wait().ok().flatten().is_some() {
                return Err(format!(
                    "PipeWire 帧读取进程提前退出: {}",
                    self.take_stderr().await
                ));
            }
            let read =
                tokio::time::timeout(FRAME_READ_TIMEOUT, self.stdout.read(&mut frame[filled..]))
                    .await
                    .map_err(|_| "等待 PipeWire 视频帧超时".to_string())?
                    .map_err(|error| format!("读取 PipeWire 视频帧失败: {error}"))?;
            if read == 0 {
                return Ok(false);
            }
            filled += read;
        }
        Ok(true)
    }

    async fn take_stderr(&mut self) -> String {
        match self.take_stderr_task() {
            Some(task) => {
                let bytes = task.await.unwrap_or_default();
                String::from_utf8_lossy(&bytes).trim().to_string()
            }
            None => String::new(),
        }
    }
}

fn drain_stderr(mut stderr: tokio::process::ChildStderr) -> JoinHandle<Vec<u8>> {
    tokio::spawn(async move {
        let mut retained = VecDeque::with_capacity(STDERR_LIMIT_BYTES);
        let mut chunk = [0_u8; 1024];
        loop {
            match stderr.read(&mut chunk).await {
                Ok(0) | Err(_) => break,
                Ok(read) => {
                    for byte in &chunk[..read] {
                        if retained.len() == STDERR_LIMIT_BYTES {
                            retained.pop_front();
                        }
                        retained.push_back(*byte);
                    }
                }
            }
        }
        retained.into_iter().collect()
    })
}

/// 使用 Mutter ScreenCast 连续帧流进行 Linux 录屏。
pub(super) async fn capture_screencast_and_encode(
    request: CaptureRequest,
    runtime: TaskRuntime,
) -> Result<RecordingArtifact, String> {
    let CaptureRequest {
        options,
        file,
        path,
        sink,
        control,
    } = request;
    let encoder = runtime
        .run_blocking_detached(move || {
            create_encoder(
                options.format,
                file,
                EncoderDimensions {
                    width: options.region.width,
                    height: options.region.height,
                    fps: options.fps,
                },
            )
        })
        .await
        .map_err(|error| format!("初始化录屏编码器失败: {error}"))??;
    let encoder = Arc::new(Mutex::new(Some(encoder)));
    let mut source = GstCapture::start(options).await?;
    let started_at = Instant::now();
    sink(RecordingEvent::Started);
    emit_degradation_warnings(options, &sink);

    let frame_size = (options.region.width as usize)
        .saturating_mul(options.region.height as usize)
        .saturating_mul(4);
    if frame_size == 0 {
        let _ = source.stop().await;
        return Err("录屏帧尺寸无效".to_string());
    }
    let mut frame = vec![0_u8; frame_size];
    let mut frame_count = 0_u64;
    let mut preview = PreviewCollector::new();
    let mut active_timer = ActiveRecordingTimer::new(started_at);
    let mut last_progress_at = started_at;
    let mut result = Ok(());
    let nominal_frame_ms = (1000_u32 / options.fps.max(1)).max(1);

    loop {
        if control.cancelled.load(Ordering::Acquire)
            || control.stop_requested.load(Ordering::Acquire)
        {
            active_timer.pause(control.stop_requested_at().unwrap_or_else(Instant::now));
            break;
        }
        let loop_now = Instant::now();
        if control.paused.load(Ordering::Acquire) {
            active_timer.pause(control.pause_requested_at().unwrap_or_else(Instant::now));
        } else {
            active_timer.resume(loop_now);
        }
        match source.read_frame(&mut frame).await {
            Ok(true) => {}
            Ok(false) => {
                result = Err("PipeWire 视频流已经结束".to_string());
                break;
            }
            Err(message) => {
                result = Err(message);
                break;
            }
        }
        if control.stop_requested.load(Ordering::Acquire) {
            active_timer.pause(control.stop_requested_at().unwrap_or_else(Instant::now));
            break;
        }
        if control.paused.load(Ordering::Acquire) {
            continue;
        }
        let image = image::RgbaImage::from_raw(options.region.width, options.region.height, frame)
            .ok_or_else(|| "PipeWire 帧缓冲区转换为 RGBA 图像失败".to_string());
        let image = match image {
            Ok(image) => image,
            Err(message) => {
                result = Err(message);
                break;
            }
        };
        let encode_target = Arc::clone(&encoder);
        let elapsed = active_timer.elapsed(Instant::now());
        let preview_due = preview.sample_due(elapsed);
        let frame_duration_ms = active_timer.frame_duration_ms(Instant::now(), nominal_frame_ms);
        let encoded = runtime
            .run_blocking_detached(move || {
                let preview_frame = preview_due.then(|| make_preview_frame(&image));
                let mut guard = encode_target
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                let result = match guard.as_mut() {
                    Some(encoder) => encoder.write_frame(&image, frame_duration_ms),
                    None => Err("录屏编码器已经关闭".to_string()),
                };
                (result, image.into_raw(), preview_frame)
            })
            .await
            .map_err(|error| format!("录屏编码任务异常: {error}"));
        match encoded {
            Ok((Ok(()), raw, preview_frame)) => {
                frame = raw;
                if let Some(preview_frame) = preview_frame {
                    preview.record(preview_frame, elapsed);
                }
            }
            Ok((Err(message), _, _)) => {
                result = Err(message);
                break;
            }
            Err(message) => {
                result = Err(message);
                break;
            }
        }
        frame_count = frame_count.saturating_add(1);
        let now = Instant::now();
        if now.duration_since(last_progress_at) >= PROGRESS_INTERVAL {
            sink(RecordingEvent::Progress {
                elapsed: active_timer.elapsed(now),
                frames: frame_count,
            });
            last_progress_at = now;
        }
    }

    let stderr = source.stop().await;
    let final_duration = active_timer.elapsed(Instant::now());
    if control.cancelled.load(Ordering::Acquire) {
        return Ok(recording_artifact(path, final_duration, preview));
    }
    if let Err(message) = result {
        return Err(if stderr.is_empty() {
            message
        } else {
            format!("{message}，GStreamer: {stderr}")
        });
    }
    if frame_count == 0 {
        return Err("没有采集到任何视频帧".to_string());
    }
    runtime
        .run_blocking_detached(move || {
            encoder
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .take()
                .ok_or_else(|| "录屏编码器已经关闭".to_string())?
                .finish()
        })
        .await
        .map_err(|error| format!("录屏编码收尾任务异常: {error}"))??;
    Ok(recording_artifact(path, final_duration, preview))
}
