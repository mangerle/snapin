//! Wayland 原生剪贴板写入后端（基于 `wl_data_device.set_selection`）。
//!
//! # 设计原理
//!
//! GNOME 等未实现 `wlr-data-control` / `ext-data-control` 的 Wayland 合成器下，
//! `arboard` 会自动降级到 X11 后端，经 `XWayland -> Mutter -> wl_data_device` 跨协议
//! 桥接写入剪贴板；当全屏覆盖层关闭触发焦点切换时容易产生同步竞态，导致微信等
//! Wayland 原生应用粘贴失败。
//!
//! 截图覆盖层与贴图窗口本身就是由 `winit` 托管的有焦点 Wayland 原生表面，因此本模块
//! 以 guest 模式接入 `winit` 的 `wl_display`，捕获当前交互席位的真实输入序列号，
//! 在覆盖层关闭前直接通过 `wl_data_device.set_selection` 注册 `wl_data_source` 并冲刷
//! 套接字，从而完全绕开 XWayland 桥接；无焦点或非 Wayland 会话时自动回退至 `arboard`。
//!
//! # 代价与权衡
//!
//! Wayland 核心协议要求 `set_selection` 必须附带聚焦表面最近收到的输入事件序列号，
//! 因此不适用于无聚焦窗口的后台复制；后台保存后自动复制与独立历史面板仍复用 `arboard`。

mod dispatch;
mod focus;
mod pump;

use self::dispatch::{ClipboardOfferData, ClipboardProtoState};
use self::focus::SeatFocusRegistry;
pub(crate) use self::focus::SelectionTarget;
pub(crate) use self::pump::PumpPauseGuard;
use self::pump::{
    is_wouldblock_wayland, run_pump_loop, PipeWriterPool, PumpControl, WaylandRuntimeState,
};
use crate::ports::ClipboardWrite;
use image::{ExtendedColorType, ImageEncoder as _};
use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
use rustix::pipe::{pipe_with, PipeFlags};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use wayland_backend::client::Backend;
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_data_device_manager, wl_seat};
use wayland_client::{Connection, Proxy};

const MIME_PNG: &str = "image/png";
const TEXT_MIMES: [&str; 5] = [
    "text/plain;charset=utf-8",
    "text/plain",
    "UTF8_STRING",
    "STRING",
    "TEXT",
];

/// 已编码待发布的 Wayland 剪贴板载荷。
#[derive(Debug, Clone)]
pub(crate) struct PreparedSelection {
    pub(crate) bytes: Arc<[u8]>,
    pub(crate) mimes: Arc<[String]>,
}

impl PreparedSelection {
    /// 将端口写入请求编码为 Wayland 可发送的字节切片与 MIME 集合。
    pub(crate) fn from_write(write: &ClipboardWrite) -> Result<Self, String> {
        match write {
            ClipboardWrite::Text(text) => Ok(Self {
                bytes: Arc::from(text.as_bytes()),
                mimes: Arc::from(
                    TEXT_MIMES
                        .iter()
                        .map(|mime| (*mime).to_string())
                        .collect::<Vec<_>>(),
                ),
            }),
            ClipboardWrite::Image(image) => {
                if image.width() == 0 || image.height() == 0 {
                    return Err("剪贴板图像宽高不能为 0".to_string());
                }
                let mut png_bytes = Vec::new();
                let encoder = image::codecs::png::PngEncoder::new(&mut png_bytes);
                encoder
                    .write_image(
                        image.as_raw(),
                        image.width(),
                        image.height(),
                        ExtendedColorType::Rgba8,
                    )
                    .map_err(|error| format!("编码剪贴板 PNG 图像失败: {error}"))?;
                Ok(Self {
                    bytes: Arc::from(png_bytes),
                    mimes: Arc::from(vec![MIME_PNG.to_string()]),
                })
            }
        }
    }
}

