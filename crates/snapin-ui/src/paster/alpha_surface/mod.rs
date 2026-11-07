//! 贴图窗口 ARGB8888 `wl_shm` 共享内存呈现通路。
//!
//! # 为什么必须自建这条通路
//!
//! softbuffer 0.4.8 的 Wayland 后端把 `wl_shm` 缓冲区格式硬编码为 `Xrgb8888`（无 alpha 通道），
//! 贴图窗口的透明像素只能被渲染成黑色：右键菜单放不下时被撑大的窗口空白区域、
//! 贴图自身的半透明阴影与圆角边框都会出现发黑硬边。本模块照搬 softbuffer 的做法
//! （memfd + mmap + `wl_shm_pool` + `wl_buffer`，经由 `raw_window_handle` 复用 winit 已创建的
//! `wl_display` 与 `wl_surface`），仅把缓冲区格式换成 `Argb8888`（预乘），
//! 合成器即可按 alpha 通道真正合成透明像素。winit 在创建透明窗口时已调用
//! `surface.set_opaque_region(None)` 告知合成器按 alpha 合成，此处无需重复设置。
//!
//! # 为什么采用双缓冲乒乓
//!
//! 每帧新建 `wl_buffer` 意味着持续创建 memfd 并通过 Socket 传递文件描述符，开销与延迟都不可接受；
//! 因此这里固定持有前后两块缓冲区：写入前依据 `wl_buffer.release` 事件归还的所有权标志
//! 判定候选缓冲区是否可写，提交后前后互换（乒乓）。如此既不写入合成器正在读取的内存，
//! 又把 memfd 创建压缩到窗口生命周期内的极少数几次（仅尺寸变化时重建）。

mod buffer;
mod dispatch;
mod ping_pong;

use self::dispatch::ShmState;
use self::ping_pong::PingPongState;

/// 单次等待合成器归还缓冲区的上限。
///
/// 该等待运行在 GUI 主线程，必须严格限制；超时后延迟整帧，由事件循环稍后重试。
const BUFFER_RELEASE_WAIT: Duration = Duration::from_millis(50);
/// 延迟呈现后的重试间隔。
const BUFFER_RETRY_DELAY: Duration = Duration::from_millis(50);

use self::buffer::{checked_wayland_size, copy_pixmap_into, damage_and_commit, ShmBuffer};
use super::PresentOutcome;
use crate::error::UiError;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use rustix::event::{poll, PollFd, PollFlags};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tiny_skia::Pixmap;
use wayland_client::backend::{Backend, ObjectId};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_compositor, wl_shm, wl_surface};
use wayland_client::{Connection, EventQueue, Proxy, QueueHandle};
use winit::window::Window;

/// 贴图窗口的 ARGB8888 `wl_shm` 呈现表面。
///
/// 每个表面自持一条以 guest 模式接入 winit 底层 `wl_display` 的连接（与 softbuffer 的做法一致），
/// 因此各贴图窗口之间、以及本通路与 snipper 的 softbuffer 通路之间完全互不干扰。
pub struct ArgbShmSurface {
    /// 前后两块乒乓缓冲区；尺寸变化时整体重建。
    ///
    /// 采用 Box 间接索引：`ShmBuffer` 内含两块较大的协议代理（`wl_shm_pool` 与 `wl_buffer`），
    /// 直接内联会让整个表面枚举膨胀到数百字节；装箱后枚举本体尺寸与软回退通路相当，
    /// 同时缓冲区重建只需替换一个指针。
    buffers: Option<Box<(ShmBuffer, ShmBuffer)>>,
    /// winit 窗口底层 `wl_surface` 的对象句柄。
    ///
    /// 本代理不是 `wl_surface` 的所有权方（析构不会发送销毁请求），不会破坏 winit 对窗口表面的管理；
    /// 它在整个表面生命周期内保持有效，由字段 `_window` 保证。
    surface: wl_surface::WlSurface,
    /// 初始化时绑定的 `wl_shm` 全局对象
    shm: wl_shm::WlShm,
    /// 初始化时绑定的 `wl_compositor` 全局对象，用于创建输入区域
    compositor: wl_compositor::WlCompositor,
    /// 本表面专属事件队列，仅承载缓冲区 release 等自有对象的事件
    event_queue: EventQueue<ShmState>,
    /// 事件队列句柄，创建内存池与缓冲区时用于指定事件归属
    qh: QueueHandle<ShmState>,
    /// 以 guest 模式接入 winit 底层 `wl_display` 的连接（析构时不会关闭 winit 持有的连接）
    conn: Connection,
    /// 目标呈现尺寸（像素，恒不小于 1 且不超过 i32 上限）
    size: (u32, u32),
    /// 乒乓候选槽位
    ping: PingPongState,
    /// 延迟呈现的下一处理时刻。
    ///
    /// 合成器未及时归还缓冲区时，本帧不阻塞等待到超时之外，而是记录重试时刻，
    /// 让 GUI 事件循环继续处理输入并安排下一次 RedrawRequested。
    present_retry_at: Option<Instant>,
    /// 持有窗口强引用，保证底层 `wl_display` 与 `wl_surface` 在表面存续期内有效。
    ///
    /// 必须按字段声明顺序最后析构（先释放协议对象与共享内存，最后才销毁窗口与其表面）。
    _window: Arc<Window>,
}

