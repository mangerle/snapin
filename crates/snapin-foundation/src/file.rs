//! 外部文件的安全读取入口。
//!
//! 元数据大小检查与文件读取之间存在天然竞态：文件可能在检查后继续增长。
//! 本模块通过 `Take` 在读取阶段再次施加硬上限，确保任何调用方都不会因文件
//! 被替换、追加或指向特殊设备而一次性分配无界内存。

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

/// 读取普通文件，并保证最终缓冲区不超过指定字节数。
///
/// 先根据文件句柄元数据预分配容量以减少重分配，再通过 `Take` 读取“上限 + 1”
/// 字节进行最终判定。代价是上限大小以内的文件仍会整体驻留内存，因此调用方必须
/// 按业务场景选择合理上限；超大流式数据应使用专门的分块处理接口。
///
/// # Errors
///
/// 文件打开、元数据读取或内容读取失败，以及实际内容超过 `max_bytes` 时返回 I/O 错误。
pub fn read_file_limited(path: &Path, max_bytes: usize) -> io::Result<Vec<u8>> {
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    if metadata.len() > max_bytes as u64 {
        return Err(limit_error(metadata.len(), max_bytes));
    }

    let capacity = usize::try_from(metadata.len())
        .unwrap_or(max_bytes)
        .min(max_bytes);
    let mut bytes = Vec::with_capacity(capacity);
    file.take(max_bytes.saturating_add(1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > max_bytes {
        return Err(limit_error(bytes.len() as u64, max_bytes));
    }
    Ok(bytes)
}

/// 读取 UTF-8 文本文件，并保证最终字符串不超过指定字节数。
///
/// # Errors
///
/// 文件读取失败、内容超过 `max_bytes` 或不是有效 UTF-8 时返回 I/O 错误。
pub fn read_utf8_file_limited(path: &Path, max_bytes: usize) -> io::Result<String> {
    let bytes = read_file_limited(path, max_bytes)?;
    String::from_utf8(bytes).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("文件内容不是有效 UTF-8: {error}"),
        )
    })
}

fn limit_error(actual: u64, limit: usize) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("文件大小 {actual} 字节超过读取上限 {limit} 字节"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_file_limited_accepts_content_at_limit() {
        let dir = tempfile::tempdir().expect("应可创建测试目录");
        let path = dir.path().join("content.txt");
        std::fs::write(&path, b"12345").expect("应可写入测试文件");

        assert_eq!(
            read_file_limited(&path, 5).expect("边界内文件应可读取"),
            b"12345"
        );
    }

    #[test]
    fn test_read_file_limited_rejects_oversized_content() {
        let dir = tempfile::tempdir().expect("应可创建测试目录");
        let path = dir.path().join("too-large.txt");
        std::fs::write(&path, b"123456").expect("应可写入测试文件");

        let error = read_file_limited(&path, 5).expect_err("超限文件必须被拒绝");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("读取上限"));
    }

    #[test]
    fn test_read_utf8_file_limited_rejects_invalid_utf8() {
        let dir = tempfile::tempdir().expect("应可创建测试目录");
        let path = dir.path().join("invalid.txt");
        std::fs::write(&path, [0xff, 0xfe]).expect("应可写入测试文件");

        let error = read_utf8_file_limited(&path, 16).expect_err("非法 UTF-8 必须被拒绝");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }
}
