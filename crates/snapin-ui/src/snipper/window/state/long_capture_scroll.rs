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

/// 自动滚动模式首拍及重置后的默认正向滚动步长（Winit 语义负数为向下/向右滚动）。
const AUTO_SCROLL_STEP_DELTA: f32 = -2.0;

/// 自动滚动自适应步长允许收缩到的最小幅值（防止微步反复无进展）。
const MIN_AUTO_STEP_DELTA: f32 = -0.5;

/// 自动滚动自适应步长允许放大的最大幅值（与手滚单批上限保持一致，防止失配跳帧）。
const MAX_AUTO_STEP_DELTA: f32 = -MAX_BATCH_WHEEL_PER_AXIS;

/// 自动滚动帧落地后到下一拍之间的窗口交互空窗期，兼顾连贯性与随时可打断性。
const AUTO_SCROLL_INTERVAL: Duration = Duration::from_millis(120);

/// 后台分析有界队列满载时触发的显式背压冷却间隔。
const BACKPRESSURE_INTERVAL: Duration = Duration::from_millis(250);

/// 自动滚动连续收到静止帧达到该次数后判定已触底并自动停止。
const AUTO_SCROLL_STATIC_STOP_THRESHOLD: u8 = 5;

/// 自动滚动连续失配达到该次数后自动停机，防止在异步弹窗或失效区域无限空转。
const AUTO_SCROLL_UNMATCHED_STOP_THRESHOLD: u8 = 5;

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
#[derive(Debug, Clone)]
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
    /// 当前捕获选区沿着拼接主轴的像素长度，用于计算自动滚动期望单步像素推进量。
    frame_main_span: u32,
    /// 自适应调节中的当前自动滚动逻辑步长（单位：Winit 行数，负值表示向下/向右）。
    auto_step_delta: f32,
    /// 上一拍实际派发且等待拼接位移动作反馈的自动滚动逻辑步长。
    last_dispatched_auto_delta: Option<f32>,
    /// 因后台分析队列满载触发的派发冷却截止时刻。
    pending_cooldown_until: Option<Instant>,
    /// 最近一次手动滚轮累加是否超出蓄水池单轴上限并被限流截断。
    wheel_overflowed: bool,
}

impl Default for LongCaptureScrollDriver {
    fn default() -> Self {
        Self {
            auto_scrolling: false,
            pending_wheel: WheelDelta::default(),
            next_auto_scroll_at: None,
            static_frames: 0,
            unmatched_frames: 0,
            last_stop_reason: None,
            frame_main_span: 0,
            auto_step_delta: AUTO_SCROLL_STEP_DELTA,
            last_dispatched_auto_delta: None,
            pending_cooldown_until: None,
            wheel_overflowed: false,
        }
    }
}

impl LongCaptureScrollDriver {
    /// 更新当前选区沿拼接主轴的像素长度。
    pub(crate) fn set_frame_main_span(&mut self, main_span: u32) {
        self.frame_main_span = main_span;
    }

    /// 将新到达的手动滚轮增量合并进蓄水池；任何手动滚轮介入都会立即终止自动滚动。
    pub(crate) fn enqueue_manual_wheel(&mut self, delta: WheelDelta) {
        if delta.is_effectively_zero() {
            return;
        }
        if self.auto_scrolling {
            self.stop_auto_scroll();
        }
        if delta.x.is_finite() {
            let next_x = self.pending_wheel.x + delta.x;
            if next_x.abs() > MAX_PENDING_WHEEL_PER_AXIS {
                self.wheel_overflowed = true;
            }
            self.pending_wheel.x =
                next_x.clamp(-MAX_PENDING_WHEEL_PER_AXIS, MAX_PENDING_WHEEL_PER_AXIS);
        }
        if delta.y.is_finite() {
            let next_y = self.pending_wheel.y + delta.y;
            if next_y.abs() > MAX_PENDING_WHEEL_PER_AXIS {
                self.wheel_overflowed = true;
            }
            self.pending_wheel.y =
                next_y.clamp(-MAX_PENDING_WHEEL_PER_AXIS, MAX_PENDING_WHEEL_PER_AXIS);
        }
    }

    /// 消费最近一次手动滚轮蓄水池是否触发溢出限流的标志。
    pub(crate) fn take_wheel_overflowed(&mut self) -> bool {
        let overflowed = self.wheel_overflowed;
        self.wheel_overflowed = false;
        overflowed
    }

