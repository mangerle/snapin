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

use self::buffer::{checked_wayland_size, copy_pixmap_into, damage_and_commit, ShmBuffer};
use crate::error::UiError;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tiny_skia::Pixmap;
use wayland_client::backend::{Backend, ObjectId};
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{wl_buffer, wl_registry, wl_shm, wl_shm_pool, wl_surface};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle};
use winit::window::Window;

/// 贴图 ARGB 表面自有事件队列的派发状态。
///
/// 该状态不持有字段：`wl_buffer` 的归还标志通过缓冲区自身的 `Arc<AtomicBool>` 用户数据传递，
/// 其余对象（registry / shm / pool）的回调只需吞掉事件，避免在派发路径上引入额外状态。
struct ShmState;

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for ShmState {
    fn event(
        _state: &mut Self,
        _proxy: &wl_registry::WlRegistry,
        _event: wl_registry::Event,
        _data: &GlobalListContents,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // 连接建立后新通告的全局对象与贴图呈现无关（本表面在初始化时已绑定所需全局），直接忽略
    }
}

impl Dispatch<wl_shm::WlShm, ()> for ShmState {
    fn event(
        _state: &mut Self,
        _proxy: &wl_shm::WlShm,
        _event: wl_shm::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // wl_shm 的 format 事件仅用于通告可用像素格式；
        // Argb8888 是协议强制要求支持的格式，无需动态判断，直接忽略
    }
}

impl Dispatch<wl_shm_pool::WlShmPool, ()> for ShmState {
    fn event(
        _state: &mut Self,
        _proxy: &wl_shm_pool::WlShmPool,
        _event: wl_shm_pool::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // 当前协议版本的 wl_shm_pool 没有任何事件，此实现仅为满足 Dispatch 契约
    }
}

impl Dispatch<wl_buffer::WlBuffer, Arc<AtomicBool>> for ShmState {
    fn event(
        _state: &mut Self,
        _proxy: &wl_buffer::WlBuffer,
        event: wl_buffer::Event,
        released: &Arc<AtomicBool>,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let wl_buffer::Event::Release = event {
            // Release 表示合成器已归还本缓冲区内存的所有权，可以安全覆写
            released.store(true, Ordering::Release);
        }
    }
}

/// 双缓冲乒乓候选槽位状态机（纯逻辑，不含任何 Wayland 代理与共享内存句柄，便于单元测试）。
///
/// 维护“下一次写入哪块缓冲区”的候选槽位，并在每次提交后于 0/1 之间互换。
/// 判定可写性的依据（两块缓冲区当前的 `release` 标志）由调用方传入，
/// 因此该状态机可以脱离真实合成器连接被完整验证。
#[derive(Debug, Clone, Copy, Default)]
struct PingPongState {
    /// 下一次呈现的候选写入槽位（0 或 1）
    candidate: usize,
}

impl PingPongState {
    /// 判定当前候选槽位是否可写。
    ///
    /// 仅当候选槽位对应的缓冲区已被合成器归还（收到 `wl_buffer.release`）时返回 `Some(槽位)`；
    /// 即便另一槽位空闲也不退而求其次：那一块是最近一次已 attach 的缓冲区，
    /// 更可能仍在被合成器读取，写入它会引发内容撕裂甚至让合成器读到未写完的帧。
    fn acquire_writable(&self, released: [bool; 2]) -> Option<usize> {
        if released[self.candidate] {
            Some(self.candidate)
        } else {
            None
        }
    }

