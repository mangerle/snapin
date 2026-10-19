//! 各大翻译服务商具体驱动实现。

pub mod aliyun;
pub mod baidu;
pub mod bing_free;
pub mod google_free;
pub mod manager;
pub mod youdao;

pub use aliyun::AliyunTranslator;
pub use baidu::BaiduTranslator;
pub use bing_free::BingFreeTranslator;
pub use google_free::GoogleFreeTranslator;
pub use manager::create_translator;
pub use youdao::YoudaoTranslator;
