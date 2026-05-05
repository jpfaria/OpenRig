
use super::*;

#[test]
fn pick_language_returns_none_for_auto_index() {
    assert!(pick_language(0).is_none());
}

/// SUPPORTED_LANGUAGES order: [auto, de-DE, zh-CN, ko-KR, es-ES,
/// fr-FR, hi-IN, en-US, ja-JP, pt-BR]. Index 1 is German, 7 is
/// English (US), 9 is Portuguese (Brazil).
#[test]
fn pick_language_returns_code_for_explicit_choice() {
    assert_eq!(pick_language(1).as_deref(), Some("de-DE"));
    assert_eq!(pick_language(7).as_deref(), Some("en-US"));
    assert_eq!(pick_language(9).as_deref(), Some("pt-BR"));
}

#[test]
fn pick_language_returns_none_for_out_of_range() {
    assert!(pick_language(99).is_none());
    assert!(pick_language(-1).is_none());
}

/// The dropdown labels must localize to the active UI language.
/// Picking en-US must yield English names; picking pt-BR must yield
/// Portuguese names. Skeleton UI locales (fr-FR, ja-JP, etc.) fall
/// back to en-US — same redirection as the runtime translations.
#[test]
fn build_language_options_uses_pt_br_names_when_ui_is_pt_br() {
    let opts = build_language_options("pt-BR");
    assert_eq!(opts[0], "Auto");
    assert_eq!(opts[1], "Alemão");
    assert_eq!(opts[7], "Inglês (US)");
    assert_eq!(opts[9], "Português (Brasil)");
}

#[test]
fn build_language_options_uses_en_us_names_when_ui_is_en_us() {
    let opts = build_language_options("en-US");
    assert_eq!(opts[0], "Auto");
    assert_eq!(opts[1], "German");
    assert_eq!(opts[7], "English (US)");
    assert_eq!(opts[9], "Portuguese (Brazil)");
}

#[test]
fn build_language_options_uses_native_script_when_ui_is_ja_jp() {
    let opts = build_language_options("ja-JP");
    assert_eq!(opts[0], "自動");
    assert_eq!(opts[8], "日本語");
}

#[test]
fn build_language_options_uses_native_script_when_ui_is_zh_cn() {
    let opts = build_language_options("zh-CN");
    assert_eq!(opts[2], "中文");
    assert_eq!(opts[7], "英语 (US)");
}

#[test]
fn build_language_options_uses_native_script_when_ui_is_ko_kr() {
    let opts = build_language_options("ko-KR");
    assert_eq!(opts[3], "한국어");
}

#[test]
fn build_language_options_has_one_entry_per_supported_language() {
    let opts = build_language_options("en-US");
    assert_eq!(opts.len(), SUPPORTED_LANGUAGES.len());
}
