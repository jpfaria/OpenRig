//! #127 — the analyzers are powered by the DISPATCHER, from any transport.
//!
//! The bug: `SelectionCommand::SetTunerEnabled` / `SetSpectrumEnabled` only
//! flipped the mirror in `SelectionState` and reported their event. What made
//! the tuner actually read — building the session that subscribes to the taps
//! and runs YIN — happened afterwards, in the GUI's POWER callback, which had
//! exactly one caller. Meanwhile `adapter-midi`'s `slots.rs` maps the
//! `toggle_tuner` / `toggle_spectrum` footswitches to those very commands, so a
//! footswitch press (and an MCP call) was silently dead — and
//! `application::read` told clients to "dispatch `SetTunerEnabled` instead of
//! reading silence as no signal", advice that only ever worked in the GUI.
//!
//! These tests never touch Slint callbacks: they dispatch the command the way
//! MIDI and MCP do and assert that the analyzer subscribed and is producing
//! readings — the same `LiveSource::tuner` / `spectrum` that `openrig://tuner`
//! answers with.
//!
//! No audio device is opened: the taps come from a fake `AudioTaps` playing a
//! 440 Hz tone, which is exactly what the seam exists for.

use std::rc::Rc;
use std::sync::{Arc, Mutex};

use application::audio_taps::{AudioTap, AudioTaps, TapPoint};
use application::command::{Command, SelectionCommand};
use application::live_source::LiveSource;
use domain::ids::{ChainId, DeviceId};
use infra_filesystem::{ChannelMode, IoBinding, IoEndpoint};
use project::chain::Chain;
use project::project::Project;

use super::AnalyzerSessions;
use crate::state::ProjectSession;

const CHAIN: &str = "analyzer-127";
const SAMPLE_RATE: u32 = 48_000;
const TONE_HZ: f32 = 440.0;

/// A tap that always has signal: a 440 Hz sine, generated on demand, so a
/// detector that is really reading gets a confident answer and one that never
/// drains gets nothing.
struct ToneTap {
    channels: usize,
    phase: Mutex<f32>,
    drained: Arc<Mutex<usize>>,
}

impl AudioTap for ToneTap {
    fn channels(&self) -> usize {
        self.channels
    }

    fn poll_peak_dbfs(&self) -> f32 {
        -6.0
    }

    fn drain_channel(&self, _channel: usize, max: usize, out: &mut Vec<f32>) -> usize {
        // Four detection buffers per drain, so one tick is enough for YIN to
        // settle. `max` is `usize::MAX` for both analyzers.
        let count = feature_dsp::pitch_yin::BUFFER_SIZE * 4;
        let count = count.min(max);
        let mut phase = self.phase.lock().expect("tone phase");
        for _ in 0..count {
            out.push((*phase * std::f32::consts::TAU).sin() * 0.5);
            *phase = (*phase + TONE_HZ / SAMPLE_RATE as f32).fract();
        }
        *self.drained.lock().expect("drain count") += count;
        count
    }
}

/// The frontend's tap authority, with a runtime up and one stream per chain.
struct ToneTaps {
    drained: Arc<Mutex<usize>>,
}

impl AudioTaps for ToneTaps {
    fn is_hosted(&self) -> bool {
        true
    }

    fn live_sample_rate(&self) -> u32 {
        SAMPLE_RATE
    }

    fn stream_count(&self, _chain: &ChainId) -> usize {
        1
    }

    fn subscribe(&self, point: &TapPoint, _capacity: usize) -> Option<Arc<dyn AudioTap>> {
        let channels = match point {
            TapPoint::InputChannels { channels, .. } => channels.len(),
            _ => 2,
        };
        Some(Arc::new(ToneTap {
            channels,
            phase: Mutex::new(0.0),
            drained: Arc::clone(&self.drained),
        }))
    }
}

