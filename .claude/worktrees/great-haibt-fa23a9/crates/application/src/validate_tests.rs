//! Orchestrator for `application::validate` tests. Re-attached to `validate.rs`
//! via `#[cfg(test)] #[path = "validate_tests.rs"] mod tests;`.
//!
//! Split into 3 sibling submodules to keep each file under the 600-line cap:
//! - `helpers` — fixtures + re-exported types (`pub(super)`).
//! - `main` — `validate_project` integration tests.
//! - `unit` — unit tests for individual helper functions in `validate.rs`.

#[path = "validate_tests_helpers.rs"]
mod helpers;

#[path = "validate_tests_main.rs"]
mod main;

#[path = "validate_tests_unit.rs"]
mod unit;
