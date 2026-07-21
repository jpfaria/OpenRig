//! Slint @tr-key ↔ .po catalog consistency tests (issue #792 split from
//! i18n_tests.rs).


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
///
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

/// Comprehensive guard (#716): EVERY symbolic `@tr("key")` used anywhere in the
/// GUI (`ui/**/*.slint`) must have a non-empty, FLAT (no-`msgctxt`) translation
/// in EVERY shipped locale. Catches the two recurring i18n breaks:
///   1. A new GUI string not translated in all 9 locales.
///   2. A context-qualified catalog (`extract-translations.sh` / `msgmerge`
///      re-add `msgctxt`), where the runtime flat lookup
///      (`build.rs` `DefaultTranslationContext::None`) fails and the whole UI
///      renders raw keys (regressions: 4590afeb, and #716 again).
///
/// Symbolic keys = lowercase identifiers with a `-` (btn-*, label-*, help-*,
/// …); English/source-literal msgids (e.g. "Volume", "OpenRig") fall back to
/// themselves and are intentionally allowed to have an empty msgstr.
#[test]
fn every_gui_tr_key_translated_in_every_locale() {
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};

    let root = env!("CARGO_MANIFEST_DIR");

    fn collect_slint(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                collect_slint(&p, out);
            } else if p.extension().and_then(|x| x.to_str()) == Some("slint") {
                let temp = p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with('_'));
                if !temp {
                    out.push(p);
                }
            }
        }
    }

    let mut files = Vec::new();
    collect_slint(&Path::new(root).join("ui"), &mut files);
    assert!(!files.is_empty(), "no .slint files found under ui/");

    let mut keys: BTreeSet<String> = BTreeSet::new();
    for f in &files {
        let src = std::fs::read_to_string(f).unwrap();
        for (i, _) in src.match_indices("@tr(\"") {
            let rest = &src[i + 5..];
            if let Some(end) = rest.find('"') {
                let key = &rest[..end];
                // symbolic key: a `-`-joined lowercase identifier (btn-add,
                // label-name). Excludes format/source strings with spaces,
                // `{}` placeholders, `:` or `->` (those fall back to msgid).
                let is_symbolic = key.contains('-')
                    && key
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
                if is_symbolic {
                    keys.insert(key.to_string());
                }
            }
        }
    }

    let locales = [
        "en_US", "pt_BR", "es_ES", "fr_FR", "de_DE", "hi_IN", "ja_JP", "ko_KR", "zh_CN",
    ];
    let mut missing: Vec<String> = Vec::new();
    for loc in locales {
        let po = std::fs::read_to_string(format!(
            "{root}/translations/{loc}/LC_MESSAGES/adapter-gui.po"
        ))
        .unwrap_or_else(|e| panic!("read {loc} catalog: {e}"));
        for key in &keys {
            let resolved = po.split("\n\n").any(|rec| {
                !rec.contains("msgctxt ")
                    && rec.contains(&format!("msgid \"{key}\""))
                    && !rec.contains("msgstr \"\"")
            });
            if !resolved {
                missing.push(format!("{loc}: {key}"));
            }
        }
    }

    assert!(
        missing.is_empty(),
        "{} GUI @tr key(s) lack a flat non-empty translation (missing in a \
         locale, or catalog got context-qualified — do NOT run \
         extract-translations.sh):\n  {}",
        missing.len(),
        missing.join("\n  "),
    );
}
