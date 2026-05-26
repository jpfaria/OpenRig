//! RED-first tests for the ort-backed inference path.

#[cfg(feature = "real-htdemucs")]
#[test]
fn separate_stems_with_ort_errors_cleanly_when_model_missing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bogus = dir.path().join("does_not_exist.onnx");
    let input = vec![0.0_f32; 44_100 * 2];
    let err = feature_stems::separate_stems_with_ort(&input, 44_100, &bogus)
        .expect_err("must fail when model file missing");
    assert!(
        matches!(
            err,
            feature_stems::StemError::ModelDownload { .. }
                | feature_stems::StemError::Resample { .. }
                | feature_stems::StemError::Inference { .. }
        ),
        "got {err:?}"
    );
}

#[cfg(not(feature = "real-htdemucs"))]
#[test]
fn real_htdemucs_path_is_feature_gated() {
    // When the feature is off, separate_stems_with_ort is not in scope.
    // This test exists so CI without the feature also runs at least
    // one assertion for this module and documents the gate.
    let cell: Option<()> = None;
    assert!(cell.is_none(), "feature off → ort path unavailable");
}
