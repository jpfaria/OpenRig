//! Red-first (#791 reopen): the Tone Doctor on the command bus.
//!
//! The diagnosis and its fix existed ONLY inside `adapter-gui` wiring, so MCP
//! (and tomorrow gRPC) could not reach them — the agent could read the quality
//! resource but never ask "what is wrong with this chain and fix it". These
//! tests pin the transport-agnostic contract: a `Command` in, a report `Event`
//! out, and an apply that actually moves the culprit's knob in the project.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Once;

use domain::ids::{BlockId, ChainId};
use domain::value_objects::ParameterValue;
use project::block::{schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;
use project::project::Project;

use crate::command::{Command, ToneDoctorCommand};
use crate::dispatcher::CommandDispatcher;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

const SR: f32 = 48_000.0;

fn init() {
    static I: Once = Once::new();
    I.call_once(|| block_gain::register_natives());
}

fn params(model: &str, values: &[(&str, f32)]) -> ParameterSet {
    let schema = schema_for_block_model("gain", model).unwrap();
    let mut ps = ParameterSet::default();
    for (k, v) in values {
        ps.insert(*k, ParameterValue::Float(*v));
    }
    ps.normalized_against(&schema).unwrap()
}

fn core(id: &str, model: &str, ps: ParameterSet) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: model.into(),
            params: ps,
        }),
    }
}

/// A chain that measurably fizzes — the same fixture shape `engine`'s
/// `tone_doctor_fix_tests` uses, so the suggestion is guaranteed to exist.
fn fizzy_chain() -> Chain {
    Chain {
        id: ChainId("chain:1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![
            core("vol", "volume", params("volume", &[("volume", 80.0)])),
            core(
                "fz",
                "fuzz_si",
                params(
                    "fuzz_si",
                    &[("fuzz", 95.0), ("tone", 90.0), ("level", 50.0)],
                ),
            ),
        ],
        di_output: None,
    }
}

fn sine() -> Vec<[f32; 2]> {
    (0..SR as usize)
        .map(|i| {
            let s = 0.5 * (2.0 * std::f32::consts::PI * 1_000.0 * i as f32 / SR).sin();
            [s, s]
        })
        .collect()
}

fn dispatcher_with_chain() -> (LocalDispatcher, Rc<RefCell<Project>>) {
    init();
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![fizzy_chain()],
        midi: None,
    }));
    (LocalDispatcher::new(Rc::clone(&project)), project)
}

/// Register a signal source for the doctor, standing in for the GUI's live
/// input tap (the DI path is covered by `di_loop_for_chain`).
fn attach_sine_source(d: &LocalDispatcher) {
    d.attach_tone_doctor_input(Box::new(|_chain, _seconds| {
        Some(Box::new(|| Some((sine(), SR))))
    }));
}

