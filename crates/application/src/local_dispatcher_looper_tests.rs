//! Issue #323 — the looper commands: what they do to the project and which
//! events they emit. Runtime side effects belong to the adapter wiring.

use std::cell::RefCell;
use std::rc::Rc;

use crate::command::{Command, LooperAction, LooperParam};
use crate::dispatcher::CommandDispatcher;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;
use domain::ids::ChainId;
use project::chain::{Chain, LooperSpeed, LOOPER_MAX_PER_CHAIN};
use project::project::Project;

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

fn dispatcher_with_chain(id: &str) -> (LocalDispatcher, Rc<RefCell<Project>>) {
    let mut project = Project::default();
    project.chains.push(chain(id));
    let project = Rc::new(RefCell::new(project));
    (LocalDispatcher::new(Rc::clone(&project)), project)
}

fn looper_uids(project: &Rc<RefCell<Project>>, chain: &ChainId) -> Vec<u64> {
    project
        .borrow()
        .chains
        .iter()
        .find(|c| &c.id == chain)
        .map(|c| c.loopers.iter().map(|l| l.uid).collect())
        .unwrap_or_default()
}

#[test]
fn add_chain_looper_appends_a_looper_and_reports_its_uid() {
    let (d, project) = dispatcher_with_chain("c1");
    let chain = ChainId("c1".into());

    let events = d
        .dispatch(Command::AddChainLooper {
            chain: chain.clone(),
        })
        .expect("add succeeds");

    let uids = looper_uids(&project, &chain);
    assert_eq!(uids.len(), 1);
    assert_eq!(
        events,
        vec![Event::ChainLooperAdded {
            chain: chain.clone(),
            looper: uids[0],
        }]
    );
}

#[test]
fn every_added_looper_gets_a_distinct_uid() {
    let (d, project) = dispatcher_with_chain("c1");
    let chain = ChainId("c1".into());
    for _ in 0..3 {
        d.dispatch(Command::AddChainLooper {
            chain: chain.clone(),
        })
        .expect("add succeeds");
    }

    let mut uids = looper_uids(&project, &chain);
    uids.sort_unstable();
    uids.dedup();
    assert_eq!(uids.len(), 3, "uids must be unique inside a chain");
    assert!(!uids.contains(&0), "0 marks a free slot — never a real uid");
}

#[test]
fn add_chain_looper_is_capped_per_chain() {
    let (d, project) = dispatcher_with_chain("c1");
    let chain = ChainId("c1".into());
    for _ in 0..LOOPER_MAX_PER_CHAIN {
        d.dispatch(Command::AddChainLooper {
            chain: chain.clone(),
        })
        .expect("add succeeds");
    }

    let err = d
        .dispatch(Command::AddChainLooper {
            chain: chain.clone(),
        })
        .expect_err("the chain is full");
    assert!(
        err.to_string().contains("looper"),
        "the error must name the limit that was hit, got: {err}"
    );
    assert_eq!(looper_uids(&project, &chain).len(), LOOPER_MAX_PER_CHAIN);
}

#[test]
fn remove_chain_looper_drops_it_and_emits() {
    let (d, project) = dispatcher_with_chain("c1");
    let chain = ChainId("c1".into());
    d.dispatch(Command::AddChainLooper {
        chain: chain.clone(),
    })
    .unwrap();
    let uid = looper_uids(&project, &chain)[0];

    let events = d
        .dispatch(Command::RemoveChainLooper {
            chain: chain.clone(),
            looper: uid,
        })
        .expect("remove succeeds");

    assert!(looper_uids(&project, &chain).is_empty());
    assert_eq!(
        events,
        vec![Event::ChainLooperRemoved {
            chain: chain.clone(),
            looper: uid,
        }]
    );
}

#[test]
fn transport_emits_the_action_without_touching_the_project() {
    let (d, project) = dispatcher_with_chain("c1");
    let chain = ChainId("c1".into());
    d.dispatch(Command::AddChainLooper {
        chain: chain.clone(),
    })
    .unwrap();
    let uid = looper_uids(&project, &chain)[0];

    let events = d
        .dispatch(Command::SetChainLooperTransport {
            chain: chain.clone(),
            looper: uid,
            action: LooperAction::Record,
        })
        .expect("transport succeeds");

    assert_eq!(
        events,
        vec![Event::ChainLooperTransportChanged {
            chain: chain.clone(),
            looper: uid,
            action: LooperAction::Record,
        }]
    );
}

#[test]
fn params_are_persisted_on_the_chain() {
    let (d, project) = dispatcher_with_chain("c1");
    let chain = ChainId("c1".into());
    d.dispatch(Command::AddChainLooper {
        chain: chain.clone(),
    })
    .unwrap();
    let uid = looper_uids(&project, &chain)[0];

    for param in [
        LooperParam::Mix(0.4),
        LooperParam::Decay(0.75),
        LooperParam::Speed(LooperSpeed::Half),
        LooperParam::Reverse(true),
    ] {
        d.dispatch(Command::SetChainLooperParam {
            chain: chain.clone(),
            looper: uid,
            param,
        })
        .expect("param succeeds");
    }

    let snapshot = project.borrow().clone();
    let cfg = snapshot
        .chains
        .iter()
        .find(|c| c.id == chain)
        .and_then(|c| c.loopers.first().cloned())
        .expect("the looper is there");
    assert_eq!(cfg.mix, 0.4);
    assert_eq!(cfg.decay, 0.75);
    assert_eq!(cfg.speed, LooperSpeed::Half);
    assert!(cfg.reverse);
}

#[test]
fn mix_and_decay_are_clamped_to_the_audible_range() {
    let (d, project) = dispatcher_with_chain("c1");
    let chain = ChainId("c1".into());
    d.dispatch(Command::AddChainLooper {
        chain: chain.clone(),
    })
    .unwrap();
    let uid = looper_uids(&project, &chain)[0];

    d.dispatch(Command::SetChainLooperParam {
        chain: chain.clone(),
        looper: uid,
        param: LooperParam::Mix(4.0),
    })
    .unwrap();
    d.dispatch(Command::SetChainLooperParam {
        chain: chain.clone(),
        looper: uid,
        param: LooperParam::Decay(-1.0),
    })
    .unwrap();

    let snapshot = project.borrow().clone();
    let cfg = snapshot.chains[0].loopers[0].clone();
    assert_eq!(cfg.mix, 1.0);
    assert_eq!(cfg.decay, 0.0);
}

#[test]
fn commands_for_an_unknown_chain_or_looper_fail_loudly() {
    let (d, project) = dispatcher_with_chain("c1");
    let known = ChainId("c1".into());
    let unknown = ChainId("nope".into());

    assert!(d
        .dispatch(Command::AddChainLooper {
            chain: unknown.clone()
        })
        .is_err());
    assert!(d
        .dispatch(Command::RemoveChainLooper {
            chain: known.clone(),
            looper: 123,
        })
        .is_err());
    assert!(d
        .dispatch(Command::SetChainLooperTransport {
            chain: known.clone(),
            looper: 123,
            action: LooperAction::Play,
        })
        .is_err());
    assert!(d
        .dispatch(Command::SetChainLooperParam {
            chain: known,
            looper: 123,
            param: LooperParam::Mix(0.5),
        })
        .is_err());
}
