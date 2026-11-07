//! 无损动画 WebP 流式编码。
//!
//! `image` 的 WebP 编码器只支持单帧，因此这里逐帧生成无损 VP8L 块，再按 WebP
//! 动画容器规范写入 `VP8X`、`ANIM` 与 `ANMF` 块。文件始终流式写入，只暂存当前
//! 帧的压缩结果，避免长时间录制时把全部画面堆在内存中。

use super::FrameEncoder;
use image::codecs::webp::WebPEncoder;
use image::{ExtendedColorType, ImageEncoder, RgbaImage};
use std::fs::File;
use std::io::{Seek, SeekFrom, Write};

const WEBP_ANIMATION_FLAG: u8 = 0x02;
const WEBP_ALPHA_FLAG: u8 = 0x10;

/// 动画 WebP 编码器。
pub(super) struct AnimatedWebpEncoder {
    file: File,
    width: u32,
    height: u32,
    frame_duration_ms: u32,
    frame_count: u64,
}

impl AnimatedWebpEncoder {
    /// 写入可回填长度的动画 WebP 容器头。
    ///
    /// # Errors
    ///
    /// 尺寸非法或写入容器头失败时返回中文错误。
    pub(super) fn new(file: File, width: u32, height: u32, fps: u32) -> Result<Self, String> {
        if width == 0 || height == 0 || width > 16_777_216 || height > 16_777_216 {
            return Err(format!("WebP 动画画布尺寸非法: {width}x{height}"));
        }
        let mut encoder = Self {
            file,
            width,
            height,
            frame_duration_ms: 1000_u32 / fps.max(1),
            frame_count: 0,
        };
        encoder.write_header()?;
        Ok(encoder)
    }

    fn write_header(&mut self) -> Result<(), String> {
        self.file
            .write_all(b"RIFF")
            .and_then(|()| self.file.write_all(&0_u32.to_le_bytes()))
            .and_then(|()| self.file.write_all(b"WEBP"))
            .map_err(|error| format!("写入 WebP 容器头失败: {error}"))?;

        self.file
            .write_all(b"VP8X")
            .and_then(|()| self.file.write_all(&10_u32.to_le_bytes()))
            .and_then(|()| {
                self.file
                    .write_all(&[WEBP_ANIMATION_FLAG | WEBP_ALPHA_FLAG, 0, 0, 0])
            })
            .and_then(|()| write_u24(&mut self.file, self.width - 1))
            .and_then(|()| write_u24(&mut self.file, self.height - 1))
            .and_then(|()| self.file.write_all(b"ANIM"))
            .and_then(|()| self.file.write_all(&6_u32.to_le_bytes()))
            .and_then(|()| self.file.write_all(&[0, 0, 0, 0]))
            .and_then(|()| self.file.write_all(&0_u16.to_le_bytes()))
            .map_err(|error| format!("写入 WebP 动画头失败: {error}"))
    }

    /// 将单帧无损编码结果中的 VP8L 块写入 ANMF。
    fn write_animation_frame(&mut self, frame_chunk: &[u8]) -> Result<(), String> {
        let payload_len = 16_usize
            .checked_add(frame_chunk.len())
            .ok_or_else(|| "WebP 动画帧长度溢出".to_string())?;
        let padding = payload_len % 2;
        self.file
            .write_all(b"ANMF")
            .map_err(|error| format!("写入 WebP 动画帧头失败: {error}"))?;
        let payload_len =
            u32::try_from(payload_len).map_err(|_| "WebP 动画帧超过 4GiB 容器上限".to_string())?;
        self.file
            .write_all(&payload_len.to_le_bytes())
            .and_then(|()| write_u24(&mut self.file, 0))
            .and_then(|()| write_u24(&mut self.file, 0))
            .and_then(|()| write_u24(&mut self.file, self.width - 1))
            .and_then(|()| write_u24(&mut self.file, self.height - 1))
            .and_then(|()| write_u24(&mut self.file, self.frame_duration_ms))
            .and_then(|()| self.file.write_all(&[0]))
            .and_then(|()| self.file.write_all(frame_chunk))
            .and_then(|()| self.file.write_all(&[0; 1][..padding]))
            .map_err(|error| format!("写入 WebP 动画帧失败: {error}"))
    }

    fn finish_payload(&mut self) -> Result<(), String> {
        let end = self
            .file
            .stream_position()
            .map_err(|error| format!("读取 WebP 动画文件长度失败: {error}"))?;
        let riff_len = end
            .checked_sub(8)
            .ok_or_else(|| "WebP 动画容器长度非法".to_string())?;
        let riff_len =
            u32::try_from(riff_len).map_err(|_| "WebP 动画超过 4GiB 容器上限".to_string())?;
        self.file
            .seek(SeekFrom::Start(4))
            .and_then(|_| self.file.write_all(&riff_len.to_le_bytes()))
            .and_then(|()| self.file.seek(SeekFrom::Start(end)))
            .map_err(|error| format!("回填 WebP 容器长度失败: {error}"))?;
        self.file
            .sync_all()
            .map_err(|error| format!("同步 WebP 文件到磁盘失败: {error}"))?;
        Ok(())
    }
}

impl FrameEncoder for AnimatedWebpEncoder {
    fn write_frame(&mut self, frame: &RgbaImage, _duration_ms: u32) -> Result<(), String> {
        let mut encoded = Vec::new();
        WebPEncoder::new_lossless(&mut encoded)
            .write_image(
                frame.as_raw(),
                self.width,
                self.height,
                ExtendedColorType::Rgba8,
            )
            .map_err(|error| format!("编码 WebP 单帧失败: {error}"))?;
        let frame_chunk = find_chunk(&encoded, b"VP8L")
            .ok_or_else(|| "WebP 编码器未返回 VP8L 数据块".to_string())?;
        self.write_animation_frame(frame_chunk)?;
        self.frame_count = self.frame_count.saturating_add(1);
        Ok(())
    }

    fn finish(mut self: Box<Self>) -> Result<(), String> {
        if self.frame_count == 0 {
            return Err("没有采集到可写入的 WebP 动画帧".to_string());
        }
        self.finish_payload()
    }
}

/// 从 RIFF 字节流中定位指定块，返回块头加块数据的完整切片。
fn find_chunk<'a>(bytes: &'a [u8], chunk_id: &[u8; 4]) -> Option<&'a [u8]> {
    let mut offset = 12_usize;
    while offset.checked_add(8)? <= bytes.len() {
        let id = bytes.get(offset..offset + 4)?;
        let size = u32::from_le_bytes(bytes.get(offset + 4..offset + 8)?.try_into().ok()?);
        let data_start = offset + 8;
        let data_end = data_start.checked_add(size as usize)?;
        let chunk_end = data_end.checked_add(size as usize % 2)?;
        if chunk_end > bytes.len() {
            return None;
        }
        if id == chunk_id {
            return bytes.get(offset..chunk_end);
        }
        offset = chunk_end;
    }
    None
}

fn write_u24(writer: &mut File, value: u32) -> std::io::Result<()> {
    let bytes = value.min(0x00FF_FFFF).to_le_bytes();
    writer.write_all(&bytes[..3])
}
