//! 双缓冲乒乓槽位状态机。

/// 双缓冲乒乓候选槽位状态机（纯逻辑，不含任何 Wayland 代理与共享内存句柄，便于单元测试）。
///
/// 维护“下一次写入哪块缓冲区”的候选槽位，并在每次提交后于 0/1 之间互换。
/// 判定可写性的依据（两块缓冲区当前的 `release` 标志）由调用方传入，
/// 因此该状态机可以脱离真实合成器连接被完整验证。
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct PingPongState {
    /// 下一次呈现的候选写入槽位（0 或 1）
    candidate: usize,
}

impl PingPongState {
    /// 判定当前候选槽位是否可写。
    ///
    /// 仅当候选槽位对应的缓冲区已被合成器归还（收到 `wl_buffer.release`）时返回 `Some(槽位)`；
    /// 即便另一槽位空闲也不退而求其次：那一块是最近一次已 attach 的缓冲区，
    /// 更可能仍在被合成器读取，写入它会引发内容撕裂甚至让合成器读到未写完的帧。
    pub(super) fn acquire_writable(&self, released: [bool; 2]) -> Option<usize> {
        if released[self.candidate] {
            Some(self.candidate)
        } else {
            None
        }
    }

    /// 本次提交完成后翻转候选槽位，下一次呈现将写入另一块缓冲区。
    pub(super) fn mark_committed(&mut self) {
        self.candidate ^= 1;
    }
}
