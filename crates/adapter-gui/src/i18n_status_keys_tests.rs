//! #913 — every status message the Rust side shows is translated everywhere.
//!
//! A raw literal reaches the user in one language whatever they picked, and a
//! key with no entry in a locale renders as the key itself. `audio_wizard`'s
//! warning was a literal until this test; the guard is here so the next one
//! cannot ship the same way.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const LOCALES_DIR: &str = "locales";

fn locales_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(LOCALES_DIR)
}

fn keys_in(locale: &str) -> BTreeSet<String> {
    let path = locales_dir().join(format!("{locale}.yml"));
    let content =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    content
        .lines()
        .filter_map(|line| {
            let line = line.trim_end();
            if line.starts_with(char::is_whitespace) || line.starts_with('#') || line.is_empty() {
                return None;
            }
            line.split_once(':').map(|(key, _)| key.trim().to_string())
        })
        .collect()
}

fn shipped_locales() -> Vec<String> {
    let mut locales: Vec<String> = std::fs::read_dir(locales_dir())
        .expect("locales dir")
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension()? != "yml" {
                return None;
            }
            Some(path.file_stem()?.to_str()?.to_string())
        })
        .collect();
    locales.sort();
    locales
}

#[test]
fn every_locale_carries_the_same_keys_as_english() {
    let reference = keys_in("en-US");
    assert!(!reference.is_empty(), "en-US must not be empty");
    for locale in shipped_locales() {
        if locale == "en-US" {
            continue;
        }
        let keys = keys_in(&locale);
        let missing: Vec<&String> = reference.difference(&keys).collect();
        assert!(
            missing.is_empty(),
            "{locale} is missing {} key(s) that en-US has — they would render \
             as the raw key: {missing:?}",
            missing.len()
        );
    }
}

#[test]
fn no_locale_carries_an_empty_translation() {
    for locale in shipped_locales() {
        let path = locales_dir().join(format!("{locale}.yml"));
        let content = std::fs::read_to_string(&path).expect("read locale");
        for (number, line) in content.lines().enumerate() {
            if line.starts_with(char::is_whitespace) || line.starts_with('#') || line.is_empty() {
                continue;
            }
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let value = value.trim().trim_matches('"');
            assert!(
                !value.is_empty(),
                "{locale}.yml:{} — '{}' has no translation, so the UI shows the key",
                number + 1,
                key.trim()
            );
        }
    }
}

#[test]
fn the_wizards_warning_is_a_key_not_a_literal() {
    // The regression this test exists for: the string was hard-coded in
    // Portuguese, so every non-Portuguese user read it in Portuguese.
    let source = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/audio_wizard_wiring.rs"),
    )
    .expect("read audio_wizard_wiring.rs");
    assert!(
        !source.contains("Selecione pelo menos um input"),
        "the warning must go through t!(), not a literal"
    );
    assert!(source.contains("status-wizard-select-input"));
    assert!(keys_in("en-US").contains("status-wizard-select-input"));
}
