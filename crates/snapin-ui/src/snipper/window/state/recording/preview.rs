//! 录制完成后的内嵌预览播放状态。

use snapin_ports::{RecordingArtifact, RecordingPreviewFrame};
use std::time::{Duration, Instant};

/// 已落盘录制的内嵌预览与播放进度。
pub(super) struct RecordingPreview {
    artifact: Option<RecordingArtifact>,
    frame_index: usize,
    playing: bool,
    next_at: Option<Instant>,
    seeking: bool,
}

impl RecordingPreview {
    /// 创建无结果的内嵌预览状态。
    pub(super) fn new() -> Self {
        Self {
            artifact: None,
            frame_index: 0,
            playing: false,
            next_at: None,
            seeking: false,
        }
    }

    /// 保存录制产物并从第一帧开始播放。
    pub(super) fn finish(&mut self, artifact: RecordingArtifact, now: Instant) {
        self.playing = artifact.preview_frames.len() > 1;
        self.next_at = self
            .playing
            .then_some(now + Self::frame_interval(&artifact));
        self.frame_index = 0;
        self.seeking = false;
        self.artifact = Some(artifact);
    }

    /// 清空结果与播放状态。
    pub(super) fn reset(&mut self) {
        self.artifact = None;
        self.frame_index = 0;
        self.playing = false;
        self.next_at = None;
        self.seeking = false;
    }

    /// 已完成的录制产物。
    pub(super) fn artifact(&self) -> Option<&RecordingArtifact> {
        self.artifact.as_ref()
    }

    /// 当前预览帧。
    pub(super) fn frame(&self) -> Option<&RecordingPreviewFrame> {
        self.artifact.as_ref()?.preview_frames.get(self.frame_index)
    }

    /// 当前是否正在播放。
    pub(super) const fn playing(&self) -> bool {
        self.playing
    }

    /// 当前预览帧索引。
    pub(super) const fn frame_index(&self) -> usize {
        self.frame_index
    }

    /// 下一帧唤醒时刻。
    pub(super) const fn deadline(&self) -> Option<Instant> {
        self.next_at
    }

    /// 当前回放进度，范围为 0.0 到 1.0。
    pub(super) fn progress(&self) -> f32 {
        let frame_count = self
            .artifact
            .as_ref()
            .map_or(0, |artifact| artifact.preview_frames.len());
        if frame_count <= 1 {
            return 0.0;
        }
        self.frame_index as f32 / (frame_count - 1) as f32
    }

    /// 当前回放位置与总时长。
    pub(super) fn elapsed(&self) -> Option<(Duration, Duration)> {
        let artifact = self.artifact.as_ref()?;
        let progress = f64::from(self.progress());
        let elapsed = Duration::from_secs_f64(artifact.duration.as_secs_f64() * progress);
        Some((elapsed, artifact.duration))
    }

    /// 是否正在拖动进度条。
    pub(super) const fn is_seeking(&self) -> bool {
        self.seeking
    }

    /// 开始拖动进度条。
    pub(super) fn begin_seek(&mut self) {
        self.seeking = true;
        self.playing = false;
        self.next_at = None;
    }

    /// 跳转到指定进度。
    pub(super) fn seek(&mut self, progress: f32) {
        let Some(frame_count) = self
            .artifact
            .as_ref()
            .map(|artifact| artifact.preview_frames.len())
        else {
            return;
        };
        if frame_count <= 1 {
            self.frame_index = 0;
            return;
        }
        let max_index = frame_count - 1;
        let progress = progress.clamp(0.0, 1.0);
        self.frame_index = (progress * max_index as f32).round() as usize;
        self.frame_index = self.frame_index.min(max_index);
    }

    /// 结束拖动进度条。
    pub(super) fn end_seek(&mut self) -> bool {
        std::mem::take(&mut self.seeking)
    }

    /// 切换播放与暂停。
    pub(super) fn toggle(&mut self, now: Instant) {
        let frame_count = self
            .artifact
            .as_ref()
            .map_or(0, |artifact| artifact.preview_frames.len());
        if frame_count <= 1 {
            self.playing = false;
            self.next_at = None;
            return;
        }
        self.playing = !self.playing;
        self.seeking = false;
        self.next_at = self.playing.then_some(now + self.interval());
    }

    /// 按时间推进动画；返回是否发生画面变化。
    pub(super) fn advance(&mut self, now: Instant) -> bool {
        if !self.playing {
            return false;
        }
        let Some(frame_count) = self
            .artifact
            .as_ref()
            .map(|artifact| artifact.preview_frames.len())
        else {
            return false;
        };
        if frame_count <= 1 {
            self.playing = false;
            self.next_at = None;
            return false;
        }

        let interval = self.interval();
        let mut next_index = self.frame_index;
        let mut next_at = self.next_at.unwrap_or(now);
        let mut changed = false;
        let mut advanced = 0;
        while next_at <= now && advanced <= frame_count {
            next_index = (next_index + 1) % frame_count;
            next_at = next_at.checked_add(interval).unwrap_or(now + interval);
            changed = true;
            advanced += 1;
        }
        if next_at <= now {
            next_at = now + interval;
        }
        self.frame_index = next_index;
        self.next_at = Some(next_at);
        changed
    }

    /// 根据录制时长计算固定动画帧间隔。
    fn interval(&self) -> Duration {
        self.artifact
            .as_ref()
            .map_or(Duration::from_millis(250), Self::frame_interval)
    }

    /// 根据一次录制产物计算固定动画帧间隔。
    fn frame_interval(artifact: &RecordingArtifact) -> Duration {
        let frame_count = artifact.preview_frames.len().max(1) as u32;
        let milliseconds = artifact.duration.as_millis() / u128::from(frame_count);
        Duration::from_millis(u64::try_from(milliseconds.max(40)).unwrap_or(40))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use snapin_ports::RecordingPreviewFrame;
    use std::path::PathBuf;
    use std::sync::Arc;

    fn artifact() -> RecordingArtifact {
        RecordingArtifact {
            path: PathBuf::from("/tmp/snapin-preview.mp4"),
            duration: Duration::from_secs(3),
            preview_frames: Arc::new(vec![
                RecordingPreviewFrame::new(2, 2, Vec::new()),
                RecordingPreviewFrame::new(2, 2, Vec::new()),
                RecordingPreviewFrame::new(2, 2, Vec::new()),
            ]),
        }
    }

    #[test]
    fn test_preview_seek_updates_progress_and_pauses_playback() {
        let now = Instant::now();
        let mut preview = RecordingPreview::new();
        preview.finish(artifact(), now);
        preview.begin_seek();
        preview.seek(0.5);
        assert_eq!(preview.frame_index(), 1);
        assert!((preview.progress() - 0.5).abs() < f32::EPSILON);
        assert!(!preview.playing());
        assert!(preview.end_seek());
    }
}
