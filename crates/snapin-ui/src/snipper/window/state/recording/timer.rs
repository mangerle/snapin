//! 录屏界面的单调时钟秒表。
//!
//! 后台编码会因分辨率、磁盘负载或关键帧计算而阻塞帧事件。界面计时如果只依赖
//! 编码成功后的进度回传，就会在卡顿时明显落后。本计时器在 UI 线程上独立累计
//! 活动区间，暂停和停止时冻结，不依赖编码吞吐。

use std::time::{Duration, Instant};

/// 录制会话的有效时长计时状态。
pub(super) struct RecordingElapsedTimer {
    elapsed: Duration,
    active_since: Option<Instant>,
}

impl RecordingElapsedTimer {
    /// 创建尚未开始的计时器。
    pub(super) fn new() -> Self {
        Self {
            elapsed: Duration::ZERO,
            active_since: None,
        }
    }

    /// 清零并从给定时刻开始计时。
    pub(super) fn start(&mut self, now: Instant) {
        self.elapsed = Duration::ZERO;
        self.active_since = Some(now);
    }

    /// 冻结当前有效时长；重复调用保持幂等。
    pub(super) fn pause(&mut self, now: Instant) {
        if let Some(started) = self.active_since.take() {
            self.elapsed = self
                .elapsed
                .saturating_add(now.saturating_duration_since(started));
        }
    }

    /// 从暂停状态继续累计，不清空已经录制的时长。
    pub(super) fn resume(&mut self, now: Instant) {
        if self.active_since.is_none() {
            self.active_since = Some(now);
        }
    }

    /// 读取当前已累计时长。
    pub(super) fn elapsed(&self) -> Duration {
        self.elapsed
    }

    /// 用最终媒体时长覆盖界面计时。
    pub(super) fn finish(&mut self, duration: Duration) {
        self.elapsed = duration;
        self.active_since = None;
    }

    /// 重置为未开始状态。
    pub(super) fn reset(&mut self) {
        self.elapsed = Duration::ZERO;
        self.active_since = None;
    }

    /// 推进到给定时刻，并返回显示秒数是否发生变化。
    pub(super) fn refresh(&mut self, now: Instant) -> bool {
        let previous_second = self.elapsed.as_secs();
        if let Some(started) = self.active_since.take() {
            self.elapsed = self
                .elapsed
                .saturating_add(now.saturating_duration_since(started));
            self.active_since = Some(now);
        }
        self.elapsed.as_secs() != previous_second
    }

    /// 吸收后台回传时长作为防回退下限。
    ///
    /// 正常情况下界面单调时钟更新更及时；仅在定时唤醒异常时使用后台值兜底。
    pub(super) fn observe(&mut self, elapsed: Duration, now: Instant) {
        self.refresh(now);
        self.elapsed = self.elapsed.max(elapsed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_refresh_uses_monotonic_time_independent_of_frame_events() {
        let start = Instant::now();
        let mut timer = RecordingElapsedTimer::new();
        timer.start(start);
        assert!(!timer.refresh(start + Duration::from_millis(900)));
        assert!(timer.refresh(start + Duration::from_millis(1_100)));
        assert_eq!(timer.elapsed(), Duration::from_millis(1_100));
    }

    #[test]
    fn test_pause_and_resume_exclude_paused_interval() {
        let start = Instant::now();
        let mut timer = RecordingElapsedTimer::new();
        timer.start(start);
        timer.pause(start + Duration::from_secs(2));
        timer.refresh(start + Duration::from_secs(7));
        assert_eq!(timer.elapsed(), Duration::from_secs(2));
        timer.resume(start + Duration::from_secs(7));
        timer.refresh(start + Duration::from_secs(9));
        assert_eq!(timer.elapsed(), Duration::from_secs(4));
    }

    #[test]
    fn test_finish_uses_authoritative_media_duration() {
        let start = Instant::now();
        let mut timer = RecordingElapsedTimer::new();
        timer.start(start);
        timer.finish(Duration::from_millis(2_345));
        assert_eq!(timer.elapsed(), Duration::from_millis(2_345));
        assert!(!timer.refresh(start + Duration::from_secs(10)));
        assert_eq!(timer.elapsed(), Duration::from_millis(2_345));
    }
}
