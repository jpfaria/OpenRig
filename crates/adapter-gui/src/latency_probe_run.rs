//! Responsibility: measures one chain's latency into its badge.
//!
//! Split out of `latency_probe` (#913). Registering the click is screen work;
//! resolving the chain, asking at the rate the rig is REALLY running at, and
//! writing the reading onto the row is the measurement itself.

use std::time::{Duration, Instant};

use application::live_source::LiveSource;
use slint::{Model, VecModel};

use crate::latency_probe::ProbeWindows;
use crate::state::ProjectSession;
use crate::ProjectChainItem;

/// How long a measured badge stays on screen before the expiry sweep clears it.
pub(crate) const BADGE_WINDOW: Duration = Duration::from_secs(10);

/// Measure the chain at `index` and write the reading onto its row, opening the
/// badge's display window. Returns the measured value, or `None` when there was
/// nothing to measure (no project, no such chain, the probe failed).
///
/// #723/#127: the rate comes from the SEAM first. `engine_sr` mirrors a running
/// stream and falls back to the reference rate when the rig is stopped, so
/// asking it first measured a stopped rig on a 44.1 kHz interface as if it ran
/// at 48 kHz. Same order as `openrig://chains/{id}/latency`, so the badge and
/// every transport report one number.
pub(crate) fn probe_chain_latency(
    session: &ProjectSession,
    live: &dyn LiveSource,
    project_chains: &VecModel<ProjectChainItem>,
    probe_windows: &ProbeWindows,
    index: usize,
    now: Instant,
) -> Option<f32> {
    let chain_id = session
        .project
        .borrow()
        .chains
        .get(index)
        .map(|c| c.id.clone())?;
    let sample_rate = live
        .chain_sample_rate(&chain_id)
        .unwrap_or_else(|| session.dispatcher.engine_sr() as f32);
    let report = application::query_latency::measure_chain_latency(
        &session.project.borrow(),
        &session.io_bindings.borrow(),
        &chain_id,
        sample_rate,
    )
    .ok()?;
    let ms = report.dsp_latency_ms;
    probe_windows.borrow_mut().insert(index, now + BADGE_WINDOW);
    if let Some(mut item) = project_chains.row_data(index) {
        item.latency_ms = ms;
        project_chains.set_row_data(index, item);
    }
    Some(ms)
}

#[cfg(test)]
#[path = "latency_probe_run_tests.rs"]
mod tests;
