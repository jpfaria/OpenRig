//! #398 — every colour the UI draws comes from the `Theme` tokens, which
//! follow the system light/dark scheme; no screen paints a raw hex literal.

use std::path::{Path, PathBuf};

fn ui_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("ui")
}

fn slint_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read ui dir") {
        let path = entry.expect("dir entry").path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if path.is_dir() {
            if name != "modules" {
                slint_files(&path, out);
            }
        } else if name.ends_with(".slint")
            && !name.starts_with('_')
            && !name.contains("test_harness")
            && name != "theme.slint"
        {
            out.push(path);
        }
    }
}

/// Hex colour literals outside `//` comments.
fn hex_literals(source: &str) -> Vec<String> {
    let mut found = Vec::new();
    for line in source.lines() {
        let code = line.split("//").next().unwrap_or("");
        let bytes = code.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'#' {
                let digits = bytes[i + 1..]
                    .iter()
                    .take_while(|b| b.is_ascii_hexdigit())
                    .count();
                let next = bytes.get(i + 1 + digits).copied().unwrap_or(b' ');
                if matches!(digits, 3 | 4 | 6 | 8) && !next.is_ascii_alphanumeric() {
                    found.push(code[i..i + 1 + digits].to_string());
                }
                i += 1 + digits;
            } else {
                i += 1;
            }
        }
    }
    found
}

#[test]
fn no_screen_paints_a_raw_hex_colour() {
    let mut files = Vec::new();
    slint_files(&ui_dir(), &mut files);
    files.sort();
    let offenders: Vec<String> = files
        .iter()
        .filter_map(|path| {
            let source = std::fs::read_to_string(path).expect("read slint");
            let hits = hex_literals(&source);
            (!hits.is_empty()).then(|| {
                format!(
                    "{} ({}: {})",
                    path.strip_prefix(ui_dir()).unwrap().display(),
                    hits.len(),
                    hits.join(" ")
                )
            })
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "{} files paint raw colours instead of Theme tokens:\n{}",
        offenders.len(),
        offenders.join("\n")
    );
}

#[test]
fn the_theme_follows_the_system_scheme_with_the_orange_accent() {
    let theme = std::fs::read_to_string(ui_dir().join("theme.slint")).expect("read theme");
    assert!(
        theme.contains("color-scheme"),
        "theme reads the system scheme"
    );
    assert!(theme.contains("out property <bool> dark"));
    assert!(theme.contains("out property <color> accent: #f28c1e;"));
    for token in [
        "page",
        "chrome",
        "panel",
        "panel-hi",
        "panel-lo",
        "well",
        "hair",
        "field",
        "field-line",
        "ink",
        "ink-2",
        "ink-3",
        "ok",
        "warn",
        "bad",
        "in",
        "out",
    ] {
        assert!(
            theme.contains(&format!("out property <color> {token}:")),
            "theme token `{token}` missing"
        );
    }
}

#[test]
fn hex_scan_ignores_comments_and_issue_numbers() {
    assert!(hex_literals("// #328 fixed").is_empty());
    assert_eq!(hex_literals("background: #ffffff; // #1"), vec!["#ffffff"]);
    assert!(hex_literals("text: \"#12a\";").len() == 1);
}
