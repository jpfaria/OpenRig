//! #127 (Task 14), red-first: the three runtime doors a frontend used to keep
//! to itself — stopping the rig, rebuilding the whole graph after a
//! device-settings save, and dropping a deleted chain's stream.
//!
//! Each of them was a GUI function call made right after a `Command` was
//! dispatched, which is the split #127 exists to close:
//!
//! * a remote client could START the rig (enable a chain, play a DI) and had
//!   no way to STOP it again — `stop_project_runtime` was reachable only from
//!   the "back to launcher" button;
//! * `SettingsCommand::SaveAudioSettings` over MCP/gRPC persisted the new
//!   sample rate and buffer size and left the audio running on the old ones,
//!   because the rebuild lived in the settings screen's callback;
//! * `ChainCommand::RemoveChain` over MCP/gRPC removed the chain from the
//!   project and left its stream sounding, because the teardown lived in the
//!   delete overlay's callback.
//!
//! Every assertion below is on what the RUNTIME was asked to do. The events
//! were never the broken half.

use std::cell::RefCell;
use std::rc::Rc;

use domain::ids::ChainId;
use project::chain::Chain;
use project::project::Project;

use project::device::DeviceSettings;

use crate::command::{ChainCommand, Command, ProjectCommand, SettingsCommand};
use crate::dispatcher::CommandDispatcher;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;
use crate::runtime_control::RuntimeControl;

/// What the runtime was asked to do, in order. Chain-scoped calls record the
/// chain they addressed: isolation is a `CLAUDE.md` LAW, and a door that
/// cannot say WHICH stream it touched cannot be proven to have left the
/// others alone.
#[derive(Default)]
struct SpyRuntimeControl {
    calls: Rc<RefCell<Vec<String>>>,
}

impl RuntimeControl for SpyRuntimeControl {
    fn stop_project_runtime(&self) {
        self.calls.borrow_mut().push("stop rig".to_string());
    }

    fn sync_project(&self) -> anyhow::Result<()> {
        self.calls.borrow_mut().push("sync project".to_string());
        Ok(())
    }

    fn apply_device_settings(&self, settings: &[DeviceSettings]) {
        for device in settings {
            self.calls.borrow_mut().push(format!(
                "apply {} at {}",
                device.device_id.0, device.sample_rate
            ));
        }
    }

    fn remove_chain(&self, chain: &ChainId) {
        self.calls
            .borrow_mut()
            .push(format!("remove chain {}", chain.0));
    }

    fn sync_chain(&self, chain: &ChainId) -> anyhow::Result<()> {
        // Never expected on any road below — recorded so the lookalike shows
        // up in the assertion instead of passing unseen.
        self.calls
            .borrow_mut()
            .push(format!("sync chain {}", chain.0));
        Ok(())
    }
}

struct FailingRuntimeControl;

impl RuntimeControl for FailingRuntimeControl {
    fn sync_project(&self) -> anyhow::Result<()> {
        Err(anyhow::anyhow!(
            "the audio host refused the new buffer size"
        ))
    }
}

fn device(id: &str, sample_rate: u32) -> DeviceSettings {
    DeviceSettings {
        device_id: domain::ids::DeviceId(id.to_string()),
        sample_rate,
        buffer_size_frames: 256,
        bit_depth: 32,
        #[cfg(target_os = "linux")]
        realtime: true,
        #[cfg(target_os = "linux")]
        rt_priority: 70,
        #[cfg(target_os = "linux")]
        nperiods: 3,
    }
}

fn chain(id: &str) -> Chain {
    Chain {
        id: ChainId(id.to_string()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
    }
}

fn dispatcher_with_spy(chains: Vec<Chain>) -> (LocalDispatcher, Rc<RefCell<Vec<String>>>) {
    let dispatcher = LocalDispatcher::new(Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains,
        midi: None,
    })));
    let calls = Rc::new(RefCell::new(Vec::new()));
    dispatcher.attach_runtime_control(Rc::new(SpyRuntimeControl {
        calls: Rc::clone(&calls),
    }));
    (dispatcher, calls)
}

// ── stopping the rig ────────────────────────────────────────────────────────

#[test]
fn stopping_the_rig_from_the_bus_reaches_the_runtime() {
    let (dispatcher, calls) = dispatcher_with_spy(vec![chain("rig:input-1")]);

    dispatcher
        .dispatch(Command::Project(ProjectCommand::StopProjectRuntime))
        .expect("stopping the rig is never an error");

    assert_eq!(
        calls.borrow().as_slice(),
        ["stop rig"],
        "an MCP/gRPC client must be able to stop the rig — the teardown was \
         reachable only from the GUI's back-to-launcher callback, so a client \
         that started the audio could not silence it again"
    );
}

#[test]
fn closing_the_project_stops_the_rig() {
    let (dispatcher, calls) = dispatcher_with_spy(vec![chain("rig:input-1")]);

    let events = dispatcher
        .dispatch(Command::Project(ProjectCommand::CloseProject))
        .expect("closing the project is never an error");

    assert_eq!(
        calls.borrow().as_slice(),
        ["stop rig"],
        "closing the project must stop its audio — a CloseProject issued \
         anywhere but the GUI left every stream open and sounding"
    );
    assert!(
        events.iter().any(|e| matches!(e, Event::ProjectClosed)),
        "the close must still report itself: {events:?}"
    );
}

// ── the whole-graph rebuild behind a device-settings save ───────────────────

