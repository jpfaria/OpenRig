//! #1070: a runtime teardown publishes the session's audio setup to crash
//! reporting — headless, no device is opened.

use std::cell::RefCell;
use std::rc::Rc;

use domain::ids::ChainId;
use infra_cpal::ProjectRuntimeController;
use project::chain::Chain;
use project::project::Project;

use crate::crash_context::last_published_on_this_thread;
use crate::runtime_teardown::stop_project_runtime;
use crate::state::ProjectSession;

#[test]
fn stopping_the_runtime_publishes_the_session_audio_context() {
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![Chain {
            id: ChainId("chain-1070".into()),
            description: Some("hook".into()),
            instrument: "electric_guitar".into(),
            enabled: false,
            volume: 100.0,
            io_binding_ids: vec![],
            blocks: vec![],
            di_output: None,
            loopers: vec![],
            disabled_endpoints: Default::default(),
            mix: Default::default(),
        }],
        midi: None,
    };
    let session = ProjectSession::new(
        project,
        None,
        None,
        std::env::temp_dir().join("openrig-1070-publish-tests"),
    );
    let runtime: Rc<RefCell<Option<ProjectRuntimeController>>> = Rc::new(RefCell::new(None));

    stop_project_runtime(&runtime, Some(&session));

    let published = last_published_on_this_thread().expect("nothing published");
    assert_eq!(published["chains"][0]["id"], "chain-1070");
    assert!(published["live_sample_rate"].is_null());
}
