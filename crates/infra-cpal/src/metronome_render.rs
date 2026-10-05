//! Responsibility: builds the metronome's output callback for the stream it opens on.

use engine::metronome_state::{MetronomeCell, MetronomeGenerator};

use crate::aux_output::{AuxOutputLayout, AuxRender};
use crate::metronome_stream::fill_metronome_buffer;

/// The click's callback, sized to the stream the backend actually opened: its
/// rate seeds the generator and its buffer pre-allocates the scratch, so the
/// callback never allocates (invariant #8).
pub(crate) fn metronome_render(
    shared: MetronomeCell,
) -> impl FnOnce(&AuxOutputLayout) -> AuxRender {
    move |layout| {
        let mut generator = MetronomeGenerator::new(layout.sample_rate as f32, shared.settings());
        let mut scratch: Vec<f32> = vec![0.0; layout.max_frames];
        let mut last_generation = shared.generation();
        let channels = layout.channels;
        let targets = layout.targets.clone();
        Box::new(move |out: &mut [f32]| {
            fill_metronome_buffer(
                &mut generator,
                &shared,
                &mut scratch,
                out,
                channels,
                &targets,
                &mut last_generation,
            );
        })
    }
}

#[cfg(test)]
#[path = "metronome_render_tests.rs"]
mod tests;
