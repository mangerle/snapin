//! 各大翻译服务商具体驱动实现。

mod aliyun;
mod baidu;
mod bing_free;
mod google_free;
mod manager;
mod youdao;

pub use manager::create_translator;

#[cfg(test)]
mod contract_tests;
