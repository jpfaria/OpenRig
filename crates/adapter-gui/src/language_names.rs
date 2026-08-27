//! Responsibility: names a language for the person reading the screen.

use crate::locale_resolve::effective_locale;

#[derive(Debug, Clone, Copy)]
pub struct Language {
    pub code: &'static str,
    pub display: &'static str,
}

/// Returns the human-readable name of a language for display in the
/// LanguageSelector dropdown, localized to the active UI locale. Each
/// shipped UI locale lists languages in its OWN script (Japanese UI =
/// Japanese names, Chinese UI = Chinese names, etc.).
///
/// Hand-curated tables instead of `@tr(...)` because the language list
/// is built in Rust and passed to Slint as a `[string]` model — the
/// gettext catalog is the wrong layer for it. Unknown locales fall
/// through `effective_locale` to en-US.
pub fn display_name(lang_code: &str, ui_locale: &str) -> &'static str {
    let active_ui = effective_locale(ui_locale);
    match active_ui.as_str() {
        "pt-BR" => match lang_code {
            "auto" => "Auto",
            "de-DE" => "Alemão",
            "zh-CN" => "Chinês",
            "ko-KR" => "Coreano",
            "es-ES" => "Espanhol",
            "fr-FR" => "Francês",
            "hi-IN" => "Hindi",
            "en-US" => "Inglês (US)",
            "ja-JP" => "Japonês",
            "pt-BR" => "Português (Brasil)",
            _ => echo_unknown(lang_code),
        },
        "de-DE" => match lang_code {
            "auto" => "Auto",
            "de-DE" => "Deutsch",
            "zh-CN" => "Chinesisch",
            "ko-KR" => "Koreanisch",
            "es-ES" => "Spanisch",
            "fr-FR" => "Französisch",
            "hi-IN" => "Hindi",
            "en-US" => "Englisch (US)",
            "ja-JP" => "Japanisch",
            "pt-BR" => "Portugiesisch (Brasilien)",
            _ => echo_unknown(lang_code),
        },
        "es-ES" => match lang_code {
            "auto" => "Auto",
            "de-DE" => "Alemán",
            "zh-CN" => "Chino",
            "ko-KR" => "Coreano",
            "es-ES" => "Español",
            "fr-FR" => "Francés",
            "hi-IN" => "Hindi",
            "en-US" => "Inglés (EE. UU.)",
            "ja-JP" => "Japonés",
            "pt-BR" => "Portugués (Brasil)",
            _ => echo_unknown(lang_code),
        },
        "fr-FR" => match lang_code {
            "auto" => "Auto",
            "de-DE" => "Allemand",
            "zh-CN" => "Chinois",
            "ko-KR" => "Coréen",
            "es-ES" => "Espagnol",
            "fr-FR" => "Français",
            "hi-IN" => "Hindi",
            "en-US" => "Anglais (US)",
            "ja-JP" => "Japonais",
            "pt-BR" => "Portugais (Brésil)",
            _ => echo_unknown(lang_code),
        },
        "hi-IN" => match lang_code {
            "auto" => "स्वत:",
            "de-DE" => "जर्मन",
            "zh-CN" => "चीनी",
            "ko-KR" => "कोरियाई",
            "es-ES" => "स्पेनिश",
            "fr-FR" => "फ्रेंच",
            "hi-IN" => "हिन्दी",
            "en-US" => "अंग्रेज़ी (US)",
            "ja-JP" => "जापानी",
            "pt-BR" => "पुर्तगाली (ब्राज़ील)",
            _ => echo_unknown(lang_code),
        },
        "ja-JP" => match lang_code {
            "auto" => "自動",
            "de-DE" => "ドイツ語",
            "zh-CN" => "中国語",
            "ko-KR" => "韓国語",
            "es-ES" => "スペイン語",
            "fr-FR" => "フランス語",
            "hi-IN" => "ヒンディー語",
            "en-US" => "英語 (US)",
            "ja-JP" => "日本語",
            "pt-BR" => "ポルトガル語 (ブラジル)",
            _ => echo_unknown(lang_code),
        },
        "ko-KR" => match lang_code {
            "auto" => "자동",
            "de-DE" => "독일어",
            "zh-CN" => "중국어",
            "ko-KR" => "한국어",
            "es-ES" => "스페인어",
            "fr-FR" => "프랑스어",
            "hi-IN" => "힌디어",
            "en-US" => "영어 (US)",
            "ja-JP" => "일본어",
            "pt-BR" => "포르투갈어 (브라질)",
            _ => echo_unknown(lang_code),
        },
        "zh-CN" => match lang_code {
            "auto" => "自动",
            "de-DE" => "德语",
            "zh-CN" => "中文",
            "ko-KR" => "韩语",
            "es-ES" => "西班牙语",
            "fr-FR" => "法语",
            "hi-IN" => "印地语",
            "en-US" => "英语 (US)",
            "ja-JP" => "日语",
            "pt-BR" => "葡萄牙语 (巴西)",
            _ => echo_unknown(lang_code),
        },
        // Default branch covers en-US plus any locale that fell back to
        // en-US via `effective_locale`.
        _ => match lang_code {
            "auto" => "Auto",
            "de-DE" => "German",
            "zh-CN" => "Chinese",
            "ko-KR" => "Korean",
            "es-ES" => "Spanish",
            "fr-FR" => "French",
            "hi-IN" => "Hindi",
            "en-US" => "English (US)",
            "ja-JP" => "Japanese",
            "pt-BR" => "Portuguese (Brazil)",
            _ => echo_unknown(lang_code),
        },
    }
}

/// Defensive fallback for an unknown lang code. The function only fires
/// when callers pass a code outside SUPPORTED_LANGUAGES — every supported
/// code is matched explicitly in `display_name`. Returning a literal "?"
/// keeps the UI showing *something* instead of nothing.
fn echo_unknown(_code: &str) -> &'static str {
    "?"
}
