//! Responsibility: reads the level one output route played for the meter row.
//!
//! #1074: one jack is one pipeline fanning out to every output, so a
//! per-stream meter can no longer tell MAIN from FRFR. Each route records the
//! loudest |sample| it handed its device — after the chain volume and its own
//! faders — and the meter row drains it here, one meter per output.

use std::sync::atomic::Ordering;

use crate::output_meter::SILENT_DBFS;
use crate::runtime_state::ChainRuntimeState;

impl ChainRuntimeState {
    /// Loudest sample `route` played since the previous read, in dBFS
    /// ([`SILENT_DBFS`] when nothing was played). `None` when this runtime
    /// does not own the route. Off the audio thread only.
    pub fn take_route_meter_dbfs(&self, route: usize) -> Option<f32> {
        let routes = self.output_routes.load();
        let state = routes.get(route)?.as_ref()?;
        let peak = f32::from_bits(state.meter_peak_bits.swap(0, Ordering::Relaxed));
        Some(if peak > 0.0 {
            20.0 * peak.log10()
        } else {
            SILENT_DBFS
        })
    }
}
