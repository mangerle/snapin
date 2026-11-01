//! Snapin 设置中心自更新交互控制器模块。
//!
//! 负责协调检查更新、下载进度跟踪、验签状态流转与安装重启逻辑。所有耗时操作
//! 统一提交到 [`snapin_runtime::TaskRuntime`]：检查与下载使用阻塞线程池，
//! 状态复位使用异步计时任务；窗口关闭时通过取消令牌停止后续 UI 回调。
//!
//! # 设计原理
//!
//! 旧实现每次点击都 `thread::spawn` 一个脱缰线程，连续点击会产生多组线程与
//! 竞态 UI 更新。改用统一运行时后，任务准入、panic 观测与退出等待由运行时负责，
//! 设置中心只保留“状态机 + UI 回调”。

use crate::SettingsWindow;
use shipup::{DownloadedUpdate, Update, UpdateEvent};
use slint::{ComponentHandle, SharedString};
use snapin_daemon::{send_request_to_daemon, DaemonRequest};
use snapin_platform::lifecycle::spawn_detached_system_command;
use snapin_runtime::{CancellationToken, TaskRuntime};
use std::sync::{Arc, Mutex};

/// 更新状态机定义（与 Slint about_tab.slint 中的 update_status 严格对齐）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateUiStatus {
    /// 闲置状态（尚未检查）
    Idle = 0,
    /// 正在检查远端清单
    Checking = 1,
    /// 已是最新版本
    UpToDate = 2,
    /// 发现新版本（待下载）
    Available = 3,
    /// 正在下载更新包
    Downloading = 4,
    /// 更新包已下载并验签完毕（待安装）
    Downloaded = 5,
    /// 检查或更新失败
    Error = 6,
}

/// 格式化下载速率与完成百分比
fn format_download_speed(speed: Option<u64>, percent: Option<f32>) -> String {
    let p_str = percent.map(|p| format!("{p:.1}%")).unwrap_or_default();
    match speed {
        Some(bytes) if bytes >= 1024 * 1024 => {
            format!("{:.2} MB/s ({})", bytes as f64 / (1024.0 * 1024.0), p_str)
        }
        Some(bytes) if bytes >= 1024 => {
            format!("{:.1} KB/s ({})", bytes as f64 / 1024.0, p_str)
        }
        Some(bytes) => format!("{bytes} B/s ({p_str})"),
        None => p_str,
    }
}

/// 自更新回调共享的运行时、取消令牌与状态容器。
#[derive(Clone)]
struct UpdateContext {
    runtime: TaskRuntime,
    cancel: CancellationToken,
    pending_update: Arc<Mutex<Option<Update>>>,
    downloaded_update: Arc<Mutex<Option<DownloadedUpdate>>>,
}

/// 注册自更新相关的界面回调与事件调度器。
///
/// `cancel` 在设置窗口关闭时触发，所有后台任务在更新 UI 前都会检查该令牌，
/// 避免窗口销毁后仍向 Slint 事件循环投递更新。
pub fn register_update_callbacks(
    win: &SettingsWindow,
    runtime: TaskRuntime,
    cancel: CancellationToken,
) {
    let context = UpdateContext {
        runtime,
        cancel,
        pending_update: Arc::new(Mutex::new(None::<Update>)),
        downloaded_update: Arc::new(Mutex::new(None::<DownloadedUpdate>)),
    };

    setup_check_callback(win, context.clone());
    setup_download_callback(win, context.clone());
    setup_install_callback(win, context);
}

