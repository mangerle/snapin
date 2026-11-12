//! 长截图滚轮累积蓄水池与自动滚动驱动器。
//!
//! # 设计原理
//!
//! 1. **零事件丢失的高频滚轮合并 (Coalescing)**：高速触控板惯性滑动或连续拨动滚轮时，
//!    前序抓屏任务往往仍在执行；若此时直接丢弃后继滚轮事件会造成明显的“卡顿/掉帧感”。
//!    这里将到达的离散/像素滚轮增量安全累加进有界蓄水池 `pending_wheel`，前帧一落地立即
//!    按单批上限 `MAX_BATCH_WHEEL_PER_AXIS` 切分派发，同时天然支持反向滚轮实时抵消。
//! 2. **稳态自动滚到底 (`AutoScroll`)**：驱动目标窗口按小步长持续推进，同时保留
//!    `AUTO_SCROLL_INTERVAL` 的操作空窗期便于用户随时点击停止按钮或按下快捷键；当连续
//!    多帧画面保持静止时判定已到达滚动底部并自动停机，避免无意义空转。
//!
//! # 代价与权衡
//!
//! 对极端高速爆发性滑动施加 `MAX_PENDING_WHEEL_PER_AXIS` 积压上限与单批步长上限，
//! 防止一次注入过大步长导致前后帧完全失去重叠区从而匹配断裂。

use crate::ports::WheelDelta;
use snapin_render::stitching::{StitchDirection, StitchFrameOutcome};
use std::time::{Duration, Instant};

/// 待发送手动滚轮累积蓄水池的单轴绝对上限，防止极端连续惯性滑行后产生不可匹配的大跳帧。
const MAX_PENDING_WHEEL_PER_AXIS: f32 = 12.0;

/// 单次提交到底层滚轮注入的单轴绝对上限，平衡拼接重叠率与滚动吞吐效率。
const MAX_BATCH_WHEEL_PER_AXIS: f32 = 6.0;

/// 自动滚动模式每拍发送的正向滚动步长（Winit 语义负数为向下/向右滚动）。
const AUTO_SCROLL_STEP_DELTA: f32 = -2.0;

/// 自动滚动帧落地后到下一拍之间的窗口交互空窗期，兼顾连贯性与随时可打断性。
const AUTO_SCROLL_INTERVAL: Duration = Duration::from_millis(120);

/// 自动滚动连续收到静止帧达到该次数后判定已触底并自动停止。
const AUTO_SCROLL_STATIC_STOP_THRESHOLD: u8 = 3;

/// 自动滚动连续失配达到该次数后自动停机，防止在异步弹窗或失效区域无限空转。
const AUTO_SCROLL_UNMATCHED_STOP_THRESHOLD: u8 = 3;

/// 自动滚动被动/正常结束的原因，供 UI 给出精准中文反馈。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AutoScrollStopReason {
    /// 连续多帧未检测到画面变化，判定已滚到底部或无法继续滚动。
    ReachedEnd,
    /// 连续多帧无法与基准画布匹配，主动安全停止。
    UnmatchedStalled,
    /// 长截图画布已触及安全尺寸或像素上限。
    CanvasLimitReached,
}

/// 管理长截图滚轮增量排队与自动连续滚动的纯状态驱动器。
#[derive(Debug, Clone, Default)]
pub(crate) struct LongCaptureScrollDriver {
    /// 是否正处于自动滚到底状态。
    pub(crate) auto_scrolling: bool,
    /// 在途抓屏期间累积的未消费手动滚轮增量。
    pending_wheel: WheelDelta,
    /// 下一次自动滚动允许派发的单调时刻；为 `None` 表示已有抓屏在途或未启用。
    next_auto_scroll_at: Option<Instant>,
    /// 自动滚动期间连续静止帧计数。
    static_frames: u8,
    /// 自动滚动期间连续失配帧计数。
    unmatched_frames: u8,
    /// 最近一次自动停机的原因（待主状态机消费展示提示）。
    last_stop_reason: Option<AutoScrollStopReason>,
}

