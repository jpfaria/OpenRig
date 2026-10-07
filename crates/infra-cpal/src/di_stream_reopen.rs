//! Responsibility: carries the isolated outputs across a device restart.
//!
//! #1081: an armed DI or looper plays through its own output stream (#808).
//! A device restart has to close that stream with the others on the device,
//! or OpenRig's IO on the device never stops. The playback itself stays
//! parked in its cell and its render worker keeps its place; only the stream
//! goes, and the one that replaces it fades in.

use project::project::Project;

use crate::di_stream::IsolatedKey;
use crate::stream_handover::StreamHandover;
use crate::ProjectRuntimeController;

impl ProjectRuntimeController {
    /// Close the own output stream of every isolated playback that plays on
    /// one of `devices`. Returns the playbacks whose stream it closed.
    pub(crate) fn release_isolated_outputs_on(&self, devices: &[String]) -> Vec<IsolatedKey> {
        let mut released = Vec::new();
        let mut closed = Vec::new();
        for (key, handle) in self.di_streams.borrow_mut().iter_mut() {
            let on_device = handle
                .output
                .as_ref()
                .is_some_and(|output| devices.contains(&output.entry.device_id.0));
            if !on_device {
                continue;
            }
            if let Some(stream) = handle.output_stream.take() {
                closed.push(stream);
                released.push(key.clone());
            }
        }
        // Closed only once the map is no longer borrowed.
        drop(closed);
        released
    }

    /// Open a new output stream for each of `keys`. A playback disarmed since,
    /// or re-armed with a stream of its own (an activation re-arms its
    /// chain's DI), is left as it is.
    pub(crate) fn reopen_isolated_outputs(&self, project: &Project, keys: &[IsolatedKey]) {
        for key in keys {
            let Some(chain) = project.chains.iter().find(|chain| chain.id == key.0) else {
                continue;
            };
            let waiting = self
                .di_streams
                .borrow()
                .get(key)
                .filter(|handle| handle.output_stream.is_none())
                .and_then(|handle| Some((handle.output.clone()?, handle.cell.clone())));
            let Some((output, cell)) = waiting else {
                continue;
            };
            let fade = StreamHandover::replacing().output_fade();
            let Some(stream) = self.build_di_output_stream(chain, &output, &cell, fade) else {
                log::error!(
                    "chain '{}': its isolated output did not reopen after the device restart",
                    key.0 .0
                );
                continue;
            };
            if let Some(handle) = self.di_streams.borrow_mut().get_mut(key) {
                handle.output_stream = Some(stream);
            }
        }
    }
}