    /// 后台分析通道满载时记录显式背压冷却窗口，延后下次滚动抓屏派发。
    pub(crate) fn record_backpressure(&mut self, now: Instant) {
        let resume_at = now + BACKPRESSURE_INTERVAL;
        self.pending_cooldown_until = Some(resume_at);
        self.last_dispatched_auto_delta = None;
        if self.auto_scrolling {
            self.next_auto_scroll_at = Some(resume_at);
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
        self.auto_step_delta = AUTO_SCROLL_STEP_DELTA;
        self.last_dispatched_auto_delta = None;
        self.pending_cooldown_until = None;
        self.wheel_overflowed = false;
    }

    /// 停止自动滚动模式并清除待触发拍点。
    pub(crate) fn stop_auto_scroll(&mut self) {
        self.auto_scrolling = false;
        self.next_auto_scroll_at = None;
        self.static_frames = 0;
        self.unmatched_frames = 0;
        self.last_dispatched_auto_delta = None;
    }

    /// 暂停录制、切换方向或退出捕获时彻底清空待处理队列与自动状态。
    pub(crate) fn clear_all(&mut self) {
        self.stop_auto_scroll();
        self.pending_wheel = WheelDelta::default();
        self.last_stop_reason = None;
        self.pending_cooldown_until = None;
        self.wheel_overflowed = false;
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
        if self.pending_cooldown_until.is_some_and(|until| now < until) {
            return None;
        }
        self.pending_cooldown_until = None;

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
            let step_delta = self.auto_step_delta;
            self.last_dispatched_auto_delta = Some(step_delta);
            let step = match direction {
                StitchDirection::Vertical => WheelDelta::vertical(step_delta),
                StitchDirection::Horizontal => WheelDelta::horizontal(step_delta),
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
            StitchFrameOutcome::Stitched { offset, .. } => {
                self.static_frames = 0;
                self.unmatched_frames = 0;
                self.adapt_auto_step(*offset);
                self.next_auto_scroll_at = Some(now + AUTO_SCROLL_INTERVAL);
            }
            StitchFrameOutcome::Unchanged => {
                self.static_frames = self.static_frames.saturating_add(1);
                // 画面重新与锚点对齐，清除此前瞬态骨架屏/弹窗带来的失配计数。
                self.unmatched_frames = 0;
                if self.static_frames >= AUTO_SCROLL_STATIC_STOP_THRESHOLD {
                    self.stop_auto_scroll();
                    self.last_stop_reason = Some(AutoScrollStopReason::ReachedEnd);
                } else {
                    self.next_auto_scroll_at = Some(now + AUTO_SCROLL_INTERVAL);
                }
            }
            StitchFrameOutcome::Unmatched { .. } => {
                self.unmatched_frames = self.unmatched_frames.saturating_add(1);
                self.static_frames = 0;
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

    fn adapt_auto_step(&mut self, offset: i32) {
        let Some(prev_delta) = self.last_dispatched_auto_delta.take() else {
            return;
        };
        if self.frame_main_span == 0 || offset <= 0 {
            return;
        }
        let span = self.frame_main_span as f32;
        let target_px = (span * 0.30).clamp(36.0, 220.0).min(span * 0.45);
        let ratio = (target_px / (offset as f32)).clamp(0.5, 2.0);
        let proposed = prev_delta * ratio;
        let blended = prev_delta * 0.5 + proposed * 0.5;
        self.auto_step_delta = blended.clamp(MAX_AUTO_STEP_DELTA, MIN_AUTO_STEP_DELTA);
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
    fn test_auto_scroll_stops_after_five_static_frames_and_respects_manual_override() {
        let mut driver = LongCaptureScrollDriver::default();
        let start = Instant::now();
        driver.start_auto_scroll(start);

        let first = driver.take_ready_dispatch_delta(StitchDirection::Horizontal, start);
        assert_eq!(first, Some(WheelDelta::horizontal(AUTO_SCROLL_STEP_DELTA)));

        // 前四次静止继续重试并设定下一拍截止时刻
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

        let mut current = due;
        for _ in 1..4 {
            driver.record_outcome(&StitchFrameOutcome::Unchanged, current);
            assert!(driver.auto_scrolling);
            current += AUTO_SCROLL_INTERVAL;
        }

        // 第五次连续静止后判定触底并自动停机
        driver.record_outcome(&StitchFrameOutcome::Unchanged, current);
        assert!(!driver.auto_scrolling);
        assert_eq!(
            driver.take_stop_reason(),
            Some(AutoScrollStopReason::ReachedEnd)
        );

        // 重新启动后若用户手动拨动滚轮，自动滚动应立刻让位
        driver.start_auto_scroll(current);
        assert!(driver.auto_scrolling);
        driver.enqueue_manual_wheel(WheelDelta::vertical(1.5));
        assert!(!driver.auto_scrolling);
        assert_eq!(
            driver.take_ready_dispatch_delta(StitchDirection::Vertical, current),
            Some(WheelDelta::vertical(1.5))
        );
    }

    #[test]
    fn test_auto_scroll_tolerates_transient_unmatched_and_recovers_on_stitch() {
        let mut driver = LongCaptureScrollDriver::default();
        let mut now = Instant::now();
        driver.start_auto_scroll(now);

        // 连续 3 帧瞬态失配（如懒加载骨架屏）不应提前停止自动滚动
        for _ in 0..3 {
            driver.record_outcome(&StitchFrameOutcome::Unmatched { confidence: 0.2 }, now);
            assert!(driver.auto_scrolling);
            now += AUTO_SCROLL_INTERVAL;
        }

        // 内容恢复匹配后清零失配计数；再连续 5 帧失配才停机
        driver.record_outcome(
            &StitchFrameOutcome::Stitched {
                offset: 30,
                confidence: 0.95,
                sticky_header_size: 0,
                sticky_footer_size: 0,
            },
            now,
        );
        assert!(driver.auto_scrolling);

        for idx in 0..5 {
            now += AUTO_SCROLL_INTERVAL;
            driver.record_outcome(&StitchFrameOutcome::Unmatched { confidence: 0.1 }, now);
            if idx < 4 {
                assert!(driver.auto_scrolling);
            } else {
                assert!(!driver.auto_scrolling);
                assert_eq!(
                    driver.take_stop_reason(),
                    Some(AutoScrollStopReason::UnmatchedStalled)
                );
            }
        }
    }

    #[test]
    fn test_auto_scroll_adapts_step_delta_to_measured_offset() {
        let mut driver = LongCaptureScrollDriver::default();
        driver.set_frame_main_span(600);
        let mut now = Instant::now();
        driver.start_auto_scroll(now);

        let first = driver
            .take_ready_dispatch_delta(StitchDirection::Vertical, now)
            .expect("首拍应派发默认步长");
        assert!((first.y - AUTO_SCROLL_STEP_DELTA).abs() < 1e-5);

        // 实测单步仅推进 30px（显著低于期望 180px）：下一拍步长应自适应放大
        now += Duration::from_millis(20);
        driver.record_outcome(
            &StitchFrameOutcome::Stitched {
                offset: 30,
                confidence: 0.95,
                sticky_header_size: 0,
                sticky_footer_size: 0,
            },
            now,
        );
        now = driver.auto_scroll_deadline().expect("应预约下一拍时刻");
        let accelerated = driver
            .take_ready_dispatch_delta(StitchDirection::Vertical, now)
            .expect("应派发放大后的步长");
        assert!(accelerated.y < AUTO_SCROLL_STEP_DELTA - 0.5);
        assert!(accelerated.y >= MAX_AUTO_STEP_DELTA);

        // 实测单步过大 (320px > 180px)：下一拍应温和回调防止过冲失配
        now += Duration::from_millis(20);
        driver.record_outcome(
            &StitchFrameOutcome::Stitched {
                offset: 320,
                confidence: 0.95,
                sticky_header_size: 0,
                sticky_footer_size: 0,
            },
            now,
        );
        now = driver.auto_scroll_deadline().expect("应预约下一拍时刻");
        let throttled = driver
            .take_ready_dispatch_delta(StitchDirection::Vertical, now)
            .expect("应派发回调后的步长");
        assert!(throttled.y > accelerated.y);
        assert!(throttled.y <= MIN_AUTO_STEP_DELTA);
    }

    #[test]
    fn test_wheel_overflow_and_backpressure_cooldown() {
        let mut driver = LongCaptureScrollDriver::default();
        driver.enqueue_manual_wheel(WheelDelta::vertical(-8.0));
        assert!(!driver.take_wheel_overflowed());

        // 累加超出 12.0 上限触发溢出标记，消费一次后自动复位
        driver.enqueue_manual_wheel(WheelDelta::vertical(-8.0));
        assert!(driver.take_wheel_overflowed());
        assert!(!driver.take_wheel_overflowed());

        // 背压冷却期间暂缓手动余量与自动节拍派发，冷却结束后自动恢复
        let now = Instant::now();
        driver.record_backpressure(now);
        assert!(driver
            .take_ready_dispatch_delta(StitchDirection::Vertical, now + Duration::from_millis(100))
            .is_none());
        let resumed = driver
            .take_ready_dispatch_delta(StitchDirection::Vertical, now + BACKPRESSURE_INTERVAL);
        assert_eq!(resumed, Some(WheelDelta::vertical(-6.0)));
    }
}
