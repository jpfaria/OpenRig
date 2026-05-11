//! `nam_loudness_audit` — escreve `output_gain_db` em cada
//! `manifest.yaml` de plugin NAM, calculado a partir do RMS medido
//! ao processar pink noise determinístico pela primeira captura
//! do pacote.
//!
//! Issue #413: nivelamento de loudness saiu do runtime
//! (`engine::auto_max`) e virou metadata estática. Rodar este
//! binário no repositório `OpenRig-plugins` antes de qualquer
//! release atualiza o offset persistido em cada plugin pra que o
//! app possa aplicar como ganho constante.
//!
//! Uso:
//!
//!     cargo run --release -p nam-loudness-audit -- \
//!         /path/to/OpenRig-plugins/plugins/source/nam
//!
//! A escrita preserva ordering / espaçamento do YAML — só substitui
//! ou insere a linha `output_gain_db:` (anchored em `type:`).

use anyhow::{anyhow, bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use nam::loudness_probe::diagnose_model;
use nam::processor::{close_model_diag, open_model_diag};

/// RMS target em dBFS — pra onde cada NAM deve convergir após o
/// ganho persistido ser aplicado. Conservador, dá ~9 dB de
/// headroom até 0 dBFS pra captures de baixo crest factor.
const TARGET_RMS_DBFS: f32 = -10.0;

/// Mínimo de ganho persistido. Auto-max já só boostava; aqui
/// idem — captures já mais altas que o target ficam intocadas.
const MIN_GAIN_DB: f32 = 0.0;

/// Máximo de ganho persistido. Acima disso suspeitamos de capture
/// quebrada (output near-silent) — vale auditar manualmente.
const MAX_GAIN_DB: f32 = 24.0;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: nam_loudness_audit <plugins-nam-root>");
        eprintln!();
        eprintln!("Expects a directory whose immediate children are NAM plugin");
        eprintln!("packages (each carrying its own manifest.yaml + captures/).");
        std::process::exit(2);
    }
    let root = PathBuf::from(&args[1]);
    if !root.is_dir() {
        bail!("not a directory: {}", root.display());
    }

    let mut audited = 0usize;
    let mut skipped = 0usize;
    eprintln!("target RMS: {TARGET_RMS_DBFS:+.2} dBFS");
    eprintln!();
    eprintln!(
        "{:<48} {:>10} {:>10} {:>10}",
        "plugin", "out_rms", "raw_gain", "applied"
    );

    let mut entries: Vec<PathBuf> = fs::read_dir(&root)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    entries.sort();

    for plugin_dir in entries {
        let manifest_path = plugin_dir.join("manifest.yaml");
        if !manifest_path.is_file() {
            continue;
        }
        match audit_plugin(&plugin_dir, &manifest_path) {
            Ok(AuditResult {
                measured_rms_dbfs,
                raw_gain_db,
                applied_gain_db,
            }) => {
                let label = plugin_dir
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("<?>");
                eprintln!(
                    "{:<48} {:>+9.2}  {:>+9.2}  {:>+9.2}",
                    label, measured_rms_dbfs, raw_gain_db, applied_gain_db
                );
                audited += 1;
            }
            Err(e) => {
                let label = plugin_dir
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("<?>");
                eprintln!("SKIP {label}: {e}");
                skipped += 1;
            }
        }
    }

    eprintln!();
    eprintln!("audited {audited} plugins, skipped {skipped}");
    Ok(())
}

struct AuditResult {
    measured_rms_dbfs: f32,
    raw_gain_db: f32,
    applied_gain_db: f32,
}

fn audit_plugin(plugin_dir: &Path, manifest_path: &Path) -> Result<AuditResult> {
    let raw = fs::read_to_string(manifest_path)
        .with_context(|| format!("read {}", manifest_path.display()))?;
    let block_type = manifest_block_type(&raw).unwrap_or_else(|| "<unknown>".into());
    if !is_loudness_normalisable(&block_type) {
        bail!("type `{block_type}` is not loudness-normalised (only amp/preamp)");
    }
    let first_capture_file = first_capture_file(&raw)
        .ok_or_else(|| anyhow!("no `captures:[].file` entry in manifest"))?;

    let model_path = plugin_dir.join(&first_capture_file);
    let model_path_str = model_path
        .to_str()
        .ok_or_else(|| anyhow!("non-utf8 capture path: {model_path:?}"))?;

    let model = open_model_diag(model_path_str)
        .with_context(|| format!("failed to load {model_path_str}"))?;
    let report = unsafe { diagnose_model(model) };
    unsafe { close_model_diag(model) };

    let measured = report.output_rms_dbfs;
    let raw_gain = TARGET_RMS_DBFS - measured;
    let applied = raw_gain.clamp(MIN_GAIN_DB, MAX_GAIN_DB);

    let updated = upsert_output_gain_db(&raw, applied);
    fs::write(manifest_path, updated)
        .with_context(|| format!("write {}", manifest_path.display()))?;

    Ok(AuditResult {
        measured_rms_dbfs: measured,
        raw_gain_db: raw_gain,
        applied_gain_db: applied,
    })
}

