//! #879: installing a TONE3000 tone downloads every capture, measures its
//! level and writes a plugin package the loader discovers; uninstalling only
//! ever removes a TONE3000 package.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use application::tone3000::api_client::{all_models, ApiError, Tone3000Api};
use application::tone3000::api_types::{Model, Page, Tone};
use application::tone3000::api_url::SearchQuery;
use application::tone3000::install::{install_tone, InstallError, InstallProgress, InstallRequest};
use application::tone3000::installed::{list_installed, remove_installed};
use application::tone3000::{Tone3000Architecture, Tone3000BlockType};
use plugin_loader::manifest::{Backend, BlockType, NamArchitecture};

/// In-memory TONE3000: one tone, its model rows and a local file per URL.
struct FakeApi {
    tone: Tone,
    models: Vec<Model>,
    files: BTreeMap<String, PathBuf>,
    page_size: usize,
    model_calls: Mutex<Vec<(Option<Tone3000Architecture>, u32)>>,
}

impl Tone3000Api for FakeApi {
    fn search(&self, _query: &SearchQuery) -> Result<Page<Tone>, ApiError> {
        Ok(Page {
            data: vec![self.tone.clone()],
            page: 1,
            page_size: 25,
            total: 1,
            total_pages: 1,
        })
    }

    fn tone(&self, id: u64) -> Result<Tone, ApiError> {
        if id == self.tone.id {
            Ok(self.tone.clone())
        } else {
            Err(ApiError::Http(404))
        }
    }

    fn models_page(
        &self,
        _tone_id: u64,
        architecture: Option<Tone3000Architecture>,
        page: u32,
    ) -> Result<Page<Model>, ApiError> {
        self.model_calls.lock().unwrap().push((architecture, page));
        let chunks: Vec<Vec<Model>> = self
            .models
            .chunks(self.page_size)
            .map(|c| c.to_vec())
            .collect();
        let total_pages = chunks.len().max(1) as u32;
        Ok(Page {
            data: chunks.get(page as usize - 1).cloned().unwrap_or_default(),
            page,
            page_size: self.page_size as u32,
            total: self.models.len() as u32,
            total_pages,
        })
    }

    fn download(&self, url: &str, dest: &Path) -> Result<(), ApiError> {
        let source = self.files.get(url).ok_or(ApiError::Http(404))?;
        std::fs::copy(source, dest).map_err(|e| ApiError::Io(e.to_string()))?;
        Ok(())
    }
}

fn model(id: u64, name: &str, url: &str) -> Model {
    serde_json::from_value(serde_json::json!({
        "id": id, "tone_id": 1, "name": name, "model_url": url
    }))
    .unwrap()
}

fn ts9_file(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/plugins/nam/ts9_grid/captures")
        .join(name)
}

fn nam_api() -> FakeApi {
    let tone: Tone = serde_json::from_value(serde_json::json!({
        "id": 1, "title": "Fake TS9", "format": "nam", "gear": "pedal",
        "a1_models_count": 2, "a2_models_count": 2, "license": "t3k",
        "user": {"username": "someone"},
        "makes": [{"name": "Ibanez TS9"}],
        "url": "https://www.tone3000.com/tones/fake-ts9-1"
    }))
    .unwrap();
    let models = vec![
        model(10, "TS9 Drive 0", "https://cdn/a/d0.nam"),
        model(11, "TS9 Drive 5", "https://cdn/a/d5.nam"),
        model(12, "TS9 Drive 5", "https://cdn/b/d5.nam"),
    ];
    let files = BTreeMap::from([
        ("https://cdn/a/d0.nam".to_string(), ts9_file("d0_t3_l6.nam")),
        ("https://cdn/a/d5.nam".to_string(), ts9_file("d5_t6_l6.nam")),
        ("https://cdn/b/d5.nam".to_string(), ts9_file("d5_t6_l6.nam")),
    ]);
    FakeApi {
        tone,
        models,
        files,
        page_size: 2,
        model_calls: Mutex::new(Vec::new()),
    }
}

fn write_ir(path: &Path, gain: f32) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 48_000,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(path, spec).unwrap();
    for tap in [1.0_f32, 0.5, 0.25] {
        writer.write_sample(gain * tap).unwrap();
    }
    writer.finalize().unwrap();
}

