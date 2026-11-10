//! Wayland 剪贴板协议状态机与事件派发实现。
//!
//! 负责跟踪全局 `wl_data_device_manager` 与全部 `wl_seat` 及其输入焦点与序列号，
//! 处理 `wl_data_source` 的 `send` / `cancelled` 事件，并及时销毁无关的
//! `wl_data_offer` 防止协议对象 ID 泄漏。

use super::focus::SeatFocusRegistry;
use super::pump::PipeWriterPool;
use std::collections::HashMap;
use std::sync::Arc;
use wayland_client::backend::ObjectId;
use wayland_client::globals::GlobalListContents;
use wayland_client::protocol::{
    wl_data_device, wl_data_device_manager, wl_data_offer, wl_data_source, wl_keyboard, wl_pointer,
    wl_registry, wl_seat, wl_touch,
};
use wayland_client::{event_created_child, Connection, Dispatch, Proxy, QueueHandle};

/// 待通过 `wl_data_source` 对外提供的剪贴板字节载荷与 MIME 列表。
#[derive(Debug, Clone)]
pub(crate) struct ClipboardOfferData {
    /// 要发送给粘贴方的完整字节切片（PNG 编码图像或 UTF-8 文本）。
    pub(crate) bytes: Arc<[u8]>,
    /// 本数据源向外通告的 MIME 类型列表。
    pub(crate) mimes: Arc<[String]>,
}

impl ClipboardOfferData {
    /// 校验目标请求的 MIME 是否属于当前数据源通告的范围。
    pub(crate) fn supports_mime(&self, mime: &str) -> bool {
        self.mimes.iter().any(|candidate| candidate == mime)
    }
}

/// 单个 `wl_seat` 持有的 Wayland 协议代理集合。
pub(crate) struct SeatObjects {
    pub(crate) seat: wl_seat::WlSeat,
    pub(crate) data_device: wl_data_device::WlDataDevice,
    pub(crate) keyboard: Option<wl_keyboard::WlKeyboard>,
    pub(crate) pointer: Option<wl_pointer::WlPointer>,
    pub(crate) touch: Option<wl_touch::WlTouch>,
}

impl SeatObjects {
    /// 向服务端释放该席位关联的全部 Wayland 协议代理。
    pub(crate) fn release_all(&mut self) {
        if let Some(keyboard) = self.keyboard.take() {
            if keyboard.version() >= 3 {
                keyboard.release();
            }
        }
        if let Some(pointer) = self.pointer.take() {
            if pointer.version() >= 3 {
                pointer.release();
            }
        }
        if let Some(touch) = self.touch.take() {
            if touch.version() >= 3 {
                touch.release();
            }
        }
        if self.data_device.version() >= 2 {
            self.data_device.release();
        }
        if self.seat.version() >= 5 {
            self.seat.release();
        }
    }
}

/// Wayland 原生剪贴板协议派发状态。
pub(crate) struct ClipboardProtoState {
    pub(crate) data_device_manager: wl_data_device_manager::WlDataDeviceManager,
    pub(crate) seats: HashMap<u32, SeatObjects>,
    pub(crate) focus: SeatFocusRegistry,
    pub(crate) active_sources: HashMap<ObjectId, wl_data_source::WlDataSource>,
    pub(crate) pipe_writers: Arc<PipeWriterPool>,
}

impl ClipboardProtoState {
    /// 绑定新的 `wl_seat` 全局对象并为其创建对应的 `wl_data_device`。
    pub(crate) fn bind_seat(
        &mut self,
        registry: &wl_registry::WlRegistry,
        qh: &QueueHandle<Self>,
        name: u32,
        version: u32,
    ) {
        if self.seats.contains_key(&name) {
            return;
        }
        let bind_version = wl_seat::WlSeat::interface().version.min(version).max(1);
        let seat = registry.bind::<wl_seat::WlSeat, _, _>(name, bind_version, qh, name);
        let data_device = self.data_device_manager.get_data_device(&seat, qh, name);
        self.focus.ensure_seat(name);
        self.seats.insert(
            name,
            SeatObjects {
                seat,
                data_device,
                keyboard: None,
                pointer: None,
                touch: None,
            },
        );
    }

