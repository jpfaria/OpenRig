//! #913 — tapping a chain makes it the footswitch's active chain.
//!
//! #591: before this reached the dispatcher, `active_chain` only moved when a
//! BLOCK was selected, so the pedal stayed frozen on the last block-selected
//! chain no matter what the player had in front of them. What must hold is that
//! the tap lands on the dispatcher-owned selection state — the same one MIDI
//! reads — and that an index with no chain behind it is refused rather than
//! silently selecting something else.

use super::{select_chain, SelectChainError};
use crate::state::ProjectSession;
use domain::ids::ChainId;
use project::chain::Chain;
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
        std::env::temp_dir().join("openrig-913-selection-tests"),
    )
}

fn active_chain(session: &ProjectSession) -> Option<String> {
    let state = session.dispatcher.selection_state();
    let read = state.read().expect("selection state poisoned");
    read.active_chain.clone()
}

#[test]
fn tapping_a_chain_puts_it_on_the_dispatcher_owned_selection() {
    let session = session(vec![chain("chain:0"), chain("chain:1")]);
    assert_eq!(select_chain(&session, 1), Ok(ChainId("chain:1".into())));
    assert_eq!(
        active_chain(&session),
        Some("chain:1".to_string()),
        "#591: the footswitch reads this — a tap that stops at the GUI leaves \
         the pedal on the previous chain"
    );
}

#[test]
fn tapping_the_first_chain_selects_the_first_chain() {
    let session = session(vec![chain("chain:0"), chain("chain:1")]);
    assert_eq!(select_chain(&session, 0), Ok(ChainId("chain:0".into())));
    assert_eq!(active_chain(&session), Some("chain:0".to_string()));
}

#[test]
fn an_index_with_no_chain_behind_it_is_refused() {
    let session = session(vec![chain("chain:0")]);
    assert_eq!(
        select_chain(&session, 5),
        Err(SelectChainError::NoSuchChain)
    );
    assert_eq!(
        active_chain(&session),
        None,
        "a stale index must not move the selection to some other chain"
    );
}

#[test]
fn tapping_in_a_project_with_no_chains_is_refused() {
    let session = session(vec![]);
    assert_eq!(
        select_chain(&session, 0),
        Err(SelectChainError::NoSuchChain)
    );
}

#[test]
fn re_tapping_the_same_chain_keeps_it_selected() {
    let session = session(vec![chain("chain:0")]);
    select_chain(&session, 0).expect("first tap");
    select_chain(&session, 0).expect("second tap");
    assert_eq!(active_chain(&session), Some("chain:0".to_string()));
}
