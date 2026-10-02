//! Issue #1039 — loading drum kits and grooves from disk.
//!
//! Kits are Hydrogen `drumkit.xml` folders; grooves are per-genre YAML files.
//! WAVs are generated into a temp dir, so no bundled audio (LFS) is decoded.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use application::drums::{
    assign_roles, load_kit, parse_groove_file, parse_hydrogen_kit, role_for_name,
    scan_drum_library, KitDescription, KitInstrument,
};
use feature_dsp::drums::DrumRole;

const KIT_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<drumkit_info xmlns="http://www.hydrogen-music.org/drumkit">
 <name>Test Kit</name>
 <componentList>
  <drumkitComponent><id>0</id><name>Main</name><volume>1</volume></drumkitComponent>
 </componentList>
 <instrumentList>
  <instrument>
   <id>0</id>
   <name>Test-22-Kick</name>
   <volume>0.5</volume>
   <pan_L>1</pan_L>
   <pan_R>1</pan_R>
   <gain>2</gain>
   <muteGroup>-1</muteGroup>
   <midiOutNote>36</midiOutNote>
   <instrumentComponent>
    <component_id>0</component_id>
    <gain>1</gain>
    <layer><filename>kick-soft.wav</filename><min>0</min><max>0.5</max><gain>1</gain></layer>
    <layer><filename>kick-hard.wav</filename><min>0.5</min><max>1</max><gain>0.8</gain></layer>
   </instrumentComponent>
  </instrument>
  <instrument>
   <id>1</id>
   <name>Test-13-HatClosed</name>
   <volume>1</volume>
   <pan_L>1</pan_L>
   <pan_R>0.5</pan_R>
   <gain>1</gain>
   <muteGroup>1</muteGroup>
   <midiOutNote>42</midiOutNote>
   <instrumentComponent>
    <component_id>0</component_id>
    <gain>1</gain>
    <layer><filename>hat-a.wav</filename><min>0</min><max>1</max><gain>1</gain></layer>
    <layer><filename>hat-b.wav</filename><min>0</min><max>1</max><gain>1</gain></layer>
   </instrumentComponent>
  </instrument>
 </instrumentList>
</drumkit_info>
"#;

const GROOVES_YAML: &str = "\
genre: rock
grooves:
  - id: rock-01
    name: Rock 1
    beats_per_bar: 4
    tempo: 110
    source: drummer1/session1/1
    beat:
      beats: 8
      hits:
        - [0.0, kick, 127]
        - [1.0, snare, 64]
    fills:
      - beats: 5
        hits:
          - [0.0, tom_high, 100]
          - [4.0, crash, 127]
";

fn write_wav(path: &Path, rate: u32, samples: &[f32]) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).unwrap();
    for s in samples {
        writer.write_sample((s * 32767.0) as i16).unwrap();
    }
    writer.finalize().unwrap();
}

fn write_test_kit(dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join("drumkit.xml"), KIT_XML).unwrap();
    for name in ["kick-soft.wav", "kick-hard.wav", "hat-a.wav", "hat-b.wav"] {
        write_wav(&dir.join(name), 44_100, &vec![0.5; 4_410]);
    }
}

fn instrument(name: &str, note: u8) -> KitInstrument {
    KitInstrument {
        name: name.into(),
        midi_note: Some(note),
        gain: 1.0,
        pan: 0.0,
        choke_group: None,
        layers: vec![],
    }
}

fn avl_style_kit() -> KitDescription {
    let names = [
        (35, "StickClick"),
        (36, "Pearl-22-Kick"),
        (37, "Pearl-14-SideStick"),
        (38, "Pearl-14-Snare"),
        (39, "HandClap"),
        (40, "Pearl-14-SnareEdge"),
        (41, "Pearl-16-FloorTom"),
        (42, "Sabian-13-HatClosed"),
        (43, "Pearl-16-Fl.TomEdge"),
        (44, "Sabian-13-HatPedal"),
        (45, "Pearl-12-Tom"),
        (46, "Sabian-13-HatSemi"),
        (47, "Pearl-12-TomEdge"),
        (48, "Sabian-13-HatSwish"),
        (49, "Sabian-16-Crash"),
        (50, "Sabian-16-CrashChoke"),
        (51, "Sabian-20-Ride"),
        (52, "Sabian-20-RideChoke"),
        (53, "Sabian-20-RideBell"),
        (54, "Tambourine"),
        (55, "Zildjian-10-Splash"),
        (56, "Cowbell"),
        (57, "Sabian-17-Crash"),
        (58, "Sabian-17-CrashChoke"),
        (59, "Sabian-20-RideShank"),
        (60, "Paiste-22-Crash"),
        (61, "Maracas"),
        (62, "Wuhan-18-ChinaCrash"),
    ];
    KitDescription {
        name: "AVL style".into(),
        instruments: names.iter().map(|(n, name)| instrument(name, *n)).collect(),
    }
}

