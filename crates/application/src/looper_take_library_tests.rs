//! #827, red-first: the app-wide library of saved looper takes.
//!
//! A take is a looper mixdown the user named and kept. It lives in ONE folder
//! every project sees, as a plain wav, so the DI can play it through
//! `DiLoopSource::File` exactly like any file the user picks by hand.

use super::*;

#[test]
fn a_take_name_becomes_a_wav_file_name() {
    assert_eq!(take_file_name("verse riff").unwrap(), "verse riff.wav");
}

#[test]
fn a_typed_wav_extension_is_not_doubled() {
    assert_eq!(take_file_name("solo.WAV").unwrap(), "solo.wav");
}

#[test]
fn a_name_cannot_climb_out_of_the_library() {
    let name = take_file_name("../../etc/passwd").unwrap();
    assert!(
        !name.contains('/') && !name.contains('\\') && !name.starts_with('.'),
        "the file must stay inside the take folder: {name}"
    );
}

#[test]
fn an_empty_name_is_refused() {
    assert_eq!(take_file_name("   "), Err(TakeSaveError::EmptyName));
    assert_eq!(take_file_name(".wav"), Err(TakeSaveError::EmptyName));
    assert_eq!(take_file_name("../.."), Err(TakeSaveError::EmptyName));
}

#[test]
fn a_saved_take_reads_back_as_the_stereo_mixdown_at_its_own_rate() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = tmp.path().join("looper-takes");
    let pcm: Vec<f32> = (0..128).map(|i| (i as f32) / 256.0).collect();

    let path = save_take(&dir, "groove", &pcm, 44_100).expect("save");

    assert_eq!(path, dir.join("groove.wav"));
    let wav = adapter_render::wav::read_wav(&path).expect("read back");
    assert_eq!(wav.channels, 2, "the looper mixdown is interleaved stereo");
    assert_eq!(
        wav.sample_rate_hz, 44_100,
        "the take keeps its recorded rate"
    );
    assert_eq!(wav.samples, pcm);
}

#[test]
fn saving_over_an_existing_take_is_refused_and_keeps_the_old_one() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = tmp.path().to_path_buf();
    save_take(&dir, "keeper", &[0.5; 8], 48_000).expect("first save");

    let second = save_take(&dir, "keeper", &[0.1; 16], 48_000);

    assert_eq!(second, Err(TakeSaveError::NameTaken("keeper.wav".into())));
    let wav = adapter_render::wav::read_wav(&dir.join("keeper.wav")).expect("read");
    assert_eq!(wav.samples, vec![0.5; 8], "the first take must survive");
}

#[test]
fn an_empty_mixdown_is_nothing_to_save() {
    let tmp = tempfile::tempdir().expect("tempdir");
    assert_eq!(
        save_take(tmp.path(), "silence", &[], 48_000),
        Err(TakeSaveError::NothingRecorded)
    );
    assert!(!tmp.path().join("silence.wav").exists());
}

#[test]
fn the_library_lists_its_wavs_in_name_order() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = tmp.path();
    save_take(dir, "b-side", &[0.1; 4], 48_000).unwrap();
    save_take(dir, "a-side", &[0.1; 4], 48_000).unwrap();
    std::fs::write(dir.join("notes.txt"), "not audio").unwrap();

    assert_eq!(
        list_takes(dir),
        vec![dir.join("a-side.wav"), dir.join("b-side.wav")]
    );
}

#[test]
fn a_library_that_was_never_written_is_empty() {
    let tmp = tempfile::tempdir().expect("tempdir");
    assert!(list_takes(&tmp.path().join("never-created")).is_empty());
}

#[test]
fn every_refusal_reads_as_a_sentence() {
    assert_eq!(TakeSaveError::EmptyName.to_string(), "a take needs a name");
    assert_eq!(
        TakeSaveError::NameTaken("verse.wav".into()).to_string(),
        "a take named verse.wav already exists"
    );
    assert_eq!(
        TakeSaveError::NothingRecorded.to_string(),
        "this looper holds no recorded audio"
    );
    assert_eq!(
        TakeSaveError::Io("disk full".into()).to_string(),
        "could not write the take: disk full"
    );
}

#[test]
fn a_take_the_file_system_cannot_create_is_an_io_error() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = tmp.path().join("looper-takes");
    // Past every file system's 255-byte name limit: the folder exists, the
    // file cannot be opened, and that is not "the name is taken".
    let name = "a".repeat(300);
    let pcm = vec![0.0_f32; 8];

    let err = save_take(&dir, &name, &pcm, 48_000).expect_err("must fail");

    assert!(matches!(err, TakeSaveError::Io(_)), "got {err:?}");
}

#[test]
fn a_take_whose_audio_cannot_be_written_leaves_no_file_behind() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = tmp.path().join("looper-takes");
    // Stereo: an odd sample count leaves the last frame half-written, which
    // the wav writer refuses on finalize.
    let pcm = vec![0.1_f32; 3];

    let err = save_take(&dir, "verse", &pcm, 48_000).expect_err("must fail");

    assert!(matches!(err, TakeSaveError::Io(_)), "got {err:?}");
    assert!(
        list_takes(&dir).is_empty(),
        "a half-written take must not show up in the DI picker"
    );
}
