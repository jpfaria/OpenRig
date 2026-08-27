//! #913 — what the "remove this recent?" dialog calls the project.
//!
//! The dialog is the last thing between the user and losing an entry, so it
//! must never ask "remove ?" — every fallback still names something the user
//! can recognise.

use super::confirm_removal_label;
use infra_filesystem::RecentProjectEntry;

fn entry(name: &str, path: &str) -> RecentProjectEntry {
    RecentProjectEntry {
        project_path: path.to_string(),
        project_name: name.to_string(),
        is_valid: true,
        invalid_reason: None,
    }
}

#[test]
fn a_named_project_is_shown_by_its_name() {
    assert_eq!(
        confirm_removal_label(&entry("Studio Rig", "/p/studio.yaml")),
        "Studio Rig"
    );
}

#[test]
fn an_unnamed_project_falls_back_to_the_file_it_points_at() {
    assert_eq!(
        confirm_removal_label(&entry("", "/p/live-set.yaml")),
        "live-set"
    );
}

#[test]
fn a_path_with_no_stem_falls_back_to_the_path_itself() {
    let label = confirm_removal_label(&entry("", "/"));
    assert!(!label.is_empty(), "the dialog must never ask 'remove ?'");
}

#[test]
fn the_name_wins_even_when_it_differs_from_the_filename() {
    assert_eq!(
        confirm_removal_label(&entry("My Rig", "/p/untitled-3.yaml")),
        "My Rig",
        "the user's own name is what they will recognise"
    );
}

#[test]
fn a_name_that_is_only_whitespace_is_still_the_name() {
    // Deliberate: only an EMPTY name falls back. Trimming here would disagree
    // with what the launcher list shows for the same entry.
    assert_eq!(confirm_removal_label(&entry(" ", "/p/x.yaml")), " ");
}
