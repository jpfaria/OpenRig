//! #879: the browser knows the TONE3000 tones the user already has, wherever
//! the package lives, and lists only plugins that came from TONE3000.

use application::tone3000::catalog_tones::{catalog_entries, manifest_tone_ids, source_tone_id};
use std::path::{Path, PathBuf};

use plugin_loader::manifest::{NamArchitecture, PluginManifest};
use plugin_loader::LoadedPackage;

fn manifest(yaml: &str) -> PluginManifest {
    serde_yaml::from_str(yaml).unwrap()
}

fn nam(id: &str, name: &str, links: &str) -> PluginManifest {
    manifest(&format!(
        "manifest_version: 1\nid: {id}\ndisplay_name: {name}\n{links}\ntype: preamp\n\
         backend: nam\narchitecture: A2\nparameters: []\ncaptures:\n\
         - values: {{}}\n  file: captures/a.nam\n"
    ))
}

fn at(root: &str, manifest: PluginManifest) -> LoadedPackage {
    LoadedPackage {
        root: PathBuf::from(root).join(&manifest.id),
        manifest,
    }
}

const PLUGINS: &str = "/data/plugins/nam";

#[test]
fn a_tone_url_gives_its_id() {
    assert_eq!(
        source_tone_id("https://www.tone3000.com/tones/52557"),
        Some(52557)
    );
    assert_eq!(
        source_tone_id("https://tone3000.com/tones/716-jcm800"),
        Some(716)
    );
    assert_eq!(source_tone_id("https://example.com/tones/12"), None);
    assert_eq!(source_tone_id("https://www.tone3000.com/tones/"), None);
}

#[test]
fn sources_and_homepage_both_name_the_tone() {
    let m = nam(
        "nam_x",
        "X",
        "homepage: https://www.tone3000.com/tones/7\nsources:\n- https://www.tone3000.com/tones/5\n- https://www.tone3000.com/tones/7",
    );
    assert_eq!(manifest_tone_ids(&m), vec![5, 7]);
}

#[test]
fn only_plugins_from_tone3000_are_listed() {
    let dumble = nam(
        "nam_synergy_dumble_os_a2",
        "Dumble OS Module",
        "sources:\n- https://www.tone3000.com/tones/52557",
    );
    let other = nam("nam_other", "Other", "sources:\n- https://example.com/pack");
    let entries = catalog_entries(
        [&at(PLUGINS, dumble), &at(PLUGINS, other)],
        Some(Path::new("/data/plugins")),
    );
    assert_eq!(entries.len(), 1);
    let entry = &entries[0];
    assert_eq!(entry.plugin_id, "nam_synergy_dumble_os_a2");
    assert_eq!(entry.tone_ids, vec![52557]);
    assert_eq!(entry.architecture, Some(NamArchitecture::A2));
    assert_eq!(entry.captures, 1);
    assert!(entry.removable, "the plugins folder is the user's");
}

#[test]
fn a_plugin_outside_the_plugins_folder_is_not_removable() {
    let bundled = nam(
        "nam_bundled",
        "Bundled",
        "sources:\n- https://www.tone3000.com/tones/3",
    );
    let lookalike = nam(
        "nam_lookalike",
        "Lookalike",
        "sources:\n- https://www.tone3000.com/tones/4",
    );
    let entries = catalog_entries(
        [
            &at("/app/plugins/nam", bundled),
            &at("/data/plugins-old/nam", lookalike),
        ],
        Some(Path::new("/data/plugins")),
    );
    assert!(entries.iter().all(|e| !e.removable));
    let none = catalog_entries(
        [&at(
            PLUGINS,
            nam("nam_x", "X", "sources:\n- https://www.tone3000.com/tones/5"),
        )],
        None,
    );
    assert!(!none[0].removable, "no plugins folder, nothing to remove");
}

#[test]
fn the_browsers_own_packages_are_left_to_their_own_list() {
    let own = nam(
        "tone3000_52557_a2",
        "Dumble",
        "homepage: https://www.tone3000.com/tones/52557",
    );
    let entries = catalog_entries([&at(PLUGINS, own)], Some(Path::new("/data/plugins")));
    assert!(entries.is_empty());
}

#[test]
fn catalog_entries_read_in_name_order() {
    let b = nam(
        "nam_b",
        "Bravo",
        "sources:\n- https://www.tone3000.com/tones/2",
    );
    let a = nam(
        "nam_a",
        "Alpha",
        "sources:\n- https://www.tone3000.com/tones/1",
    );
    let names: Vec<String> = catalog_entries([&at(PLUGINS, b), &at(PLUGINS, a)], None)
        .into_iter()
        .map(|e| e.display_name)
        .collect();
    assert_eq!(names, vec!["Alpha", "Bravo"]);
}

#[test]
fn the_snapshot_lists_the_browsers_packages_then_the_catalogs() {
    use application::tone3000::catalog_tones::installed_entry;
    use application::tone3000_state::Tone3000ControlState;
    use std::sync::Arc;

    let root = tempfile::tempdir().unwrap();
    let own = nam(
        "tone3000_9_a2",
        "Mine",
        "homepage: https://www.tone3000.com/tones/9",
    );
    let dir = root.path().join("nam").join("tone3000_9_a2");
    std::fs::create_dir_all(dir.join("captures")).unwrap();
    std::fs::write(dir.join("captures/a.nam"), "{}").unwrap();
    std::fs::write(
        dir.join("manifest.yaml"),
        serde_yaml::to_string(&own).unwrap(),
    )
    .unwrap();

    let dumble = nam(
        "nam_synergy_dumble_os_a2",
        "Dumble OS Module",
        "sources:\n- https://www.tone3000.com/tones/52557",
    );
    let catalog = vec![installed_entry(&dumble, Path::new("/elsewhere"), false)];
    let state =
        Tone3000ControlState::restored(&Default::default(), None, Some(root.path().to_path_buf()))
            .with_catalog(Arc::new(move || catalog.clone()));

    let listed = state.snapshot().installed;
    let ids: Vec<(&str, bool)> = listed
        .iter()
        .map(|e| (e.plugin_id.as_str(), e.removable))
        .collect();
    assert_eq!(
        ids,
        vec![("tone3000_9_a2", true), ("nam_synergy_dumble_os_a2", false)]
    );
    assert_eq!(listed[0].tone_ids, vec![9]);
    assert_eq!(listed[0].dir, dir);
}
