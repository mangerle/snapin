//! 录屏输出编码器统一入口。
//!
//! 三种格式共享同一套逐帧写入与收尾契约，采集线程无需了解容器细节。编码器一直
//! 持有输出文件并在每帧完成时立即写盘；取消或失败时由上层删除未完成文件。

mod gif;
mod mp4;
mod webp;

use crate::recording::RecordingFormat;
use gif::GifEncoder;
use image::RgbaImage;
use mp4::H264Mp4Encoder;
use std::fs::File;
use webp::AnimatedWebpEncoder;

/// 逐帧媒体编码器。
pub(super) trait FrameEncoder: Send {
    /// 写入一帧 RGBA 图像。
    ///
    /// # Errors
    ///
    /// 单帧编码、容器写入或磁盘 I/O 失败时返回中文错误。
    fn write_frame(&mut self, frame: &RgbaImage, duration_ms: u32) -> Result<(), String>;

    /// 完成文件尾或容器索引并同步磁盘。
    ///
    /// # Errors
    ///
    /// 收尾写入或磁盘同步失败时返回中文错误。
    fn finish(self: Box<Self>) -> Result<(), String>;
}

/// 编码器初始化参数。
pub(super) struct EncoderInit {
    /// 输出格式。
    pub(super) format: RecordingFormat,
    /// 已创建的目标文件。
    pub(super) file: File,
    /// 编码宽度。
    pub(super) width: u32,
    /// 编码高度。
    pub(super) height: u32,
    /// 目标帧率。
    pub(super) fps: u32,
}

/// 根据目标格式构建编码器。
///
/// # Errors
///
/// 尺寸不满足目标格式约束、编码器初始化失败或文件头写入失败时返回中文错误。
pub(super) fn open_encoder(init: EncoderInit) -> Result<Box<dyn FrameEncoder>, String> {
    let EncoderInit {
        format,
        file,
        width,
        height,
        fps,
    } = init;
    match format {
        RecordingFormat::Gif => Ok(Box::new(GifEncoder::new(file, width, height, fps)?)),
        RecordingFormat::WebP => Ok(Box::new(AnimatedWebpEncoder::new(
            file, width, height, fps,
        )?)),
        RecordingFormat::Mp4 => Ok(Box::new(H264Mp4Encoder::new(file, width, height, fps)?)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;
    use std::fs::OpenOptions;
    use std::io::Read;
    use std::time::Duration;
    use tempfile::tempdir;

    fn test_file(path: &std::path::Path) -> File {
        OpenOptions::new()
            .create(true)
            .truncate(true)
            .read(true)
            .write(true)
            .open(path)
            .expect("测试媒体文件应可创建")
    }

    fn test_frame(width: u32, height: u32, color: Rgba<u8>) -> RgbaImage {
        RgbaImage::from_pixel(width, height, color)
    }

    #[test]
    fn test_gif_encoder_writes_animation_bytes() {
        let dir = tempdir().expect("临时目录应可创建");
        let path = dir.path().join("test.gif");
        let mut encoder = open_encoder(EncoderInit {
            format: RecordingFormat::Gif,
            file: test_file(&path),
            width: 16,
            height: 16,
            fps: 30,
        })
        .expect("GIF 编码器应初始化");
        encoder
            .write_frame(&test_frame(16, 16, Rgba([255, 0, 0, 255])), 33)
            .expect("GIF 帧应写入");
        encoder.finish().expect("GIF 应收尾");
        let mut bytes = Vec::new();
        File::open(path)
            .expect("GIF 应可读取")
            .read_to_end(&mut bytes)
            .expect("GIF 应可读取");
        assert_eq!(&bytes[..6], b"GIF89a");
    }

    #[test]
    fn test_webp_encoder_writes_animation_container() {
        let dir = tempdir().expect("临时目录应可创建");
        let path = dir.path().join("test.webp");
        let mut encoder = open_encoder(EncoderInit {
            format: RecordingFormat::WebP,
            file: test_file(&path),
            width: 16,
            height: 16,
            fps: 30,
        })
        .expect("WebP 编码器应初始化");
        encoder
            .write_frame(&test_frame(16, 16, Rgba([0, 255, 0, 255])), 33)
            .expect("WebP 帧应写入");
        encoder.finish().expect("WebP 应收尾");
        let bytes = std::fs::read(path).expect("WebP 应可读取");
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WEBP");
        assert!(bytes.windows(4).any(|window| window == b"ANMF"));
    }

    #[test]
    fn test_mp4_encoder_writes_video_track() {
        let dir = tempdir().expect("临时目录应可创建");
        let path = dir.path().join("test.mp4");
        let mut encoder = open_encoder(EncoderInit {
            format: RecordingFormat::Mp4,
            file: test_file(&path),
            width: 16,
            height: 16,
            fps: 30,
        })
        .expect("MP4 编码器应初始化");
        for index in 0..3 {
            let value = (index * 40) as u8;
            encoder
                .write_frame(&test_frame(16, 16, Rgba([value, 0, 0, 255])), 33)
                .expect("MP4 帧应写入");
        }
        encoder.finish().expect("MP4 应收尾");
        let file = File::open(&path).expect("MP4 应可读取");
        let reader = ::mp4::Mp4Reader::read_header(
            file,
            std::fs::metadata(path).expect("MP4 元数据应存在").len(),
        )
        .expect("MP4 应可解析");
        assert_eq!(reader.tracks().len(), 1);
        assert_eq!(
            reader.tracks().values().next().map(|track| track.width()),
            Some(16)
        );
    }

    #[test]
    fn test_mp4_encoder_preserves_variable_frame_durations() {
        let dir = tempdir().expect("临时目录应可创建");
        let path = dir.path().join("variable-duration.mp4");
        let mut encoder = open_encoder(EncoderInit {
            format: RecordingFormat::Mp4,
            file: test_file(&path),
            width: 16,
            height: 16,
            fps: 30,
        })
        .expect("MP4 编码器应初始化");
        let durations = [120_u32, 80, 200];
        for (index, duration) in durations.iter().copied().enumerate() {
            let value = (index * 60) as u8;
            encoder
                .write_frame(&test_frame(16, 16, Rgba([value, 0, 0, 255])), duration)
                .expect("MP4 帧应写入");
        }
        encoder.finish().expect("MP4 应收尾");

        let file = File::open(&path).expect("MP4 应可读取");
        let mut reader = ::mp4::Mp4Reader::read_header(
            file,
            std::fs::metadata(path).expect("MP4 元数据应存在").len(),
        )
        .expect("MP4 应可解析");
        let track_id = reader
            .tracks()
            .keys()
            .next()
            .copied()
            .expect("MP4 应包含视频轨");
        assert_eq!(
            reader.sample_count(track_id).expect("应可读取 MP4 样本数"),
            durations.len() as u32
        );

        let mut expected_start = 0_u64;
        for (index, duration_ms) in durations.iter().copied().enumerate() {
            let sample_id = u32::try_from(index).expect("测试样本索引应可转换") + 1;
            let sample = reader
                .read_sample(track_id, sample_id)
                .expect("应可读取 MP4 样本")
                .expect("MP4 样本应存在");
            let expected_duration = duration_ms.saturating_mul(90);
            assert_eq!(sample.start_time, expected_start);
            assert_eq!(sample.duration, expected_duration);
            expected_start = expected_start.saturating_add(u64::from(expected_duration));
        }
        assert_eq!(reader.duration(), Duration::from_millis(400));
    }
}