/// Drain the async completion the diagnosis lands on (it renders the chain many
/// times, so it runs off the dispatching thread — same pattern as the DI load).
fn drain(d: &LocalDispatcher) -> Vec<Event> {
    for _ in 0..600 {
        let events = d.poll_async_results();
        if !events.is_empty() {
            return events;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    Vec::new()
}

/// Red-first: a run that fails must be visible to a transport that only reads.
///
/// A client (MCP today, gRPC next) calls the tool, gets an empty accept, and
/// then polls the resource. When the run fails off-thread — a silent window, a
/// render error — the failure reaches the GUI as an `Event::Error` and the
/// reader sees nothing at all: `{"tone": null}` forever, indistinguishable from
/// "still running" and from "never asked". The read side must carry the state
/// of the last run, not only its happy result.
#[test]
fn the_read_side_reports_the_state_of_the_last_run() {
    let (d, _project) = dispatcher_with_chain();
    // A source that yields digital silence: the diagnosis rejects it.
    d.attach_tone_doctor_input(Box::new(|_chain, _seconds| {
        Some(Box::new(|| Some((vec![[0.0f32; 2]; SR as usize], SR))))
    }));
    let chain = ChainId("chain:1".into());

    let idle = d.tone_report_json(&chain);
    assert!(
        idle.contains("\"state\":\"idle\""),
        "before any run the reader must see it never ran: {idle}"
    );

    d.dispatch(Command::ToneDoctor(ToneDoctorCommand::DiagnoseChainTone {
        chain: chain.clone(),
        genre: None,
        seconds: None,
    }))
    .expect("DiagnoseChainTone dispatches");

    let running = d.tone_report_json(&chain);
    assert!(
        running.contains("\"state\":\"running\""),
        "an accepted run must be visible as in-flight, not as nothing: {running}"
    );

    drain(&d);

    let failed = d.tone_report_json(&chain);
    assert!(
        failed.contains("\"state\":\"failed\""),
        "the failed run must be visible to a reader: {failed}"
    );
    assert!(
        failed.contains("no usable signal"),
        "and it must carry WHY it failed: {failed}"
    );
}

#[test]
fn a_finished_run_reads_as_ok_with_its_verdict() {
    let (d, _project) = dispatcher_with_chain();
    attach_sine_source(&d);
    let chain = ChainId("chain:1".into());

    d.dispatch(Command::ToneDoctor(ToneDoctorCommand::DiagnoseChainTone {
        chain: chain.clone(),
        genre: None,
        seconds: None,
    }))
    .expect("DiagnoseChainTone dispatches");
    drain(&d);

    let done = d.tone_report_json(&chain);
    assert!(done.contains("\"state\":\"ok\""), "{done}");
    assert!(
        done.contains("\"symptom\":\"Fizz\""),
        "the verdict still travels under `tone`: {done}"
    );
}

#[test]
fn diagnose_chain_tone_reports_the_symptom_the_culprit_and_the_fix() {
    let (d, _project) = dispatcher_with_chain();
    attach_sine_source(&d);

    d.dispatch(Command::ToneDoctor(ToneDoctorCommand::DiagnoseChainTone {
        chain: ChainId("chain:1".into()),
        genre: None,
        seconds: None,
    }))
    .expect("DiagnoseChainTone dispatches");

    let report = drain(&d)
        .into_iter()
        .find_map(|e| match e {
            Event::ChainToneDiagnosed { report, .. } => Some(report),
            _ => None,
        })
        .expect("expected Event::ChainToneDiagnosed");

    assert_eq!(report.symptom, "Fizz", "the fuzz at tone 90 fizzes");
    assert_eq!(
        report.culprit.as_ref().map(|b| b.0.as_str()),
        Some("fz"),
        "the culprit is named by block id, not by index"
    );
    let fix = report.suggestion.expect("a fixable fizz yields a fix");
    assert!(
        fix.suggested < fix.current,
        "the fix lowers the knob: {fix:?}"
    );
}

#[test]
fn apply_tone_doctor_fix_writes_the_suggested_value_into_the_project() {
    let (d, project) = dispatcher_with_chain();
    attach_sine_source(&d);

    d.dispatch(Command::ToneDoctor(ToneDoctorCommand::DiagnoseChainTone {
        chain: ChainId("chain:1".into()),
        genre: None,
        seconds: None,
    }))
    .expect("DiagnoseChainTone dispatches");
    let report = drain(&d)
        .into_iter()
        .find_map(|e| match e {
            Event::ChainToneDiagnosed { report, .. } => Some(report),
            _ => None,
        })
        .expect("expected Event::ChainToneDiagnosed");
    let fix = report.suggestion.expect("a fix to apply");

    d.dispatch(Command::ToneDoctor(ToneDoctorCommand::ApplyToneDoctorFix {
        chain: ChainId("chain:1".into()),
    }))
    .expect("ApplyToneDoctorFix dispatches");

    let value = project.borrow().chains[0].blocks[1]
        .model_ref()
        .and_then(|m| m.params.get_f32(&fix.param_path))
        .expect("the culprit's parameter");
    assert!(
        (value - fix.suggested).abs() < 0.01,
        "apply must write the suggested value: {value} != {}",
        fix.suggested
    );
}

#[test]
fn applying_without_a_diagnosis_errors_instead_of_silently_doing_nothing() {
    let (d, _project) = dispatcher_with_chain();

    let err = d
        .dispatch(Command::ToneDoctor(ToneDoctorCommand::ApplyToneDoctorFix {
            chain: ChainId("chain:1".into()),
        }))
        .expect_err("no diagnosis yet — the caller must be told");
    assert!(
        err.to_string().contains("no diagnosis"),
        "the error must name the missing diagnosis: {err}"
    );
}

#[test]
fn diagnosing_without_any_signal_source_errors() {
    let (d, _project) = dispatcher_with_chain();

    let err = d
        .dispatch(Command::ToneDoctor(ToneDoctorCommand::DiagnoseChainTone {
            chain: ChainId("chain:1".into()),
            genre: None,
            seconds: None,
        }))
        .expect_err("no DI loaded and no live input — nothing to analyse");
    assert!(
        err.to_string().contains("no signal"),
        "the error must say there is no signal to analyse: {err}"
    );
}
