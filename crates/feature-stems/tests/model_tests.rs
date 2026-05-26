//! RED-first tests for the model download / cache layer.

use std::cell::RefCell;
use std::fs;

use sha2::{Digest, Sha256};

struct StaticDownloader {
    bytes: Vec<u8>,
    calls: RefCell<usize>,
}

impl feature_stems::ModelDownloader for StaticDownloader {
    fn download(&self, _url: &str) -> Result<Vec<u8>, String> {
        *self.calls.borrow_mut() += 1;
        Ok(self.bytes.clone())
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex_encode(&hasher.finalize())
}

fn hex_encode(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len() * 2);
    for byte in input {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[test]
fn ensure_model_downloads_when_cache_is_empty_and_writes_target_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let model_dir = dir.path().join("models").join("htdemucs");
    let bytes = b"fake-onnx-model-bytes".to_vec();
    let sha = sha256_hex(&bytes);
    let downloader = StaticDownloader {
        bytes: bytes.clone(),
        calls: RefCell::new(0),
    };

    let path = feature_stems::ensure_model_with(
        &model_dir,
        "https://example.invalid/model.onnx",
        &sha,
        "htdemucs.onnx",
        &downloader,
    )
    .expect("download new model");

    assert!(path.exists(), "model file must exist on disk");
    assert_eq!(
        path.file_name().and_then(|s| s.to_str()),
        Some("htdemucs.onnx")
    );
    assert_eq!(fs::read(&path).expect("read model"), bytes);
    assert_eq!(*downloader.calls.borrow(), 1, "must call downloader once");
}

#[test]
fn ensure_model_is_a_noop_when_cache_already_valid() {
    let dir = tempfile::tempdir().expect("tempdir");
    let model_dir = dir.path().join("htdemucs");
    let bytes = b"fake-onnx-model-bytes".to_vec();
    let sha = sha256_hex(&bytes);

    let first = feature_stems::ensure_model_with(
        &model_dir,
        "https://example.invalid/model.onnx",
        &sha,
        "htdemucs.onnx",
        &StaticDownloader {
            bytes: bytes.clone(),
            calls: RefCell::new(0),
        },
    )
    .expect("seed cache");

    let second_downloader = StaticDownloader {
        bytes: b"different-bytes-that-would-be-served".to_vec(),
        calls: RefCell::new(0),
    };
    let second = feature_stems::ensure_model_with(
        &model_dir,
        "https://example.invalid/model.onnx",
        &sha,
        "htdemucs.onnx",
        &second_downloader,
    )
    .expect("hit cache");

    assert_eq!(first, second);
    assert_eq!(
        *second_downloader.calls.borrow(),
        0,
        "must NOT call downloader when cache is valid"
    );
    assert_eq!(fs::read(&second).expect("read cached"), bytes);
}

#[test]
fn ensure_model_returns_error_when_downloaded_bytes_have_wrong_sha() {
    let dir = tempfile::tempdir().expect("tempdir");
    let model_dir = dir.path().join("htdemucs");
    let bytes = b"actual-bytes".to_vec();
    let wrong_sha = sha256_hex(b"different-bytes");
    let downloader = StaticDownloader {
        bytes,
        calls: RefCell::new(0),
    };

    let err = feature_stems::ensure_model_with(
        &model_dir,
        "https://example.invalid/model.onnx",
        &wrong_sha,
        "htdemucs.onnx",
        &downloader,
    )
    .expect_err("must fail on SHA mismatch");

    assert!(
        matches!(err, feature_stems::StemError::ModelDownload { .. }),
        "expected ModelDownload error, got {err:?}"
    );
    assert!(
        !model_dir.join("htdemucs.onnx").exists(),
        "must not write file when SHA mismatches"
    );
}

#[test]
fn ensure_model_redownloads_when_cached_file_has_unexpected_sha() {
    let dir = tempfile::tempdir().expect("tempdir");
    let model_dir = dir.path().join("htdemucs");
    fs::create_dir_all(&model_dir).expect("create model dir");
    let stale_path = model_dir.join("htdemucs.onnx");
    fs::write(&stale_path, b"stale-corrupted-bytes").expect("seed stale");

    let fresh = b"fresh-correct-bytes".to_vec();
    let sha = sha256_hex(&fresh);
    let downloader = StaticDownloader {
        bytes: fresh.clone(),
        calls: RefCell::new(0),
    };

    let path = feature_stems::ensure_model_with(
        &model_dir,
        "https://example.invalid/model.onnx",
        &sha,
        "htdemucs.onnx",
        &downloader,
    )
    .expect("re-download on stale cache");

    assert_eq!(fs::read(&path).expect("read fresh"), fresh);
    assert_eq!(
        *downloader.calls.borrow(),
        1,
        "must download once to replace stale"
    );
}
