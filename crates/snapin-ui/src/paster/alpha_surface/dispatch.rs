//! Wayland 协议对象的派发状态与缓冲区归还事件处理。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use wayland_client::globals::GlobalListContents;
use wayland_client::protocol::{
    wl_buffer, wl_compositor, wl_region, wl_registry, wl_shm, wl_shm_pool,
};
use wayland_client::{Connection, Dispatch, QueueHandle};

/// 贴图 ARGB 表面自有事件队列的派发状态。
///
/// 该状态不持有字段：`wl_buffer` 的归还标志通过缓冲区自身的 `Arc<AtomicBool>` 用户数据传递，
/// 其余对象（registry / shm / pool）的回调只需吞掉事件，避免在派发路径上引入额外状态。
pub(super) struct ShmState;

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

impl Dispatch<wl_compositor::WlCompositor, ()> for ShmState {
    fn event(
        _state: &mut Self,
        _proxy: &wl_compositor::WlCompositor,
        _event: wl_compositor::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // wl_compositor 没有事件，此实现仅为满足 Dispatch 契约
    }
}

impl Dispatch<wl_region::WlRegion, ()> for ShmState {
    fn event(
        _state: &mut Self,
        _proxy: &wl_region::WlRegion,
        _event: wl_region::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // wl_region 没有事件，此实现仅为满足 Dispatch 契约
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
