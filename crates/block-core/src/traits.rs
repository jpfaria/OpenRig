//! Per-block processor traits + the dispatch enum used by the engine.
//!
//! Lifted out of `lib.rs` (Phase 6 of issue #194). One responsibility:
//! the contract every block processor implements.

pub trait MonoProcessor: Send + Sync + 'static {
    fn process_sample(&mut self, input: f32) -> f32;
    fn process_block(&mut self, buffer: &mut [f32]) {
        for sample in buffer {
            *sample = self.process_sample(*sample);
        }
    }
}

pub trait StereoProcessor: Send + Sync + 'static {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2];
    fn process_block(&mut self, buffer: &mut [[f32; 2]]) {
        for frame in buffer {
            *frame = self.process_frame(*frame);
        }
    }
}

pub enum BlockProcessor {
    Mono(Box<dyn MonoProcessor>),
    Stereo(Box<dyn StereoProcessor>),
}

pub trait NamedModel {
    fn model_key(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
}

/// Opaque handle to an open plugin editor window.
///
/// Dropping the handle closes the window and releases all resources.
/// The concrete type is an implementation detail of the plugin host crate.
pub trait PluginEditorHandle: Send {}
