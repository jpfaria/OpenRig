pub mod block;
pub mod catalog;
pub mod chain;
pub mod device;
pub mod midi;
pub mod migrate;
pub mod param;
pub mod project;
pub mod rig;
pub mod rig_command;
pub mod rig_methods;
pub mod rig_sync;
pub mod vst3_editor;

#[cfg(test)]
#[path = "midi_tests.rs"]
mod midi_tests;
