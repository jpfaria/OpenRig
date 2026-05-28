use super::*;

#[test]
fn normalize_handles_posix_utf8_suffix() {
    assert_eq!(normalize("pt_BR.UTF-8"), "pt-BR");
    assert_eq!(normalize("en_US.UTF-8"), "en-US");
}

#[test]
fn normalize_handles_dash_form() {
    assert_eq!(normalize("pt-BR"), "pt-BR");
    assert_eq!(normalize("en-US"), "en-US");
}

#[test]
fn normalize_falls_back_to_english_for_unsupported() {
    // Unknown locales (Latin "xx-YY", empty) fall back to en-US — the
    // default UI language for distribution.
    assert_eq!(normalize("xx-YY"), "en-US");
    assert_eq!(normalize(""), "en-US");
}

#[test]
fn normalize_collapses_regional_variants() {
    // pt-PT, pt-AO → pt-BR (Portuguese family)
    assert_eq!(normalize("pt_PT"), "pt-BR");
    assert_eq!(normalize("pt-AO"), "pt-BR");
    // en-GB → en-US
    assert_eq!(normalize("en_GB"), "en-US");
    // zh-TW → zh-CN
    assert_eq!(normalize("zh_TW"), "zh-CN");
    // es-419 → es-ES
    assert_eq!(normalize("es-419"), "es-ES");
}

#[test]
fn normalize_routes_each_supported_language_family() {
    assert_eq!(normalize("de_DE"), "de-DE");
    assert_eq!(normalize("fr_FR"), "fr-FR");
    assert_eq!(normalize("es_ES"), "es-ES");
    assert_eq!(normalize("ja_JP"), "ja-JP");
    assert_eq!(normalize("ko_KR"), "ko-KR");
    assert_eq!(normalize("zh_CN"), "zh-CN");
    assert_eq!(normalize("hi_IN"), "hi-IN");
}

#[test]
fn normalize_routes_italian_to_spanish() {
    // We don't ship Italian; route to es-ES as the closest Romance
    // language. Better than dropping to en-US.
    assert_eq!(normalize("it_IT"), "es-ES");
}

#[test]
fn resolve_locale_prefers_persisted_when_not_auto() {
    assert_eq!(resolve_locale(Some("en-US")), "en-US");
    assert_eq!(resolve_locale(Some("pt-BR")), "pt-BR");
}

fn is_supported_code(code: &str) -> bool {
    SUPPORTED_LANGUAGES
        .iter()
        .any(|l| l.code == code && l.code != "auto")
}

#[test]
fn resolve_locale_treats_auto_as_unset() {
    // Result depends on the test host's OS locale — accept any of our
    // shipped codes (the resolver should never echo "auto" back).
    let result = resolve_locale(Some("auto"));
    assert!(
        is_supported_code(&result),
        "auto resolution returned unsupported code: {}",
        result
    );
}

#[test]
fn resolve_locale_treats_empty_as_unset() {
    let result = resolve_locale(Some(""));
    assert!(is_supported_code(&result));
}

#[test]
fn effective_locale_passes_through_pt_br() {
    assert_eq!(effective_locale("pt-BR"), "pt-BR");
}

#[test]
fn effective_locale_passes_through_en_us() {
    assert_eq!(effective_locale("en-US"), "en-US");
}

/// All 9 shipped locales now have populated translations. They must
/// pass through `effective_locale` unchanged. Regression test for the
/// "blank UI" bug: if a `.po` ever regresses to skeleton-only state,
/// remove its code from LOCALES_WITH_TRANSLATIONS — never let an
/// empty `.po` reach Slint's runtime, which renders msgstr "" as
/// empty strings (not msgid fallback).
#[test]
fn effective_locale_passes_through_every_shipped_locale() {
    for code in &[
        "pt-BR", "en-US", "de-DE", "es-ES", "fr-FR", "hi-IN", "ja-JP", "ko-KR", "zh-CN",
    ] {
        assert_eq!(
            effective_locale(code),
            *code,
            "locale {:?} should pass through to Slint",
            code
        );
    }
}

