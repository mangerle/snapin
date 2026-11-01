//! 存储领域错误模型。

use thiserror::Error;

/// 图像保存与缓存持久化错误。
#[derive(Debug, Error)]
pub enum StorageError {
    /// 本地文件读取、创建或写入失败
    #[error("图像存储 I/O 操作失败，路径: {path}，原因: {source}")]
    Io {
        /// 失败的绝对或相对路径
        path: String,
        /// 底层 I/O 错误
        #[source]
        source: std::io::Error,
    },

    /// 保存路径或扩展名不符合要求
    #[error("图像保存路径不合法，路径: {path}，原因: {message}")]
    InvalidSavePath {
        /// 用户选择的路径
        path: String,
        /// 可直接展示的中文原因
        message: String,
    },
}
