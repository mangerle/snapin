//! 截图会话输出结果与交互配置类型定义。
//!
//! 会话载荷与决策结果已下沉至 [`snapin_ports::runner`]。
//! 本模块重导出该接口以维持表现层原有用例契约。

pub use snapin_ports::{SnipMode, SnipperLaunch, SnipperOutcome, SnipperRuntime};