/// 绑定在 `winit` Wayland 连接上的原生剪贴板写入器。
pub(crate) struct WaylandClipboard {
    shutting_down: AtomicBool,
    runtime: Arc<Mutex<Option<WaylandRuntimeState>>>,
    control: Arc<PumpControl>,
    pipe_writers: Arc<PipeWriterPool>,
    pump_handle: Mutex<Option<JoinHandle<()>>>,
}

impl WaylandClipboard {
    /// 尝试在 `winit` 的 Wayland 显示连接上初始化原生剪贴板写入器。
    ///
    /// 非 Wayland 会话（X11/Windows/macOS）或缺少 `wl_data_device_manager` 时返回 `None`，
    /// 调用方据此保留原有 `arboard` 路径。
    pub(crate) fn try_new<D: HasDisplayHandle>(display: &D) -> Option<Arc<Self>> {
        let raw = display.display_handle().ok()?.as_raw();
        let RawDisplayHandle::Wayland(handle) = raw else {
            return None;
        };

        // SAFETY: 指针来自存活的 winit EventLoop，`from_foreign_display` 仅以 guest 模式接入且
        // 不负责关闭连接；`shutdown()` 会在 EventLoop 销毁前停止泵线程并清空协议代理。
        let backend = unsafe { Backend::from_foreign_display(handle.display.as_ptr().cast()) };
        let conn = Connection::from_backend(backend);

        let (globals, mut event_queue) = match registry_queue_init::<ClipboardProtoState>(&conn) {
            Ok(pair) => pair,
            Err(error) => {
                tracing::warn!("初始化 Wayland 原生剪贴板注册表失败，将回退 arboard: {error}");
                return None;
            }
        };
        let qh = event_queue.handle();
        let data_device_manager = match globals
            .bind::<wl_data_device_manager::WlDataDeviceManager, _, _>(&qh, 1..=3, ())
        {
            Ok(manager) => manager,
            Err(error) => {
                tracing::warn!(
                    "合成器未提供 wl_data_device_manager，Wayland 原生剪贴板不可用: {error}"
                );
                return None;
            }
        };

        let pipe_writers = Arc::new(PipeWriterPool::default());
        let mut proto = ClipboardProtoState {
            data_device_manager,
            seats: HashMap::new(),
            focus: SeatFocusRegistry::default(),
            active_sources: HashMap::new(),
            pipe_writers: Arc::clone(&pipe_writers),
        };

        let registry = globals.registry();
        globals.contents().with_list(|list| {
            for global in list {
                if global.interface == wl_seat::WlSeat::interface().name {
                    proto.bind_seat(registry, &qh, global.name, global.version);
                }
            }
        });

        // 在 EventLoop 尚未启动前同步执行一次 roundtrip，确保初始 wl_seat 能力事件已就绪。
        if let Err(error) = event_queue.roundtrip(&mut proto) {
            tracing::warn!("同步 Wayland 剪贴板初始席位状态失败，将回退 arboard: {error}");
            return None;
        }

        let (wake_rd, wake_wr) = match pipe_with(PipeFlags::CLOEXEC | PipeFlags::NONBLOCK) {
            Ok(fds) => fds,
            Err(error) => {
                tracing::warn!("创建 Wayland 剪贴板唤醒管道失败，将回退 arboard: {error}");
                return None;
            }
        };

        let runtime = Arc::new(Mutex::new(Some(WaylandRuntimeState {
            conn,
            event_queue,
            proto,
        })));
        let control = Arc::new(PumpControl::new(wake_wr));
        let pump_runtime = Arc::clone(&runtime);
        let pump_control = Arc::clone(&control);
        let pump_handle = match std::thread::Builder::new()
            .name("wl-clip-pump".to_string())
            .spawn(move || run_pump_loop(pump_runtime, pump_control, wake_rd))
        {
            Ok(handle) => handle,
            Err(error) => {
                tracing::warn!("启动 Wayland 剪贴板事件泵线程失败，将回退 arboard: {error}");
                return None;
            }
        };

        tracing::info!("已启用 Wayland 原生剪贴板写入通道（wl_data_device）");
        Some(Arc::new(Self {
            shutting_down: AtomicBool::new(false),
            runtime,
            control,
            pipe_writers,
            pump_handle: Mutex::new(Some(pump_handle)),
        }))
    }

