//! Responsibility: declares how a command reaches the frontend's drum machine.
//!
//! The frontend that owns the audio host implements this; the dispatcher
//! reaches it through [`crate::runtime_control::RuntimeControl::drums`]. The
//! output travels as an opaque endpoint key, so this crate never learns which
//! device or channels the drums play on.
//!
//! The drums are an independent pipeline: their own output stream, summed
//! with everything else by the backend, never routed through a chain.
//! Every method runs on the control thread.

use std::path::Path;
use std::sync::Arc;

use anyhow::Result;

use feature_dsp::drums::{DrumSettings, Groove};

/// Everything needed to open the drums' output.
pub struct DrumsSetup<'a> {
    pub settings: DrumSettings,
    /// `None` means the project's first output endpoint.
    pub output_key: Option<&'a str>,
    /// The kit folder to load at the stream's sample rate.
    pub kit_dir: Option<&'a Path>,
    pub groove: Option<Arc<Groove>>,
}

pub trait DrumsRuntime {
    /// Open the drums' own output stream, stopped. The only door allowed to
    /// create an audio runtime, so the drums play with no chain enabled.
    fn start_drums(&self, setup: DrumsSetup<'_>) -> Result<()>;

    /// Close the output. Idempotent and never an error.
    fn stop_drums(&self);

    /// Hand new settings to the running drums; never restarts the stream.
    fn set_drums_settings(&self, settings: DrumSettings);

    fn set_drums_playing(&self, playing: bool);

    fn trigger_drum_fill(&self);

    /// Load the kit in `dir` off the control thread and swap it in.
    fn set_drum_kit(&self, dir: &Path);

    fn set_drum_groove(&self, groove: Arc<Groove>);

    /// Move an open output to the endpoint `output_key` names; never opens one.
    fn refresh_drums_output(&self, output_key: Option<&str>) -> Result<()>;
}