impl LongCaptureScrollDriver {
    /// 将新到达的手动滚轮增量合并进蓄水池；任何手动滚轮介入都会立即终止自动滚动。
    pub(crate) fn enqueue_manual_wheel(&mut self, delta: WheelDelta) {
        if delta.is_effectively_zero() {
            return;
        }
        if self.auto_scrolling {
            self.stop_auto_scroll();
        }
        if delta.x.is_finite() {
            self.pending_wheel.x = (self.pending_wheel.x + delta.x)
                .clamp(-MAX_PENDING_WHEEL_PER_AXIS, MAX_PENDING_WHEEL_PER_AXIS);
        }
        if delta.y.is_finite() {
            self.pending_wheel.y = (self.pending_wheel.y + delta.y)
                .clamp(-MAX_PENDING_WHEEL_PER_AXIS, MAX_PENDING_WHEEL_PER_AXIS);
        }
    }

    /// 开启自动滚到底模式并清零累积手动余量，在 `now` 时刻立即安排首拍滚动。
    pub(crate) fn start_auto_scroll(&mut self, now: Instant) {
        self.auto_scrolling = true;
        self.pending_wheel = WheelDelta::default();
        self.next_auto_scroll_at = Some(now);
        self.static_frames = 0;
        self.unmatched_frames = 0;
        self.last_stop_reason = None;
    }

    /// 停止自动滚动模式并清除待触发拍点。
    pub(crate) fn stop_auto_scroll(&mut self) {
        self.auto_scrolling = false;
        self.next_auto_scroll_at = None;
        self.static_frames = 0;
        self.unmatched_frames = 0;
    }

    /// 暂停录制、切换方向或退出捕获时彻底清空待处理队列与自动状态。
    pub(crate) fn clear_all(&mut self) {
        self.stop_auto_scroll();
        self.pending_wheel = WheelDelta::default();
        self.last_stop_reason = None;
    }

    /// 提取下一批准备提交到底层的滚动增量。
    ///
    /// 优先切分消化用户累积的手动滚轮余量；仅当手滚队列为空且已到达自动滚动拍点时，
    /// 才按当前长截图拼接方向返回一步自动滚动增量。
    pub(crate) fn take_ready_dispatch_delta(
        &mut self,
        direction: StitchDirection,
        now: Instant,
    ) -> Option<WheelDelta> {
        if !self.pending_wheel.is_effectively_zero() {
            let batch_x = clamp_batch_step(self.pending_wheel.x);
            let batch_y = clamp_batch_step(self.pending_wheel.y);
            self.pending_wheel.x -= batch_x;
            self.pending_wheel.y -= batch_y;
            normalize_small_residual(&mut self.pending_wheel.x);
            normalize_small_residual(&mut self.pending_wheel.y);
            let batch = WheelDelta::new(batch_x, batch_y);
            if !batch.is_effectively_zero() {
                return Some(batch);
            }
        }

        if self.auto_scrolling && self.next_auto_scroll_at.is_some_and(|due| now >= due) {
            self.next_auto_scroll_at = None;
            let step = match direction {
                StitchDirection::Vertical => WheelDelta::vertical(AUTO_SCROLL_STEP_DELTA),
                StitchDirection::Horizontal => WheelDelta::horizontal(AUTO_SCROLL_STEP_DELTA),
            };
            return Some(step);
        }

        None
    }

    /// 根据刚落地的帧拼接结果更新自动滚动静止/失配计数，并在满足停机条件时自动停机。
    pub(crate) fn record_outcome(&mut self, outcome: &StitchFrameOutcome, now: Instant) {
        if !self.auto_scrolling {
            return;
        }
        match outcome {
            StitchFrameOutcome::Stitched { .. } => {
                self.static_frames = 0;
                self.unmatched_frames = 0;
                self.next_auto_scroll_at = Some(now + AUTO_SCROLL_INTERVAL);
            }
            StitchFrameOutcome::Unchanged => {
                self.static_frames = self.static_frames.saturating_add(1);
                if self.static_frames >= AUTO_SCROLL_STATIC_STOP_THRESHOLD {
                    self.stop_auto_scroll();
                    self.last_stop_reason = Some(AutoScrollStopReason::ReachedEnd);
                } else {
                    self.next_auto_scroll_at = Some(now + AUTO_SCROLL_INTERVAL);
                }
            }
            StitchFrameOutcome::Unmatched { .. } => {
                self.unmatched_frames = self.unmatched_frames.saturating_add(1);
                if self.unmatched_frames >= AUTO_SCROLL_UNMATCHED_STOP_THRESHOLD {
                    self.stop_auto_scroll();
                    self.last_stop_reason = Some(AutoScrollStopReason::UnmatchedStalled);
                } else {
                    self.next_auto_scroll_at = Some(now + AUTO_SCROLL_INTERVAL);
                }
            }
            StitchFrameOutcome::LimitReached => {
                self.stop_auto_scroll();
                self.last_stop_reason = Some(AutoScrollStopReason::CanvasLimitReached);
            }
            StitchFrameOutcome::Ignored => {}
        }
    }

