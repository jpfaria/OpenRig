//! #808 — playing the DI must NOT require a chain to be enabled.
//!
//! The owner: "I open the project, don't touch the chain, hit play on the DI —
//! nothing. I toggle the chain on/off and then play works." Root cause: the
//! runtime controller is created ONLY when a chain is enabled
//! (`sync_live_chain_runtime`), so with nothing enabled the play was a silent
//! no-op — there was no controller to arm the DI on. The DI is an independent
//! pipeline (invariant #4); `ensure_runtime` creates the controller regardless
//! of any chain being enabled.

use std::cell::RefCell;
use std::rc::Rc;

use domain::ids::ChainId;
use infra_cpal::ProjectRuntimeController;
use project::chain::Chain;
use project::project::Project;

use super::ensure_runtime;
use crate::state::ProjectSession;

fn session_with_disabled_chain() -> ProjectSession {
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![Chain {
            id: ChainId("di808-play".into()),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: false, // opened the project, never enabled the chain
            volume: 100.0,
            io_binding_ids: vec![],
            blocks: vec![],
            di_output: None,
        }],
        midi: None,
    };
    ProjectSession::new(
        project,
        None,
        None,
        std::env::temp_dir().join("openrig-808-play-tests"),
    )
}

#[test]
fn playing_the_di_gets_a_runtime_without_enabling_a_chain() {
    let session = session_with_disabled_chain();
    let project_runtime: Rc<RefCell<Option<ProjectRuntimeController>>> =
        Rc::new(RefCell::new(None));

    assert!(
        project_runtime.borrow().is_none(),
        "precondition: no chain enabled, so no controller exists yet"
    );

    ensure_runtime(&project_runtime, &session).expect("ensure_runtime must succeed");

    assert!(
        project_runtime.borrow().is_some(),
        "#808: hitting play on the DI must create the runtime controller even \
         though no chain is enabled — otherwise arming the DI is a silent no-op \
         until a chain toggle happens to create one."
    );
}
