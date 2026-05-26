//! Offline render adapter for OpenRig.
//!
//! Drives the engine through a `.openrig` project headlessly, reading an input
//! WAV and writing an output WAV. No audio device, no GUI, no MIDI, no MCP.
//! Same `engine.process_block()` as live mode — deterministic.

pub mod cli;
pub mod wav;