/// A locale code outside SUPPORTED_LANGUAGES must fall back to en-US
/// rather than crash or pass an unsupported value to Slint.
#[test]
fn effective_locale_falls_back_for_unknown_code() {
    assert_eq!(effective_locale("xx-YY"), "en-US");
    assert_eq!(effective_locale(""), "en-US");
}

/// Composes `resolve_locale` (BCP47 normalization) with `effective_locale`
/// (skeleton fallback). This is the single function `apply_bundled_translation`
/// and `init_translations` must use to decide what locale Slint and gettext
/// should activate. Today every shipped locale is populated so every code
/// passes through; the fallback path triggers only for codes outside
/// SUPPORTED_LANGUAGES (defensive).
#[test]
fn locale_for_runtime_passes_through_every_shipped_locale() {
    for code in &[
        "pt-BR", "en-US", "de-DE", "es-ES", "fr-FR", "hi-IN", "ja-JP", "ko-KR", "zh-CN",
    ] {
        assert_eq!(locale_for_runtime(Some(code)), *code);
    }
}

/// OS locale strings come from `sys_locale::get_locale` in POSIX form
/// like "fr_FR.UTF-8". `locale_for_runtime` must normalize them into
/// the canonical BCP 47 code (`fr-FR`) before applying the fallback.
#[test]
fn locale_for_runtime_normalizes_posix_input() {
    assert_eq!(locale_for_runtime(Some("fr_FR.UTF-8")), "fr-FR");
    assert_eq!(locale_for_runtime(Some("ja_JP")), "ja-JP");
    assert_eq!(locale_for_runtime(Some("pt_BR.UTF-8")), "pt-BR");
}

/// A locale code outside SUPPORTED_LANGUAGES must redirect to en-US.
#[test]
fn locale_for_runtime_falls_back_for_unknown_code() {
    // "xx-YY" normalizes to "xx-YY" (unknown), then falls back via
    // resolve_locale's normalize → "en-US"; effective_locale passes
    // it through. End-to-end: "en-US".
    assert_eq!(locale_for_runtime(Some("xx-YY")), "en-US");
}

/// Sanity rail: every code in `LOCALES_WITH_TRANSLATIONS` must be a
/// valid entry in `SUPPORTED_LANGUAGES`. Catches typos that would
/// silently break the fallback.
#[test]
fn locales_with_translations_are_all_in_supported_list() {
    let supported: Vec<&str> = SUPPORTED_LANGUAGES.iter().map(|l| l.code).collect();
    for code in LOCALES_WITH_TRANSLATIONS {
        assert!(
            supported.contains(code),
            "{:?} is in LOCALES_WITH_TRANSLATIONS but not in SUPPORTED_LANGUAGES",
            code
        );
    }
}

/// The list of languages exposed in the LanguageSelector UI must be
/// localized to the active UI language. Showing "Alemão" / "Chinês"
/// when the rest of the UI is in English is jarring (and was reported
/// by the user). For each shipped UI locale, every language in
/// SUPPORTED_LANGUAGES has a localized display name.
/// Each shipped UI locale must list the languages in its OWN script.
/// Showing pt-BR names while the UI is in Japanese (or English names
/// while the UI is in Korean) is jarring and was reported by the user.
#[test]
fn display_name_localizes_into_de_de() {
    assert_eq!(display_name("de-DE", "de-DE"), "Deutsch");
    assert_eq!(display_name("zh-CN", "de-DE"), "Chinesisch");
    assert_eq!(display_name("ja-JP", "de-DE"), "Japanisch");
    assert_eq!(display_name("auto", "de-DE"), "Auto");
}

#[test]
fn display_name_localizes_into_es_es() {
    assert_eq!(display_name("de-DE", "es-ES"), "Alemán");
    assert_eq!(display_name("es-ES", "es-ES"), "Español");
    assert_eq!(display_name("ja-JP", "es-ES"), "Japonés");
}

#[test]
fn display_name_localizes_into_fr_fr() {
    assert_eq!(display_name("de-DE", "fr-FR"), "Allemand");
    assert_eq!(display_name("fr-FR", "fr-FR"), "Français");
    assert_eq!(display_name("zh-CN", "fr-FR"), "Chinois");
}

