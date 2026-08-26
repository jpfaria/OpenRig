//! Responsibility: says which bindings survived a device refresh.

use domain::io_binding::IoBinding;
use domain::AudioDeviceDescriptor;

/// Result of checking an [`IoBinding`] against the live device list.
///
/// `unresolved = true` means at least one endpoint references a device id
/// that is no longer present in the enumerated input or output devices.  The
/// binding is always retained (never silently dropped); the UI can inspect
/// this flag to surface a warning to the user.
pub(crate) struct BindingStatus {
    /// The original binding, unchanged.
    pub binding: IoBinding,
    /// `true` when any endpoint device id is absent from the live device lists.
    pub unresolved: bool,
}

/// Check a slice of bindings against the currently-live input and output
/// device lists (typically obtained right after a hot-swap refresh).
///
/// Returns one [`BindingStatus`] per binding, **in the same order**.
/// Bindings are never removed — callers must retain all entries and use
/// `BindingStatus::unresolved` to decide how to surface the problem.
pub(crate) fn check_bindings_after_refresh(
    bindings: &[IoBinding],
    live_inputs: &[AudioDeviceDescriptor],
    live_outputs: &[AudioDeviceDescriptor],
) -> Vec<BindingStatus> {
    bindings
        .iter()
        .map(|b| {
            let unresolved = b
                .inputs
                .iter()
                .any(|ep| !live_inputs.iter().any(|d| d.id == ep.device_id.0))
                || b.outputs
                    .iter()
                    .any(|ep| !live_outputs.iter().any(|d| d.id == ep.device_id.0));
            BindingStatus {
                binding: b.clone(),
                unresolved,
            }
        })
        .collect()
}
