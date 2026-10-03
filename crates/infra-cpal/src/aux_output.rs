//! Responsibility: routes the auxiliary output opener to the active audio backend.
//!
//! An auxiliary output is a stream that belongs to one independent pipeline
//! (the metronome, the backing-track player): it opens on its own, never joins
//! a chain's stream, and the backend sums it with whatever else the device
//! plays. cpal builds open a cpal output stream; JACK builds open a JACK client.

/// What the opened stream turned out to be, handed to the pipeline before its
/// first callback so it can size and route its render.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AuxOutputLayout {
    pub(crate) sample_rate: u32,
    /// Interleaved channels of every buffer the render gets.
    pub(crate) channels: usize,
    /// Channels of that buffer the pipeline should play into.
    pub(crate) targets: Vec<usize>,
    /// Largest buffer, in frames, the backend may hand the render.
    pub(crate) max_frames: usize,
}

/// The pipeline's callback body: fills one interleaved buffer.
pub(crate) type AuxRender = Box<dyn FnMut(&mut [f32]) + Send + 'static>;

#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) use crate::aux_output_cpal::{open_aux_output, AuxOutputHandle};
#[cfg(all(target_os = "linux", feature = "jack"))]
pub(crate) use crate::aux_output_jack::{open_aux_output, AuxOutputHandle};