    /// 本次提交完成后翻转候选槽位，下一次呈现将写入另一块缓冲区。
    fn mark_committed(&mut self) {
        self.candidate ^= 1;
    }
}

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
        let surface = winit_surface_proxy(&window, &conn)?;

        Ok(Self {
            buffers: None,
            surface,
            shm,
            event_queue,
            qh,
            conn,
            size: (1, 1),
            ping: PingPongState::default(),
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
        self.size = (width, height);
        Ok(())
    }

    /// 将一帧预乘 RGBA 像素提交给合成器显示。
    ///
    /// # Errors
    ///
    /// 位图尺寸与 [`Self::resize`] 声明的尺寸不一致时返回 [`UiError::Render`]
    /// （避免缩放/重绘竞态下静默残留上一帧像素）；memfd 创建、共享内存映射或协议交互失败
    /// 返回 [`UiError::ShmSurface`]。
    ///
    /// # Panics
    ///
    /// 本函数不会 panic。
    pub fn present(&mut self, pixmap: &Pixmap) -> Result<(), UiError> {
        let (width, height) = self.size;
        let expected = width as usize * height as usize;
        let src = pixmap.data().as_chunks::<4>().0;
        if src.len() != expected {
            return Err(UiError::Render {
                message: format!(
                    "ARGB8888 表面像素容量与源位图不匹配，表面: {expected}，源位图: {}",
                    src.len()
                ),
            });
        }

        // 1. 先回收已到达的 wl_buffer.release 事件，尽量让候选缓冲区处于可写状态
        self.dispatch_pending()?;

        // 2. 尺寸变化或首次呈现时懒重建前后两块缓冲区
        self.ensure_buffers(width, height)?;

        // 3. 取可写槽位：候选缓冲区仍被合成器持有时阻塞等待 release 事件
        let slot = self.acquire_writable_slot()?;

        // ensure_buffers 刚刚确认缓冲区存在，None 分支仅为防御性兜底
        let Some(buffers) = self.buffers.as_mut() else {
            return Err(UiError::ShmSurface {
                message: "ARGB8888 共享内存缓冲区未就绪".to_string(),
            });
        };
        let (front, back) = (&mut buffers.0, &mut buffers.1);
        let buffer = if slot == 0 { front } else { back };

        // 4. 写入像素、挂载、施加 damage 并 commit
        copy_pixmap_into(buffer.pixels_mut(), pixmap);
        buffer.attach(&self.surface);
        damage_and_commit(&self.surface, buffer.width, buffer.height);

        // 5. 乒乓翻转候选槽位，并立即冲刷请求，确保本帧尽快到达合成器
        self.ping.mark_committed();
        self.conn.flush().map_err(|e| UiError::ShmSurface {
            message: format!("冲刷 ARGB8888 提交请求失败: {e}"),
        })
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

    /// 等待并返回本次可安全写入的缓冲区槽位（0 或 1）。
    ///
    /// 乒乓规则要求只有候选槽位已被合成器归还才允许写入；尚未归还时阻塞派发事件直到
    /// `wl_buffer.release` 到达。宁可短暂阻塞事件循环，也绝不在合成器读取期间写入同一块内存，
    /// 该策略与 softbuffer `buffer_mut` 的等待语义保持一致。
    fn acquire_writable_slot(&mut self) -> Result<usize, UiError> {
        loop {
            let released = match &self.buffers {
                Some(buffers) => [buffers.0.is_released(), buffers.1.is_released()],
                // ensure_buffers 已保证缓冲区就绪；此分支仅作防御性兜底，避免无缓冲区时空转阻塞
                None => {
                    return Err(UiError::ShmSurface {
                        message: "ARGB8888 共享内存缓冲区尚未创建，无法获取可写槽位".to_string(),
                    });
                }
            };
            if let Some(slot) = self.ping.acquire_writable(released) {
                return Ok(slot);
            }

            // 候选缓冲区仍被合成器占用（上一帧 commit 尚未处理完），阻塞等待 release 事件
            self.event_queue
                .blocking_dispatch(&mut ShmState)
                .map_err(|e| UiError::ShmSurface {
                    message: format!("等待 wl_buffer.release 事件失败: {e}"),
                })?;
        }
    }
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
mod tests {
    use super::buffer::{buffer_byte_len, pack_premultiplied_rgba};
    use super::*;
    use tiny_skia::{Color, Pixmap};

    /// 预乘 RGBA 像素必须按 Argb8888 位布局逐位打包（含半透明与全透明边界）
    #[test]
    fn test_pack_premultiplied_rgba() {
        // 不透明红：预乘后 alpha 与颜色通道均为满值
        assert_eq!(pack_premultiplied_rgba([255, 0, 0, 255]), 0xFFFF_0000);
        // 不透明绿与不透明蓝
        assert_eq!(pack_premultiplied_rgba([0, 255, 0, 255]), 0xFF00_FF00);
        assert_eq!(pack_premultiplied_rgba([0, 0, 255, 255]), 0xFF00_00FF);
        // 半透明黑（预乘值即阴影用的 rgba(0,0,0,90)）：alpha 位于最高字节
        assert_eq!(pack_premultiplied_rgba([0, 0, 0, 90]), 90u32 << 24);
        // 半透明红：预乘后 r=64、a=128，不做任何二次乘法
        assert_eq!(pack_premultiplied_rgba([64, 0, 0, 128]), 0x8040_0000);
        // 全透明像素必须打包为 0，保证透明区域不残留颜色
        assert_eq!(pack_premultiplied_rgba([0, 0, 0, 0]), 0);
    }

    /// 位图拷贝必须逐像素重排字节序，并保留预乘通道数值
    #[test]
    fn test_copy_pixmap_into_rewrites_premultiplied_pixels() {
        let mut pixmap = Pixmap::new(2, 1).expect("测试位图分配必须成功");
        pixmap.fill(Color::from_rgba8(255, 0, 0, 128));
        let mut dest = vec![0u32; 2];
        copy_pixmap_into(&mut dest, &pixmap);
        // tiny-skia 的 from_rgba8 会做非预乘到预乘转换：r=255,a=128 -> r≈128
        for packed in dest {
            let alpha = (packed >> 24) & 0xFF;
            let red = (packed >> 16) & 0xFF;
            assert_eq!(alpha, 128, "alpha 通道必须保留原值");
            assert!(
                (127..=129).contains(&red),
                "红色通道必须是预乘值，实际: {red}"
            );
            // 绿色与蓝色通道必须为零
            assert_eq!(packed & 0x0000_FFFF, 0, "未参与的颜色通道必须为零");
        }
    }

    /// 乒乓候选槽位在提交后必须交替，且仅在候选缓冲区被归还时才可写
    #[test]
    fn test_ping_pong_slot_selection() {
        let mut ping = PingPongState::default();
        // 初始状态：两块缓冲都未被占用（归还真），候选槽位 0 可写
        assert_eq!(ping.acquire_writable([true, true]), Some(0));
        ping.mark_committed();
        // 提交翻转后候选槽位变为 1
        assert_eq!(ping.acquire_writable([true, true]), Some(1));
        ping.mark_committed();
        // 候选槽位回到 0，但其缓冲仍被合成器持有：必须等待，不得改写槽位 1（更可能仍在被读取）
        assert_eq!(ping.acquire_writable([false, true]), None);
        // 两块都在途时同样必须等待
        assert_eq!(ping.acquire_writable([false, false]), None);
        // 槽位 0 收到 release 后恢复可写
        assert_eq!(ping.acquire_writable([true, false]), Some(0));
    }

    /// 缓冲区字节长度计算必须拒绝非法尺寸并防止算术溢出
    #[test]
    fn test_buffer_byte_len_guards() {
        assert_eq!(buffer_byte_len(4, 3).expect("合法尺寸必须成功"), 48);
        assert!(buffer_byte_len(0, 3).is_err(), "零宽度必须被拒绝");
        assert!(buffer_byte_len(3, 0).is_err(), "零高度必须被拒绝");
        assert!(buffer_byte_len(-1, 3).is_err(), "负尺寸必须被拒绝");
        assert!(
            buffer_byte_len(i32::MAX, i32::MAX).is_err(),
            "溢出尺寸必须被检查算术拦截而不 panic"
        );
    }

    /// Wayland 尺寸转换必须约束在 i32 范围内
    #[test]
    fn test_checked_wayland_size() {
        assert_eq!(
            checked_wayland_size(1920, 1080).expect("常规尺寸必须成功"),
            (1920, 1080)
        );
        assert!(checked_wayland_size(u32::MAX, 10).is_err());
        assert!(checked_wayland_size(10, u32::MAX).is_err());
    }
}