    /// 清理所有活跃剪贴板数据源与席位代理，供关停阶段调用。
    pub(crate) fn destroy_all(&mut self) {
        for (_, source) in self.active_sources.drain() {
            source.destroy();
        }
        for (_, mut seat_objs) in self.seats.drain() {
            seat_objs.release_all();
        }
    }

    /// 根据席位能力标志按需绑定或销毁键盘、指针与触控代理。
    fn sync_seat_capabilities(
        &mut self,
        seat_name: u32,
        caps: wl_seat::Capability,
        qh: &QueueHandle<Self>,
    ) {
        let Some(objs) = self.seats.get_mut(&seat_name) else {
            return;
        };
        sync_keyboard_cap(objs, seat_name, caps, qh, &mut self.focus);
        sync_pointer_cap(objs, seat_name, caps, qh, &mut self.focus);
        sync_touch_cap(objs, seat_name, caps, qh, &mut self.focus);
    }
}

fn sync_keyboard_cap(
    objs: &mut SeatObjects,
    seat_name: u32,
    caps: wl_seat::Capability,
    qh: &QueueHandle<ClipboardProtoState>,
    focus: &mut SeatFocusRegistry,
) {
    let want = caps.contains(wl_seat::Capability::Keyboard);
    match (want, objs.keyboard.is_some()) {
        (true, false) => objs.keyboard = Some(objs.seat.get_keyboard(qh, seat_name)),
        (false, true) => {
            if let Some(kb) = objs.keyboard.take() {
                if kb.version() >= 3 {
                    kb.release();
                }
            }
            focus.set_keyboard_focus(seat_name, false, None);
        }
        _ => {}
    }
}

fn sync_pointer_cap(
    objs: &mut SeatObjects,
    seat_name: u32,
    caps: wl_seat::Capability,
    qh: &QueueHandle<ClipboardProtoState>,
    focus: &mut SeatFocusRegistry,
) {
    let want = caps.contains(wl_seat::Capability::Pointer);
    match (want, objs.pointer.is_some()) {
        (true, false) => objs.pointer = Some(objs.seat.get_pointer(qh, seat_name)),
        (false, true) => {
            if let Some(ptr) = objs.pointer.take() {
                if ptr.version() >= 3 {
                    ptr.release();
                }
            }
            focus.set_pointer_focus(seat_name, false, None);
        }
        _ => {}
    }
}

