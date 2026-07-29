//! #127: `attach_tone_doctor_input` reached the trait so the GUI can attach the
//! Tone Doctor's live-input provider while holding `Rc<dyn CommandDispatcher>`.
//!
//! A defaulted trait method is a silent trap: any implementation that forgets to
//! override it swallows the provider and the doctor then reports "no signal to
//! analyse" — the adapter attached one and nothing said otherwise. These tests
//! pin that BOTH shipping implementations really carry the provider through to
//! the `LocalDispatcher` that consults it.
//!
//! The observable effect needs no audio runtime: `DiagnoseChainTone` asks the
//! provider for a capture, so a provider that records its arguments proves it
//! was consulted, and the dispatch flipping from `Err("no signal…")` to `Ok`
//! proves the doctor actually used what came back.

use std::cell::RefCell;
use std::rc::Rc;

use domain::ids::ChainId;
use project::chain::Chain;
use project::project::Project;

use crate::bridge::event_sink;
use crate::command::{Command, ToneDoctorCommand};
use crate::dispatcher::CommandDispatcher;
use crate::local_dispatcher::{LocalDispatcher, ToneDoctorInput};
use crate::publishing_dispatcher::PublishingDispatcher;

const CHAIN: &str = "chain:1";
const SR: f32 = 48_000.0;

/// What the provider was asked for: `(chain id, seconds)`.
type Asked = Rc<RefCell<Vec<(String, usize)>>>;

/// One empty chain — the diagnosis outcome is irrelevant here, only whether the
/// doctor got a source to work from at all.
fn project_with_chain() -> Rc<RefCell<Project>> {
    Rc::new(RefCell::new(Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![Chain {
            id: ChainId(CHAIN.into()),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec![],
            blocks: vec![],
            di_output: None,
            loopers: vec![],
        }],
        midi: None,
    }))
}

/// A stand-in for the GUI's live input tap: records every request and hands back
/// a short buffer, so the doctor has something to accept.
fn recording_provider(asked: &Asked) -> ToneDoctorInput {
    let asked = Rc::clone(asked);
    Box::new(move |chain, seconds| {
        asked.borrow_mut().push((chain.0.clone(), seconds));
        Some(Box::new(|| Some((vec![[0.1f32; 2]; 64], SR))))
    })
}

fn diagnose() -> Command {
    Command::ToneDoctor(ToneDoctorCommand::DiagnoseChainTone {
        chain: ChainId(CHAIN.into()),
        genre: None,
        seconds: Some(1),
    })
}

/// The whole contract, exercised ONLY through `&dyn CommandDispatcher` — the
/// shape the GUI now holds. Both shipping implementations must satisfy it.
fn provider_attached_through_the_trait_reaches_the_doctor(d: &dyn CommandDispatcher, who: &str) {
    let before = d.dispatch(diagnose());
    assert!(
        before
            .as_ref()
            .err()
            .is_some_and(|e| e.to_string().contains("no signal")),
        "{who}: with no provider the doctor must refuse for lack of signal, got {before:?}"
    );

    let asked: Asked = Rc::new(RefCell::new(Vec::new()));
    d.attach_tone_doctor_input(recording_provider(&asked));

    let accepted = d.dispatch(diagnose());
    assert_eq!(
        asked.borrow().as_slice(),
        [(CHAIN.to_string(), 1)],
        "{who}: attaching through &dyn CommandDispatcher must reach the \
         LocalDispatcher that consults the provider — it was never asked"
    );
    assert!(
        accepted.is_ok(),
        "{who}: the doctor must accept the run once a provider supplies a \
         capture, got {:?}",
        accepted.err()
    );
}

#[test]
fn local_dispatcher_carries_the_tone_doctor_provider() {
    let d = LocalDispatcher::new(project_with_chain());
    provider_attached_through_the_trait_reaches_the_doctor(&d, "LocalDispatcher");
}

#[test]
fn publishing_dispatcher_forwards_the_tone_doctor_provider_to_the_wrapped_one() {
    let (sink, _rx) = event_sink();
    let d = PublishingDispatcher::new(LocalDispatcher::new(project_with_chain()), sink);
    provider_attached_through_the_trait_reaches_the_doctor(&d, "PublishingDispatcher");
}