#[test]
fn saving_the_device_settings_rebuilds_the_whole_graph() {
    crate::local_dispatcher_paths_tests::with_tmp_home("127-audio-settings-sync", || {
        let (dispatcher, calls) =
            dispatcher_with_spy(vec![chain("rig:input-1"), chain("rig:input-2")]);

        dispatcher
            .dispatch(Command::Settings(SettingsCommand::SaveAudioSettings {
                input_devices: vec![],
                output_devices: vec![],
            }))
            .expect("saving device settings must succeed");

        assert_eq!(
            calls.borrow().as_slice(),
            ["sync project"],
            "a device-settings save must re-open the running graph against the \
             new rate/buffer — over MCP it persisted the numbers and left the \
             audio on the old ones. And it is ONE whole-project rebuild, not a \
             per-chain fan-out: the door carries the project's identity"
        );
    });
}

#[test]
fn a_rebuild_the_runtime_refused_surfaces_as_a_dispatch_error() {
    crate::local_dispatcher_paths_tests::with_tmp_home("127-audio-settings-err", || {
        let dispatcher = LocalDispatcher::new(Rc::new(RefCell::new(Project {
            name: None,
            device_settings: vec![],
            chains: vec![chain("rig:input-1")],
            midi: None,
        })));
        dispatcher.attach_runtime_control(Rc::new(FailingRuntimeControl));

        let result = dispatcher.dispatch(Command::Settings(SettingsCommand::SaveAudioSettings {
            input_devices: vec![],
            output_devices: vec![],
        }));

        assert!(
            result.is_err(),
            "a rebuild the audio host refused must not be swallowed — the user \
             picked a buffer size the device cannot do and has to be told: \
             {result:?}"
        );
    });
}

/// The other half of the same save, and the one the FINAL review found still
/// GUI-only: the driver has to be told the new rate BEFORE the graph is
/// re-opened against it. On macOS/Windows that is a throwaway stream built at
/// the requested rate — the only thing that makes CoreAudio/WASAPI reconfigure
/// the device — and it lived in `settings/audio.rs`, three call sites, right
/// before this dispatch. So an MCP client saving 44.1 kHz persisted the pick
/// and re-opened the graph with the device still at 48 kHz.
#[test]
fn saving_the_device_settings_makes_the_driver_adopt_them_first() {
    crate::local_dispatcher_paths_tests::with_tmp_home("127-audio-settings-apply", || {
        let (dispatcher, calls) = dispatcher_with_spy(vec![chain("rig:input-1")]);

        dispatcher
            .dispatch(Command::Settings(SettingsCommand::SaveAudioSettings {
                input_devices: vec![device("scarlett-in", 44_100)],
                output_devices: vec![device("scarlett-out", 44_100)],
            }))
            .expect("saving device settings must succeed");

        assert_eq!(
            calls.borrow().as_slice(),
            [
                "apply scarlett-in at 44100",
                "apply scarlett-out at 44100",
                "sync project"
            ],
            "every device the save names must be configured, and BEFORE the \
             rebuild — a graph re-opened while the driver is still at the old \
             rate is the bug this closes"
        );
    });
}

// ── dropping a deleted chain's stream ───────────────────────────────────────

#[test]
fn removing_a_chain_drops_that_chains_runtime() {
    let (dispatcher, calls) = dispatcher_with_spy(vec![chain("rig:input-1")]);

    dispatcher
        .dispatch(Command::Chain(ChainCommand::RemoveChain {
            chain: ChainId("rig:input-1".into()),
        }))
        .expect("remove");

    assert_eq!(
        calls.borrow().as_slice(),
        ["remove chain rig:input-1"],
        "deleting a chain must stop its sound from the bus — over MCP the \
         chain left the project and its stream kept playing"
    );
}

/// The isolation pin (`CLAUDE.md` LAW, invariant #4). Deleting one chain is
/// the operation where "it also tore down the neighbours" is most tempting:
/// routing the teardown through `SyncChainRuntime` for the deleted chain looks
/// equivalent and is not — that road validates the WHOLE project (an unrelated
/// invalid chain aborts the teardown) and tears the runtime down for everyone
/// when the last chain goes.
#[test]
fn removing_a_chain_never_touches_another_chains_runtime() {
    let (dispatcher, calls) = dispatcher_with_spy(vec![chain("rig:input-1"), chain("rig:input-2")]);

    dispatcher
        .dispatch(Command::Chain(ChainCommand::RemoveChain {
            chain: ChainId("rig:input-1".into()),
        }))
        .expect("remove");

    assert_eq!(
        calls.borrow().as_slice(),
        ["remove chain rig:input-1"],
        "only the deleted chain's stream may be touched: the survivor must be \
         neither re-synced nor named, and the rig-wide stop must not fire"
    );
    assert!(
        !calls.borrow().iter().any(|c| c.contains("rig:input-2")),
        "the surviving chain's runtime was addressed by a delete of another \
         chain: {:?}",
        calls.borrow()
    );
}

/// The guard against the fix over-reaching: a delete that changed nothing must
/// not silence anything. `RemoveChain` fails when the chain is not there, and
/// a failed command has no runtime half.
#[test]
fn a_delete_that_finds_no_chain_never_touches_the_runtime() {
    let (dispatcher, calls) = dispatcher_with_spy(vec![chain("rig:input-1")]);

    let result = dispatcher.dispatch(Command::Chain(ChainCommand::RemoveChain {
        chain: ChainId("rig:input-9".into()),
    }));

    assert!(
        result.is_err(),
        "removing a chain that is not there must fail"
    );
    assert!(
        calls.borrow().is_empty(),
        "a delete that removed nothing must not stop any audio: {:?}",
        calls.borrow()
    );
}
