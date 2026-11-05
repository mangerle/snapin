//! UI 端口错误契约。

use snapin_config::config::UiLanguage;
use snapin_foundation::i18n;
use std::fmt;

/// UI 端口错误类别。
///
/// 调用方需要区分“用户可以重试的繁忙/超时”“权限被拒绝”“输入非法”等场景，
/// 不能只依赖展示字符串做分支；具体平台错误仍被压成中文 message，不泄漏第三方类型。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum PortErrorKind {
    /// 未归类的失败
    #[default]
    Other,
    /// 宿主能力暂时不可用
    Unavailable,
    /// 等待宿主响应超时
    Timeout,
    /// 权限或系统策略拒绝
    PermissionDenied,
    /// 输入参数非法
    InvalidInput,
    /// 执行器繁忙或队列已满
    Busy,
    /// 本地持久化 I/O 失败
    Io,
}

/// UI 端口操作失败原因。
///
/// 端口是表现层与宿主之间的边界，错误只需携带可直接展示的中文描述与操作类别，
/// 不向外泄漏平台或服务 crate 的第三方错误类型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortError {
    /// 失败类别，供调用方做程序化分支
    pub kind: PortErrorKind,
    /// 失败的操作名称，例如“写入剪贴板”
    pub operation: String,
    /// 可直接展示给用户的中文原因
    pub message: String,
}

impl PortError {
    /// 构造未归类的端口错误。
    pub fn new(operation: impl Into<String>, message: impl Into<String>) -> Self {
        Self::with_kind(PortErrorKind::Other, operation, message)
    }

    /// 以明确类别构造端口错误。
    pub fn with_kind(
        kind: PortErrorKind,
        operation: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            operation: operation.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for PortError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if i18n::language() == UiLanguage::EnUs {
            write!(
                f,
                "{} failed: {}",
                self.operation,
                i18n::localize_error(&self.message)
            )
        } else {
            write!(f, "{}失败: {}", self.operation, self.message)
        }
    }
}

impl std::error::Error for PortError {}
