//! Responsibility: measures the level a set of capture files plays at.

use std::path::{Path, PathBuf};

use super::api_enums::Tone3000BlockType;
use super::axes::CaptureKind;
use super::install_error::InstallError;
use super::ir_wav::load_ir_mono_48k;
use super::level_nam::nam_output_gain_db;
use super::level_policy::{ir_level_peak_dbfs, target_gain_db, IrRole};
use super::manifest_build::CaptureFile;
use super::synthetic_di::default_guitar_di;

/// NAM: one gain for the package (the loudest capture). IR: one gain per
/// capture, as a cab or a body. `files` are relative to `dir`.
pub fn measure_captures(
    dir: &Path,
    mut files: Vec<CaptureFile>,
    kind: CaptureKind,
    block_type: Tone3000BlockType,
) -> Result<(Vec<CaptureFile>, Option<f32>), InstallError> {
    let di = default_guitar_di();
    let measure_err = |e: anyhow::Error| InstallError::Measure(format!("{e:#}"));
    match kind {
        CaptureKind::Nam => {
            let paths: Vec<PathBuf> = files.iter().map(|f| dir.join(&f.file)).collect();
            let gain = nam_output_gain_db(&di, &paths).map_err(measure_err)?;
            Ok((files, Some(gain)))
        }
        CaptureKind::Ir => {
            let role = if block_type == Tone3000BlockType::Body {
                IrRole::Body
            } else {
                IrRole::Cab
            };
            for file in &mut files {
                let ir = load_ir_mono_48k(&dir.join(&file.file)).map_err(measure_err)?;
                file.output_gain_db = Some(target_gain_db(ir_level_peak_dbfs(&ir, &di, role)));
            }
            Ok((files, None))
        }
    }
}
