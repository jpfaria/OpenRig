//! Responsibility: measures the output level of NAM captures through the reference DI.
//!
//! Port of OpenRig-plugins `tools/loudness_audit/src/nam_run.rs`: the whole
//! DI runs offline through each model, spread over half the cores so an
//! install does not starve the audio threads. The manifest carries one gain
//! for every capture, so the loudest one decides it.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::{anyhow, Context, Result};
use nam::model_diag::{close_model_diag, nam_process, open_model_diag};

use super::level_policy::{peak_dbfs, target_gain_db};

pub fn run_nam(input: &[f32], model: &Path) -> Result<Vec<f32>> {
    let path = model
        .to_str()
        .ok_or_else(|| anyhow!("non-UTF-8 model path: {}", model.display()))?;
    let handle = open_model_diag(path).with_context(|| format!("open {path}"))?;
    let mut out = vec![0.0_f32; input.len()];
    // SAFETY: `handle` was just opened, is used by this thread only and
    // closed right after; `out` matches `input` in length.
    unsafe {
        nam_process(handle, input, &mut out);
        close_model_diag(handle);
    }
    Ok(out)
}

/// Peak of `input` through each model, in `models` order.
pub fn nam_level_peaks_dbfs(input: &[f32], models: &[PathBuf]) -> Result<Vec<f32>> {
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    let workers = (cores / 2).max(1).min(models.len().max(1));
    let next = AtomicUsize::new(0);
    let per_worker: Vec<Result<Vec<(usize, f32)>>> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                s.spawn(|| -> Result<Vec<(usize, f32)>> {
                    let mut mine = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some(model) = models.get(i) else { break };
                        mine.push((i, peak_dbfs(&run_nam(input, model)?)));
                    }
                    Ok(mine)
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| {
                h.join()
                    .unwrap_or_else(|_| Err(anyhow!("NAM worker panicked")))
            })
            .collect()
    });
    let mut peaks = vec![f32::NEG_INFINITY; models.len()];
    for worker in per_worker {
        for (i, peak) in worker? {
            peaks[i] = peak;
        }
    }
    Ok(peaks)
}

/// Manifest `output_gain_db` of a set of captures.
pub fn nam_output_gain_db(di: &[f32], models: &[PathBuf]) -> Result<f32> {
    let loudest = nam_level_peaks_dbfs(di, models)?
        .into_iter()
        .fold(f32::NEG_INFINITY, f32::max);
    Ok(target_gain_db(loudest))
}
