//! #913 — committing one edited block parameter.
//!
//! Number, text and bool all come through here, so the rules hold for all
//! three. The one that bites: a draft for a block still being ADDED has nothing
//! to address, and committing against its indexes would write the value onto
//! whatever block already sits at that position.

use super::{apply_block_parameter, parse_number_text, ApplyParamError, ParamValue};
use crate::state::{BlockEditorDraft, ProjectSession};
use crate::ProjectChainItem;
use domain::ids::{BlockId, ChainId};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::project::Project;
use slint::VecModel;
use std::cell::RefCell;
use std::rc::Rc;

fn block(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "volume".into(),
            params: Default::default(),
        }),
    }
}

fn session(blocks: Vec<AudioBlock>) -> Rc<RefCell<Option<ProjectSession>>> {
    Rc::new(RefCell::new(Some(ProjectSession::new(
        Project {
            name: None,
            device_settings: vec![],
            chains: vec![Chain {
                id: ChainId("chain:0".into()),
                description: None,
                instrument: "electric_guitar".into(),
                enabled: false,
                volume: 100.0,
                io_binding_ids: vec![],
                blocks,
                di_output: None,
                loopers: vec![],
            }],
            midi: None,
        },
        None,
        None,
        std::env::temp_dir().join("openrig-913-param-tests"),
    ))))
}

fn draft(block_index: Option<usize>) -> Rc<RefCell<Option<BlockEditorDraft>>> {
    Rc::new(RefCell::new(Some(BlockEditorDraft {
        chain_index: 0,
        block_index,
        before_index: 0,
        instrument: "electric_guitar".into(),
        effect_type: "gain".into(),
        model_id: "volume".into(),
        enabled: true,
        is_select: false,
    })))
}

fn rows() -> Rc<VecModel<ProjectChainItem>> {
    infra_filesystem::init_asset_paths(infra_filesystem::AssetPaths::default());
    Rc::new(VecModel::from(Vec::<ProjectChainItem>::new()))
}

fn apply(
    session: &Rc<RefCell<Option<ProjectSession>>>,
    draft: &Rc<RefCell<Option<BlockEditorDraft>>>,
    path: &str,
    value: ParamValue,
) -> Result<bool, ApplyParamError> {
    apply_block_parameter(session, draft, path, value, &rows(), &[], &[])
}

// ── Reading what the field holds ──────────────────────────────────────────

#[test]
fn a_plain_decimal_is_read() {
    assert_eq!(parse_number_text("0.5"), Some(0.5));
    assert_eq!(parse_number_text("-3"), Some(-3.0));
}

#[test]
fn a_comma_is_accepted_as_the_decimal_separator() {
    assert_eq!(
        parse_number_text("0,5"),
        Some(0.5),
        "the app ships in nine locales — a pt-BR keyboard types a comma"
    );
}

#[test]
fn text_that_is_not_a_number_is_refused_rather_than_defaulted() {
    assert_eq!(parse_number_text("loud"), None);
    assert_eq!(parse_number_text(""), None);
    assert_eq!(
        parse_number_text("1.2.3"),
        None,
        "silently taking 1.2 here would set a knob the user never asked for"
    );
}

// ── Committing the value ──────────────────────────────────────────────────

#[test]
fn a_number_reaches_the_block_the_draft_points_at() {
    let session = session(vec![block("gain")]);
    let applied = apply(&session, &draft(Some(0)), "level", ParamValue::Number(0.5))
        .expect("the block exists");
    assert!(applied, "the dispatcher reported the change");
}

#[test]
fn a_draft_for_a_block_being_added_commits_nothing() {
    let session = session(vec![block("gain")]);
    assert_eq!(
        apply(&session, &draft(None), "level", ParamValue::Number(0.5)),
        Err(ApplyParamError::NotAddressable),
        "there is no block in the project to address yet"
    );
}