/// 注册【检查更新】按钮回调
fn setup_check_callback(win: &SettingsWindow, context: UpdateContext) {
    let UpdateContext {
        runtime,
        cancel,
        pending_update,
        downloaded_update: _,
    } = context;
    let win_weak = win.as_weak();
    win.on_check_for_updates(move || {
        let Some(window) = win_weak.upgrade() else {
            return;
        };
        window.set_update_status(UpdateUiStatus::Checking as i32);
        window.set_update_status_text(SharedString::from("正在连接更新服务器..."));

        let win_weak_async = win_weak.clone();
        let update_holder = pending_update.clone();
        let task_cancel = cancel.clone();
        let reset_runtime = runtime.clone();
        let reset_cancel = cancel.clone();
        let spawn_result = runtime.spawn_blocking(move || {
            let result = snapin_core::check_update(None);
            if task_cancel.is_cancelled() {
                return;
            }
            let _ = slint::invoke_from_event_loop(move || {
                let Some(window) = win_weak_async.upgrade() else {
                    return;
                };
                match result {
                    Ok(Some(update)) => {
                        let version = update.version().to_string();
                        let notes = update.notes().unwrap_or("").to_string();
                        if let Ok(mut guard) = update_holder.lock() {
                            *guard = Some(update);
                        }
                        window.set_update_status(UpdateUiStatus::Available as i32);
                        window.set_update_version(SharedString::from(version));
                        window.set_update_notes(SharedString::from(notes));
                    }
                    Ok(None) => {
                        window.set_update_status(UpdateUiStatus::UpToDate as i32);
                        schedule_status_reset(
                            win_weak_async.clone(),
                            reset_runtime.clone(),
                            reset_cancel.clone(),
                            UpdateUiStatus::UpToDate,
                        );
                    }
                    Err(error) => {
                        tracing::warn!("用户主动检查更新失败: {error}");
                        window.set_update_status(UpdateUiStatus::Error as i32);
                        window.set_update_status_text(SharedString::from(format!(
                            "检查失败: {error}"
                        )));
                        schedule_status_reset(
                            win_weak_async.clone(),
                            reset_runtime.clone(),
                            reset_cancel.clone(),
                            UpdateUiStatus::Error,
                        );
                    }
                }
            });
        });
        if let Err(error) = spawn_result {
            window.set_update_status(UpdateUiStatus::Error as i32);
            window.set_update_status_text(SharedString::from(format!(
                "无法提交检查更新任务: {error}"
            )));
        }
    });
}

/// 安排一次状态复位；窗口关闭或任务取消后不再回调 UI。
fn schedule_status_reset(
    win_weak: slint::Weak<SettingsWindow>,
    runtime: TaskRuntime,
    cancel: CancellationToken,
    expected: UpdateUiStatus,
) {
    let win_weak = win_weak.clone();
    let spawn_result = runtime.spawn(async move {
        tokio::select! {
            _ = cancel.cancelled() => return,
            _ = tokio::time::sleep(std::time::Duration::from_secs(6)) => {}
        }
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(window) = win_weak.upgrade() {
                if window.get_update_status() == expected as i32 {
                    window.set_update_status(UpdateUiStatus::Idle as i32);
                    if expected == UpdateUiStatus::Error {
                        window.set_update_status_text(SharedString::from(""));
                    }
                }
            }
        });
    });
    if let Err(error) = spawn_result {
        tracing::debug!("提交更新状态复位任务失败: {error}");
    }
}

