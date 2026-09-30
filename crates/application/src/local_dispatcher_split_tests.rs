//! #328 — `AddSplit`, `SetSplitEnd`, `RemoveSplit`, driven through their wire
//! form (the road an MCP tool call takes).

use project::block::split_params::default_split_params;
use project::block::SplitEnd;
use serde_json::json;

use crate::local_dispatcher_tests::*;
use crate::split_tests_fixtures::*;

#[test]
fn add_split_inserts_an_empty_split_with_the_default_knobs() {
    let project = project_with(vec![
        make_core_block("pre", true),
        make_core_block("post", true),
    ]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let events = dispatch_json(
        &dispatcher,
        "AddSplit",
        json!({ "chain": CHAIN, "position": 1, "end": "mix" }),
    )
    .expect("AddSplit on a chain without a split");

    let blocks = project.borrow().chains[0].blocks.clone();
    assert_eq!(blocks.len(), 3);
    let AudioBlockKind::Split(split) = &blocks[1].kind else {
        panic!("slot 1 holds the split, got '{}'", blocks[1].kind.label());
    };
    assert!(matches!(split.end, SplitEnd::Mix));
    assert!(
        split.a.is_empty() && split.b.is_empty(),
        "both paths start empty"
    );
    assert_eq!(split.params, default_split_params());
    assert!(
        blocks[1].id.0.starts_with("chain_0:block:"),
        "{}",
        blocks[1].id.0
    );
    assert_eq!(
        events,
        vec![Event::BlockAdded {
            chain: ChainId(CHAIN.into()),
            block: blocks[1].id.clone(),
        }]
    );
}

#[test]
fn add_split_refuses_a_second_split() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let before = project.borrow().chains[0].blocks.clone();

    let err = dispatch_json(
        &dispatcher,
        "AddSplit",
        json!({ "chain": CHAIN, "position": 0, "end": "mix" }),
    )
    .expect_err("a chain holds one split");

    assert!(err.to_string().contains("at most one split"), "{err}");
    assert_eq!(project.borrow().chains[0].blocks, before);
}