#[test]
fn no_draft_open_commits_nothing() {
    let session = session(vec![block("gain")]);
    let none: Rc<RefCell<Option<BlockEditorDraft>>> = Rc::new(RefCell::new(None));
    assert_eq!(
        apply(&session, &none, "level", ParamValue::Number(0.5)),
        Err(ApplyParamError::NotAddressable)
    );
}

#[test]
fn a_block_index_that_no_longer_resolves_commits_nothing() {
    let session = session(vec![block("gain")]);
    assert_eq!(
        apply(&session, &draft(Some(7)), "level", ParamValue::Number(0.5)),
        Err(ApplyParamError::NotAddressable),
        "writing onto whatever shifted into that slot is the bug this prevents"
    );
}

#[test]
fn committing_with_no_project_open_is_not_addressable() {
    let none: Rc<RefCell<Option<ProjectSession>>> = Rc::new(RefCell::new(None));
    assert_eq!(
        apply(&none, &draft(Some(0)), "level", ParamValue::Number(0.5)),
        Err(ApplyParamError::NotAddressable)
    );
}

#[test]
fn the_dispatcher_validates_text_and_bool_paths_but_not_number_paths() {
    // Pinning an ASYMMETRY, not endorsing it: `SetBlockParameterNumber` CREATES
    // a path the model never declared, while `SetBlockParameterText` and
    // `SetBlockParameterBool` refuse one. Each case gets a FRESH block —
    // reusing one hides the asymmetry, because the number call leaves the
    // invented parameter behind for the next kind to find.
    assert_eq!(
        apply(
            &session(vec![block("gain")]),
            &draft(Some(0)),
            "no_such_knob",
            ParamValue::Number(1.0)
        ),
        Ok(true),
        "a number is accepted at any path today, and the parameter is created"
    );

    for value in [ParamValue::Text("warm".into()), ParamValue::Bool(true)] {
        match apply(
            &session(vec![block("gain")]),
            &draft(Some(0)),
            "no_such_knob",
            value.clone(),
        ) {
            Err(ApplyParamError::Failed(message)) => assert!(
                message.contains("no_such_knob"),
                "the refusal must name the parameter: {message}"
            ),
            other => panic!("{value:?} was not refused: {other:?}"),
        }
    }
}

#[test]
fn an_option_pick_carries_both_the_value_and_the_index() {
    // The project stores the string, the widget shows the index. A command
    // carrying only one of them leaves the other surface out of step.
    let session = session(vec![block("gain")]);
    let result = apply(
        &session,
        &draft(Some(0)),
        "no_such_select",
        ParamValue::Option {
            value: "warm".into(),
            index: 2,
        },
    );
    match result {
        Err(ApplyParamError::Failed(message)) => assert!(
            message.contains("no_such_select"),
            "the refusal must name the parameter: {message}"
        ),
        other => panic!("expected the unknown select to be refused, got {other:?}"),
    }
}

#[test]
fn an_option_pick_on_a_draft_still_being_added_commits_nothing() {
    let session = session(vec![block("gain")]);
    assert_eq!(
        apply(
            &session,
            &draft(None),
            "mode",
            ParamValue::Option {
                value: "warm".into(),
                index: 0,
            }
        ),
        Err(ApplyParamError::NotAddressable)
    );
}

#[test]
fn setting_a_parameter_to_the_value_it_already_has_still_counts_as_a_change() {
    // Observed, not endorsed: the dispatcher emits `BlockParameterChanged`
    // even when the value did not move, so a knob dragged back to where it
    // started still triggers a runtime resync and a row republish. This layer's
    // `Ok(false)` path — "accepted but nothing changed" — is therefore never
    // taken today. Pinned so that if the dispatcher starts comparing, this
    // test says so instead of the behaviour changing unnoticed.
    let session = session(vec![block("gain")]);
    let draft = draft(Some(0));
    apply(&session, &draft, "level", ParamValue::Number(0.5)).expect("first set");
    assert_eq!(
        apply(&session, &draft, "level", ParamValue::Number(0.5)),
        Ok(true)
    );
}