/// 注册【立即下载更新】按钮回调
fn setup_download_callback(win: &SettingsWindow, context: UpdateContext) {
    let UpdateContext {
        runtime,
        cancel,
        pending_update,
        downloaded_update: downloaded_holder,
    } = context;
    let win_weak = win.as_weak();
    win.on_start_download_update(move || {
        let Some(window) = win_weak.upgrade() else {
            return;
        };
        let update = match pending_update.lock() {
            Ok(guard) => guard.clone(),
            Err(_) => None,
        };
        let Some(update) = update else {
            window.set_update_status(UpdateUiStatus::Error as i32);
            window.set_update_status_text(SharedString::from("未找到待下载的更新实体"));
            return;
        };

        window.set_update_status(UpdateUiStatus::Downloading as i32);
        window.set_download_progress(0.0);
        window.set_download_speed_text(SharedString::from("准备下载..."));

        let win_weak_async = win_weak.clone();
        let downloaded_holder_clone = downloaded_holder.clone();
        let task_cancel = cancel.clone();
        let spawn_result = runtime.spawn_blocking(move || {
            let win_progress = win_weak_async.clone();
            let progress_cancel = task_cancel.clone();
            let download_res = update.download(move |event| {
                if progress_cancel.is_cancelled() {
                    return;
                }
                let win_clone = win_progress.clone();
                match event {
                    UpdateEvent::DownloadProgress {
                        percent,
                        speed_bytes_per_sec,
                        ..
                    } => {
                        let text = format_download_speed(speed_bytes_per_sec, percent);
                        let progress = percent.unwrap_or(0.0) / 100.0;
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(window) = win_clone.upgrade() {
                                window.set_download_progress(progress);
                                window.set_download_speed_text(SharedString::from(text));
                            }
                        });
                    }
                    UpdateEvent::VerifyingSignature => {
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(window) = win_clone.upgrade() {
                                window.set_download_speed_text(SharedString::from(
                                    "正在验证数字签名...",
                                ));
                            }
                        });
                    }
                    _ => {}
                }
            });

            if task_cancel.is_cancelled() {
                return;
            }
            let _ = slint::invoke_from_event_loop(move || {
                let Some(window) = win_weak_async.upgrade() else {
                    return;
                };
                match download_res {
                    Ok(downloaded) => {
                        if let Ok(mut guard) = downloaded_holder_clone.lock() {
                            *guard = Some(downloaded);
                        }
                        window.set_update_status(UpdateUiStatus::Downloaded as i32);
                        window.set_download_progress(1.0);
                        window.set_download_speed_text(SharedString::from("已就绪"));
                    }
                    Err(error) => {
                        tracing::error!("下载或验签更新包失败: {error}");
                        window.set_update_status(UpdateUiStatus::Error as i32);
                        window.set_update_status_text(SharedString::from(format!(
                            "下载失败: {error}"
                        )));
                    }
                }
            });
        });
        if let Err(error) = spawn_result {
            window.set_update_status(UpdateUiStatus::Error as i32);
            window.set_update_status_text(SharedString::from(format!("无法提交下载任务: {error}")));
        }
    });
}

/// 注册【立即重启并安装】按钮回调
fn setup_install_callback(win: &SettingsWindow, context: UpdateContext) {
    let UpdateContext {
        runtime,
        cancel,
        pending_update: _,
        downloaded_update: downloaded_holder,
    } = context;
    let win_weak = win.as_weak();
    win.on_restart_and_install(move || {
        let Some(window) = win_weak.upgrade() else {
            return;
        };
        let downloaded = match downloaded_holder.lock() {
            Ok(mut guard) => guard.take(),
            Err(_) => None,
        };
        let Some(downloaded) = downloaded else {
            window.set_update_status(UpdateUiStatus::Error as i32);
            window.set_update_status_text(SharedString::from("未找到已就绪的安装包"));
            return;
        };

        tracing::info!("准备应用更新并重启，目标版本: {}", downloaded.version());

        let win_thread = win_weak.clone();
        let install_cancel = cancel.clone();
        let spawn_result = runtime.spawn(async move {
            // 物理安装会阻塞，放入阻塞线程池；安装完成后回到异步上下文执行停机与重启。
            let installed = tokio::task::spawn_blocking(move || {
                downloaded.install(|event| {
                    tracing::info!("更新包物理落地事件: {event:?}");
                })
            })
            .await;

            match installed {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    tracing::error!("物理应用安装包失败: {error}");
                    if install_cancel.is_cancelled() {
                        return;
                    }
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(window) = win_thread.upgrade() {
                            window.set_update_status(UpdateUiStatus::Error as i32);
                            window.set_update_status_text(SharedString::from(format!(
                                "安装失败: {error}"
                            )));
                        }
                    });
                    return;
                }
                Err(error) => {
                    tracing::error!("安装任务异常: {error}");
                    if install_cancel.is_cancelled() {
                        return;
                    }
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(window) = win_thread.upgrade() {
                            window.set_update_status(UpdateUiStatus::Error as i32);
                            window.set_update_status_text(SharedString::from(
                                "安装任务异常，请查看运行日志",
                            ));
                        }
                    });
                    return;
                }
            }

            // 安装完成：停机旧后台守护进程并拉起新版；该流程不因设置窗口关闭而取消。
            let _ = send_request_to_daemon(&DaemonRequest::Shutdown).await;
            spawn_detached_system_command("snapin", &["daemon"]);
            let _ = slint::invoke_from_event_loop(|| {
                let _ = slint::quit_event_loop();
            });
        });
        if let Err(error) = spawn_result {
            window.set_update_status(UpdateUiStatus::Error as i32);
            window.set_update_status_text(SharedString::from(format!("无法提交安装任务: {error}")));
        }
    });
}
