//! 原生 RGBA 像素载荷尺寸校验。
//!
//! IPC 与外部文件传入的宽高属于未信任输入，必须使用检查算术并限制最大像素数，
//! 防止整数回绕或恶意超大尺寸导致巨量内存分配。

/// 单张贴图允许的最大像素数量（约 1 亿像素，RGBA 约 400MB）
pub const MAX_RAW_PIXELS: u64 = 100_000_000;

/// 计算 RGBA 图像的字节长度；宽高非法、溢出或超过像素上限时返回 `None`。
pub fn checked_rgba_byte_len(width: u32, height: u32) -> Option<usize> {
    if width == 0 || height == 0 {
        return None;
    }

    let pixels = (width as u64).checked_mul(height as u64)?;
    if pixels > MAX_RAW_PIXELS {
        return None;
    }

    let bytes = pixels.checked_mul(4)?;
    usize::try_from(bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checked_rgba_byte_len_normal() {
        assert_eq!(checked_rgba_byte_len(100, 50), Some(20_000));
    }

    #[test]
    fn test_checked_rgba_byte_len_rejects_invalid_and_overflow() {
        assert_eq!(checked_rgba_byte_len(0, 50), None);
        assert_eq!(checked_rgba_byte_len(50, 0), None);
        assert_eq!(checked_rgba_byte_len(u32::MAX, u32::MAX), None);
        assert_eq!(checked_rgba_byte_len(20_000, 20_000), None);
    }
}
