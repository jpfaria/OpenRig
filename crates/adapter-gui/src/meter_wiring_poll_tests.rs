//! #913 — one chain's meter row, refreshed from the engine readings.
//!
//! This is the timer's per-tick work for a single chain. What must hold:
//! the OUTPUT reading is compensated for the chain volume (the tap reads
//! BEFORE the callback applies the slider, so without this the knob never
//! moved the meter — #496); a chain that is not in the project is skipped
//! rather than indexed; and the overload badge lights on NEW xruns only, so a
//! chain that xruned once does not stay lit forever (#670).

use super::refresh_chain_meter_row;
use crate::state::ProjectSession;
use crate::{ProjectChainItem, StreamMeter};
use application::live_source::{ChainRuntimeReading, LiveSource};
use application::runtime_control::RuntimeControl;
use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::chain::Chain;
use project::project::Project;
use slint::{Model, VecModel};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// A frontend that hosts no audio: every reading is absent unless a test
/// overrides it. Mirrors what the seam answers before a rig is running.
#[derive(Default)]
struct FakeReads {
    runtime: Option<(bool, u64, u64)>,
}
impl FakeReads {
    fn with_xruns(xruns: u64) -> Self {
        Self {
            runtime: Some((true, xruns, 0)),
        }
    }
    fn with_underruns(underruns: u64) -> Self {
        Self {
            runtime: Some((true, 0, underruns)),
        }
    }
}
impl LiveSource for FakeReads {
    fn chain_runtime(&self, _chain: &ChainId) -> Option<ChainRuntimeReading> {
        self.runtime
            .map(|(live, xruns, underruns)| ChainRuntimeReading {
                live,
                xruns,
                underruns,
            })
    }
}

struct NoWrites;
impl RuntimeControl for NoWrites {}