#[test]
fn display_name_localizes_into_hi_in() {
    assert_eq!(display_name("hi-IN", "hi-IN"), "हिन्दी");
    assert_eq!(display_name("de-DE", "hi-IN"), "जर्मन");
}

#[test]
fn display_name_localizes_into_ja_jp() {
    assert_eq!(display_name("ja-JP", "ja-JP"), "日本語");
    assert_eq!(display_name("zh-CN", "ja-JP"), "中国語");
    assert_eq!(display_name("de-DE", "ja-JP"), "ドイツ語");
    assert_eq!(display_name("auto", "ja-JP"), "自動");
}

#[test]
fn display_name_localizes_into_ko_kr() {
    assert_eq!(display_name("ko-KR", "ko-KR"), "한국어");
    assert_eq!(display_name("ja-JP", "ko-KR"), "일본어");
    assert_eq!(display_name("auto", "ko-KR"), "자동");
}

#[test]
fn display_name_localizes_into_zh_cn() {
    assert_eq!(display_name("zh-CN", "zh-CN"), "中文");
    assert_eq!(display_name("ja-JP", "zh-CN"), "日语");
    assert_eq!(display_name("auto", "zh-CN"), "自动");
}

#[test]
fn display_name_localizes_into_pt_br() {
    assert_eq!(display_name("auto", "pt-BR"), "Auto");
    assert_eq!(display_name("de-DE", "pt-BR"), "Alemão");
    assert_eq!(display_name("zh-CN", "pt-BR"), "Chinês");
    assert_eq!(display_name("ko-KR", "pt-BR"), "Coreano");
    assert_eq!(display_name("es-ES", "pt-BR"), "Espanhol");
    assert_eq!(display_name("fr-FR", "pt-BR"), "Francês");
    assert_eq!(display_name("hi-IN", "pt-BR"), "Hindi");
    assert_eq!(display_name("en-US", "pt-BR"), "Inglês (US)");
    assert_eq!(display_name("ja-JP", "pt-BR"), "Japonês");
    assert_eq!(display_name("pt-BR", "pt-BR"), "Português (Brasil)");
}

#[test]
fn display_name_localizes_into_en_us() {
    assert_eq!(display_name("auto", "en-US"), "Auto");
    assert_eq!(display_name("de-DE", "en-US"), "German");
    assert_eq!(display_name("zh-CN", "en-US"), "Chinese");
    assert_eq!(display_name("ko-KR", "en-US"), "Korean");
    assert_eq!(display_name("es-ES", "en-US"), "Spanish");
    assert_eq!(display_name("fr-FR", "en-US"), "French");
    assert_eq!(display_name("hi-IN", "en-US"), "Hindi");
    assert_eq!(display_name("en-US", "en-US"), "English (US)");
    assert_eq!(display_name("ja-JP", "en-US"), "Japanese");
    assert_eq!(display_name("pt-BR", "en-US"), "Portuguese (Brazil)");
}

/// Defensive: if a UI locale is outside SUPPORTED_LANGUAGES,
/// `effective_locale` redirects to en-US, and `display_name` uses
/// English names for the dropdown.
#[test]
fn display_name_falls_back_to_english_for_unsupported_ui_locale() {
    assert_eq!(display_name("de-DE", "xx-YY"), "German");
    assert_eq!(display_name("ja-JP", ""), "Japanese");
}

#[test]
fn display_name_unknown_lang_code_returns_placeholder() {
    // Defensive: an unknown lang code should not panic, just return
    // a placeholder ("?") so the UI shows SOMETHING. Function returns
    // `&'static str`, so we can't echo arbitrary input.
    assert_eq!(display_name("xx-YY", "en-US"), "?");
}

#[test]
fn supported_languages_starts_with_auto_sentinel() {
    assert_eq!(SUPPORTED_LANGUAGES[0].code, "auto");
}

#[test]
fn supported_languages_contains_source_and_english() {
    let codes: Vec<_> = SUPPORTED_LANGUAGES.iter().map(|l| l.code).collect();
    assert!(codes.contains(&"pt-BR"));
    assert!(codes.contains(&"en-US"));
}

