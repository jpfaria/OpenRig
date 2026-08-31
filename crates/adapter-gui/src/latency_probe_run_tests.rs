//! #913 — measuring one chain's latency into its badge.
//!
//! The measurement itself lives in `application::query_latency`; what this
//! layer owns is which chain gets measured, at WHICH RATE, and where the answer
//! lands. #723: asking the dispatcher's engine rate first measured a stopped
//! rig on a 44.1 kHz interface as if it ran at 48 kHz, so the seam is asked
//! first and the dispatcher is only the fallback.

use super::{probe_chain_latency, BADGE_WINDOW};
use crate::latency_probe::new_windows;
use crate::state::ProjectSession;
use crate::ProjectChainItem;
use application::live_source::LiveSource;
use domain::ids::ChainId;
use project::chain::Chain;
use project::project::Project;
use slint::{Model, VecModel};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

#[derive(Default)]
struct Seam {
    rate: Option<f32>,
    asked: RefCell<Vec<ChainId>>,
}
impl LiveSource for Seam {
    fn chain_sample_rate(&self, chain: &ChainId) -> Option<f32> {
        self.asked.borrow_mut().push(chain.clone());
        self.rate
    }
}

fn chain(id: &str) -> Chain {
    Chain {
        id: ChainId(id.into()),
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

fn session(chains: Vec<Chain>) -> ProjectSession {
    ProjectSession::new(
        Project {
            name: None,
            device_settings: vec![],
            chains,
            midi: None,
        },
        None,
        None,
        std::env::temp_dir().join("openrig-913-latency-tests"),
    )
}

fn rows(n: usize) -> Rc<VecModel<ProjectChainItem>> {
    Rc::new(VecModel::from(
        (0..n)
            .map(|_| ProjectChainItem::default())
            .collect::<Vec<_>>(),
    ))
}

#[test]
fn the_seam_is_asked_for_the_rate_before_the_dispatcher() {
    let session = session(vec![chain("chain:0")]);
    let seam = Seam {
        rate: Some(44_100.0),
        ..Default::default()
    };
    let chains = rows(1);
    let windows = new_windows();

    probe_chain_latency(&session, &seam, &chains, &windows, 0, Instant::now());

    assert_eq!(
        seam.asked.borrow().as_slice(),
        &[ChainId("chain:0".into())],
        "#723: the rate must come from the running stream, not a tracked default"
    );
}

#[test]
fn a_measurement_lands_on_that_chains_row() {
    let session = session(vec![chain("chain:0")]);
    let chains = rows(1);
    let windows = new_windows();

    let measured = probe_chain_latency(
        &session,
        &Seam::default(),
        &chains,
        &windows,
        0,
        Instant::now(),
    )
    .expect("an empty chain still has a measurable DSP latency");

    assert_eq!(chains.row_data(0).expect("row").latency_ms, measured);
}

#[test]
fn a_measurement_opens_the_badges_display_window() {
    let now = Instant::now();
    let session = session(vec![chain("chain:0")]);
    let chains = rows(1);
    let windows = new_windows();

    probe_chain_latency(&session, &Seam::default(), &chains, &windows, 0, now);

    assert_eq!(
        windows.borrow().get(&0).copied(),
        Some(now + BADGE_WINDOW),
        "the sweep clears the badge from this instant"
    );
}

#[test]
fn the_second_chain_is_measured_onto_the_second_row() {
    let session = session(vec![chain("chain:0"), chain("chain:1")]);
    let chains = rows(2);
    let windows = new_windows();

    probe_chain_latency(
        &session,
        &Seam::default(),
        &chains,
        &windows,
        1,
        Instant::now(),
    );

    assert_eq!(chains.row_data(0).expect("row 0").latency_ms, 0.0);
    assert!(windows.borrow().contains_key(&1));
    assert!(!windows.borrow().contains_key(&0));
}

#[test]
fn a_chain_index_that_does_not_exist_measures_nothing() {
    let session = session(vec![chain("chain:0")]);
    let chains = rows(1);
    let windows = new_windows();

    assert!(probe_chain_latency(
        &session,
        &Seam::default(),
        &chains,
        &windows,
        7,
        Instant::now()
    )
    .is_none());
    assert!(windows.borrow().is_empty(), "no badge window is opened");
    assert_eq!(chains.row_data(0).expect("row").latency_ms, 0.0);
}