fn assigned(kit: &KitDescription, roles: &[Option<usize>], role: DrumRole) -> Option<String> {
    roles[role.index()].map(|i| kit.instruments[i].name.clone())
}

#[test]
fn a_hydrogen_kit_parses_its_instruments_and_layers() {
    let kit = parse_hydrogen_kit(KIT_XML, Path::new("/kits/test")).unwrap();
    assert_eq!(kit.name, "Test Kit");
    assert_eq!(kit.instruments.len(), 2);

    let kick = &kit.instruments[0];
    assert_eq!(kick.name, "Test-22-Kick");
    assert_eq!(kick.midi_note, Some(36));
    assert!((kick.gain - 1.0).abs() < 1e-6, "volume 0.5 x gain 2");
    assert!(kick.pan.abs() < 1e-6);
    assert_eq!(kick.choke_group, None);
    assert_eq!(kick.layers.len(), 2);
    assert_eq!(kick.layers[1].min_velocity, 0.5);
    assert_eq!(kick.layers[1].max_velocity, 1.0);
    assert!((kick.layers[1].gain - 0.8).abs() < 1e-6);
    assert_eq!(
        kick.layers[0].files,
        vec![PathBuf::from("/kits/test/kick-soft.wav")]
    );

    let hat = &kit.instruments[1];
    assert_eq!(hat.choke_group, Some(1));
    assert!((hat.pan + 0.5).abs() < 1e-6, "pan_R 0.5 leans left");
    assert_eq!(hat.layers.len(), 1, "same range = one round-robin layer");
    assert_eq!(hat.layers[0].files.len(), 2);
}

#[test]
fn instrument_names_map_to_drum_roles() {
    let cases = [
        ("Pearl-22-Kick", Some(DrumRole::Kick)),
        ("Pearl-14-Snare", Some(DrumRole::Snare)),
        ("Pearl-14-SnareEdge", Some(DrumRole::SnareRim)),
        ("Pearl-14-SideStick", Some(DrumRole::SideStick)),
        ("HandClap", Some(DrumRole::Clap)),
        ("Sabian-13-HatClosed", Some(DrumRole::HatClosed)),
        ("Sabian-13-HatPedal", Some(DrumRole::HatPedal)),
        ("Sabian-13-HatSemi", Some(DrumRole::HatOpen)),
        ("Pearl-16-FloorTom", Some(DrumRole::TomFloor)),
        ("Wuhan-18-ChinaCrash", Some(DrumRole::China)),
        ("Zildjian-10-Splash", Some(DrumRole::Splash)),
        ("Sabian-20-RideBell", Some(DrumRole::RideBell)),
        ("Sabian-20-Ride", Some(DrumRole::Ride)),
        ("Sabian-16-Crash", Some(DrumRole::Crash)),
        ("Tambourine", Some(DrumRole::Tambourine)),
        ("Cowbell", Some(DrumRole::Cowbell)),
        ("Pearl-16-Fl.TomEdge", None),
        ("Sabian-16-CrashChoke", None),
        ("Sabian-20-RideChoke", None),
        ("Sabian-13-HatSwish", None),
        ("Sabian-20-RideShank", None),
    ];
    for (name, role) in cases {
        assert_eq!(role_for_name(name), role, "{name}");
    }
}

#[test]
fn an_avl_kit_gets_one_instrument_per_role() {
    let kit = avl_style_kit();
    let roles = assign_roles(&kit, &BTreeMap::new());
    let name = |role| assigned(&kit, &roles, role);
    assert_eq!(name(DrumRole::Kick).as_deref(), Some("Pearl-22-Kick"));
    assert_eq!(
        name(DrumRole::HatOpen).as_deref(),
        Some("Sabian-13-HatSemi")
    );
    assert_eq!(name(DrumRole::Crash).as_deref(), Some("Sabian-16-Crash"));
    assert_eq!(name(DrumRole::Crash2).as_deref(), Some("Sabian-17-Crash"));
    assert_eq!(
        name(DrumRole::TomFloor).as_deref(),
        Some("Pearl-16-FloorTom")
    );
    assert_eq!(name(DrumRole::TomMid).as_deref(), Some("Pearl-12-Tom"));
    assert_eq!(name(DrumRole::TomHigh), None);
    assert_eq!(name(DrumRole::Ride).as_deref(), Some("Sabian-20-Ride"));
    for role in DrumRole::ALL {
        let picked = name(role).unwrap_or_default();
        assert!(
            !picked.contains("Choke") && !picked.contains("Edge") || role == DrumRole::SnareRim,
            "{role:?} got {picked}"
        );
    }
}