fn ir_api(dir: &Path) -> FakeApi {
    let tone: Tone = serde_json::from_value(serde_json::json!({
        "id": 2, "title": "Fake Cab", "format": "ir", "gear": "cab",
        "irs_count": 2, "user": {"username": "cabber", "display_name": "Cab Co"}
    }))
    .unwrap();
    let loud = dir.join("loud.wav");
    let soft = dir.join("soft.wav");
    write_ir(&loud, 0.5);
    write_ir(&soft, 0.125);
    FakeApi {
        tone,
        models: vec![
            model(20, "Cab SM57 Cap", "https://cdn/ir/loud.wav"),
            model(21, "Cab SM57 Cone", "https://cdn/ir/soft.wav"),
        ],
        files: BTreeMap::from([
            ("https://cdn/ir/loud.wav".to_string(), loud),
            ("https://cdn/ir/soft.wav".to_string(), soft),
        ]),
        page_size: 100,
        model_calls: Mutex::new(Vec::new()),
    }
}

fn request(tone_id: u64) -> InstallRequest {
    InstallRequest {
        tone_id,
        architecture: None,
        block_type: None,
    }
}

fn entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|it| {
            it.map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

#[test]
fn paging_collects_every_model_row() {
    let api = nam_api();
    let rows = all_models(&api, 1, Some(Tone3000Architecture::A1)).unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(
        *api.model_calls.lock().unwrap(),
        [
            (Some(Tone3000Architecture::A1), 1),
            (Some(Tone3000Architecture::A1), 2)
        ]
    );
}

#[test]
fn a_nam_tone_installs_as_a_discoverable_plugin() {
    let root = tempfile::tempdir().unwrap();
    let api = nam_api();
    let mut steps = Vec::new();
    let installed = install_tone(
        &api,
        root.path(),
        &InstallRequest {
            tone_id: 1,
            architecture: Some(Tone3000Architecture::A1),
            block_type: None,
        },
        &mut |p| steps.push(p),
    )
    .unwrap();

    assert_eq!(installed.plugin_id, "tone3000_1_a1");
    assert_eq!(installed.dir, root.path().join("tone3000_1_a1"));
    assert_eq!(installed.manifest.block_type, BlockType::GainPedal);
    assert_eq!(installed.manifest.architecture, Some(NamArchitecture::A1));
    let gain = installed.manifest.output_gain_db.expect("NAM level");
    assert!((-24.0..=24.0).contains(&gain));

    let Backend::Nam { captures, .. } = &installed.manifest.backend else {
        panic!("expected NAM");
    };
    assert_eq!(captures.len(), 2, "duplicate file rows collapse");
    for capture in captures {
        assert!(installed.dir.join(&capture.file).is_file());
    }

    let found = plugin_loader::discover(root.path()).unwrap();
    assert_eq!(found.len(), 1);
    let package = found.into_iter().next().unwrap().expect("valid package");
    assert_eq!(package.manifest.id, "tone3000_1_a1");

    assert_eq!(steps.first(), Some(&InstallProgress::Fetching));
    assert!(steps.contains(&InstallProgress::Downloading { done: 2, total: 2 }));
    assert!(steps.contains(&InstallProgress::Measuring));
    assert_eq!(entries(root.path()), ["tone3000_1_a1"], "no staging left");
}

#[test]
fn a2_is_the_default_architecture() {
    let root = tempfile::tempdir().unwrap();
    let api = nam_api();
    let installed = install_tone(&api, root.path(), &request(1), &mut |_| {}).unwrap();
    assert_eq!(installed.plugin_id, "tone3000_1_a2");
    assert!(api
        .model_calls
        .lock()
        .unwrap()
        .iter()
        .all(|(arch, _)| *arch == Some(Tone3000Architecture::A2)));
}

#[test]
fn a1_is_the_default_when_the_tone_has_no_a2_models() {
    let root = tempfile::tempdir().unwrap();
    let mut api = nam_api();
    api.tone.a2_models_count = 0;
    let installed = install_tone(&api, root.path(), &request(1), &mut |_| {}).unwrap();
    assert_eq!(installed.plugin_id, "tone3000_1_a1");
}

#[test]
fn the_user_can_override_the_block_type() {
    let root = tempfile::tempdir().unwrap();
    let api = nam_api();
    let installed = install_tone(
        &api,
        root.path(),
        &InstallRequest {
            tone_id: 1,
            architecture: None,
            block_type: Some(Tone3000BlockType::Preamp),
        },
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(installed.manifest.block_type, BlockType::Preamp);
}

#[test]
fn an_ir_tone_gets_one_level_per_capture() {
    let root = tempfile::tempdir().unwrap();
    let assets = tempfile::tempdir().unwrap();
    let api = ir_api(assets.path());
    let installed = install_tone(&api, root.path(), &request(2), &mut |_| {}).unwrap();

    assert_eq!(installed.plugin_id, "tone3000_2");
    assert_eq!(installed.manifest.block_type, BlockType::Cab);
    assert_eq!(installed.manifest.output_gain_db, None);
    assert_eq!(installed.manifest.author.as_deref(), Some("Cab Co"));
    let Backend::Ir {
        parameters,
        captures,
    } = &installed.manifest.backend
    else {
        panic!("expected IR");
    };
    assert_eq!(parameters[0].name, "position");
    let loud = captures[0].output_gain_db.unwrap();
    let soft = captures[1].output_gain_db.unwrap();
    assert!(
        (soft - loud - 12.04).abs() < 0.05,
        "a quarter-level IR needs ~12 dB more ({loud} vs {soft})"
    );
    assert_eq!(api.model_calls.lock().unwrap()[0].0, None);
    assert!(plugin_loader::discover(root.path()).unwrap()[0].is_ok());
}

#[test]
fn installing_twice_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let api = nam_api();
    install_tone(&api, root.path(), &request(1), &mut |_| {}).unwrap();
    let again = install_tone(&api, root.path(), &request(1), &mut |_| {});
    assert_eq!(
        again.unwrap_err(),
        InstallError::AlreadyInstalled("tone3000_1_a2".into())
    );
}

#[test]
fn a_failed_download_leaves_nothing_behind() {
    let root = tempfile::tempdir().unwrap();
    let mut api = nam_api();
    api.files.remove("https://cdn/a/d5.nam");
    let result = install_tone(&api, root.path(), &request(1), &mut |_| {});
    assert_eq!(result.unwrap_err(), InstallError::Api(ApiError::Http(404)));
    assert!(
        entries(root.path()).is_empty(),
        "{:?}",
        entries(root.path())
    );
}

#[test]
fn a_tone_without_captures_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let mut api = nam_api();
    api.models.clear();
    let result = install_tone(&api, root.path(), &request(1), &mut |_| {});
    assert_eq!(result.unwrap_err(), InstallError::NoCaptures);
    assert!(entries(root.path()).is_empty());
}

