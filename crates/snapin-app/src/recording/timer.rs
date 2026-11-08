//! 录制有效时长与真实帧间隔计时。
//!
//! 编码吞吐下降时，成功编码帧数不能代表真实录制时长。这里使用单调时钟累计
//! 活动区间，并在暂停时冻结、恢复时重新建立帧间隔起点，保证界面计时与视频时间轴
//! 都不包含暂停区间，也不受丢帧影响。

use std::time::{Duration, Instant};

/// 录制有效时长计时器。
pub(super) struct ActiveRecordingTimer {
    active_since: Option<Instant>,
    accumulated: Duration,
    last_frame_at: Option<Instant>,
    frame_remainder_micros: u64,
}

impl ActiveRecordingTimer {
    /// 从给定时刻开始计时。
    pub(super) fn new(now: Instant) -> Self {
        Self {
            active_since: Some(now),
            accumulated: Duration::ZERO,
            last_frame_at: Some(now),
            frame_remainder_micros: 0,
        }
    }

    /// 冻结计时；重复调用保持幂等。
    pub(super) fn pause(&mut self, now: Instant) {
        if let Some(started) = self.active_since.take() {
            self.accumulated = self
                .accumulated
                .saturating_add(now.saturating_duration_since(started));
        }
        self.last_frame_at = None;
        self.frame_remainder_micros = 0;
    }

    /// 恢复计时；恢复后的首帧使用目标帧间隔。
    pub(super) fn resume(&mut self, now: Instant) {
        if self.active_since.is_none() {
            self.active_since = Some(now);
            self.last_frame_at = None;
            self.frame_remainder_micros = 0;
        }
    }

    /// 返回当前真实有效录制时长。
    pub(super) fn elapsed(&self, now: Instant) -> Duration {
        self.active_since.map_or(self.accumulated, |started| {
            self.accumulated
                .saturating_add(now.saturating_duration_since(started))
        })
    }

    /// 记录当前帧并返回相对上一帧的真实显示时长。
    ///
    /// 首帧也包含从录制开始到首帧到达的启动延迟，避免视频时间轴短于界面计时。
    pub(super) fn frame_duration_ms(&mut self, now: Instant, nominal_ms: u32) -> u32 {
        let duration = if let Some(previous) = self.last_frame_at {
            let total_micros = now
                .saturating_duration_since(previous)
                .as_micros()
                .saturating_add(u128::from(self.frame_remainder_micros));
            let duration_ms = u32::try_from(total_micros / 1_000)
                .unwrap_or(u32::MAX)
                .max(1);
            self.frame_remainder_micros = u64::try_from(total_micros % 1_000).unwrap_or(0);
            duration_ms
        } else {
            nominal_ms.max(1)
        };
        self.last_frame_at = Some(now);
        duration
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fractional_millisecond_remainder_prevents_timeline_drift() {
        let start = Instant::now();
        let mut timer = ActiveRecordingTimer::new(start);
        let durations = [
            timer.frame_duration_ms(start + Duration::from_micros(16_667), 16),
            timer.frame_duration_ms(start + Duration::from_micros(33_334), 16),
            timer.frame_duration_ms(start + Duration::from_micros(50_001), 16),
        ];
        assert_eq!(durations, [16, 17, 17]);
    }

    #[test]
    fn test_pause_excludes_time_and_resets_frame_gap() {
        let start = Instant::now();
        let mut timer = ActiveRecordingTimer::new(start);
        assert_eq!(
            timer.frame_duration_ms(start + Duration::from_millis(120), 33),
            120
        );
        assert_eq!(
            timer.frame_duration_ms(start + Duration::from_millis(320), 33),
            200
        );
        assert_eq!(
            timer.elapsed(start + Duration::from_secs(2)),
            Duration::from_secs(2)
        );
        timer.pause(start + Duration::from_secs(2));
        assert_eq!(
            timer.elapsed(start + Duration::from_secs(7)),
            Duration::from_secs(2)
        );
        timer.resume(start + Duration::from_secs(7));
        assert_eq!(
            timer.elapsed(start + Duration::from_secs(9)),
            Duration::from_secs(4)
        );
        assert_eq!(
            timer.frame_duration_ms(start + Duration::from_secs(9), 33),
            33
        );
    }
}
