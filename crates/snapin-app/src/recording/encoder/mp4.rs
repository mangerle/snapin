//! H.264 与 MP4 流式编码。
//!
//! OpenH264 输出 Annex B 起始码格式，而 MP4 的 `avc1` 样本要求四字节长度前缀，
//! 并把 SPS/PPS 存入样本描述。本模块在第一帧编码后提取参数集、建立视频轨，
//! 随后逐帧写入 MP4，避免把整段录制缓存成大量原始 RGBA 帧。

use super::FrameEncoder;
use image::RgbaImage;
use mp4::{AvcConfig, MediaConfig, Mp4Config, Mp4Sample, Mp4Writer, TrackConfig};
use openh264::encoder::{Encoder, EncoderConfig, FrameType, UsageType};
use openh264::formats::{RgbaSliceU8, YUVBuffer};
use openh264::{OpenH264API, Timestamp};
use std::fs::File;

const MP4_TIMESCALE: u32 = 90_000;

/// H.264 MP4 编码器。
pub(super) struct H264Mp4Encoder {
    writer: Mp4Writer<File>,
    track_ready: bool,
    encoder: Encoder,
    width: u16,
    height: u16,
    frame_duration: u32,
    timestamp_ms: u64,
    frame_count: u64,
}

impl H264Mp4Encoder {
    /// 创建 H.264 编码器；MP4 视频轨在首帧参数集可用后建立。
    ///
    /// # Errors
    ///
    /// 尺寸非法或 OpenH264 初始化失败时返回中文错误。
    pub(super) fn new(file: File, width: u32, height: u32, fps: u32) -> Result<Self, String> {
        let width = u16::try_from(width).map_err(|_| "MP4 宽度超过 65535 像素".to_string())?;
        let height = u16::try_from(height).map_err(|_| "MP4 高度超过 65535 像素".to_string())?;
        if width < 2 || height < 2 || width % 2 != 0 || height % 2 != 0 {
            return Err(format!("MP4 编码要求偶数尺寸，当前为 {width}x{height}"));
        }
        let fps = fps.clamp(1, 60);
        let config = EncoderConfig::new()
            .usage_type(UsageType::ScreenContentRealTime)
            .max_frame_rate(openh264::encoder::FrameRate::from_hz(fps as f32))
            .bitrate(openh264::encoder::BitRate::from_bps(8_000_000));
        let encoder = Encoder::with_api_config(OpenH264API::from_source(), config)
            .map_err(|error| format!("初始化 H.264 编码器失败: {error}"))?;
        let frame_duration = MP4_TIMESCALE / fps;
        Ok(Self {
            writer: file_writer(file)?,
            track_ready: false,
            encoder,
            width,
            height,
            frame_duration,
            timestamp_ms: 0,
            frame_count: 0,
        })
    }

    fn encode_packet(&mut self, frame: &RgbaImage) -> Result<EncodedPacket, String> {
        let rgba = RgbaSliceU8::new(
            frame.as_raw(),
            (usize::from(self.width), usize::from(self.height)),
        );
        let yuv = YUVBuffer::from_rgba8_source(rgba);
        let timestamp = Timestamp::from_millis(self.timestamp_ms);
        let bitstream = self
            .encoder
            .encode_at(&yuv, timestamp)
            .map_err(|error| format!("编码 H.264 帧失败: {error}"))?;
        let frame_type = bitstream.frame_type();
        let annex_b = bitstream.to_vec();
        let mut packet = EncodedPacket::default();
        for nal in annex_b_nals(&annex_b) {
            let Some(header) = nal.first().copied() else {
                continue;
            };
            match header & 0x1F {
                7 => packet.sps = Some(nal.to_vec()),
                8 => packet.pps = Some(nal.to_vec()),
                _ => {
                    let len = u32::try_from(nal.len())
                        .map_err(|_| "单个 H.264 NAL 超过 4GiB".to_string())?;
                    packet.sample.extend_from_slice(&len.to_be_bytes());
                    packet.sample.extend_from_slice(nal);
                }
            }
        }
        packet.is_sync = matches!(frame_type, FrameType::IDR | FrameType::I);
        Ok(packet)
    }

    fn ensure_track(&mut self, packet: &EncodedPacket) -> Result<(), String> {
        if self.track_ready {
            return Ok(());
        }
        let sps = packet
            .sps
            .as_ref()
            .ok_or_else(|| "H.264 首帧缺少 SPS 参数集".to_string())?;
        let pps = packet
            .pps
            .as_ref()
            .ok_or_else(|| "H.264 首帧缺少 PPS 参数集".to_string())?;
        self.writer
            .add_track(&avc_track_config(self.width, self.height, sps, pps))
            .map_err(|error| format!("建立 MP4 视频轨失败: {error}"))?;
        self.track_ready = true;
        Ok(())
    }
}