#[test]
fn an_unknown_tone_reports_the_api_error() {
    let root = tempfile::tempdir().unwrap();
    let result = install_tone(&nam_api(), root.path(), &request(99), &mut |_| {});
    assert_eq!(result.unwrap_err(), InstallError::Api(ApiError::Http(404)));
}

#[test]
fn only_tone3000_packages_are_listed() {
    let root = tempfile::tempdir().unwrap();
    let assets = tempfile::tempdir().unwrap();
    install_tone(&nam_api(), root.path(), &request(1), &mut |_| {}).unwrap();
    install_tone(
        &ir_api(assets.path()),
        root.path(),
        &request(2),
        &mut |_| {},
    )
    .unwrap();
    std::fs::create_dir_all(root.path().join("someone_else")).unwrap();
    std::fs::create_dir_all(root.path().join("tone3000_broken")).unwrap();

    let listed: Vec<String> = list_installed(root.path())
        .into_iter()
        .map(|p| p.plugin_id)
        .collect();
    assert_eq!(listed, ["tone3000_1_a2", "tone3000_2"]);
}

#[test]
fn listing_a_missing_root_is_empty() {
    assert!(list_installed(Path::new("/nonexistent/tone3000")).is_empty());
}

#[test]
fn uninstall_removes_the_package() {
    let root = tempfile::tempdir().unwrap();
    install_tone(&nam_api(), root.path(), &request(1), &mut |_| {}).unwrap();
    remove_installed(root.path(), "tone3000_1_a2").unwrap();
    assert!(entries(root.path()).is_empty());
    assert_eq!(
        remove_installed(root.path(), "tone3000_1_a2").unwrap_err(),
        InstallError::NotInstalled("tone3000_1_a2".into())
    );
}

#[test]
fn uninstall_refuses_anything_outside_tone3000() {
    let root = tempfile::tempdir().unwrap();
    let outside = root.path().join("keep");
    std::fs::create_dir_all(&outside).unwrap();
    let tone_root = root.path().join("tone3000");
    std::fs::create_dir_all(tone_root.join("tone3000_1")).unwrap();

    for bad in [
        "../keep",
        "keep",
        "tone3000_1/..",
        "tone3000_1/x",
        "",
        "tone3000_",
    ] {
        assert!(
            matches!(
                remove_installed(&tone_root, bad),
                Err(InstallError::InvalidPluginId(_))
            ),
            "`{bad}` must be refused"
        );
    }
    assert!(outside.is_dir());
    assert!(tone_root.join("tone3000_1").is_dir());
}
