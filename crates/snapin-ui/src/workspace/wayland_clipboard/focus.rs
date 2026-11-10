//! Wayland 输入席位焦点与触发序列号注册表。
//!
//! 负责记录每个 `wl_seat` 的键盘、指针与触控焦点状态以及最近一次用户交互
//! 产生的序列号（`serial`），供剪贴板发布阶段筛选出合法的有焦点席位。

use std::collections::HashMap;

/// 本次原生 Wayland 剪贴板写入锁定的目标席位与输入事件序列号。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SelectionTarget {
    /// 对应 `wl_seat` 全局注册表名称，用于在多席位下精确定位 `wl_data_device`。
    pub(crate) seat_name: u32,
    /// 触发用户操作时由合成器签发的最近有效输入序列号。
    pub(crate) serial: u32,
}

/// 单个输入席位（`wl_seat`）在剪贴板视角下的焦点与序列号状态。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SeatFocusState {
    pub(crate) keyboard_focused: bool,
    pub(crate) pointer_focused: bool,
    pub(crate) touch_active: bool,
    pub(crate) latest_serial: Option<u32>,
    /// 单调递增的输入序号，用于在多席位环境下挑选最近交互的席位。
    pub(crate) interaction_seq: u64,
}

impl SeatFocusState {
    /// 当前席位是否持有任意形式的有效交互焦点。
    pub(crate) fn has_focus(self) -> bool {
        self.keyboard_focused || self.pointer_focused || self.touch_active
    }
}

/// 跨席位焦点与输入序列号注册表（纯状态逻辑，可脱离合成器独立单测）。
#[derive(Debug, Default)]
pub(crate) struct SeatFocusRegistry {
    seats: HashMap<u32, SeatFocusState>,
    next_seq: u64,
}

impl SeatFocusRegistry {
    /// 登记新席位。
    pub(crate) fn ensure_seat(&mut self, seat_name: u32) {
        self.seats.entry(seat_name).or_default();
    }

    /// 移除已下线席位。
    pub(crate) fn remove_seat(&mut self, seat_name: u32) {
        self.seats.remove(&seat_name);
    }

    /// 记录一次有效输入事件（键盘进入/按键、指针进入/按键、触控按下），刷新序列号。
    pub(crate) fn record_input_serial(&mut self, seat_name: u32, serial: u32) {
        self.next_seq = self.next_seq.saturating_add(1);
        let entry = self.seats.entry(seat_name).or_default();
        entry.latest_serial = Some(serial);
        entry.interaction_seq = self.next_seq;
    }

    /// 更新键盘焦点状态。
    pub(crate) fn set_keyboard_focus(
        &mut self,
        seat_name: u32,
        focused: bool,
        serial: Option<u32>,
    ) {
        if let Some(serial) = serial.filter(|_| focused) {
            self.record_input_serial(seat_name, serial);
        }
        self.seats.entry(seat_name).or_default().keyboard_focused = focused;
    }

    /// 更新指针焦点状态。
    pub(crate) fn set_pointer_focus(&mut self, seat_name: u32, focused: bool, serial: Option<u32>) {
        if let Some(serial) = serial.filter(|_| focused) {
            self.record_input_serial(seat_name, serial);
        }
        self.seats.entry(seat_name).or_default().pointer_focused = focused;
    }

    /// 更新触控激活状态。
    pub(crate) fn set_touch_active(&mut self, seat_name: u32, active: bool, serial: Option<u32>) {
        if let Some(serial) = serial.filter(|_| active) {
            self.record_input_serial(seat_name, serial);
        }
        self.seats.entry(seat_name).or_default().touch_active = active;
    }

    /// 选择最近发生交互且当前仍保有焦点的席位与序列号。
    pub(crate) fn active_target(&self) -> Option<SelectionTarget> {
        self.seats
            .iter()
            .filter_map(|(&seat_name, state)| {
                if !state.has_focus() {
                    return None;
                }
                let serial = state.latest_serial?;
                Some((state.interaction_seq, SelectionTarget { seat_name, serial }))
            })
            .max_by_key(|(seq, _)| *seq)
            .map(|(_, target)| target)
    }

    /// 校验给定席位在发布剪贴板时是否仍持有焦点，避免失焦后被合成器静默忽略。
    pub(crate) fn seat_has_focus(&self, seat_name: u32) -> bool {
        self.seats
            .get(&seat_name)
            .is_some_and(|state| state.has_focus())
    }
}