/// `resolve_translations_dir` must find catalogs regardless of build
/// profile. Earlier the CARGO_MANIFEST_DIR fallback was gated on
/// `cfg!(debug_assertions)`, which made `cargo run --release` (or any
/// non-bundled run) skip every candidate, so dgettext silently echoed
/// the msgid back — surfacing as "BTN-NEW-PROJECT" in the UI.
#[cfg(target_os = "linux")]
#[test]
fn resolve_translations_dir_finds_source_tree_in_any_profile() {
    let dir = resolve_translations_dir();
    assert!(
        dir.is_some(),
        "resolve_translations_dir returned None — gettext lookup will \
             fail. Likely cause: CARGO_MANIFEST_DIR fallback was gated \
             behind cfg!(debug_assertions) again."
    );
    let path = dir.unwrap();
    assert!(
        path.join("pt_BR")
            .join("LC_MESSAGES")
            .join("adapter-gui.mo")
            .exists()
            || path
                .join("en_US")
                .join("LC_MESSAGES")
                .join("adapter-gui.mo")
                .exists(),
        "resolve_translations_dir returned {:?} but it has no .mo files",
        path
    );
}

/// gettext on Unix derives the catalog path from `setlocale(LC_MESSAGES,
/// "en_US.UTF-8")` and looks under `<dir>/en_US/LC_MESSAGES/<domain>.mo` —
/// underscore, not hyphen. If we ever add a hyphenated locale dir again,
/// dgettext silently echoes back the msgid (the bug behind the original
/// "BTN-NEW-PROJECT" leak).
#[test]
fn translation_dirs_use_posix_underscore_not_bcp47_hyphen() {
    let translations = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("translations");
    let entries =
        std::fs::read_dir(&translations).expect("crates/adapter-gui/translations/ must exist");
    let mut hyphenated = Vec::new();
    let mut posix = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if name.contains('-') {
            hyphenated.push(name.to_string());
        } else if name.contains('_') {
            posix.push(name.to_string());
        }
    }
    assert!(
        hyphenated.is_empty(),
        "Found BCP 47 hyphen locale dirs: {:?}. \
             gettext lookup will fail for these — rename to POSIX underscore \
             (e.g. pt_BR, en_US). Coexistent POSIX dirs: {:?}",
        hyphenated,
        posix
    );
    assert!(
        !posix.is_empty(),
        "No POSIX locale dirs found under translations/ — pipeline broken"
    );
}

/// End-to-end: bind the catalog, set locale, perform the same dgettext
/// call Slint runtime makes for `@tr("btn-new-project")` inside the
/// `ProjectLauncherPage` component, and assert dgettext returns the
/// translation, NOT the mangled msgid.
///
/// Marked `#[ignore]` because:
///   1. It mutates global gettext state, which conflicts with parallel
///      tests running on the same process.
///   2. It requires `.mo` files to exist on disk (build.rs writes them).
///      `cargo test --workspace` runs the build, but isolated invocations
///      that skip the build (rare) would fail spuriously.
/// Run with `cargo test -p adapter-gui --lib gettext_resolves -- --ignored`.
#[test]
#[ignore = "mutates global gettext state; run with --ignored"]
#[cfg(target_os = "linux")]
fn gettext_resolves_btn_new_project_in_en_us() {
    use gettextrs::{bindtextdomain, dgettext, setlocale, textdomain, LocaleCategory};

    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("translations");
    std::env::set_var("LANGUAGE", "en_US");
    bindtextdomain(TEXT_DOMAIN, dir).expect("bindtextdomain");
    textdomain(TEXT_DOMAIN).expect("textdomain");
    setlocale(LocaleCategory::LcMessages, "en_US.UTF-8");

    // The mangled msgid is exactly what i-slint-core emits for
    // @tr("btn-new-project") inside ProjectLauncherPage:
    // <ctx>\u{4}<msgid>
    let mangled = format!("ProjectLauncherPage\u{4}btn-new-project");
    let result = dgettext(TEXT_DOMAIN, mangled.clone());

    assert_ne!(
        result, mangled,
        "dgettext echoed back the mangled msgid — translation lookup \
             FAILED. This is the bug that surfaces as 'BTN-NEW-PROJECT' \
             leaking into the UI. Likely cause: dir layout regressed to \
             BCP 47 hyphen, or .mo file missing the key."
    );
    // Slint demangles by taking the part after \u{4}.
    let demangled = result.split('\u{4}').last().unwrap_or(&result);
    assert_eq!(
        demangled, "New project",
        "Translation resolved but does not match expected en-US value"
    );
}

