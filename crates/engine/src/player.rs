//! Responsibility: routes the backing-track player's engine modules.
//!
//! The player is its own stream: a decoded track rendered off the audio thread
//! into a ring that one output callback drains. Nothing here touches a chain,
//! a DI loop or any other stream's runtime.

pub mod output;
pub mod pcm;
pub mod position;
pub mod render;
pub mod resample;
pub mod settings;
pub mod shared;