fn sync_touch_cap(
    objs: &mut SeatObjects,
    seat_name: u32,
    caps: wl_seat::Capability,
    qh: &QueueHandle<ClipboardProtoState>,
    focus: &mut SeatFocusRegistry,
) {
    let want = caps.contains(wl_seat::Capability::Touch);
    match (want, objs.touch.is_some()) {
        (true, false) => objs.touch = Some(objs.seat.get_touch(qh, seat_name)),
        (false, true) => {
            if let Some(touch) = objs.touch.take() {
                if touch.version() >= 3 {
                    touch.release();
                }
            }
            focus.set_touch_active(seat_name, false, None);
        }
        _ => {}
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for ClipboardProtoState {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _data: &GlobalListContents,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } if interface == wl_seat::WlSeat::interface().name => {
                state.bind_seat(registry, qh, name, version);
            }
            wl_registry::Event::GlobalRemove { name } => {
                if let Some(mut seat_objs) = state.seats.remove(&name) {
                    seat_objs.release_all();
                }
                state.focus.remove_seat(name);
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_data_device_manager::WlDataDeviceManager, ()> for ClipboardProtoState {
    fn event(
        _state: &mut Self,
        _proxy: &wl_data_device_manager::WlDataDeviceManager,
        _event: wl_data_device_manager::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_seat::WlSeat, u32> for ClipboardProtoState {
    fn event(
        state: &mut Self,
        _proxy: &wl_seat::WlSeat,
        event: wl_seat::Event,
        &seat_name: &u32,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities { capabilities } = event {
            let caps = wl_seat::Capability::from_bits_truncate(capabilities.into());
            state.sync_seat_capabilities(seat_name, caps, qh);
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, u32> for ClipboardProtoState {
    fn event(
        state: &mut Self,
        _proxy: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        &seat_name: &u32,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_keyboard::Event::Enter { serial, .. } => {
                state
                    .focus
                    .set_keyboard_focus(seat_name, true, Some(serial));
            }
            wl_keyboard::Event::Leave { .. } => {
                state.focus.set_keyboard_focus(seat_name, false, None);
            }
            wl_keyboard::Event::Key { serial, .. } => {
                state.focus.record_input_serial(seat_name, serial);
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, u32> for ClipboardProtoState {
    fn event(
        state: &mut Self,
        _proxy: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        &seat_name: &u32,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_pointer::Event::Enter { serial, .. } => {
                state.focus.set_pointer_focus(seat_name, true, Some(serial));
            }
            wl_pointer::Event::Leave { .. } => {
                state.focus.set_pointer_focus(seat_name, false, None);
            }
            wl_pointer::Event::Button { serial, .. } => {
                state.focus.record_input_serial(seat_name, serial);
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_touch::WlTouch, u32> for ClipboardProtoState {
    fn event(
        state: &mut Self,
        _proxy: &wl_touch::WlTouch,
        event: wl_touch::Event,
        &seat_name: &u32,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_touch::Event::Down { serial, .. } => {
                state.focus.set_touch_active(seat_name, true, Some(serial));
            }
            wl_touch::Event::Up { .. } | wl_touch::Event::Cancel => {
                state.focus.set_touch_active(seat_name, false, None);
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_data_device::WlDataDevice, u32> for ClipboardProtoState {
    event_created_child!(ClipboardProtoState, wl_data_device::WlDataDevice, [
        wl_data_device::EVT_DATA_OFFER_OPCODE => (wl_data_offer::WlDataOffer, ())
    ]);

    fn event(
        _state: &mut Self,
        _proxy: &wl_data_device::WlDataDevice,
        event: wl_data_device::Event,
        &_seat_name: &u32,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // 本模块只负责原生写入，不在此队列消费外部 offer；在对象诞生事件中销毁一次即可，
        // 避免后续在 Enter/Selection 重复调用 destroy 触发 Wayland 协议错误。
        if let wl_data_device::Event::DataOffer { id } = event {
            id.destroy();
        }
    }
}

impl Dispatch<wl_data_offer::WlDataOffer, ()> for ClipboardProtoState {
    fn event(
        _state: &mut Self,
        _proxy: &wl_data_offer::WlDataOffer,
        _event: wl_data_offer::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_data_source::WlDataSource, Arc<ClipboardOfferData>> for ClipboardProtoState {
    fn event(
        state: &mut Self,
        source: &wl_data_source::WlDataSource,
        event: wl_data_source::Event,
        offer: &Arc<ClipboardOfferData>,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_source::Event::Send { mime_type, fd } => {
                if !offer.supports_mime(&mime_type) {
                    tracing::debug!("收到未通告的 Wayland 剪贴板 MIME 请求，已忽略: {mime_type}");
                    return;
                }
                state
                    .pipe_writers
                    .submit(offer.bytes.clone(), mime_type, fd);
            }
            wl_data_source::Event::Cancelled => {
                state.active_sources.remove(&source.id());
                source.destroy();
            }
            wl_data_source::Event::Target { .. }
            | wl_data_source::Event::DndDropPerformed
            | wl_data_source::Event::DndFinished
            | wl_data_source::Event::Action { .. } => {}
            _ => {}
        }
    }
}