/// One enabled chain bound to one stereo input endpoint — enough for the
/// analyzers to resolve a stream to subscribe to.
fn session_with_one_bound_chain() -> ProjectSession {
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![Chain {
            id: ChainId(CHAIN.into()),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec!["io-1".into()],
            blocks: vec![],
            di_output: None,
            loopers: vec![],
        }],
        midi: None,
    };
    let session = ProjectSession::new(
        project,
        None,
        None,
        std::env::temp_dir().join("openrig-analyzers-127"),
    );
    *session.io_bindings.borrow_mut() = vec![IoBinding {
        id: "io-1".into(),
        name: "Test Interface".into(),
        inputs: vec![IoEndpoint {
            name: "In 1/2".into(),
            device_id: DeviceId("openrig-test-device".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
        outputs: vec![IoEndpoint {
            name: "Main Out".into(),
            device_id: DeviceId("openrig-test-device".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }];
    session
}

/// The analyzers, attached to the session's dispatcher exactly as the app
/// attaches them, over a stopped rig (no controller) and fake taps.
fn attached(session: &ProjectSession) -> (AnalyzerSessions, Arc<Mutex<usize>>) {
    let drained = Arc::new(Mutex::new(0usize));
    let taps: Rc<dyn AudioTaps> = Rc::new(ToneTaps {
        drained: Arc::clone(&drained),
    });
    let analyzers = AnalyzerSessions::new(&Rc::new(std::cell::RefCell::new(None)), &taps);
    crate::runtime_lifecycle::attach_runtime_control(
        &Rc::new(std::cell::RefCell::new(None)),
        &analyzers,
        session,
    );
    (analyzers, drained)
}

/// The reading every transport gets: `GuiLiveSource` over the same cells.
fn tuner_readings(
    session: &ProjectSession,
    analyzers: &AnalyzerSessions,
) -> Option<Vec<application::query_analyzers::TunerReading>> {
    let project = session.project.borrow();
    let bindings = session.io_bindings.borrow();
    let chain_rows = Rc::new(slint::VecModel::from(Vec::<crate::ProjectChainItem>::new()));
    let runtime = Rc::new(std::cell::RefCell::new(None));
    crate::gui_live_source::GuiLiveSource {
        project: &project,
        chain_rows: &chain_rows,
        io_bindings: &bindings,
        tuner: analyzers.tuner_cell(),
        spectrum: analyzers.spectrum_cell(),
        runtime: &runtime,
    }
    .tuner()
}

fn spectrum_readings(
    session: &ProjectSession,
    analyzers: &AnalyzerSessions,
) -> Option<Vec<application::query_analyzers::SpectrumReading>> {
    let project = session.project.borrow();
    let bindings = session.io_bindings.borrow();
    let chain_rows = Rc::new(slint::VecModel::from(Vec::<crate::ProjectChainItem>::new()));
    let runtime = Rc::new(std::cell::RefCell::new(None));
    crate::gui_live_source::GuiLiveSource {
        project: &project,
        chain_rows: &chain_rows,
        io_bindings: &bindings,
        tuner: analyzers.tuner_cell(),
        spectrum: analyzers.spectrum_cell(),
        runtime: &runtime,
    }
    .spectrum()
}

/// The bug, at the transport level: exactly what the MIDI `toggle_tuner` slot
/// and the MCP tool dispatch, asserted on what the analyzer produces.
#[test]
fn enabling_the_tuner_off_the_gui_starts_the_pitch_detection() {
    let session = session_with_one_bound_chain();
    let (analyzers, drained) = attached(&session);
    assert!(
        tuner_readings(&session, &analyzers).is_none(),
        "precondition: nothing hosts a tuner session before POWER"
    );

    session
        .dispatcher
        .dispatch(Command::Selection(SelectionCommand::SetTunerEnabled {
            enabled: true,
        }))
        .expect("powering the tuner on is never an error");

    let rows = tuner_readings(&session, &analyzers).expect(
        "#127: dispatching SetTunerEnabled must BUILD the analyzer — a footswitch or an MCP \
         client that flips the mirror and subscribes to nothing reads silence forever",
    );
    assert_eq!(
        rows.len(),
        2,
        "one row per device channel the chain's input is wired to: {rows:?}"
    );

    // The tick the analyzer's own timer runs, driven here because a unit test
    // has no Slint event loop.
    analyzers.tick_tuner();
    assert!(
        *drained.lock().expect("drain count") > 0,
        "#127: the analyzer must actually READ its taps"
    );
    let rows = tuner_readings(&session, &analyzers).expect("still hosted");
    assert!(
        rows.iter().all(|r| r.active && r.note == "A"),
        "#127: a 440 Hz tone on every tapped channel must be detected as A: {rows:?}"
    );
}

#[test]
fn disabling_the_tuner_off_the_gui_stops_it() {
    let session = session_with_one_bound_chain();
    let (analyzers, _) = attached(&session);
    session
        .dispatcher
        .dispatch(Command::Selection(SelectionCommand::SetTunerEnabled {
            enabled: true,
        }))
        .expect("powering on");
    assert!(
        tuner_readings(&session, &analyzers).is_some(),
        "precondition"
    );

    session
        .dispatcher
        .dispatch(Command::Selection(SelectionCommand::SetTunerEnabled {
            enabled: false,
        }))
        .expect("powering the tuner off is never an error");

    assert!(
        tuner_readings(&session, &analyzers).is_none(),
        "#127: powering off must release the taps and stop reporting — `running: false` with \
         no rows is what `openrig://tuner` promises"
    );
}

#[test]
fn enabling_the_spectrum_off_the_gui_starts_the_analyzer() {
    let session = session_with_one_bound_chain();
    let (analyzers, drained) = attached(&session);
    assert!(
        spectrum_readings(&session, &analyzers).is_none(),
        "precondition: nothing hosts a spectrum session before POWER"
    );

    session
        .dispatcher
        .dispatch(Command::Selection(SelectionCommand::SetSpectrumEnabled {
            enabled: true,
        }))
        .expect("powering the spectrum on is never an error");

    let rows = spectrum_readings(&session, &analyzers).expect(
        "#127: dispatching SetSpectrumEnabled must BUILD the analyzer — the `toggle_spectrum` \
         footswitch subscribed to nothing",
    );
    assert_eq!(rows.len(), 2, "L and R of the chain's one stream: {rows:?}");

    analyzers.tick_spectrum();
    assert!(
        *drained.lock().expect("drain count") > 0,
        "#127: the analyzer must actually READ its taps"
    );
    let rows = spectrum_readings(&session, &analyzers).expect("still hosted");
    assert!(
        rows.iter().all(|r| r.levels.iter().any(|l| *l > 0.0)),
        "#127: a 440 Hz tone must move at least one band: {rows:?}"
    );
}

#[test]
fn disabling_the_spectrum_off_the_gui_stops_it() {
    let session = session_with_one_bound_chain();
    let (analyzers, _) = attached(&session);
    session
        .dispatcher
        .dispatch(Command::Selection(SelectionCommand::SetSpectrumEnabled {
            enabled: true,
        }))
        .expect("powering on");
    assert!(
        spectrum_readings(&session, &analyzers).is_some(),
        "precondition"
    );

    session
        .dispatcher
        .dispatch(Command::Selection(SelectionCommand::SetSpectrumEnabled {
            enabled: false,
        }))
        .expect("powering the spectrum off is never an error");

    assert!(
        spectrum_readings(&session, &analyzers).is_none(),
        "#127: powering off must release the taps and stop reporting"
    );
}
