//! #1014: a project is only ever a `.yaml` file. Every project field
//! round-trips through it, and no second `.openrig` project format exists.

use std::fs;
use std::path::{Path, PathBuf};

use domain::ids::{BlockId, ChainId};
use domain::mixer_strip::MixerDirection;
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::{Chain, ChainMix};
use project::param::ParameterSet;
use project::project::Project;

fn project_with_mix(mix: ChainMix) -> Project {
    Project {
        name: Some("p".to_string()),
        device_settings: Vec::new(),
        midi: None,
        chains: vec![Chain {
            id: ChainId("chain-0".into()),
            description: Some("Guitar".into()),
            instrument: "electric_guitar".into(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec!["main".into()],
            blocks: vec![AudioBlock {
                id: BlockId("gain".into()),
                enabled: true,
                kind: AudioBlockKind::Core(CoreBlock {
                    effect_type: "gain".into(),
                    model: "volume".into(),
                    params: ParameterSet::default(),
                }),
            }],
            di_output: None,
            loopers: vec![],
            disabled_endpoints: Default::default(),
            mix,
        }],
    }
}

#[test]
fn chain_mix_survives_save_and_reload_of_project_yaml() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("project.yaml");
    let mut mix = ChainMix::default();
    mix.di_gain_db = -3.5;
    mix.endpoint_mut(MixerDirection::Input, "main", "Guitar")
        .gain_db = -6.0;
    mix.endpoint_mut(MixerDirection::Output, "main", "Main")
        .muted = true;
    let rig = project::migrate::migrate_legacy_project(&project_with_mix(mix.clone()));

    infra_yaml::save_project_file(&path, &rig).expect("save");
    let loaded = infra_yaml::load_project_file(&path).expect("load");

    let input = loaded.inputs.values().next().expect("one input");
    assert_eq!(input.mix, mix);
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("repo root")
        .to_path_buf()
}

fn scan(dir: &Path, hits: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            if name != "target" && name != "modules" && !name.starts_with('.') {
                scan(&path, hits);
            }
            continue;
        }
        let is_source = matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("rs" | "md" | "slint" | "yaml" | "yml" | "sh")
        );
        if !is_source || path.ends_with(file!().rsplit('/').next().unwrap()) {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        for (n, line) in text.lines().enumerate() {
            if mentions_openrig_project_format(line) {
                hits.push(format!("{}:{}: {}", path.display(), n + 1, line.trim()));
            }
        }
    }
}

/// `.openrig` as a file extension, or the extension literal / loader of the
/// removed format. Not a hit: the `~/.openrig` user folder (also when joined
/// onto a path as `.join(".openrig")`), the
/// `.openrig-plugin` package, `com.openrig.app`, or `openrig` as a name.
fn mentions_openrig_project_format(line: &str) -> bool {
    let extension = line.match_indices(".openrig").any(|(i, m)| {
        let before = line[..i].chars().next_back();
        let after = line[i + m.len()..].chars().next();
        let is_dir = matches!(before, Some('/' | '~')) || line[..i].ends_with("join(\"");
        let is_longer_name =
            matches!(after, Some(c) if c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | '/'));
        !is_dir && !is_longer_name
    });
    extension
        || line.contains("extension(\"openrig\")")
        || line.contains("== Some(\"openrig\")")
        || line.contains("load_project_any")
        || line.contains("migrate_legacy_project_file")
        || line.contains("rig_yaml")
}

#[test]
fn no_project_openrig_format_is_named_anywhere() {
    let root = repo_root();
    let mut hits = Vec::new();
    for dir in ["crates", "docs", "scripts"] {
        scan(&root.join(dir), &mut hits);
    }
    for file in [
        "CLAUDE.md",
        "README.md",
        "README.pt-BR.md",
        "README.es-ES.md",
    ] {
        scan_file(&root.join(file), &mut hits);
    }
    assert!(
        hits.is_empty(),
        "a project is only ever a .yaml file; found {} mention(s) of the .openrig format:\n{}",
        hits.len(),
        hits.join("\n")
    );
}

fn scan_file(path: &Path, hits: &mut Vec<String>) {
    let Ok(text) = fs::read_to_string(path) else {
        return;
    };
    for (n, line) in text.lines().enumerate() {
        if mentions_openrig_project_format(line) {
            hits.push(format!("{}:{}: {}", path.display(), n + 1, line.trim()));
        }
    }
}
