//! 跨平台屏幕录制服务。
//!
//! # 设计原理
//!
//! 采集、编码与文件写入全部运行在统一后台运行时中，GUI 线程只发送控制指令并
//! 接收状态事件。每帧从原生屏幕流取得后立即裁剪、编码并写入容器，避免长时间
//! 录制把大量 RGBA 帧保存在内存中。
//!
//! GIF、WebP 与 MP4 使用同一套采集循环；编码器差异被 [`FrameEncoder`] 隔离。
//! 录制取消或失败时删除未完成文件，防止留下无法播放的半截媒体。暂停语义只停止
//! 采集和计时，不销毁原生采集器，因此恢复时无需重新申请系统权限。
//!
//! # 代价与权衡
//!
//! xcap 的原生帧流没有统一时间戳，帧间隔按目标帧率节流并写入固定帧时长；
//! 系统负载较高时可能略低于目标帧率，但不会因补帧而重复写入同一画面。
//! 当前系统声音采集尚未接入，开启该选项会明确回传降级提示而不是静默忽略。

mod capture;
mod encoder;

use crate::config::{config_snapshot, SharedConfig};
#[cfg(target_os = "linux")]
use capture::capture_linux_and_encode;
#[cfg(any(target_os = "windows", target_os = "macos"))]
use capture::capture_native_and_encode;
use capture::CaptureRequest;
use encoder::{open_encoder, EncoderInit, FrameEncoder};
use snapin_ports::{
    PortError, PortErrorKind, RecordingEvent, RecordingEventSink, RecordingFormat, RecordingHandle,
    RecordingOptions, ScreenRecorder,
};
use snapin_runtime::{TaskHandle, TaskPool, TaskRuntime};
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// 应用级屏幕录制服务。
pub(crate) struct RecordingService {
    config: SharedConfig,
    runtime: TaskRuntime,
}

impl RecordingService {
    /// 使用共享配置与统一后台运行时创建录制服务。
    pub(crate) fn new(config: SharedConfig, runtime: TaskRuntime) -> Self {
        Self { config, runtime }
    }
}

impl ScreenRecorder for RecordingService {
    fn start(
        &self,
        options: RecordingOptions,
        sink: RecordingEventSink,
    ) -> Result<Box<dyn RecordingHandle>, PortError> {
        let mut options = options.normalized_fps();
        options.region.width &= !1;
        options.region.height &= !1;
        if !options.region.is_valid() {
            return Err(PortError::with_kind(
                PortErrorKind::InvalidInput,
                "启动屏幕录制",
                format!(
                    "录制区域尺寸过小: {}x{}",
                    options.region.width, options.region.height
                ),
            ));
        }
        tracing::info!(
            "提交屏幕录制任务，格式: {}, 帧率: {}, 区域: ({}, {}) {}x{}",
            options.format.label(),
            options.fps,
            options.region.x,
            options.region.y,
            options.region.width,
            options.region.height
        );
        let control = Arc::new(RecordingControl::default());
        let task_control = control.clone();
        let worker_control = control.clone();
        let worker_sink = sink.clone();
        let task_sink = sink.clone();
        let config = config_snapshot(&self.config);
        let runtime = self.runtime.clone();
        let task = self
            .runtime
            .spawn_in(TaskPool::Background, async move {
                let result = async {
                    let (file, path) = runtime
                        .run_blocking_detached({
                            let output = config.output.clone();
                            let format = options.format;
                            move || create_recording_file(&output, format)
                        })
                        .await
                        .map_err(|error| format!("准备录屏输出文件失败: {error}"))??;
                    let request = CaptureRequest {
                        options,
                        file,
                        path: path.clone(),
                        sink: worker_sink,
                        control: worker_control,
                    };
                    #[cfg(any(target_os = "windows", target_os = "macos"))]
                    let result = runtime
                        .run_blocking_detached(move || capture_native_and_encode(request))
                        .await
                        .map_err(|error| format!("录屏后台任务异常: {error}"))?;
                    #[cfg(target_os = "linux")]
                    let result = capture_linux_and_encode(request, runtime.clone()).await;
                    if result.is_err() {
                        capture::remove_partial_file(&path);
                    }
                    match result {
                        Ok(path) if !task_control.cancelled.load(Ordering::Acquire) => {
                            tracing::info!("屏幕录制完成，文件路径: {}", path.display());
                            task_sink(RecordingEvent::Finished { path });
                            Ok(())
                        }
                        Ok(_) => {
                            tracing::info!("屏幕录制已取消，未生成最终文件");
                            task_sink(RecordingEvent::Cancelled);
                            Ok(())
                        }
                        Err(_message) if task_control.cancelled.load(Ordering::Acquire) => {
                            tracing::info!("屏幕录制已取消，未生成最终文件");
                            task_sink(RecordingEvent::Cancelled);
                            Ok(())
                        }
                        Err(message) => Err(message),
                    }
                }
                .await;
                task_control.finished.store(true, Ordering::Release);
                if let Err(message) = result {
                    tracing::warn!("屏幕录制失败: {message}");
                    task_sink(RecordingEvent::Failed { message });
                }
            })
            .map_err(|error| {
                PortError::with_kind(
                    PortErrorKind::Busy,
                    "启动屏幕录制",
                    format!("提交录屏后台任务失败: {error}"),
                )
            })?;
        Ok(Box::new(ApplicationRecordingHandle {
            control,
            sink,
            task,
        }))
    }
}