#[test]
fn a_roles_sidecar_overrides_the_name_heuristics() {
    let kit = avl_style_kit();
    let overrides = BTreeMap::from([("crash".to_string(), "Paiste-22-Crash".to_string())]);
    let roles = assign_roles(&kit, &overrides);
    assert_eq!(
        assigned(&kit, &roles, DrumRole::Crash).as_deref(),
        Some("Paiste-22-Crash")
    );
}

#[test]
fn a_kit_folder_loads_resampled_to_the_stream_rate() {
    let dir = tempfile::tempdir().unwrap();
    write_test_kit(dir.path());
    let kit = load_kit(dir.path(), 48_000).unwrap();
    assert_eq!(kit.name(), "Test Kit");
    assert_eq!(kit.sample_rate(), 48_000);

    let kick = kit.piece(DrumRole::Kick).expect("kick");
    assert_eq!(kick.layers.len(), 2);
    let len = kick.layers[0].samples[0].len();
    assert!(
        (4_799..=4_801).contains(&len),
        "4410 frames at 44.1k -> {len}"
    );
    assert!((kick.layers[0].samples[0][2_000] - 0.5).abs() < 1e-3);

    let hat = kit.piece(DrumRole::HatClosed).expect("hat");
    assert_eq!(hat.choke_group, Some(1));
    assert_eq!(hat.layers[0].samples.len(), 2);
    assert!(kit.piece(DrumRole::Snare).is_none());
}

#[test]
fn a_kit_folder_with_a_missing_sample_fails_to_load() {
    let dir = tempfile::tempdir().unwrap();
    write_test_kit(dir.path());
    fs::remove_file(dir.path().join("hat-b.wav")).unwrap();
    assert!(load_kit(dir.path(), 48_000).is_err());
}

#[test]
fn a_groove_file_parses_beats_and_fills() {
    let grooves = parse_groove_file(GROOVES_YAML).unwrap();
    assert_eq!(grooves.len(), 1);
    let g = &grooves[0];
    assert_eq!(g.id, "rock-01");
    assert_eq!(g.name, "Rock 1");
    assert_eq!(g.genre, "rock");
    assert_eq!(g.beats_per_bar, 4);
    assert_eq!(g.tempo, 110.0);
    assert_eq!(g.beat.beats(), 8.0);
    assert_eq!(g.beat.hits()[0].role, DrumRole::Kick);
    assert_eq!(g.beat.hits()[0].velocity, 1.0);
    assert!((g.beat.hits()[1].velocity - 64.0 / 127.0).abs() < 1e-6);
    assert_eq!(g.fills.len(), 1);
    assert_eq!(g.fills[0].beats(), 5.0);
    assert_eq!(g.fills[0].hits()[1].role, DrumRole::Crash);
}

#[test]
fn a_groove_file_with_an_unknown_role_is_rejected() {
    let bad = GROOVES_YAML.replace("tom_high", "gong");
    assert!(parse_groove_file(&bad).is_err());
}

#[test]
fn the_library_lists_kits_and_grooves_without_decoding_audio() {
    let root = tempfile::tempdir().unwrap();
    let kit_dir = root.path().join("kits").join("test-kit");
    fs::create_dir_all(&kit_dir).unwrap();
    fs::write(kit_dir.join("drumkit.xml"), KIT_XML).unwrap();
    fs::create_dir_all(root.path().join("grooves")).unwrap();
    fs::write(root.path().join("grooves").join("rock.yaml"), GROOVES_YAML).unwrap();

    let library = scan_drum_library(&[root.path().to_path_buf()]);
    assert_eq!(library.kits.len(), 1);
    assert_eq!(library.kits[0].id, "test-kit");
    assert_eq!(library.kits[0].name, "Test Kit");
    assert_eq!(library.kits[0].dir, kit_dir);
    assert_eq!(library.grooves.len(), 1);
    assert_eq!(library.grooves[0].id, "rock-01");
}

#[test]
fn every_bundled_groove_parses_and_has_a_fill() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/drums/grooves");
    let mut genres = 0;
    for entry in fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("yaml") {
            continue;
        }
        genres += 1;
        let grooves = parse_groove_file(&fs::read_to_string(&path).unwrap())
            .unwrap_or_else(|e| panic!("{path:?}: {e}"));
        assert!(!grooves.is_empty(), "{path:?}");
        for g in grooves {
            assert!(!g.beat.hits().is_empty(), "{}", g.id);
            assert!(!g.fills.is_empty(), "{} has no fill", g.id);
            assert!(g.tempo >= 20.0 && g.tempo <= 400.0, "{}", g.id);
        }
    }
    assert!(genres >= 10, "only {genres} genres bundled");
}
