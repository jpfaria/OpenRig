//! #829: re-enumerating audio devices used to be reachable only by
//! clicking the GUI; it is a command now, so MCP / gRPC reach it too.

use crate::command::{Command, SettingsCommand};
use crate::dispatcher::CommandDispatcher;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;
use crate::local_dispatcher_tests::empty_project_rc;

#[test]
fn refresh_audio_devices_emits_the_refresh_event() {
    let dispatcher = LocalDispatcher::new(empty_project_rc());

    let events = dispatcher
        .dispatch(Command::Settings(SettingsCommand::RefreshAudioDevices))
        .expect("refresh should dispatch");

    assert!(
        matches!(events.as_slice(), [Event::AudioDevicesRefreshed]),
        "expected one AudioDevicesRefreshed, got {events:?}"
    );
}