/// 构造“窗口系统不是 Wayland”的中文错误（ARGB 通路的两处裸句柄解析共用）。
fn non_wayland_error() -> UiError {
    UiError::ShmSurface {
        message: "当前窗口系统不是 Wayland，无法创建 ARGB8888 共享内存表面".to_string(),
    }
}

/// 提取 winit 窗口底层 `wl_display` 的裸指针。
///
/// # Errors
///
/// 窗口后端不是 Wayland，或裸显示句柄获取失败时返回 [`UiError::ShmSurface`]。
fn raw_wayland_display_ptr(window: &Window) -> Result<*mut std::ffi::c_void, UiError> {
    let raw = window
        .display_handle()
        .map_err(|e| UiError::ShmSurface {
            message: format!("获取窗口显示连接裸句柄失败: {e}"),
        })?
        .as_raw();
    let RawDisplayHandle::Wayland(handle) = raw else {
        return Err(non_wayland_error());
    };
    Ok(handle.display.as_ptr())
}

/// 复用 winit 窗口底层的 `wl_surface`，在本表面的连接上创建对应的对象代理。
///
/// # Errors
///
/// 窗口后端不是 Wayland、裸表面句柄获取失败或对象标识与接口不匹配时返回 [`UiError::ShmSurface`]。
fn winit_surface_proxy(
    window: &Window,
    conn: &Connection,
) -> Result<wl_surface::WlSurface, UiError> {
    let raw = window
        .window_handle()
        .map_err(|e| UiError::ShmSurface {
            message: format!("获取窗口表面裸句柄失败: {e}"),
        })?
        .as_raw();
    let RawWindowHandle::Wayland(handle) = raw else {
        return Err(non_wayland_error());
    };
    // SAFETY: 裸指针指向 winit 窗口底层长期有效的 wl_surface 代理（由 `_window` 强引用保证），
    // 接口匹配由 `from_ptr` 自行校验；`from_ptr` 仅复用对象标识而不接管所有权
    // （析构不会发送销毁请求），因此不会破坏 winit 对该表面的管理。
    let surface_id = unsafe {
        ObjectId::from_ptr(
            wl_surface::WlSurface::interface(),
            handle.surface.as_ptr().cast(),
        )
    }
    .map_err(|e| UiError::ShmSurface {
        message: format!("解析窗口表面对象标识失败: {e}"),
    })?;
    wl_surface::WlSurface::from_id(conn, surface_id).map_err(|e| UiError::ShmSurface {
        message: format!("创建窗口表面代理失败: {e}"),
    })
}

