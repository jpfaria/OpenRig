//! End-to-end wiring test for the calibration shell: a synthetic on-disk corpus
//! (two genres, distinct spectra) → `calibrate_corpus` → per-genre profiles.
//!
//! Everything is written under `CARGO_TARGET_TMPDIR` — never the user's real
//! `~/.openrig` files, never the session scratchpad.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tone_calibrate::{calibrate_corpus, Manifest};

const SR: u32 = 48_000;

/// One second of a sum of sines, written as 32-bit float WAV mono.
fn write_stem(path: &Path, components: &[(f32, f32)]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: SR,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    for i in 0..SR as usize {
        let s: f32 = components
            .iter()
            .map(|(f, a)| a * (2.0 * std::f32::consts::PI * f * i as f32 / SR as f32).sin())
            .sum();
        w.write_sample(s).unwrap();
    }
    w.finalize().unwrap();
}

/// A song folder with both `refs/{lead,rhythm}.wav`.
fn write_song(root: &Path, song: &str, components: &[(f32, f32)]) {
    for stem in ["lead.wav", "rhythm.wav"] {
        write_stem(&root.join(song).join("refs").join(stem), components);
    }
}

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).join("corpus_calibration")
}

#[test]
fn calibrates_one_profile_per_genre_from_disk() {
    let root = corpus_root();
    let _ = std::fs::remove_dir_all(&root);

    // Two "bright" songs and one "dark" song — distinct spectra so the two
    // genres get measurably different limits.
    write_song(&root, "bright-a", &[(1_000.0, 0.2), (5_000.0, 0.5)]);
    write_song(&root, "bright-b", &[(1_000.0, 0.2), (5_000.0, 0.5)]);
    write_song(&root, "dark-a", &[(220.0, 0.5), (1_000.0, 0.2)]);

    let manifest: Manifest = BTreeMap::from([
        ("bright-a".to_string(), "bright".to_string()),
        ("bright-b".to_string(), "bright".to_string()),
        ("dark-a".to_string(), "dark".to_string()),
    ]);

    let profiles = calibrate_corpus(&root, &manifest, 0.9).unwrap();

    let genres: Vec<&str> = profiles.iter().map(|p| p.genre.as_str()).collect();
    assert_eq!(genres, vec!["bright", "dark"], "one profile per genre: {profiles:?}");

    let bright = profiles.iter().find(|p| p.genre == "bright").unwrap();
    let dark = profiles.iter().find(|p| p.genre == "dark").unwrap();
    // 2 songs x 2 stems = 4 stems for bright; 1 song x 2 stems = 2 for dark.
    assert_eq!(bright.n, 4, "{bright:?}");
    assert_eq!(dark.n, 2, "{dark:?}");
    // The bright genre must carry more presence-band energy than the dark one.
    assert!(
        bright.fizz_limit > dark.fizz_limit,
        "bright fizz {} should exceed dark fizz {}",
        bright.fizz_limit,
        dark.fizz_limit
    );
}

#[test]
fn missing_stems_are_skipped_not_fatal() {
    let root = corpus_root().join("partial");
    let _ = std::fs::remove_dir_all(&root);
    // Only the lead stem exists; rhythm is absent.
    write_stem(
        &root.join("solo-song").join("refs").join("lead.wav"),
        &[(1_000.0, 0.3)],
    );

    let manifest: Manifest =
        BTreeMap::from([("solo-song".to_string(), "test".to_string())]);

    let profiles = calibrate_corpus(&root, &manifest, 0.9).unwrap();
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0].n, 1, "only the present stem counts: {:?}", profiles[0]);
}