fn binding() -> IoBinding {
    IoBinding {
        id: "io-main".into(),
        name: "io-main".into(),
        inputs: vec![IoEndpoint {
            name: "In 1".into(),
            device_id: DeviceId("dev-in".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "Out 1".into(),
            device_id: DeviceId("dev-out".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }
}

fn chain(id: &str, volume: f32) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume,
        io_binding_ids: vec!["io-main".into()],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
    }
}

fn session(chains: Vec<Chain>) -> ProjectSession {
    let session = ProjectSession::new(
        Project {
            name: None,
            device_settings: vec![],
            chains,
            midi: None,
        },
        None,
        None,
        std::env::temp_dir().join("openrig-913-meter-tests"),
    );
    *session.io_bindings.borrow_mut() = vec![binding()];
    session
}

fn rows(n: usize) -> Rc<VecModel<ProjectChainItem>> {
    Rc::new(VecModel::from(
        (0..n)
            .map(|_| ProjectChainItem::default())
            .collect::<Vec<_>>(),
    ))
}

struct Counters {
    xruns: RefCell<HashMap<ChainId, u64>>,
    underruns: RefCell<HashMap<ChainId, u64>>,
}
impl Counters {
    fn new() -> Self {
        Self {
            xruns: RefCell::new(HashMap::new()),
            underruns: RefCell::new(HashMap::new()),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn refresh(
    session: &ProjectSession,
    model: &Rc<VecModel<ProjectChainItem>>,
    cid: &ChainId,
    in_db: f32,
    out_db_raw: f32,
    reads: &dyn LiveSource,
    counters: &Counters,
) {
    let project = session.project.borrow().clone();
    refresh_chain_meter_row(
        cid,
        in_db,
        out_db_raw,
        &project,
        session,
        reads,
        &NoWrites,
        model,
        &[],
        &[],
        &counters.xruns,
        &counters.underruns,
    );
}

#[test]
fn the_output_reading_is_compensated_for_the_chain_volume() {
    let cid = ChainId("chain:0".into());
    // Half volume: the tap reads before the slider, so the row must show the
    // reading 6 dB lower than the raw tap.
    let session = session(vec![chain("chain:0", 50.0)]);
    let model = rows(1);
    refresh(
        &session,
        &model,
        &cid,
        -20.0,
        -12.0,
        &FakeReads::default(),
        &Counters::new(),
    );

    let row = model.row_data(0).expect("row");
    assert_eq!(row.meter_in_dbfs, -20.0, "the input reading is untouched");
    assert!(
        row.meter_out_dbfs < -12.0,
        "#496: at 50% the output meter must read BELOW the raw tap, got {}",
        row.meter_out_dbfs
    );
}

#[test]
fn at_unity_the_output_reading_is_the_raw_tap() {
    let cid = ChainId("chain:0".into());
    let session = session(vec![chain("chain:0", 100.0)]);
    let model = rows(1);
    refresh(
        &session,
        &model,
        &cid,
        -20.0,
        -12.0,
        &FakeReads::default(),
        &Counters::new(),
    );
    let row = model.row_data(0).expect("row");
    assert!((row.meter_out_dbfs - (-12.0)).abs() < 0.001);
}

#[test]
fn a_chain_that_is_not_in_the_project_is_skipped() {
    let session = session(vec![chain("chain:0", 100.0)]);
    let model = rows(1);
    refresh(
        &session,
        &model,
        &ChainId("chain:gone".into()),
        -3.0,
        -3.0,
        &FakeReads::default(),
        &Counters::new(),
    );
    let row = model.row_data(0).expect("row");
    assert_eq!(
        row.meter_in_dbfs, 0.0,
        "an unknown chain must not write over row 0"
    );
}

#[test]
fn the_second_chains_reading_lands_on_the_second_row() {
    let session = session(vec![chain("chain:0", 100.0), chain("chain:1", 100.0)]);
    let model = rows(2);
    refresh(
        &session,
        &model,
        &ChainId("chain:1".into()),
        -7.0,
        -7.0,
        &FakeReads::default(),
        &Counters::new(),
    );
    assert_eq!(model.row_data(0).expect("row 0").meter_in_dbfs, 0.0);
    assert_eq!(model.row_data(1).expect("row 1").meter_in_dbfs, -7.0);
}

#[test]
fn the_overload_badge_lights_on_a_new_xrun_and_clears_when_they_stop() {
    let cid = ChainId("chain:0".into());
    let session = session(vec![chain("chain:0", 100.0)]);
    let model = rows(1);
    let counters = Counters::new();

    let quiet = FakeReads::with_xruns(0);
    refresh(&session, &model, &cid, -20.0, -20.0, &quiet, &counters);
    assert!(!model.row_data(0).expect("row").audio_overload);

    let xruning = FakeReads::with_xruns(3);
    refresh(&session, &model, &cid, -20.0, -20.0, &xruning, &counters);
    assert!(
        model.row_data(0).expect("row").audio_overload,
        "3 new xruns since the last tick must light the badge"
    );

    // Same total on the next tick = no NEW xrun; the badge clears.
    refresh(&session, &model, &cid, -20.0, -20.0, &xruning, &counters);
    assert!(
        !model.row_data(0).expect("row").audio_overload,
        "#670: the badge tracks NEW failures, it does not latch"
    );
}

#[test]
fn an_underrun_lights_the_same_badge_as_an_xrun() {
    let cid = ChainId("chain:0".into());
    let session = session(vec![chain("chain:0", 100.0)]);
    let model = rows(1);
    let counters = Counters::new();
    let underruning = FakeReads::with_underruns(5);
    refresh(
        &session,
        &model,
        &cid,
        -20.0,
        -20.0,
        &underruning,
        &counters,
    );
    assert!(
        model.row_data(0).expect("row").audio_overload,
        "the user hears an empty elastic buffer as crackle too"
    );
}

#[test]
fn a_disabled_chain_renders_no_per_stream_rows() {
    let cid = ChainId("chain:0".into());
    let mut off = chain("chain:0", 100.0);
    off.enabled = false;
    let session = session(vec![off]);
    let model = rows(1);
    refresh(
        &session,
        &model,
        &cid,
        -20.0,
        -20.0,
        &FakeReads::default(),
        &Counters::new(),
    );
    assert_eq!(
        model.row_data(0).expect("row").stream_meters.row_count(),
        0,
        "#750: the graph must not stay stuck on after toggle-off"
    );
}

#[test]
fn an_enabled_chain_renders_one_row_per_project_stream() {
    let cid = ChainId("chain:0".into());
    let session = session(vec![chain("chain:0", 100.0)]);
    let model = rows(1);
    refresh(
        &session,
        &model,
        &cid,
        -20.0,
        -20.0,
        &FakeReads::default(),
        &Counters::new(),
    );
    let meters = model.row_data(0).expect("row").stream_meters;
    assert_eq!(
        meters.row_count(),
        1,
        "#532: the row count follows the PROJECT's input entries, not the engine"
    );
    let silent: StreamMeter = meters.row_data(0).expect("stream row");
    assert_eq!(silent.in_dbfs, engine::output_meter::SILENT_DBFS);
}
