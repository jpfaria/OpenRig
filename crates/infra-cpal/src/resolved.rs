//! Responsibility: describes the audio configuration one chain resolved to.
//!
//! The devices live in [`crate::resolved_device`] and the signatures in
//! [`crate::stream_signature_types`] (#873); this file keeps the whole-chain
//! config that carries both, and the path importers already use.

pub(crate) use crate::resolved_device::{ResolvedInputDevice, ResolvedOutputDevice};
#[cfg(all(target_os = "linux", feature = "jack"))]
pub(crate) use crate::stream_signature_types::{
    stream_signatures_require_client_rebuild, MAX_JACK_FRAMES,
};
pub(crate) use crate::stream_signature_types::{
    ChainStreamSignature, InputStreamSignature, OutputStreamSignature,
};

#[allow(dead_code)]
pub(crate) struct ResolvedChainAudioConfig {
    pub(crate) inputs: Vec<ResolvedInputDevice>,
    pub(crate) outputs: Vec<ResolvedOutputDevice>,
    pub(crate) sample_rate: f32,
    /// Per-input-device resolved rate (#736). One isolated runtime per input
    /// device is clocked at its own rate from this map; the scalar
    /// `sample_rate` above is only the representative (first binding) rate for
    /// legacy single-rate consumers.
    pub(crate) by_device: std::collections::HashMap<domain::ids::DeviceId, f32>,
    /// Per input cpal index (= Nth distinct input device, first-seen over the
    /// resolved input order), the output device id(s) of its OWN binding. LAW:
    /// streams are fully isolated — an output device's stream mixes ONLY the
    /// runtimes that feed THAT device, never "all runtimes at the same rate".
    /// Mixing a runtime that does not write this device pops its empty elastic
    /// buffer every callback = the underrun flood ("N streams at 44 kHz are N
    /// separate pipelines, not one"). Empty on the JACK path.
    pub(crate) output_devices_by_input_cpal: Vec<Vec<String>>,
    pub(crate) stream_signature: ChainStreamSignature,
}
