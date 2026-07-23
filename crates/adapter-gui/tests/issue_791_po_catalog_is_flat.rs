//! Regression guard (#791): the translation catalog MUST stay FLAT — no
//! per-component `msgctxt`.
//!
//! The GUI compiles with slint `DefaultTranslationContext::None` (see
//! `build.rs`), so every `@tr(...)` resolves by BARE msgid at runtime. A `.po`
//! entry carrying `msgctxt "<Component>"` can never match that context-less
//! lookup, so the UI falls back to the raw key (e.g. `TONE-DOCTOR-TITLE`).
//!
//! This actually happened: a newer `slint-tr-extractor` stamped msgctxt on
//! every string, the reformatted catalog was committed, and a clean rebuild
//! silently broke EVERY translation. Nothing caught it. This test does —
//! `extract-translations.sh` strips msgctxt, and this fails the build if a
//! catalog ever regresses to the contextual form.

use std::path::{Path, PathBuf};

fn catalog_files() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("translations");
    let mut files = vec![root.join("adapter-gui.pot")];
    for lang in std::fs::read_dir(&root).expect("translations dir").flatten() {
        let po = lang.path().join("LC_MESSAGES").join("adapter-gui.po");
        if po.exists() {
            files.push(po);
        }
    }
    files
}

#[test]
fn every_catalog_entry_is_flat_no_msgctxt() {
    let mut offenders = Vec::new();
    for file in catalog_files() {
        let content = std::fs::read_to_string(&file)
            .unwrap_or_else(|e| panic!("read {}: {e}", file.display()));
        for (i, line) in content.lines().enumerate() {
            if line.starts_with("msgctxt ") {
                offenders.push(format!("{}:{}  {}", file.display(), i + 1, line.trim()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "translation catalog carries msgctxt — the GUI uses \
         DefaultTranslationContext::None, so @tr would show raw keys. \
         Run scripts/extract-translations.sh (it strips msgctxt). Offenders:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn tone_doctor_keys_are_present_and_translated_in_pt_br() {
    // Slint `@tr(...)` strings resolve through the gettext `.po` catalog.
    let po = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("translations/pt_BR/LC_MESSAGES/adapter-gui.po");
    let content = std::fs::read_to_string(&po).expect("pt_BR .po");
    for key in [
        "tone-doctor-title",
        "tone-doctor-diagnose",
        "tone-doctor-apply",
    ] {
        let needle = format!("msgid \"{key}\"");
        let idx = content
            .find(&needle)
            .unwrap_or_else(|| panic!("pt_BR .po missing {key}"));
        // The msgstr on the next line must be non-empty.
        let after = &content[idx + needle.len()..];
        let msgstr_line = after
            .lines()
            .find(|l| l.starts_with("msgstr "))
            .unwrap_or_else(|| panic!("{key} has no msgstr"));
        assert_ne!(
            msgstr_line.trim(),
            "msgstr \"\"",
            "{key} is untranslated in pt_BR"
        );
    }

    // `tone-doctor-no-di` is rendered from Rust via the rust-i18n `t!` catalog
    // (locales/*.yml), not through a Slint `@tr` — so it lives in the flat YAML
    // catalog, never in the gettext `.po`. Guard it in its actual home.
    let yml = Path::new(env!("CARGO_MANIFEST_DIR")).join("locales/pt-BR.yml");
    let yml_content = std::fs::read_to_string(&yml).expect("pt-BR.yml");
    let line = yml_content
        .lines()
        .find(|l| l.trim_start().starts_with("tone-doctor-no-di:"))
        .expect("pt-BR.yml missing tone-doctor-no-di");
    let value = line.split_once(':').map(|(_, v)| v.trim()).unwrap_or("");
    assert!(
        value != "\"\"" && value != "''" && !value.is_empty(),
        "tone-doctor-no-di is untranslated in pt-BR.yml"
    );
}
