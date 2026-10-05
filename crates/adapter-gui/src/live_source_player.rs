//! Responsibility: serves the backing-track player's live reading.

use std::cell::RefCell;
use std::rc::Rc;

use application::live_source::LiveSource;
use application::query_player::PlayerReading;
use infra_cpal::ProjectRuntimeController;

/// The player's live reading, on its own: like the click, the player depends
/// on no chain, so its window reads it through this seam without holding any
/// chain handle.
pub(crate) struct PlayerLiveSource {
    runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
}

impl LiveSource for PlayerLiveSource {
    fn player(&self) -> Option<PlayerReading> {
        player_reading(&self.runtime)
    }
}

pub(crate) fn player_live_source(
    runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
) -> Rc<dyn LiveSource> {
    Rc::new(PlayerLiveSource {
        runtime: Rc::clone(runtime),
    })
}

/// Where the track is, from the worker's own shared cell. `None` when no
/// runtime is hosted: the dispatcher's snapshot answers then.
pub(crate) fn player_reading(
    runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
) -> Option<PlayerReading> {
    let borrow = runtime.borrow();
    let controller = borrow.as_ref()?;
    controller.player_track()?;
    let shared = controller.player_shared();
    Some(PlayerReading {
        playing: shared.is_playing(),
        position_seconds: shared.position_seconds(),
        duration_seconds: shared.duration_seconds(),
        loading: shared.is_loading(),
        failed: shared.has_failed(),
    })
}
