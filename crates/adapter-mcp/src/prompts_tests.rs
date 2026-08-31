//! #913 — the static prompt catalog an agent reads to learn how to drive the rig.
//!
//! The contract is thin but load-bearing: every advertised prompt must be
//! fetchable by the exact name it advertises, an unknown name must return
//! nothing rather than a placeholder, and no prompt may ship with an empty
//! body — an agent that fetches one and gets "" has no instructions at all.

use super::{get, prompts};

#[test]
fn every_advertised_prompt_can_be_fetched_by_its_own_name() {
    let advertised = prompts();
    assert!(!advertised.is_empty(), "the catalog must not be empty");
    for prompt in &advertised {
        let fetched = get(&prompt.name)
            .unwrap_or_else(|| panic!("advertised prompt '{}' is not fetchable", prompt.name));
        assert_eq!(
            fetched.description, prompt.description,
            "the listing and the fetch must describe the same prompt"
        );
    }
}

#[test]
fn an_unknown_name_returns_nothing() {
    assert!(get("no_such_prompt").is_none());
    assert!(get("").is_none());
}

#[test]
fn every_prompt_carries_a_description_and_a_non_empty_body() {
    for prompt in prompts() {
        let description = prompt
            .description
            .as_deref()
            .unwrap_or_else(|| panic!("prompt '{}' has no description", prompt.name));
        assert!(!description.trim().is_empty());
        let fetched = get(&prompt.name).expect("fetchable");
        assert!(
            !fetched.messages.is_empty(),
            "prompt '{}' would hand the agent no instructions",
            prompt.name
        );
    }
}

#[test]
fn prompt_names_are_unique() {
    let mut names: Vec<String> = prompts().into_iter().map(|p| p.name).collect();
    let before = names.len();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), before, "two prompts share a name");
}