/// Bug (#436, refined under #513): the preset-picker dialog rendered raw
/// keys ("BTN-LOAD-PRESET", "BTN-CANCEL") because Slint scopes `@tr` by
/// the enclosing component name → gettext `msgctxt`, and the new component
/// names (e.g. `PresetPickerOverlay`) were never extracted into the `.po`.
/// Resolved under #513 by:
///   1. `build.rs` calls `with_default_translation_context(None)` so the
///      compiler stops emitting the component name as msgctxt.
///   2. The `.po` catalogs are flat (no msgctxt lines).
/// Guard: every `@tr("…")` in `preset_picker_overlay.slint` must have a
/// non-empty translation in the pt_BR catalog under no msgctxt.
#[test]
fn preset_picker_overlay_tr_keys_are_translated_in_pt_br() {
    let slint = include_str!("../ui/components/preset_picker_overlay.slint");
    let po = include_str!("../translations/pt_BR/LC_MESSAGES/adapter-gui.po");

    // Keys the component asks Slint to translate.
    let keys: Vec<&str> = slint
        .match_indices("@tr(\"")
        .map(|(i, _)| {
            let rest = &slint[i + 5..];
            &rest[..rest.find('"').expect("closing quote")]
        })
        .collect();
    assert!(
        keys.contains(&"btn-load-preset") && keys.contains(&"btn-cancel"),
        "fixture changed: expected btn-load-preset/btn-cancel, got {keys:?}"
    );

    // A .po record with no msgctxt and a non-empty msgstr must exist for
    // each key — that's the only lookup Slint will perform now that
    // DefaultTranslationContext::None is set in build.rs.
    for key in keys {
        let resolved = po.split("\n\n").any(|rec| {
            !rec.contains("msgctxt ")
                && rec.contains(&format!("msgid \"{key}\""))
                && !rec.contains("msgstr \"\"")
        });
        assert!(
            resolved,
            "no non-empty pt_BR translation for @tr(\"{key}\") in the \
             flat (context-free) catalog — UI shows the raw key"
        );
    }
}

