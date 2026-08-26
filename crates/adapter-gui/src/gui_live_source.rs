//! Responsibility: reads the GUI's live rig state.
//! #831: the GUI's live readings, as [`LiveSource`].
//!
//! Everything here is state that only exists inside the running window —
//! the meter rows, the tuner/spectrum sessions, the audio runtime. The
//! resolver in `application::read` owns the wire shape for all of it; this
//! module only hands over the DATA the GUI already has, never JSON.
//!
//! A `None` means "the GUI is not hosting this right now" (the tuner window
//! is closed, the project is not started) — never "the reading failed" and
//! never a fabricated row. The resolver answers those with the documented
//! empty shape, so a client reads the same fields whichever frontend and
//! whichever transport served it.

pub(crate) use crate::live_source_block_stream::block_stream_live_source;
pub(crate) use crate::live_source_chain_rate::chain_rate_live_source;
pub(crate) use crate::live_source_chain_row::chain_row_live_source;
pub(crate) use crate::live_source_gui::GuiLiveSource;
pub(crate) use crate::live_source_health::health_live_source;
pub(crate) use crate::live_source_looper::looper_live_source;
pub(crate) use crate::live_source_metronome::metronome_live_source;

#[cfg(test)]
#[path = "gui_live_source_tests.rs"]
mod tests;