/// 录制控制状态。
#[derive(Default)]
struct RecordingControl {
    paused: AtomicBool,
    stop_requested: AtomicBool,
    cancelled: AtomicBool,
    finished: AtomicBool,
    overlay: Mutex<Option<image::RgbaImage>>,
}

/// 应用层录制控制器。
struct ApplicationRecordingHandle {
    control: Arc<RecordingControl>,
    sink: RecordingEventSink,
    task: TaskHandle,
}

impl RecordingHandle for ApplicationRecordingHandle {
    fn pause(&self) -> Result<(), PortError> {
        if self.control.finished.load(Ordering::Acquire) {
            return Err(recording_finished_error());
        }
        self.control.paused.store(true, Ordering::Release);
        (self.sink)(RecordingEvent::Paused);
        Ok(())
    }

    fn resume(&self) -> Result<(), PortError> {
        if self.control.finished.load(Ordering::Acquire) {
            return Err(recording_finished_error());
        }
        self.control.paused.store(false, Ordering::Release);
        (self.sink)(RecordingEvent::Resumed);
        Ok(())
    }

    fn stop(&self) -> Result<(), PortError> {
        if self.control.finished.load(Ordering::Acquire) {
            return Err(recording_finished_error());
        }
        self.control.stop_requested.store(true, Ordering::Release);
        Ok(())
    }

    fn update_overlay(&self, overlay: Option<image::RgbaImage>) {
        *self
            .control
            .overlay
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = overlay;
    }

    fn cancel(&self) {
        self.control.cancelled.store(true, Ordering::Release);
        self.control.stop_requested.store(true, Ordering::Release);
        if self.control.finished.load(Ordering::Acquire) {
            self.task.abort();
        }
    }
}

impl Drop for ApplicationRecordingHandle {
    fn drop(&mut self) {
        if !self.control.finished.load(Ordering::Acquire) {
            self.control.cancelled.store(true, Ordering::Release);
            self.control.stop_requested.store(true, Ordering::Release);
        }
    }
}

fn recording_finished_error() -> PortError {
    PortError::with_kind(
        PortErrorKind::Unavailable,
        "控制屏幕录制",
        "录制任务已经结束",
    )
}

/// 按输出配置创建唯一的录屏文件。
fn create_recording_file(
    output: &snapin_config::config::OutputConfig,
    format: RecordingFormat,
) -> Result<(File, PathBuf), String> {
    let directory = snapin_storage::ensure_output_directory(output)
        .map_err(|error| format!("创建录屏输出目录失败: {error}"))?;
    let suggestion = snapin_storage::suggest_save_path(output);
    let stem = Path::new(&suggestion.file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("snapin_recording");
    let extension = format.extension();
    for index in 0..10_000_u32 {
        let suffix = if index == 0 {
            String::new()
        } else {
            format!("_{index}")
        };
        let path = directory.join(format!("{stem}{suffix}.{extension}"));
        match OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(&path)
        {
            Ok(file) => return Ok((file, path)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "创建录屏文件失败，路径: {}，原因: {error}",
                    path.display()
                ));
            }
        }
    }
    Err(format!(
        "无法为录屏生成唯一文件名，目录: {}",
        directory.display()
    ))
}

/// 编码器初始化尺寸与帧率。
struct EncoderDimensions {
    width: u32,
    height: u32,
    fps: u32,
}

/// 创建编码器，供采集模块在首帧到达前完成初始化。
fn create_encoder(
    format: RecordingFormat,
    file: File,
    dimensions: EncoderDimensions,
) -> Result<Box<dyn FrameEncoder>, String> {
    open_encoder(EncoderInit {
        format,
        file,
        width: dimensions.width,
        height: dimensions.height,
        fps: dimensions.fps,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use snapin_config::config::OutputConfig;
    use tempfile::tempdir;

    #[test]
    fn test_create_recording_file_uses_requested_extension_and_unique_name() {
        let dir = tempdir().expect("临时目录应可创建");
        let output = OutputConfig {
            quick_save_dir: dir.path().display().to_string(),
            file_name_template: "recording".to_string(),
            ..OutputConfig::default()
        };
        let (first, first_path) =
            create_recording_file(&output, RecordingFormat::Mp4).expect("首个文件应创建");
        let (_second, second_path) =
            create_recording_file(&output, RecordingFormat::Mp4).expect("第二个文件应创建");
        assert_eq!(
            first_path.extension().and_then(|value| value.to_str()),
            Some("mp4")
        );
        assert_ne!(first_path, second_path);
        drop(first);
    }

    #[test]
    fn test_recording_options_clamps_fps() {
        let options = RecordingOptions {
            region: snapin_ports::RecordingRegion {
                x: 0,
                y: 0,
                width: 100,
                height: 100,
            },
            fps: 240,
            format: RecordingFormat::Mp4,
            include_cursor: false,
            include_system_audio: false,
            highlight_cursor: false,
        };
        assert_eq!(options.normalized_fps().fps, 60);
    }
}