impl ArgbShmSurface {
    /// 在 winit 已创建的 Wayland 窗口上初始化 ARGB8888 共享内存呈现通路。
    ///
    /// # Errors
    ///
    /// 非 Wayland 后端、窗口裸句柄获取失败、`wl_shm` 全局缺失或事件队列初始化失败时
    /// 返回 [`UiError::ShmSurface`]；调用方（`PasterSurface`）据此回退 softbuffer 通路。
    pub fn new(window: Arc<Window>) -> Result<Self, UiError> {
        let display_ptr = raw_wayland_display_ptr(&window)?;
        // SAFETY: 裸显示指针指向 winit 持有的 wl_display，`from_foreign_display` 以 guest 模式接入
        // 且不会在析构时关闭该连接；本结构体的 `_window` 字段保证窗口（及其 wl_display）
        // 存活时间长于本连接，满足“指针在整个后端生命周期内保持有效”的安全前提。
        let backend = unsafe { Backend::from_foreign_display(display_ptr.cast()) };
        let conn = Connection::from_backend(backend);
        let (globals, event_queue) =
            registry_queue_init::<ShmState>(&conn).map_err(|e| UiError::ShmSurface {
                message: format!("初始化贴图 ARGB 表面事件队列失败: {e}"),
            })?;
        let qh = event_queue.handle();
        let shm = globals
            .bind::<wl_shm::WlShm, _, _>(&qh, 1..=1, ())
            .map_err(|e| UiError::ShmSurface {
                message: format!("绑定 wl_shm 全局对象失败: {e}"),
            })?;
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&qh, 4..=6, ())
            .map_err(|e| UiError::ShmSurface {
                message: format!("绑定 wl_compositor 全局对象失败: {e}"),
            })?;
        let surface = winit_surface_proxy(&window, &conn)?;

