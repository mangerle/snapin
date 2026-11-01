//! Snapin 设置中心自更新交互控制器模块。
//!
//! 负责协调检查更新、下载进度跟踪、验签状态流转与安装重启逻辑，
//! 并安全跨线程调度更新 Slint 声明式 UI 控件属性。

use crate::SettingsWindow;
use shipup::{DownloadedUpdate, Update, UpdateEvent};
use slint::{ComponentHandle, SharedString};
use std::sync::{Arc, Mutex};
use std::thread;

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

/// 注册自更新相关的界面回调与事件调度器
pub fn register_update_callbacks(win: &SettingsWindow) {
    let pending_update = Arc::new(Mutex::new(None::<Update>));
    let downloaded_update = Arc::new(Mutex::new(None::<DownloadedUpdate>));

    setup_check_callback(win, pending_update.clone());
    setup_download_callback(win, pending_update, downloaded_update.clone());
    setup_install_callback(win, downloaded_update);
}

/// 注册【检查更新】按钮回调
fn setup_check_callback(win: &SettingsWindow, pending_update: Arc<Mutex<Option<Update>>>) {
    let win_weak = win.as_weak();
    win.on_check_for_updates(move || {
        let Some(window) = win_weak.upgrade() else {
            return;
        };
        window.set_update_status(UpdateUiStatus::Checking as i32);
        window.set_update_status_text(SharedString::from("正在连接更新服务器..."));

        let win_weak_async = win_weak.clone();
        let update_holder = pending_update.clone();

        thread::spawn(move || {
            let result = snapin_core::check_update(None);
            let _ = slint::invoke_from_event_loop(move || {
                let Some(w) = win_weak_async.upgrade() else {
                    return;
                };
                match result {
                    Ok(Some(update)) => {
                        let version = update.version().to_string();
                        let notes = update.notes().unwrap_or("").to_string();
                        if let Ok(mut guard) = update_holder.lock() {
                            *guard = Some(update);
                        }
                        w.set_update_status(UpdateUiStatus::Available as i32);
                        w.set_update_version(SharedString::from(version));
                        w.set_update_notes(SharedString::from(notes));
                    }
                    Ok(None) => {
                        w.set_update_status(UpdateUiStatus::UpToDate as i32);
                    }
                    Err(error) => {
                        tracing::warn!("用户主动检查更新失败: {error}");
                        w.set_update_status(UpdateUiStatus::Error as i32);
                        w.set_update_status_text(SharedString::from(format!("检查失败: {error}")));
                    }
                }
            });
        });
    });
}

/// 注册【立即下载更新】按钮回调
fn setup_download_callback(
    win: &SettingsWindow,
    pending_update: Arc<Mutex<Option<Update>>>,
    downloaded_holder: Arc<Mutex<Option<DownloadedUpdate>>>,
) {
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

        thread::spawn(move || {
            let win_progress = win_weak_async.clone();
            let download_res = update.download(move |event| {
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
                            if let Some(w) = win_clone.upgrade() {
                                w.set_download_progress(progress);
                                w.set_download_speed_text(SharedString::from(text));
                            }
                        });
                    }
                    UpdateEvent::VerifyingSignature => {
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(w) = win_clone.upgrade() {
                                w.set_download_speed_text(SharedString::from(
                                    "正在验证数字签名...",
                                ));
                            }
                        });
                    }
                    _ => {}
                }
            });

            let _ = slint::invoke_from_event_loop(move || {
                let Some(w) = win_weak_async.upgrade() else {
                    return;
                };
                match download_res {
                    Ok(downloaded) => {
                        if let Ok(mut guard) = downloaded_holder_clone.lock() {
                            *guard = Some(downloaded);
                        }
                        w.set_update_status(UpdateUiStatus::Downloaded as i32);
                        w.set_download_progress(1.0);
                        w.set_download_speed_text(SharedString::from("已就绪"));
                    }
                    Err(error) => {
                        tracing::error!("下载或验签更新包失败: {error}");
                        w.set_update_status(UpdateUiStatus::Error as i32);
                        w.set_update_status_text(SharedString::from(format!("下载失败: {error}")));
                    }
                }
            });
        });
    });
}

/// 注册【立即重启并安装】按钮回调
fn setup_install_callback(
    win: &SettingsWindow,
    downloaded_holder: Arc<Mutex<Option<DownloadedUpdate>>>,
) {
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
        thread::spawn(move || {
            // 执行物理安装或拉起系统安装器
            let install_res = downloaded.install(|event| {
                tracing::info!("更新包物理落地事件: {event:?}");
            });

            if let Err(error) = install_res {
                tracing::error!("物理应用安装包失败: {error}");
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(w) = win_thread.upgrade() {
                        w.set_update_status(UpdateUiStatus::Error as i32);
                        w.set_update_status_text(SharedString::from(format!("安装失败: {error}")));
                    }
                });
                return;
            }

            // 安装就绪，关闭设置中心窗口并优雅退出
            let _ = slint::invoke_from_event_loop(|| {
                let _ = slint::quit_event_loop();
            });
        });
    });
}