    /// 取走最近一次自动滚动停机的原因。
    pub(crate) fn take_stop_reason(&mut self) -> Option<AutoScrollStopReason> {
        self.last_stop_reason.take()
    }

    /// 返回下一次自动滚动触发时间，供主事件循环与局部重绘调度使用。
    pub(crate) fn auto_scroll_deadline(&self) -> Option<Instant> {
        self.auto_scrolling
            .then_some(self.next_auto_scroll_at)
            .flatten()
    }

    /// 判断蓄水池是否仍积压了待消耗的手动滚轮余量（仅用于测试或状态观测）。
    #[cfg(test)]
    pub(crate) fn has_pending_manual_wheel(&self) -> bool {
        !self.pending_wheel.is_effectively_zero()
    }
}

fn clamp_batch_step(value: f32) -> f32 {
    if !value.is_finite() || value.abs() < f32::EPSILON {
        return 0.0;
    }
    value.clamp(-MAX_BATCH_WHEEL_PER_AXIS, MAX_BATCH_WHEEL_PER_AXIS)
}

fn normalize_small_residual(value: &mut f32) {
    if !value.is_finite() || value.abs() < 1e-4 {
        *value = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manual_wheel_coalesces_batches_and_preserves_residual() {
        let mut driver = LongCaptureScrollDriver::default();
        let now = Instant::now();

        // 连续到达三次滚轮事件，模拟抓屏在途期间未被丢弃的高频滚动
        for _ in 0..3 {
            driver.enqueue_manual_wheel(WheelDelta::vertical(-3.0));
        }

        // 第一批受单步 6.0 封顶约束
        let first = driver
            .take_ready_dispatch_delta(StitchDirection::Vertical, now)
            .expect("应吐出第一批合并滚轮");
        assert!((first.y - (-6.0)).abs() < 1e-5);
        assert!(driver.has_pending_manual_wheel());

        // 第二批将剩余 -3.0 完整消费，不丢失任何滑动
        let second = driver
            .take_ready_dispatch_delta(StitchDirection::Vertical, now)
            .expect("应吐出第二批剩余滚轮");
        assert!((second.y - (-3.0)).abs() < 1e-5);
        assert!(!driver.has_pending_manual_wheel());
        assert!(driver
            .take_ready_dispatch_delta(StitchDirection::Vertical, now)
            .is_none());
    }

    #[test]
    fn test_auto_scroll_stops_after_three_static_frames_and_respects_manual_override() {
        let mut driver = LongCaptureScrollDriver::default();
        let start = Instant::now();
        driver.start_auto_scroll(start);

        let first = driver.take_ready_dispatch_delta(StitchDirection::Horizontal, start);
        assert_eq!(first, Some(WheelDelta::horizontal(AUTO_SCROLL_STEP_DELTA)));

        // 前两次静止继续重试并设定下一拍截止时刻
        let t1 = start + Duration::from_millis(50);
        driver.record_outcome(&StitchFrameOutcome::Unchanged, t1);
        assert!(driver.auto_scrolling);
        assert!(driver
            .take_ready_dispatch_delta(StitchDirection::Horizontal, t1)
            .is_none());
        let due = driver.auto_scroll_deadline().expect("应预约下一拍时刻");
        assert!(driver
            .take_ready_dispatch_delta(StitchDirection::Horizontal, due)
            .is_some());

        driver.record_outcome(&StitchFrameOutcome::Unchanged, due);
        assert!(driver.auto_scrolling);

        // 第三次连续静止后判定触底并自动停机
        let t3 = due + AUTO_SCROLL_INTERVAL;
        driver.record_outcome(&StitchFrameOutcome::Unchanged, t3);
        assert!(!driver.auto_scrolling);
        assert_eq!(
            driver.take_stop_reason(),
            Some(AutoScrollStopReason::ReachedEnd)
        );

        // 重新启动后若用户手动拨动滚轮，自动滚动应立刻让位
        driver.start_auto_scroll(t3);
        assert!(driver.auto_scrolling);
        driver.enqueue_manual_wheel(WheelDelta::vertical(1.5));
        assert!(!driver.auto_scrolling);
        assert_eq!(
            driver.take_ready_dispatch_delta(StitchDirection::Vertical, t3),
            Some(WheelDelta::vertical(1.5))
        );
    }
}