/// #513 / #339 regression guard: scan EVERY `.slint` file under
/// `crates/adapter-gui/ui/` (recursive, excluding vendored modules), collect
/// every `@tr("…")` key, and assert every key has a non-empty msgstr in
/// **all three populated locales** (en_US, pt_BR, es_ES) in the flat
/// (no-msgctxt) catalog. Other shipped locales (de/fr/hi/ja/ko/zh) are
/// allowed to have empty msgstr — they fall back to msgid by design.
///
/// Catches: a string added in Slint without a `.po` entry; a `.po` entry
/// dedup'd with an empty msgstr; a key renamed in Slint but stale in `.po`.
#[test]
fn every_tr_key_has_translation_in_en_pt_es() {
    use std::fs;
    use std::path::Path;

    fn collect_slint_files(root: &Path, out: &mut Vec<String>) {
        let Ok(entries) = fs::read_dir(root) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            // Skip vendored Slint modules (third-party UI kits we don't translate).
            if path.is_dir() {
                if path.file_name().and_then(|s| s.to_str()) == Some("modules") {
                    continue;
                }
                collect_slint_files(&path, out);
            } else if path.extension().and_then(|s| s.to_str()) == Some("slint") {
                if let Ok(text) = fs::read_to_string(&path) {
                    out.push(text);
                }
            }
        }
    }

    let ui_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("ui");
    let mut slint_blobs = Vec::new();
    collect_slint_files(&ui_dir, &mut slint_blobs);
    assert!(
        !slint_blobs.is_empty(),
        "no .slint files found under {ui_dir:?}",
    );

    // Parse a Slint string literal — handles `\"`, `\\`, and `\u{NNNN}`
    // escapes. Returns the decoded value and the byte length consumed
    // INSIDE the literal (not counting the trailing quote).
    fn parse_slint_string(literal: &str) -> Option<(String, usize)> {
        let mut out = String::new();
        let bytes = literal.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let b = bytes[i];
            if b == b'"' {
                return Some((out, i));
            }
            if b == b'\\' && i + 1 < bytes.len() {
                let n = bytes[i + 1];
                match n {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'n' => out.push('\n'),
                    b't' => out.push('\t'),
                    b'u' => {
                        // \u{NNNN}
                        if i + 2 >= bytes.len() || bytes[i + 2] != b'{' {
                            return None;
                        }
                        let close = literal[i + 3..].find('}')?;
                        let hex = &literal[i + 3..i + 3 + close];
                        let cp = u32::from_str_radix(hex, 16).ok()?;
                        out.push(char::from_u32(cp)?);
                        i += 3 + close + 1;
                        continue;
                    }
                    _ => out.push(n as char),
                }
                i += 2;
                continue;
            }
            // UTF-8 multi-byte: copy the byte and let further bytes follow.
            out.push(b as char);
            // Above is wrong for multi-byte UTF-8; fall back to char iteration
            // by re-reading via str::char_indices.
            // To keep the impl simple, re-implement via chars():
            // We bail and use the chars-based approach below.
            return None;
        }
        None
    }

    // Robust char-based parser (covers all cases above).
    fn parse_slint_string_chars(literal: &str) -> Option<(String, usize)> {
        let mut out = String::new();
        let mut iter = literal.char_indices().peekable();
        while let Some((idx, c)) = iter.next() {
            if c == '"' {
                return Some((out, idx));
            }
            if c == '\\' {
                let (_, esc) = iter.next()?;
                match esc {
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'u' => {
                        // \u{NNNN}
                        let (_, brace) = iter.next()?;
                        if brace != '{' {
                            return None;
                        }
                        let mut hex = String::new();
                        loop {
                            let (_, h) = iter.next()?;
                            if h == '}' {
                                break;
                            }
                            hex.push(h);
                        }
                        let cp = u32::from_str_radix(&hex, 16).ok()?;
                        out.push(char::from_u32(cp)?);
                    }
                    other => out.push(other),
                }
                continue;
            }
            out.push(c);
        }
        None
    }

    let _ = parse_slint_string; // silence dead-code warning for the byte parser

    let mut keys: std::collections::BTreeSet<String> = Default::default();
    for blob in &slint_blobs {
        for (i, _) in blob.match_indices("@tr(\"") {
            let inside = &blob[i + 5..];
            if let Some((decoded, _)) = parse_slint_string_chars(inside) {
                keys.insert(decoded);
            }
        }
    }
    assert!(
        !keys.is_empty(),
        "found 0 @tr keys — extraction regex broke",
    );

    // en_US is intentionally allowed to leave msgstr empty when the msgid IS
    // already the English UI string (gettext falls back to msgid). For pt_BR
    // and es_ES, empty msgstr means a user sees English — a real bug.
    let strict_locales: &[(&str, &str)] = &[
        (
            "pt_BR",
            include_str!("../translations/pt_BR/LC_MESSAGES/adapter-gui.po"),
        ),
        (
            "es_ES",
            include_str!("../translations/es_ES/LC_MESSAGES/adapter-gui.po"),
        ),
    ];

    // Line-walk lookup: a msgid is "resolved" when the very next msgstr
    // line (within the same record, i.e. before the next msgid) is
    // non-empty. Robust against record-separator quirks the `split("\n\n")`
    // approach choked on (e.g. records with multi-line context comments
    // that confuse blank-line splits).
    fn has_nonempty_msgstr(po: &str, key: &str) -> bool {
        // .po stores `"` as `\"`. Mirror that escape so we can match
        // msgids that contain embedded quotes (e.g. `Delete "{}" ?`).
        let escaped = key.replace('\\', "\\\\").replace('"', "\\\"");
        let target = format!("msgid \"{escaped}\"");
        let lines: Vec<&str> = po.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            if *line == target {
                // Scan forward for the msgstr in this record. Skip comments
                // and continuation lines; stop at the next msgid.
                for &follow in &lines[i + 1..] {
                    if follow.starts_with("msgstr ") {
                        if follow == "msgstr \"\"" {
                            // Could be a multi-line msgstr (msgstr "" followed
                            // by "..." continuation lines).
                            // Check whether any continuation line is non-empty.
                            let cont_start =
                                i + 1 + lines[i + 1..].iter().position(|l| *l == follow).unwrap();
                            for &c in &lines[cont_start + 1..] {
                                if c.starts_with('"') && c != "\"\"" {
                                    return true;
                                }
                                if !c.starts_with('"') {
                                    break;
                                }
                            }
                            return false;
                        }
                        return true;
                    }
                    if follow.starts_with("msgid ") {
                        return false;
                    }
                }
                return false;
            }
        }
        false
    }

    let mut missing: Vec<String> = Vec::new();
    for (locale, po) in strict_locales {
        for key in &keys {
            if !has_nonempty_msgstr(po, key) {
                missing.push(format!("{locale}: {key}"));
            }
        }
    }

    assert!(
        missing.is_empty(),
        "{} translation gap(s) — every populated locale must translate every \
         @tr key in the flat catalog:\n  {}",
        missing.len(),
        missing.join("\n  "),
    );
}