impl FrameEncoder for H264Mp4Encoder {
    fn write_frame(&mut self, frame: &RgbaImage, duration_ms: u32) -> Result<(), String> {
        let packet = self.encode_packet(frame)?;
        if packet.sample.is_empty() {
            return Err("H.264 编码结果不包含可写入的视频 NAL".to_string());
        }
        self.ensure_track(&packet)?;
        let writer = &mut self.writer;
        let duration = self.frame_duration;
        writer
            .write_sample(
                1,
                &Mp4Sample {
                    start_time: self.timestamp_ms,
                    duration,
                    rendering_offset: 0,
                    is_sync: packet.is_sync,
                    bytes: packet.sample.into(),
                },
            )
            .map_err(|error| format!("写入 MP4 视频样本失败: {error}"))?;
        self.timestamp_ms = self
            .timestamp_ms
            .saturating_add(u64::from(duration_ms.max(1)));
        self.frame_count = self.frame_count.saturating_add(1);
        Ok(())
    }

    fn finish(mut self: Box<Self>) -> Result<(), String> {
        if self.frame_count == 0 {
            return Err("没有采集到可写入的 MP4 视频帧".to_string());
        }
        self.writer
            .write_end()
            .map_err(|error| format!("完成 MP4 文件索引写入失败: {error}"))?;
        let file = self.writer.into_writer();
        file.sync_all()
            .map_err(|error| format!("同步 MP4 文件到磁盘失败: {error}"))
    }
}

#[derive(Default)]
struct EncodedPacket {
    sample: Vec<u8>,
    sps: Option<Vec<u8>>,
    pps: Option<Vec<u8>>,
    is_sync: bool,
}

fn file_writer(file: File) -> Result<Mp4Writer<File>, String> {
    let config = Mp4Config {
        major_brand: "isom"
            .parse()
            .map_err(|_| "构造 MP4 主品牌失败".to_string())?,
        minor_version: 512,
        compatible_brands: ["isom", "iso2", "avc1", "mp41"]
            .into_iter()
            .map(|brand| {
                brand
                    .parse()
                    .map_err(|_| format!("构造 MP4 兼容品牌失败: {brand}"))
            })
            .collect::<Result<Vec<_>, _>>()?,
        timescale: MP4_TIMESCALE,
    };
    Mp4Writer::write_start(file, &config).map_err(|error| format!("写入 MP4 文件头失败: {error}"))
}

fn avc_track_config(width: u16, height: u16, sps: &[u8], pps: &[u8]) -> TrackConfig {
    TrackConfig {
        track_type: mp4::TrackType::Video,
        timescale: MP4_TIMESCALE,
        language: "und".to_string(),
        media_conf: MediaConfig::AvcConfig(AvcConfig {
            width,
            height,
            seq_param_set: sps.to_vec(),
            pic_param_set: pps.to_vec(),
        }),
    }
}

/// 从 Annex B 码流中切出不含起始码的 NAL 单元。
fn annex_b_nals(bytes: &[u8]) -> Vec<&[u8]> {
    let mut nals = Vec::new();
    let mut cursor = 0_usize;
    let mut current_start = None;
    while cursor + 3 <= bytes.len() {
        if bytes[cursor..cursor + 3] == [0, 0, 1] {
            if let Some(start) = current_start {
                let mut end = cursor;
                if end > start && bytes[end - 1] == 0 {
                    end -= 1;
                }
                if end > start {
                    nals.push(&bytes[start..end]);
                }
            }
            cursor += 3;
            current_start = Some(cursor);
        } else {
            cursor += 1;
        }
    }
    if let Some(start) = current_start {
        if start < bytes.len() {
            nals.push(&bytes[start..]);
        }
    }
    nals
}

#[cfg(test)]
mod tests {
    use super::annex_b_nals;

    #[test]
    fn test_split_annex_b_nals_with_three_and_four_byte_start_codes() {
        let data = [0, 0, 0, 1, 0x67, 1, 2, 0, 0, 1, 0x68, 3];
        let nals = annex_b_nals(&data);
        assert_eq!(nals.len(), 2);
        assert_eq!(nals[0], &[0x67, 1, 2]);
        assert_eq!(nals[1], &[0x68, 3]);
    }
}