#[test]
fn add_split_y_refuses_a_block_after_its_position() {
    let project = project_with(vec![
        make_core_block("pre", true),
        make_core_block("post", true),
    ]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let err = dispatch_json(
        &dispatcher,
        "AddSplit",
        json!({ "chain": CHAIN, "position": 1, "end": "y" }),
    )
    .expect_err("post would follow the Y split");

    assert!(err.to_string().contains("Y split"), "{err}");
    assert_eq!(ids(&project.borrow().chains[0].blocks), vec!["pre", "post"]);
}

#[test]
fn add_split_y_at_the_end_is_accepted() {
    let project = project_with(vec![
        make_core_block("pre", true),
        make_core_block("post", true),
    ]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatch_json(
        &dispatcher,
        "AddSplit",
        json!({ "chain": CHAIN, "position": 2, "end": "y" }),
    )
    .expect("a Y split may be the last block");

    assert_eq!(project.borrow().chains[0].blocks.len(), 3);
    assert!(matches!(split_of(&project.borrow()).end, SplitEnd::Y));
}

#[test]
fn set_split_end_to_y_is_refused_while_a_block_follows_the_split() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let err = dispatch_json(
        &dispatcher,
        "SetSplitEnd",
        json!({ "chain": CHAIN, "split_id": "split_0", "end": "y" }),
    )
    .expect_err("post follows the split");

    assert!(err.to_string().contains("Y split"), "{err}");
    assert!(
        matches!(split_of(&project.borrow()).end, SplitEnd::Mix),
        "the split kept its end"
    );
}

#[test]
fn set_split_end_to_y_is_accepted_when_the_split_is_last() {
    let project = project_with(vec![
        make_core_block("pre", true),
        split("split_0", SplitEnd::Mix, vec![], vec![]),
    ]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let events = dispatch_json(
        &dispatcher,
        "SetSplitEnd",
        json!({ "chain": CHAIN, "split_id": "split_0", "end": "y" }),
    )
    .expect("nothing follows the split");

    assert!(matches!(split_of(&project.borrow()).end, SplitEnd::Y));
    assert_eq!(
        events,
        vec![Event::ChainReloaded {
            chain: ChainId(CHAIN.into())
        }]
    );
}

#[test]
fn remove_split_keeps_path_a_in_its_place_and_drops_path_b() {
    let project = project_with(vec![
        make_core_block("pre", true),
        split(
            "split_0",
            SplitEnd::Mix,
            vec![make_core_block("a_0", true), make_core_block("a_1", true)],
            vec![make_core_block("b_0", true)],
        ),
        make_core_block("post", true),
    ]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatch_json(
        &dispatcher,
        "RemoveSplit",
        json!({ "chain": CHAIN, "split_id": "split_0" }),
    )
    .expect("RemoveSplit");

    assert_eq!(
        ids(&project.borrow().chains[0].blocks),
        vec!["pre", "a_0", "a_1", "post"]
    );
}

#[test]
fn remove_split_refuses_a_block_that_is_not_a_split() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let before = project.borrow().chains[0].blocks.clone();

    let err = dispatch_json(
        &dispatcher,
        "RemoveSplit",
        json!({ "chain": CHAIN, "split_id": "pre" }),
    )
    .expect_err("pre is not a split");

    assert!(err.to_string().contains("is not a split"), "{err}");
    assert_eq!(project.borrow().chains[0].blocks, before);
}

#[test]
fn add_split_reaches_the_rig_preset_on_capture() {
    let (rig, _project, dispatcher) = rig_session_from(rig_with_presets(vec![(
        "p1",
        vec![make_core_block("A", true)],
    )]));

    let events = dispatch_json(
        &dispatcher,
        "AddSplit",
        json!({ "chain": RIG_CHAIN, "position": 1, "end": "mix" }),
    )
    .expect("AddSplit on the rig chain");
    let Some(Event::BlockAdded {
        block: split_id, ..
    }) = events.first()
    else {
        panic!("AddSplit answers BlockAdded, got {events:?}");
    };
    dispatcher
        .dispatch(Command::Project(ProjectCommand::CaptureRigEdits))
        .expect("capture");

    assert_eq!(
        ids(&rig.borrow().presets["p1"].blocks),
        vec!["A".to_string(), split_id.0.clone()],
        "the new split must reach project.openrig"
    );
}

#[test]
fn add_split_y_after_a_mix_is_accepted() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatch_json(
        &dispatcher,
        "AddSplit",
        json!({ "chain": CHAIN, "position": 3, "end": "y" }),
    )
    .expect("a Y may end a chain that holds a Mix");

    let blocks = project.borrow().chains[0].blocks.clone();
    assert_eq!(ids(&blocks[..3]), vec!["pre", "split_0", "post"]);
    let AudioBlockKind::Split(y) = &blocks[3].kind else {
        panic!("slot 3 holds the Y, got '{}'", blocks[3].kind.label());
    };
    assert!(matches!(y.end, SplitEnd::Y));
}

#[test]
fn add_split_mix_after_the_y_is_refused() {
    let project = project_with(y_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let before = project.borrow().chains[0].blocks.clone();

    let err = dispatch_json(
        &dispatcher,
        "AddSplit",
        json!({ "chain": CHAIN, "position": 2, "end": "mix" }),
    )
    .expect_err("the Mix comes before the Y");

    assert!(err.to_string().contains("Y split"), "{err}");
    assert_eq!(project.borrow().chains[0].blocks, before);
}

/// Pin: a second Y was refused under the one-split rule and still is.
#[test]
fn add_split_refuses_a_second_y() {
    let project = project_with(mix_then_y_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let before = project.borrow().chains[0].blocks.clone();

    let err = dispatch_json(
        &dispatcher,
        "AddSplit",
        json!({ "chain": CHAIN, "position": 4, "end": "y" }),
    )
    .expect_err("a chain holds one Y");

    assert!(err.to_string().contains("at most one split"), "{err}");
    assert_eq!(project.borrow().chains[0].blocks, before);
}

/// Pin: switching either split of a Mix + Y chain would leave two of one end.
#[test]
fn set_split_end_refuses_two_mix_or_two_y() {
    for (split_id, end) in [("y", "mix"), ("mix", "y")] {
        let project = project_with(mix_then_y_chain());
        let dispatcher = LocalDispatcher::new(Rc::clone(&project));
        let before = project.borrow().chains[0].blocks.clone();

        let err = dispatch_json(
            &dispatcher,
            "SetSplitEnd",
            json!({ "chain": CHAIN, "split_id": split_id, "end": end }),
        )
        .expect_err("a chain holds one split of each end");

        assert!(
            err.to_string().contains("at most one split"),
            "{split_id} → {end}: {err}"
        );
        assert_eq!(project.borrow().chains[0].blocks, before);
    }
}

#[test]
fn set_split_end_to_y_is_accepted_behind_a_mix() {
    let project = project_with(vec![
        make_core_block("pre", true),
        split("mix", SplitEnd::Mix, vec![], vec![]),
        make_core_block("mid", true),
        split("last", SplitEnd::Mix, vec![], vec![]),
    ]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatch_json(
        &dispatcher,
        "SetSplitEnd",
        json!({ "chain": CHAIN, "split_id": "last", "end": "y" }),
    )
    .expect("the last split may end in a Y behind the Mix");

    assert!(matches!(
        split_by_id(&project.borrow(), "last").end,
        SplitEnd::Y
    ));
    assert!(matches!(
        split_by_id(&project.borrow(), "mix").end,
        SplitEnd::Mix
    ));
}

#[test]
fn remove_split_of_the_mix_keeps_the_y_last() {
    let project = project_with(mix_then_y_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatch_json(
        &dispatcher,
        "RemoveSplit",
        json!({ "chain": CHAIN, "split_id": "mix" }),
    )
    .expect("RemoveSplit of the Mix");

    assert_eq!(
        ids(&project.borrow().chains[0].blocks),
        vec!["pre", "a_0", "mid", "y"]
    );
}
