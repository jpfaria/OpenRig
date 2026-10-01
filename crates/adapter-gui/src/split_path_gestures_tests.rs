//! #328 spec §11: the split editor's "+ path" and remove-path gestures.

use application::command::{Command, SplitCommand};
use domain::ids::BlockId;
use project::block::SplitEnd;

use super::{add_path_command, path_letters, remove_path_request, RemovePathRequest};
use crate::chain_graph_fixtures_tests::{chain, core, split_paths};

fn id(s: &str) -> BlockId {
    BlockId(s.into())
}

/// A chain whose split "y" has three paths: A with two blocks, B empty and
/// C holding the nested split "inner".
fn three_path_chain() -> project::chain::Chain {
    let inner = split_paths("inner", SplitEnd::Mix, vec![vec![], vec![core("cab")]]);
    chain(vec![split_paths(
        "y",
        SplitEnd::Y,
        vec![vec![core("a1"), core("a2")], vec![], vec![inner]],
    )])
}

fn removed_path(command: &Command) -> (BlockId, usize) {
    match command {
        Command::Split(SplitCommand::RemoveSplitPath { split_id, path, .. }) => {
            (split_id.clone(), *path)
        }
        other => panic!("expected RemoveSplitPath, got {other:?}"),
    }
}

#[test]
fn plus_path_adds_to_the_split_the_editor_shows_at_any_depth() {
    let chain = three_path_chain();
    for split in ["y", "inner"] {
        match add_path_command(&chain, &id(split)) {
            Some(Command::Split(SplitCommand::AddSplitPath { chain: c, split_id })) => {
                assert_eq!(c, chain.id);
                assert_eq!(split_id, id(split));
            }
            other => panic!("{split}: expected AddSplitPath, got {other:?}"),
        }
    }
}

#[test]
fn plus_path_on_a_missing_split_does_nothing() {
    assert!(add_path_command(&three_path_chain(), &id("ghost")).is_none());
}

#[test]
fn an_empty_path_is_removed_without_asking() {
    match remove_path_request(&three_path_chain(), &id("y"), 1) {
        Some(RemovePathRequest::Direct(command)) => {
            assert_eq!(removed_path(&command), (id("y"), 1));
        }
        other => panic!("expected a direct removal, got {other:?}"),
    }
}

#[test]
fn a_path_with_blocks_asks_first_and_names_it() {
    match remove_path_request(&three_path_chain(), &id("y"), 0) {
        Some(RemovePathRequest::Confirm { command, name }) => {
            assert_eq!(removed_path(&command), (id("y"), 0));
            assert_eq!(name, "A", "the dialog names the path by its letter alone");
        }
        other => panic!("expected a confirmation, got {other:?}"),
    }
}

#[test]
fn a_path_with_one_block_is_named_by_its_letter_alone() {
    let chain = chain(vec![split_paths(
        "y",
        SplitEnd::Y,
        vec![vec![core("a1")], vec![], vec![]],
    )]);
    match remove_path_request(&chain, &id("y"), 0) {
        Some(RemovePathRequest::Confirm { name, .. }) => {
            assert_eq!(name, "A", "no block count, so no \"1 blocks\"");
        }
        other => panic!("expected a confirmation, got {other:?}"),
    }
}

#[test]
fn a_split_at_its_minimum_keeps_its_paths() {
    assert!(
        remove_path_request(&three_path_chain(), &id("inner"), 0).is_none(),
        "#328: a split never drops below two paths"
    );
}

#[test]
fn a_missing_path_or_split_is_not_removed() {
    let chain = three_path_chain();
    assert!(remove_path_request(&chain, &id("y"), 3).is_none());
    assert!(remove_path_request(&chain, &id("ghost"), 0).is_none());
}

#[test]
fn the_editor_letters_every_path() {
    let chain = three_path_chain();
    assert_eq!(path_letters(&chain, &id("y")), vec!["A", "B", "C"]);
    assert_eq!(path_letters(&chain, &id("inner")), vec!["A", "B"]);
    assert!(path_letters(&chain, &id("ghost")).is_empty());
}