        Ok(Self {
            buffers: None,
            surface,
            shm,
            compositor,
            event_queue,
            qh,
            conn,
            size: (1, 1),
            ping: PingPongState::default(),
            present_retry_at: None,
            _window: window,
        })
    }

    /// 声明下一帧的目标呈现尺寸。
    ///
    /// 零尺寸（窗口最小化等瞬时状态）收敛为 1 像素；尺寸仅在 `present` 时用于懒重建缓冲区，
    /// 因此连续 resize 事件风暴不会反复创建 memfd。
    ///
    /// # Errors
    ///
    /// 尺寸超出 Wayland `i32` 表示范围时返回 [`UiError::ShmSurface`]。
    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), UiError> {
        let width = width.max(1);
        let height = height.max(1);
        checked_wayland_size(width, height)?;
        if self.size != (width, height) {
            self.present_retry_at = None;
        }
        self.size = (width, height);
        Ok(())
    }

    /// 用 surface-local 逻辑坐标声明本窗口的可交互区域。
    ///
    /// 透明扩窗后若不做限制，Wayland 会把整块表面都当作可点击区域，透明空白
    /// 会吞掉本应落到桌面的点击。传入空切片时设置一个空 `wl_region`，
    /// 语义为“整窗不接受任何输入”；每次调用新建区域并在提交后立即销毁，
    /// 避免长期持有可能过期的矩形集合。
    ///
    /// # Errors
    ///
    /// 冲刷连接失败时返回 [`UiError::ShmSurface`]；区域创建本身由协议代理完成，
    /// 提交失败会在下一次派发事件或冲刷时体现。
    pub fn set_input_rects(&mut self, rects: &[(i32, i32, i32, i32)]) -> Result<(), UiError> {
        let region = self.compositor.create_region(&self.qh, ());
        for &(x, y, width, height) in rects {
            // 非正尺寸矩形对合成器没有意义，直接跳过避免发送无效请求
            if width > 0 && height > 0 {
                region.add(x, y, width, height);
            }
        }
        self.surface.set_input_region(Some(&region));
        region.destroy();
        self.conn.flush().map_err(|e| UiError::ShmSurface {
            message: format!("冲刷贴图输入区域请求失败: {e}"),
        })
    }

    /// 将一帧预乘 RGBA 像素提交给合成器显示。
    ///
    /// 合成器在固定等待上限内未归还可写缓冲区时返回
    /// [`PresentOutcome::Deferred`]，不阻塞 GUI 线程继续等待。
    ///
    /// # Errors
    ///
    /// 位图尺寸与 [`Self::resize`] 声明的尺寸不一致时返回 [`UiError::Render`]
    /// （避免缩放/重绘竞态下静默残留上一帧像素）；memfd 创建、共享内存映射或协议交互失败
    /// 返回 [`UiError::ShmSurface`]。
    pub fn present(&mut self, pixmap: &Pixmap) -> Result<PresentOutcome, UiError> {
        let now = Instant::now();
        if self.present_retry_at.is_some_and(|retry_at| now < retry_at) {
            return Ok(PresentOutcome::Deferred);
        }
        self.validate_source_size(pixmap)?;

        // 1. 先回收已到达的 wl_buffer.release 事件，尽量让候选缓冲区处于可写状态
        self.dispatch_pending()?;
        // 2. 尺寸变化或首次呈现时懒重建前后两块缓冲区
        let (width, height) = self.size;
        self.ensure_buffers(width, height)?;
        // 3. 在严格上限内等待可写槽位；超时则安全延迟本帧
        let Some(slot) = self.acquire_writable_slot(now + BUFFER_RELEASE_WAIT)? else {
            self.present_retry_at = Some(now + BUFFER_RETRY_DELAY);
            return Ok(PresentOutcome::Deferred);
        };

        // 4. 写入像素、挂载、施加 damage 并 commit
        self.write_pixmap_to_slot(pixmap, slot)?;
        // 5. 乒乓翻转候选槽位，并立即冲刷请求，确保本帧尽快到达合成器
        self.ping.mark_committed();
        self.present_retry_at = None;
        self.conn.flush().map_err(|error| UiError::ShmSurface {
            message: format!("冲刷 ARGB8888 提交请求失败: {error}"),
        })?;
        Ok(PresentOutcome::Presented)
    }

    /// 校验源位图像素容量与表面声明尺寸一致。
    fn validate_source_size(&self, pixmap: &Pixmap) -> Result<(), UiError> {
        let (width, height) = self.size;
        let expected = width as usize * height as usize;
        let actual = pixmap.data().as_chunks::<4>().0.len();
        if actual != expected {
            return Err(UiError::Render {
                message: format!(
                    "ARGB8888 表面像素容量与源位图不匹配，表面: {expected}，源位图: {actual}"
                ),
            });
        }
        Ok(())
    }

    /// 将源位图写入指定乒乓槽位并提交给合成器。
    fn write_pixmap_to_slot(&mut self, pixmap: &Pixmap, slot: usize) -> Result<(), UiError> {
        // ensure_buffers 刚刚确认缓冲区存在，None 分支仅为防御性兜底。
        let Some(buffers) = self.buffers.as_mut() else {
            return Err(UiError::ShmSurface {
                message: "ARGB8888 共享内存缓冲区未就绪".to_string(),
            });
        };
        let buffer = if slot == 0 {
            &mut buffers.0
        } else {
            &mut buffers.1
        };
        copy_pixmap_into(buffer.pixels_mut(), pixmap);
        buffer.attach(&self.surface);
        damage_and_commit(&self.surface, buffer.width, buffer.height);
        Ok(())
    }

    /// 返回延迟呈现的下一处理时刻。
    pub fn present_retry_deadline(&self) -> Option<Instant> {
        self.present_retry_at
    }

    /// 派发事件队列中已到达的事件（非阻塞），回收缓冲区归还标志。
    fn dispatch_pending(&mut self) -> Result<(), UiError> {
        self.event_queue
            .dispatch_pending(&mut ShmState)
            .map(|_| ())
            .map_err(|e| UiError::ShmSurface {
                message: format!("派发贴图 ARGB 表面事件失败: {e}"),
            })
    }

    /// 尺寸变化或首次呈现时重建前后两块缓冲区。
    fn ensure_buffers(&mut self, width: u32, height: u32) -> Result<(), UiError> {
        let (buffer_w, buffer_h) = checked_wayland_size(width, height)?;
        if let Some(buffers) = &self.buffers {
            if buffers.0.matches(buffer_w, buffer_h) {
                // 尺寸未变：复用现有乒乓缓冲区，避免重复创建 memfd 与协议对象
                return Ok(());
            }
        }

        // 尺寸变化：先整体释放旧缓冲区（发送销毁请求），再重建，并复位乒乓候选槽位
        self.buffers = None;
        self.ping = PingPongState::default();
        let front = ShmBuffer::new(&self.shm, &self.qh, buffer_w, buffer_h)?;
        let back = ShmBuffer::new(&self.shm, &self.qh, buffer_w, buffer_h)?;
        self.buffers = Some(Box::new((front, back)));
        Ok(())
    }

    /// 在截止时刻前等待并返回本次可安全写入的缓冲区槽位（0 或 1）。
    ///
    /// 乒乓规则要求只有候选槽位已被合成器归还才允许写入。这里使用带超时的文件描述符
    /// 轮询代替 `blocking_dispatch`，确保 GUI 事件循环不会被异常合成器永久挂起。
    fn acquire_writable_slot(&mut self, deadline: Instant) -> Result<Option<usize>, UiError> {
        loop {
            let released = self.released_slots()?;
            if let Some(slot) = self.ping.acquire_writable(released) {
                return Ok(Some(slot));
            }
            if Instant::now() >= deadline {
                return Ok(None);
            }
            if self.wait_for_release_event(deadline)? == BufferWait::Timeout {
                return Ok(None);
            }
        }
    }

    /// 读取两块缓冲区当前是否已被合成器归还。
    fn released_slots(&self) -> Result<[bool; 2], UiError> {
        let Some(buffers) = &self.buffers else {
            return Err(UiError::ShmSurface {
                message: "ARGB8888 共享内存缓冲区尚未创建，无法获取可写槽位".to_string(),
            });
        };
        Ok([buffers.0.is_released(), buffers.1.is_released()])
    }

    /// 等待一次文件描述符可读事件并派发缓冲区归还消息。
    fn wait_for_release_event(&mut self, deadline: Instant) -> Result<BufferWait, UiError> {
        let now = Instant::now();
        if now >= deadline {
            return Ok(BufferWait::Timeout);
        }
        let Some(guard) = self.event_queue.prepare_read() else {
            self.dispatch_pending()?;
            std::thread::yield_now();
            return Ok(BufferWait::Progress);
        };
        let timeout_ms = duration_to_poll_timeout(deadline.saturating_duration_since(now));
        let mut fds = [PollFd::from_borrowed_fd(
            guard.connection_fd(),
            PollFlags::IN,
        )];
        match poll(&mut fds, timeout_ms) {
            Ok(0) => Ok(BufferWait::Timeout),
            Ok(_) => {
                let ready = fds[0].revents();
                if ready.intersects(PollFlags::ERR | PollFlags::HUP | PollFlags::NVAL) {
                    return Err(UiError::ShmSurface {
                        message: format!("等待 wl_buffer.release 时 Wayland 连接异常: {ready:?}"),
                    });
                }
                guard.read().map_err(|error| UiError::ShmSurface {
                    message: format!("读取 wl_buffer.release 事件失败: {error}"),
                })?;
                self.dispatch_pending()?;
                Ok(BufferWait::Progress)
            }
            Err(error) => Err(UiError::ShmSurface {
                message: format!("轮询 Wayland 缓冲区释放事件失败: {error}"),
            }),
        }
    }
}

/// 等待缓冲区归还事件的单次结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BufferWait {
    Progress,
    Timeout,
}

/// 将剩余等待时间转换为 `poll` 支持的毫秒上限。
fn duration_to_poll_timeout(duration: Duration) -> i32 {
    duration.as_millis().min(i32::MAX as u128) as i32
}

impl Drop for ArgbShmSurface {
    fn drop(&mut self) {
        // 先析构缓冲区（内部发送 wl_buffer 与 wl_shm_pool 的销毁请求），随后冲刷连接把请求送达服务端。
        // guest 连接的析构不会自动 flush，若省略此次冲刷，内存池与 memfd 将泄漏至 winit 连接关闭为止。
        self.buffers = None;
        if let Err(e) = self.conn.flush() {
            tracing::warn!("销毁贴图 ARGB 表面时冲刷连接失败，共享内存资源可能延迟回收: {e}");
        }
    }
}

#[cfg(test)]
mod tests;
