//! 第三方翻译服务语言代码统一映射。
//!
//! 不同服务商对同一语言使用不同代码（如日语：ja / jp / ja），
//! 统一在此转换，避免每个 provider 重复维护一份 match。

use super::config::TranslateProvider;

/// 将通用语言代码转换为目标服务商要求的代码。
pub fn normalize_language(provider: TranslateProvider, language: &str) -> &str {
    match provider {
        TranslateProvider::BingFree => match language {
            "zh" | "zh-CN" | "zh-Hans" => "zh-Hans",
            "zh-TW" | "zh-Hant" => "zh-Hant",
            "en" => "en",
            "ja" => "ja",
            "ko" => "ko",
            "fr" => "fr",
            "de" => "de",
            "es" => "es",
            "ru" => "ru",
            other => other,
        },
        TranslateProvider::GoogleFree => match language {
            "zh" | "zh-CN" | "zh-Hans" => "zh-CN",
            "zh-TW" | "zh-Hant" => "zh-TW",
            "en" => "en",
            "ja" => "ja",
            "ko" => "ko",
            "fr" => "fr",
            "de" => "de",
            "es" => "es",
            "ru" => "ru",
            other => other,
        },
        TranslateProvider::Baidu => match language {
            "zh" | "zh-CN" | "zh-Hans" => "zh",
            "zh-TW" | "zh-Hant" => "cht",
            "en" => "en",
            "ja" => "jp",
            "ko" => "kor",
            "fr" => "fra",
            "es" => "spa",
            "ru" => "ru",
            other => other,
        },
        TranslateProvider::Youdao => match language {
            "zh" | "zh-CN" | "zh-Hans" => "zh-CHS",
            "zh-TW" | "zh-Hant" => "zh-CHT",
            "en" => "en",
            "ja" => "ja",
            "ko" => "ko",
            "fr" => "fr",
            "es" => "es",
            "ru" => "ru",
            other => other,
        },
        TranslateProvider::Aliyun => match language {
            "zh" | "zh-CN" | "zh-Hans" => "zh",
            "zh-TW" | "zh-Hant" => "zh-tw",
            "en" => "en",
            "ja" => "ja",
            "ko" => "ko",
            "fr" => "fr",
            "es" => "es",
            "ru" => "ru",
            other => other,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_normalization_per_provider() {
        assert_eq!(
            normalize_language(TranslateProvider::BingFree, "zh"),
            "zh-Hans"
        );
        assert_eq!(
            normalize_language(TranslateProvider::GoogleFree, "zh-TW"),
            "zh-TW"
        );
        assert_eq!(normalize_language(TranslateProvider::Baidu, "ja"), "jp");
        assert_eq!(
            normalize_language(TranslateProvider::Youdao, "zh"),
            "zh-CHS"
        );
        assert_eq!(
            normalize_language(TranslateProvider::Aliyun, "zh-Hant"),
            "zh-tw"
        );
        assert_eq!(normalize_language(TranslateProvider::Baidu, "pt"), "pt");
    }
}
