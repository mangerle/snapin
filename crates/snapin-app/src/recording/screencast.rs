//! Linux Mutter ScreenCast 连续帧采集。

use super::capture::{emit_degradation_warnings, CaptureRequest};
use super::{create_encoder, EncoderDimensions};
use snapin_ports::RecordingEvent;
use snapin_runtime::TaskRuntime;
use std::collections::VecDeque;
use std::path::PathBuf;
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
        let pipeline = format!(
            "pipewiresrc path={node_id} do-timestamp=true ! videoconvert ! videoscale ! videorate ! video/x-raw,format=RGBA,width={},height={},framerate={}/1 ! fdsink fd=1 sync=false",
            options.region.width, options.region.height, options.fps
        );
        let mut child = match Command::new("gst-launch-1.0")
            .args(["-q", pipeline.as_str()])
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
) -> Result<PathBuf, String> {
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
    let mut last_progress_at = Instant::now();
    let mut result = Ok(());

    loop {
        if control.cancelled.load(Ordering::Acquire)
            || control.stop_requested.load(Ordering::Acquire)
        {
            break;
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
        let nominal_frame_ms = (1000_u32 / options.fps.max(1)).max(1);
        let encoded = runtime
            .run_blocking_detached(move || {
                let mut guard = encode_target
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                let result = match guard.as_mut() {
                    Some(encoder) => encoder.write_frame(&image, nominal_frame_ms),
                    None => Err("录屏编码器已经关闭".to_string()),
                };
                (result, image.into_raw())
            })
            .await
            .map_err(|error| format!("录屏编码任务异常: {error}"));
        match encoded {
            Ok((Ok(()), raw)) => frame = raw,
            Ok((Err(message), _)) => {
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
                elapsed: Duration::from_millis(
                    frame_count.saturating_mul(u64::from(nominal_frame_ms)),
                ),
                frames: frame_count,
            });
            last_progress_at = now;
        }
    }

    let stderr = source.stop().await;
    if control.cancelled.load(Ordering::Acquire) {
        return Ok(path);
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
    Ok(path)
}
