//! Responsibility: builds the backing-track player's output callback for the stream it opens on.

use std::sync::Arc;

use engine::player::output::{fill_player_buffer, PlayerOutputState};
use engine::player::shared::PlayerCell;
use engine::spsc::SpscRing;

use crate::aux_output::{AuxOutputLayout, AuxRender};

/// The track's callback for the stream the backend actually opened: it drains
/// `ring` (filled by the player's worker) onto the layout's target channels.
pub(crate) fn player_render(
    shared: PlayerCell,
    ring: Arc<SpscRing<f32>>,
) -> impl FnOnce(&AuxOutputLayout) -> AuxRender {
    move |layout| {
        let mut state = PlayerOutputState::default();
        let channels = layout.channels;
        let targets = layout.targets.clone();
        Box::new(move |out: &mut [f32]| {
            fill_player_buffer(&mut state, &shared, &ring, out, channels, &targets);
        })
    }
}

#[cfg(test)]
#[path = "player_render_tests.rs"]
mod tests;
