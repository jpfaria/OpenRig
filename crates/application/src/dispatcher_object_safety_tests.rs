use super::*;
use domain::ids::ChainId;

/// A dispatcher that is NOT `LocalDispatcher`. If the GUI-facing surface
/// lives on the trait, this compiles and answers through `dyn`.
struct FakeDispatcher {
    engine_sr: u32,
}

impl CommandDispatcher for FakeDispatcher {
    fn dispatch(&self, _cmd: Command) -> Result<Vec<Event>> {
        Ok(Vec::new())
    }
    fn engine_sr(&self) -> u32 {
        self.engine_sr
    }
    fn selection_state(
        &self,
    ) -> std::sync::Arc<std::sync::RwLock<crate::selection_state::SelectionState>> {
        std::sync::Arc::new(std::sync::RwLock::new(
            crate::selection_state::SelectionState::default(),
        ))
    }
}

#[test]
fn gui_surface_is_reachable_through_a_trait_object() {
    let dispatcher: std::rc::Rc<dyn CommandDispatcher> =
        std::rc::Rc::new(FakeDispatcher { engine_sr: 44_100 });

    assert_eq!(dispatcher.engine_sr(), 44_100);
    // Defaulted reads answer "nothing here" instead of forcing every
    // implementation to carry local-only state.
    assert!(dispatcher
        .chain_snapshot(&ChainId("missing".into()))
        .is_none());
    assert!(dispatcher
        .di_loop_for_chain(&ChainId("missing".into()))
        .is_none());
    assert_eq!(
        dispatcher.tone_report_json(&ChainId("missing".into())),
        "{}"
    );
    // Attach is local session setup: a no-op default, never a panic.
    dispatcher.attach_presets_path(std::path::PathBuf::from("/tmp/presets"));
    assert!(dispatcher.attach_engine_sr(48_000).is_empty());
}
