//! #879: a downloaded TONE3000 tone becomes an OpenRig plugin manifest the
//! loader accepts — id, credits, block type, capture grid and levels.

use std::path::PathBuf;

use application::tone3000::api_types::{Model, Page, Tone};
use application::tone3000::block_type::block_type_for;
use application::tone3000::manifest_build::{build_manifest, plugin_id, CaptureFile, PackageKind};
use application::tone3000::models_pick::unique_models;
use application::tone3000::{Tone3000Architecture, Tone3000BlockType};
use plugin_loader::manifest::{Backend, BlockType, NamArchitecture, PluginManifest};
use plugin_loader::validate_manifest;

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/tone3000")
        .join(name);
    std::fs::read_to_string(path).unwrap()
}

fn search_tone(file: &str, id: u64) -> Tone {
    let page: Page<Tone> = serde_json::from_str(&fixture(file)).unwrap();
    page.data.into_iter().find(|t| t.id == id).unwrap()
}

fn tone(json: &str) -> Tone {
    serde_json::from_str(json).unwrap()
}

fn captures(models_fixture: &str, ext: &str, gains: Option<f32>) -> Vec<CaptureFile> {
    let page: Page<Model> = serde_json::from_str(&fixture(models_fixture)).unwrap();
    unique_models(page.data)
        .into_iter()
        .enumerate()
        .map(|(i, m)| CaptureFile {
            name: m.name,
            file: PathBuf::from(format!("captures/{i:03}.{ext}")),
            output_gain_db: gains.map(|g| g - i as f32),
        })
        .collect()
}

#[test]
fn nam_gear_maps_to_its_block_type() {
    let cases = [
        (
            r#"{"id":1,"title":"t","format":"nam","gear":"amp"}"#,
            Tone3000BlockType::Amp,
        ),
        (
            r#"{"id":1,"title":"t","format":"nam","gear":"full-rig"}"#,
            Tone3000BlockType::Amp,
        ),
        (
            r#"{"id":1,"title":"t","format":"nam","gear":"amp-cab"}"#,
            Tone3000BlockType::Amp,
        ),
        (
            r#"{"id":1,"title":"t","format":"nam"}"#,
            Tone3000BlockType::Amp,
        ),
        (
            r#"{"id":1,"title":"t","format":"nam","gear":"pedal"}"#,
            Tone3000BlockType::GainPedal,
        ),
        (
            r#"{"id":1,"title":"t","format":"nam","gear":"outboard"}"#,
            Tone3000BlockType::Preamp,
        ),
    ];
    for (json, expected) in cases {
        assert_eq!(block_type_for(&tone(json)), expected, "{json}");
    }
}