/// Reads the top-level `type:` field from the manifest. Returns
/// `None` if the field is missing (which should be a manifest bug).
fn manifest_block_type(yaml: &str) -> Option<String> {
    for line in yaml.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("type:") {
            return Some(rest.trim().trim_matches('"').trim_matches('\'').to_string());
        }
    }
    None
}

/// Loudness normalisation is only valid for blocks whose natural
/// level is "amp-like". Gain pedals, fuzz, boost, distortion, etc.
/// are quiet by design — boosting them upstream of an amp creates
/// a runaway gain stack (microphonics / feedback). Cabs and bodies
/// are pure spectral shapers and don't carry a single loudness
/// signature either.
fn is_loudness_normalisable(block_type: &str) -> bool {
    matches!(block_type, "amp" | "preamp")
}

/// Yamzy: grep o primeiro capture `file:` field. Mais simples e
/// preserva o resto do manifest do que round-tripping via serde.
fn first_capture_file(yaml: &str) -> Option<String> {
    let mut in_captures = false;
    for line in yaml.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("captures:") {
            in_captures = true;
            continue;
        }
        if !in_captures {
            continue;
        }
        // Stripping `- ` so list-entry inline shapes like `- file: foo`
        // are picked up too.
        let after_dash = trimmed.strip_prefix("- ").unwrap_or(trimmed);
        if let Some(rest) = after_dash.strip_prefix("file:") {
            let value = rest.trim().trim_matches('"').trim_matches('\'').to_string();
            return Some(value);
        }
    }
    None
}

/// In-place line edit: substitui o valor de `output_gain_db:` se
/// existir, senão insere antes da linha `type:`. Preserva tudo o
/// resto do manifest.
fn upsert_output_gain_db(yaml: &str, gain: f32) -> String {
    let new_line = format!("output_gain_db: {gain:.7}");
    let already_present = yaml
        .lines()
        .any(|l| l.trim_start().starts_with("output_gain_db:"));
    let trailing_newline = yaml.ends_with('\n');
    let body: String = if already_present {
        yaml.lines()
            .map(|l| {
                if l.trim_start().starts_with("output_gain_db:") {
                    new_line.clone()
                } else {
                    l.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        let mut out = Vec::with_capacity(yaml.lines().count() + 1);
        let mut inserted = false;
        for l in yaml.lines() {
            if !inserted && l.trim_start().starts_with("type:") {
                out.push(new_line.clone());
                inserted = true;
            }
            out.push(l.to_string());
        }
        if !inserted {
            out.push(new_line);
        }
        out.join("\n")
    };
    if trailing_newline {
        format!("{body}\n")
    } else {
        body
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserts_field_before_type() {
        let yaml = "id: nam_test\nbrand: foo\ntype: amp\nbackend: nam\n";
        let out = upsert_output_gain_db(yaml, 7.5);
        assert!(out.contains("output_gain_db: 7.5000000\ntype: amp"));
        assert!(out.starts_with("id: nam_test"));
    }

    #[test]
    fn replaces_existing_field_in_place() {
        let yaml = "id: x\noutput_gain_db: 1.2\ntype: amp\n";
        let out = upsert_output_gain_db(yaml, 3.4);
        // f32 isn't exact for 3.4 — assert prefix instead of full literal.
        assert!(out.contains("output_gain_db: 3.4"));
        assert!(!out.contains("1.2"));
    }

    #[test]
    fn finds_first_capture_file_with_quoted_path() {
        let yaml = "id: x\ncaptures:\n- values: {}\n  file: \"captures/foo bar.nam\"\n";
        assert_eq!(
            first_capture_file(yaml),
            Some("captures/foo bar.nam".to_string())
        );
    }

    #[test]
    fn finds_first_capture_file_with_unquoted_path() {
        let yaml = "captures:\n- file: captures/clean.nam\n";
        assert_eq!(
            first_capture_file(yaml),
            Some("captures/clean.nam".to_string())
        );
    }

    #[test]
    fn reads_block_type() {
        let yaml = "id: x\ntype: amp\nbackend: nam\n";
        assert_eq!(manifest_block_type(yaml), Some("amp".to_string()));
    }

    #[test]
    fn loudness_normalisable_only_for_amp_and_preamp() {
        for ok in ["amp", "preamp"] {
            assert!(is_loudness_normalisable(ok), "{ok} should be eligible");
        }
        for skip in ["gain_pedal", "cab", "body", "reverb", "delay", "mod", "filter"] {
            assert!(
                !is_loudness_normalisable(skip),
                "{skip} should NOT be eligible (would gain-stack into the amp)"
            );
        }
    }
}
