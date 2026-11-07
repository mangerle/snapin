//! 贴图实体与分组领域标识。

/// 贴图全局标识。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PasterId(u64);

impl PasterId {
    /// 由全局唯一序号构造贴图标识。
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// 读取原始序号，供日志与 IPC 边界转换。
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for PasterId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

impl From<u64> for PasterId {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

/// 贴图分组标识。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GroupId(usize);

impl GroupId {
    /// 构造分组标识。
    pub const fn new(value: usize) -> Self {
        Self(value)
    }

    /// 读取原始分组序号。
    pub const fn get(self) -> usize {
        self.0
    }
}

impl std::fmt::Display for GroupId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

impl From<usize> for GroupId {
    fn from(value: usize) -> Self {
        Self(value)
    }
}