#[test]
fn ir_tones_are_cabs_unless_tagged_acoustic() {
    let cab = tone(r#"{"id":1,"title":"t","format":"ir","gear":"cab"}"#);
    assert_eq!(block_type_for(&cab), Tone3000BlockType::Cab);
    let body = tone(
        r#"{"id":1,"title":"t","format":"ir","gear":"ir","tags":[{"name":"Acoustic Guitar"}]}"#,
    );
    assert_eq!(block_type_for(&body), Tone3000BlockType::Body);
}

#[test]
fn every_block_type_names_its_manifest_type() {
    let cases = [
        (Tone3000BlockType::Amp, BlockType::Amp),
        (Tone3000BlockType::Preamp, BlockType::Preamp),
        (Tone3000BlockType::GainPedal, BlockType::GainPedal),
        (Tone3000BlockType::Cab, BlockType::Cab),
        (Tone3000BlockType::Body, BlockType::Body),
    ];
    for (ours, manifest) in cases {
        assert_eq!(ours.manifest_block_type(), manifest);
    }
}

#[test]
fn plugin_ids_carry_the_tone_and_the_architecture() {
    assert_eq!(
        plugin_id(66606, PackageKind::Nam(Tone3000Architecture::A2)),
        "tone3000_66606_a2"
    );
    assert_eq!(
        plugin_id(66606, PackageKind::Nam(Tone3000Architecture::A1)),
        "tone3000_66606_a1"
    );
    assert_eq!(plugin_id(67544, PackageKind::Ir), "tone3000_67544");
}

#[test]
fn a_nam_tone_becomes_a_valid_amp_plugin() {
    let tone = search_tone("search_nam.json", 66606);
    let files = captures("models_66606_a2.json", "nam", None);
    let manifest = build_manifest(
        &tone,
        PackageKind::Nam(Tone3000Architecture::A2),
        Tone3000BlockType::Amp,
        &files,
        Some(-3.5),
    );
    validate_manifest(&manifest).unwrap();

    assert_eq!(manifest.manifest_version, 1);
    assert_eq!(manifest.id, "tone3000_66606_a2");
    assert_eq!(manifest.display_name, "REVV GENERATOR 120 (PURPLE) MKII");
    assert_eq!(manifest.author.as_deref(), Some("Deathblossomaudio"));
    assert_eq!(
        manifest.description.as_deref(),
        Some("Fixture description.")
    );
    assert_eq!(
        manifest.inspired_by.as_deref(),
        Some("REVV GENERATOR 120 MKII")
    );
    assert_eq!(manifest.brand.as_deref(), Some("revv"));
    assert_eq!(manifest.license.as_deref(), Some("t3k"));
    let page = "https://www.tone3000.com/tones/revv-generator-120-purple-mkii-66606";
    assert_eq!(manifest.homepage.as_deref(), Some(page));
    assert_eq!(manifest.sources, Some(vec![page.to_string()]));
    assert_eq!(manifest.output_gain_db, Some(-3.5));
    assert_eq!(manifest.architecture, Some(NamArchitecture::A2));
    assert_eq!(manifest.block_type, BlockType::Amp);

    let Backend::Nam {
        parameters,
        captures,
    } = &manifest.backend
    else {
        panic!("expected a NAM backend, got {:?}", manifest.backend);
    };
    assert_eq!(parameters.len(), 1);
    assert_eq!(parameters[0].name, "preset");
    assert_eq!(parameters[0].display_name.as_deref(), Some("Preset"));
    assert_eq!(captures.len(), 10);
    assert_eq!(captures[0].file, PathBuf::from("captures/000.nam"));
    assert!(captures.iter().all(|c| c.output_gain_db.is_none()));
}

#[test]
fn the_author_falls_back_to_the_username() {
    let tone: Tone = serde_json::from_str(&fixture("tone_716.json")).unwrap();
    let files = captures("models_716_a1.json", "nam", None);
    let manifest = build_manifest(
        &tone,
        PackageKind::Nam(Tone3000Architecture::A1),
        Tone3000BlockType::Amp,
        &files,
        Some(0.0),
    );
    validate_manifest(&manifest).unwrap();
    assert_eq!(manifest.author.as_deref(), Some("northernfox"));
    assert_eq!(manifest.brand.as_deref(), Some("marshall"));
    assert_eq!(manifest.architecture, Some(NamArchitecture::A1));
}

#[test]
fn the_brand_is_the_make_the_title_names() {
    // makes[0] is the speaker (Celestion); the title names the cab (Mesa).
    let tone = search_tone("search_ir.json", 67544);
    let files = captures("models_67544_ir.json", "wav", Some(2.0));
    let manifest = build_manifest(&tone, PackageKind::Ir, Tone3000BlockType::Cab, &files, None);
    assert_eq!(manifest.brand.as_deref(), Some("mesa"));
}

#[test]
fn an_ir_tone_keeps_one_level_per_capture() {
    let tone = search_tone("search_ir.json", 67544);
    let files = captures("models_67544_ir.json", "wav", Some(2.0));
    let manifest = build_manifest(&tone, PackageKind::Ir, Tone3000BlockType::Cab, &files, None);
    validate_manifest(&manifest).unwrap();

    assert_eq!(manifest.id, "tone3000_67544");
    assert_eq!(manifest.block_type, BlockType::Cab);
    assert_eq!(manifest.output_gain_db, None);
    assert_eq!(manifest.architecture, None);
    let Backend::Ir {
        parameters,
        captures,
    } = &manifest.backend
    else {
        panic!("expected an IR backend, got {:?}", manifest.backend);
    };
    let names: Vec<&str> = parameters.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["mic", "preset"]);
    assert_eq!(captures.len(), 12);
    assert_eq!(captures[0].output_gain_db, Some(2.0));
    assert_eq!(captures[3].output_gain_db, Some(-1.0));
    assert_eq!(captures[3].file, PathBuf::from("captures/003.wav"));
}

#[test]
fn a_single_capture_plugin_has_no_parameters() {
    let tone = tone(r#"{"id":5,"title":"One Shot","format":"nam","gear":"pedal"}"#);
    let files = vec![CaptureFile {
        name: "Only".into(),
        file: PathBuf::from("captures/000.nam"),
        output_gain_db: None,
    }];
    let manifest = build_manifest(
        &tone,
        PackageKind::Nam(Tone3000Architecture::A2),
        Tone3000BlockType::GainPedal,
        &files,
        Some(1.0),
    );
    validate_manifest(&manifest).unwrap();
    assert_eq!(manifest.author, None);
    assert_eq!(manifest.brand, None);
    assert_eq!(
        manifest.sources,
        Some(vec!["https://www.tone3000.com/tones/5".to_string()])
    );
    let Backend::Nam {
        parameters,
        captures,
    } = &manifest.backend
    else {
        panic!("expected a NAM backend");
    };
    assert!(parameters.is_empty());
    assert!(captures[0].values.is_empty());
}

#[test]
fn the_manifest_survives_a_yaml_round_trip() {
    let tone = search_tone("search_ir.json", 67544);
    let files = captures("models_67544_ir.json", "wav", Some(2.0));
    let manifest = build_manifest(&tone, PackageKind::Ir, Tone3000BlockType::Cab, &files, None);
    let yaml = serde_yaml::to_string(&manifest).unwrap();
    let back: PluginManifest = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(back, manifest);
}
