//! Cold-activation scheduling (issue #672), split out of `controller.rs` to keep
//! it under the line cap — a cohesive unit like `controller_offthread_live_rebuild`
//! and `controller_block_toggle`.
//!
//! `schedule_chain_activation` builds a not-yet-streaming chain's per-input
//! runtimes off the control worker; the poll tick then creates the cpal streams
//! (they are `!Send`) and installs the chain. It also re-arms a monitored DI
//! (#808) so a live config edit is heard even when the guitar stream is not
//! active — see the method doc.

use anyhow::Result;

use project::chain::Chain;
use project::project::Project;

use crate::ProjectRuntimeController;

#[cfg(not(all(target_os = "linux", feature = "jack")))]
use crate::chain_resolve::resolve_chain_audio_config;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use crate::elastic::compute_elastic_targets_for_chain;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use crate::host::get_host;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use crate::validation::validate_chain_channels_against_devices;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use crate::{build_chain_runtime, BuildRequest};

impl ProjectRuntimeController {
    /// Issue #672 — cold activation. If `chain` is not yet streaming, build its
    /// per-input runtimes (the heavy NAM/IR load) on the control worker and
    /// return `true`; the next poll creates the cpal streams on the frontend
    /// (they are `!Send`) and installs the chain. Returns `false` only for an
    /// already-streaming chain, which the caller then rebuilds synchronously.
    ///
    /// Issue #740: this used to defer ANY multi-device chain to the synchronous
    /// build (`input_devices.len() != 1`), so a rig bound to several interfaces
    /// (the owner's four-binding, two-interface boot) brought every stream up
    /// serially on the calling thread — the first streams ran their callback,
    /// counting underruns, while the remaining NAM/IR builds still blocked. The
    /// off-thread path already builds one runtime per input group
    /// (`build_per_input_runtime_states`) and installs one stream per device
    /// (`build_chain_streams`), so multi-device chains take it too: every
    /// runtime is built off-thread, then ALL streams are created and played
    /// together in one poll tick — no sibling starves another's bring-up.
    ///
    /// Issue #808: a monitored DI is an independent pre-render (invariant #4),
    /// so a live config edit must re-render it even when the guitar stream is
    /// NOT active — the "only the DI is running" case, where the GUI's live edit
    /// lands here (cold activation) instead of the live-rebuild path. The active
    /// path (`request_offthread_rebuild_if_live`) and the block-toggle path both
    /// re-arm; this one used to forget, so the DI kept the stale render and the
    /// timbre only changed after a block toggle. Re-arm here too: no-op when
    /// nothing is armed, and the guitar activation landing later re-arms again
    /// harmlessly (the #785 hand-off leaves exactly one render alive).
    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    pub fn schedule_chain_activation(&mut self, project: &Project, chain: &Chain) -> Result<bool> {
        if self.active_chains.contains_key(&chain.id) {
            return Ok(false); // already streaming — not a cold activation
        }

        // #693: validation + device resolution are CoreAudio property
        // queries costing hundreds of ms — they run on the control worker
        // together with the heavy build, never on the calling thread.
        let project_for_build = project.clone();
        let chain_for_build = chain.clone();
        let registry_for_build = self.io_bindings.clone();
        let rx = self.worker.submit(move || {
            let host = get_host();
            validate_chain_channels_against_devices(host, &chain_for_build, &registry_for_build)?;
            let resolved = resolve_chain_audio_config(
                host,
                &project_for_build,
                &chain_for_build,
                &registry_for_build,
            )?;
            let elastic_targets =
                compute_elastic_targets_for_chain(&chain_for_build, &resolved, &registry_for_build);
            let request = BuildRequest {
                chain: chain_for_build,
                sample_rate: resolved.sample_rate,
                device_sample_rates: resolved.by_device.clone(),
                buffer_sizes: elastic_targets,
                io_bindings: registry_for_build,
            };
            Ok((build_chain_runtime(&request)?, resolved))
        });
        self.pending_activations
            .push((chain.id.clone(), chain.clone(), rx));
        // #808: re-render the monitored DI from the edited config now (no-op when
        // nothing is armed), decoupled from the guitar build landing.
        self.rearm_di_stream_after_rebuild(chain);
        Ok(true)
    }

    /// JACK build keeps cold activation synchronous (issue #672 wires cpal first).
    #[cfg(all(target_os = "linux", feature = "jack"))]
    pub fn schedule_chain_activation(
        &mut self,
        _project: &Project,
        _chain: &Chain,
    ) -> Result<bool> {
        Ok(false)
    }
}
