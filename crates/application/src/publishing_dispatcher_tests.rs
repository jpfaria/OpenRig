use super::*;
use crate::bridge::event_sink;
use crate::command::{Command, ProjectCommand};
use crate::local_dispatcher::LocalDispatcher;
use project::project::Project;
use std::cell::RefCell;
use std::rc::Rc;

#[test]
fn publishes_every_dispatch_to_sink() {
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![],
        midi: None,
    }));
    let inner = LocalDispatcher::new(project);
    let (sink, mut rx) = event_sink();
    let pd = PublishingDispatcher::new(inner, sink);

    let events = pd
        .dispatch(Command::Project(ProjectCommand::SaveProject))
        .unwrap();
    assert!(!events.is_empty());

    let pushed = rx.try_recv().expect("event batch fanned out");
    assert_eq!(pushed.len(), events.len());
}

/// #1007: the GUI and MCP read the mixer through the publishing wrapper; it
/// must hand the restored state to the local dispatcher and read its strips
/// back, not answer with the trait's empty defaults.
#[test]
fn the_mixer_state_and_strips_reach_the_wrapped_dispatcher() {
    use domain::ids::DeviceId;
    use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
    use infra_filesystem::MixerStripConfig;

    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![],
        midi: None,
    }));
    let (sink, _rx) = event_sink();
    let pd = PublishingDispatcher::new(LocalDispatcher::new(project), sink);
    pd.attach_io_bindings(Rc::new(RefCell::new(vec![IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![],
        outputs: vec![IoEndpoint {
            name: "Main".into(),
            device_id: DeviceId("pd-mixer".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }])));
    pd.attach_mixer_state(Rc::new(RefCell::new(
        crate::mixer_state::MixerControlState::restored(
            &[MixerStripConfig {
                id: "out:0,1@pd-mixer".into(),
                gain_db: -4.0,
                muted: true,
                soloed: false,
            }],
            None,
        ),
    )));

    let strips: Vec<(String, f32, bool)> = pd
        .mixer_strips()
        .into_iter()
        .map(|s| (s.id, s.gain_db, s.muted))
        .collect();
    assert_eq!(strips, vec![("out:0,1@pd-mixer".to_string(), -4.0, true)]);
}