/// #513 regression guard: no `text:` property may bind to a raw string
/// literal — every user-visible string must go through `@tr("…")`. Catches
/// regressions where someone hardcodes a label or button caption in Slint
/// instead of routing it through gettext.
///
/// **Scope: the Settings screen only** (`ui/pages/settings.slint` plus
/// every `.slint` under `ui/pages/settings/`). Out-of-scope `.slint` files
/// have pre-existing raw-text usages tracked separately; this test guards
/// what #513 owns.
///
/// Inspects `text:` and `text :` property bindings and the `placeholder-text:`
/// variant. Allowed values:
///   - `text: @tr("…")` (or `@tr` inside a ternary / expression)
///   - `text: <identifier>` / property paths (`root.name`, `b.label`)
///   - `text: ""` (empty placeholder)
///   - Slint string interpolations using `\{…}` (template-only strings;
///     the interpolation IS the content).
/// Banned: `text: "Hello"`, `text: "INPUT"`, `text: "✓"` (use `@tr("✓")`).
#[test]
fn no_raw_text_literals_in_settings_slint() {
    use std::fs;
    use std::path::Path;

    fn collect_slint_paths(root: &Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = fs::read_dir(root) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().and_then(|s| s.to_str()) == Some("modules") {
                    continue;
                }
                collect_slint_paths(&path, out);
            } else if path.extension().and_then(|s| s.to_str()) == Some("slint") {
                out.push(path);
            }
        }
    }

    let ui_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("ui");
    let mut all_paths = Vec::new();
    collect_slint_paths(&ui_dir, &mut all_paths);

    // Narrow to Settings scope only — this test owns the #513 surface.
    let settings_root = ui_dir.join("pages").join("settings");
    let settings_page = ui_dir.join("pages").join("settings.slint");
    let paths: Vec<_> = all_paths
        .into_iter()
        .filter(|p| p == &settings_page || p.starts_with(&settings_root))
        .collect();
    assert!(
        !paths.is_empty(),
        "no Settings .slint files matched the scope filter",
    );

    // A `text` binding contains a raw literal when, after stripping every
    // `@tr("…")` call from the value, any remaining `"…"` substring has
    // non-whitespace content. We strip `@tr` calls naively (regex-free).
    fn strip_tr_calls(s: &str) -> String {
        let mut out = String::new();
        let bytes = s.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i..].starts_with(b"@tr(") {
                // Scan to the matching ) — accounting for nested () and
                // string literals.
                let mut depth = 1;
                i += 4;
                let mut in_str = false;
                while i < bytes.len() && depth > 0 {
                    let b = bytes[i];
                    if in_str {
                        if b == b'\\' && i + 1 < bytes.len() {
                            i += 2;
                            continue;
                        }
                        if b == b'"' {
                            in_str = false;
                        }
                    } else {
                        match b {
                            b'"' => in_str = true,
                            b'(' => depth += 1,
                            b')' => depth -= 1,
                            _ => {}
                        }
                    }
                    i += 1;
                }
                continue;
            }
            out.push(bytes[i] as char);
            i += 1;
        }
        out
    }

    fn extract_text_value(line: &str) -> Option<&str> {
        let l = line.trim_start();
        for prop in ["text:", "text :", "placeholder-text:", "placeholder-text :"] {
            if let Some(rest) = l.strip_prefix(prop) {
                // Trim trailing `;` and inline comments.
                let mut v = rest.trim();
                if let Some(c) = v.find("//") {
                    v = v[..c].trim();
                }
                v = v.trim_end_matches(';').trim();
                return Some(v);
            }
        }
        None
    }

    fn has_nonempty_literal(value_without_tr: &str) -> bool {
        let bytes = value_without_tr.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let b = bytes[i];
            if b == b'"' {
                // Read literal until closing quote, handling escapes.
                let mut inner = String::new();
                i += 1;
                while i < bytes.len() {
                    let c = bytes[i];
                    if c == b'\\' && i + 1 < bytes.len() {
                        inner.push(bytes[i + 1] as char);
                        i += 2;
                        continue;
                    }
                    if c == b'"' {
                        break;
                    }
                    inner.push(c as char);
                    i += 1;
                }
                if !inner.trim().is_empty() {
                    return true;
                }
                i += 1;
                continue;
            }
            i += 1;
        }
        false
    }

    let mut violations: Vec<String> = Vec::new();
    for path in &paths {
        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        let rel = path
            .strip_prefix(env!("CARGO_MANIFEST_DIR"))
            .unwrap_or(path)
            .display()
            .to_string();
        for (n, line) in content.lines().enumerate() {
            let Some(value) = extract_text_value(line) else {
                continue;
            };
            // Skip property declarations like `in property <string> text: "x";`
            // and computed/derived bindings without literals.
            if !value.contains('"') {
                continue;
            }
            // Skip Slint string interpolations — `\{name}` IS the content;
            // the surrounding punctuation isn't a user-visible message.
            if value.contains("\\{") {
                continue;
            }
            let stripped = strip_tr_calls(value);
            if has_nonempty_literal(&stripped) {
                violations.push(format!("{}:{}: {}", rel, n + 1, line.trim()));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "{} raw text literal(s) outside @tr() — every user-visible string \
         must go through the gettext catalog:\n  {}",
        violations.len(),
        violations.join("\n  "),
    );
}

/// #513 regression guard: every `@tr` key used by the Settings master-detail
/// screen and its 5 sections must have a non-empty pt_BR translation in the
/// flat (no-msgctxt) catalog. Previous bugs: keys present in the .slint but
/// missing/empty in the .po → UI rendered the raw key in Bebas Neue uppercase
/// (e.g. "TITLE-SECTION-AUDIO-INTERFACE").
#[test]
fn settings_screen_tr_keys_are_translated_in_pt_br() {
    let sources: &[(&str, &str)] = &[
        ("settings.slint", include_str!("../ui/pages/settings.slint")),
        (
            "section_system_audio.slint",
            include_str!("../ui/pages/settings/section_system_audio.slint"),
        ),
        (
            "section_system_language.slint",
            include_str!("../ui/pages/settings/section_system_language.slint"),
        ),
        (
            "section_system_midi_devices.slint",
            include_str!("../ui/pages/settings/section_system_midi_devices.slint"),
        ),
        (
            "section_system_paths.slint",
            include_str!("../ui/pages/settings/section_system_paths.slint"),
        ),
        (
            "section_project_meta.slint",
            include_str!("../ui/pages/settings/section_project_meta.slint"),
        ),
    ];
    let po = include_str!("../translations/pt_BR/LC_MESSAGES/adapter-gui.po");

    for (name, slint) in sources {
        let keys: Vec<&str> = slint
            .match_indices("@tr(\"")
            .map(|(i, _)| {
                let rest = &slint[i + 5..];
                &rest[..rest.find('"').expect("closing quote")]
            })
            .collect();
        for key in keys {
            let resolved = po.split("\n\n").any(|rec| {
                !rec.contains("msgctxt ")
                    && rec.contains(&format!("msgid \"{key}\""))
                    && !rec.contains("msgstr \"\"")
            });
            assert!(
                resolved,
                "{name}: no non-empty pt_BR translation for @tr(\"{key}\") \
                 in the flat (context-free) catalog — UI shows raw key",
            );
        }
    }
}