    /// 暂停后台事件泵以安全执行 GUI 主线程上的 `winit` / `softbuffer` / `alpha_surface` 调度。
    pub(crate) fn pause_pump(&self) -> PumpPauseGuard<'_> {
        self.control.pause()
    }

    /// 派发当前队列中的输入事件，并抓取具有有效焦点的目标席位与序列号。
    ///
    /// 应当在 GUI 线程持有 [`PumpPauseGuard`] 时调用，以保证刚刚触发复制操作的鼠标点击
    /// 或键盘按键事件已经被本连接消费。
    pub(crate) fn capture_selection_target(&self) -> Option<SelectionTarget> {
        if self.shutting_down.load(Ordering::Acquire) {
            return None;
        }
        let mut lock = self
            .runtime
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let rt = lock.as_mut()?;
        if let Err(error) = rt.event_queue.dispatch_pending(&mut rt.proto) {
            tracing::warn!("派发 Wayland 输入序列号事件失败: {error}");
            return None;
        }
        rt.proto.focus.active_target()
    }

    /// 通过原生 `wl_data_device.set_selection` 发布剪贴板载荷。
    ///
    /// 返回 `false` 表示当前实例已关停、目标窗口已在编码期间失去焦点或协议冲刷失败，
    /// 调用方应当透明回退至 `arboard`。
    pub(crate) fn publish_selection(
        &self,
        target: SelectionTarget,
        payload: PreparedSelection,
    ) -> bool {
        if self.shutting_down.load(Ordering::Acquire) {
            return false;
        }
        let mut lock = self
            .runtime
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(rt) = lock.as_mut() else {
            return false;
        };
        if let Err(error) = rt.event_queue.dispatch_pending(&mut rt.proto) {
            tracing::warn!("发布 Wayland 剪贴板前派发事件失败: {error}");
            return false;
        }
        if !rt.proto.focus.seat_has_focus(target.seat_name) {
            tracing::debug!(
                "Wayland 窗口已在编码期间失去席位 {} 焦点，回退 arboard 写入",
                target.seat_name
            );
            return false;
        }
        let Some(seat_objs) = rt.proto.seats.get(&target.seat_name) else {
            return false;
        };

        let offer = Arc::new(ClipboardOfferData {
            bytes: payload.bytes,
            mimes: payload.mimes.clone(),
        });
        let qh = rt.event_queue.handle();
        let source = rt.proto.data_device_manager.create_data_source(&qh, offer);
        for mime in payload.mimes.iter() {
            source.offer(mime.clone());
        }
        let source_id = source.id();
        rt.proto
            .active_sources
            .insert(source_id.clone(), source.clone());
        seat_objs
            .data_device
            .set_selection(Some(&source), target.serial);

        if let Err(error) = rt.conn.flush() {
            if !is_wouldblock_wayland(&error) {
                tracing::warn!("冲刷 Wayland set_selection 请求失败，回退 arboard: {error}");
                rt.proto.active_sources.remove(&source_id);
                source.destroy();
                return false;
            }
        }

        tracing::info!(
            "已通过 Wayland 原生协议写入系统剪贴板，MIME: {:?}",
            payload.mimes
        );
        true
    }

    /// 在 `winit` `EventLoop` 销毁前停止事件泵、清理所有协议代理并回收写入线程。
    pub(crate) fn shutdown(&self) {
        if self.shutting_down.swap(true, Ordering::AcqRel) {
            return;
        }
        self.control.request_shutdown();
        if let Some(handle) = self
            .pump_handle
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
        {
            if let Err(payload) = handle.join() {
                tracing::warn!("Wayland 剪贴板事件泵线程异常退出: {payload:?}");
            }
        }
        {
            let mut lock = self
                .runtime
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some(mut rt) = lock.take() {
                rt.proto.destroy_all();
                let _ = rt.conn.flush();
            }
        }
        self.pipe_writers.join_all();
    }
}

impl Drop for WaylandClipboard {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests;
