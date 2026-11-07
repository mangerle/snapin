//! GIF 动图流式编码。

use super::FrameEncoder;
use gif::{Encoder, Frame, Repeat};
use image::RgbaImage;
use std::fs::File;

/// 按固定帧时长持续追加全帧 GIF 图像。
pub(super) struct GifEncoder {
    encoder: Encoder<File>,
    width: u16,
    height: u16,
    frame_delay_cs: u16,
}

impl GifEncoder {
    /// 创建 GIF 文件头并写入无限循环标识。
    ///
    /// # Errors
    ///
    /// 尺寸超出 GIF 上限或写入文件头失败时返回中文错误。
    pub(super) fn new(file: File, width: u32, height: u32, fps: u32) -> Result<Self, String> {
        let width = u16::try_from(width).map_err(|_| "GIF 宽度超过 65535 像素".to_string())?;
        let height = u16::try_from(height).map_err(|_| "GIF 高度超过 65535 像素".to_string())?;
        let mut encoder = Encoder::new(file, width, height, &[])
            .map_err(|error| format!("写入 GIF 文件头失败: {error}"))?;
        encoder
            .set_repeat(Repeat::Infinite)
            .map_err(|error| format!("写入 GIF 循环标识失败: {error}"))?;
        let frame_delay_cs = ((1000.0 / f32::from(fps.max(1) as u16)) / 10.0).round() as u32;
        let frame_delay_cs = frame_delay_cs.max(1);
        let frame_delay_cs =
            u16::try_from(frame_delay_cs).map_err(|_| "GIF 帧间隔超过编码器上限".to_string())?;
        Ok(Self {
            encoder,
            width,
            height,
            frame_delay_cs,
        })
    }
}

impl FrameEncoder for GifEncoder {
    fn write_frame(&mut self, frame: &RgbaImage, _duration_ms: u32) -> Result<(), String> {
        let frame =
            Frame::from_rgba_speed(self.width, self.height, &mut frame.as_raw().clone(), 10);
        let mut frame = frame;
        frame.delay = self.frame_delay_cs;
        self.encoder
            .write_frame(&frame)
            .map_err(|error| format!("写入 GIF 帧失败: {error}"))
    }

    fn finish(self: Box<Self>) -> Result<(), String> {
        let file = self
            .encoder
            .into_inner()
            .map_err(|error| format!("完成 GIF 文件尾写入失败: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("同步 GIF 文件到磁盘失败: {error}"))
    }
}
