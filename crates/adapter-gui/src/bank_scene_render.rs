//! Pure presentation mapper for the #453 navigator: `BankSceneState` →
//! plain rows. No Slint, no `AppWindow` (memory: no business logic in the
//! screen; testable without a window). The Slint glue converts `BankNavRow`
//! into the generated `BankNavItem` model — a trivial field copy.

use crate::bank_scene_session::BankSceneState;

/// One rendered input row (mirrors the Slint `BankNavItem` struct, 1:1).
#[derive(Debug, Clone, PartialEq)]
pub struct BankNavRow {
    pub input: String,
    pub label: String,
    pub active_preset: i32,
    pub active_scene: i32,
    pub bank_slots: Vec<i32>,
    pub selected: bool,
}

/// Project `BankSceneState` onto the rows the navigator renders. Order is the
/// state's input order (deterministic — `BTreeMap` derived). `selected`
/// flags the currently focused input; empty label falls back in the screen.
pub fn render(state: &BankSceneState) -> Vec<BankNavRow> {
    state
        .inputs
        .iter()
        .map(|nav| BankNavRow {
            input: nav.input.clone(),
            label: nav.label.clone().unwrap_or_default(),
            active_preset: nav.active_preset as i32,
            active_scene: nav.active_scene as i32,
            bank_slots: nav.bank_slots.iter().map(|&s| s as i32).collect(),
            selected: state.selected_input.as_deref() == Some(nav.input.as_str()),
        })
        .collect()
}

#[cfg(test)]
#[path = "bank_scene_render_tests.rs"]
mod tests;
