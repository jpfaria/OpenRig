//! #913 — the gate between the wizard's input and output steps.
//!
//! The wizard runs once, on first launch. Letting it through with no input
//! selected produces a rig that opens with nothing to listen to, and letting a
//! row with unreadable numbers through would open a stream at a rate nobody
//! chose — so an unparseable row is refused with its own message rather than
//! silently skipped.

use super::{next_step, WizardStep};
use crate::DeviceSelectionItem;
use slint::{SharedString, VecModel};
use std::rc::Rc;

fn row(name: &str, selected: bool, rate: &str, buffer: &str) -> DeviceSelectionItem {
    DeviceSelectionItem {
        device_id: SharedString::from(name),
        name: SharedString::from(name),
        selected,
        sample_rate_text: SharedString::from(rate),
        buffer_size_text: SharedString::from(buffer),
        bit_depth_text: SharedString::from("24"),
    }
}

fn model(rows: Vec<DeviceSelectionItem>) -> Rc<VecModel<DeviceSelectionItem>> {
    Rc::new(VecModel::from(rows))
}

#[test]
fn one_selected_input_lets_the_wizard_advance() {
    let devices = model(vec![row("Scarlett", true, "48000", "128")]);
    assert_eq!(next_step(&devices), WizardStep::Advance);
}

#[test]
fn with_nothing_selected_the_user_is_asked_to_pick_an_input() {
    let devices = model(vec![
        row("Scarlett", false, "48000", "128"),
        row("Built-in", false, "48000", "128"),
    ]);
    assert_eq!(
        next_step(&devices),
        WizardStep::NeedsAnInput,
        "advancing here would finish the wizard with nothing to listen to"
    );
}

#[test]
fn an_empty_device_list_asks_for_an_input_too() {
    assert_eq!(next_step(&model(Vec::new())), WizardStep::NeedsAnInput);
}

#[test]
fn a_selected_row_with_an_unreadable_rate_is_refused_with_its_message() {
    let devices = model(vec![row("Scarlett", true, "forty-eight thousand", "128")]);
    match next_step(&devices) {
        WizardStep::Invalid(message) => assert!(!message.is_empty()),
        other => panic!("expected the bad row to be refused, got {other:?}"),
    }
}

#[test]
fn an_unreadable_row_that_is_not_selected_does_not_block_the_wizard() {
    let devices = model(vec![
        row("Scarlett", true, "48000", "128"),
        row("Broken", false, "nonsense", "nonsense"),
    ]);
    assert_eq!(
        next_step(&devices),
        WizardStep::Advance,
        "only the rows the user actually picked are read"
    );
}

#[test]
fn several_selected_inputs_advance_as_one_does() {
    let devices = model(vec![
        row("Scarlett", true, "48000", "128"),
        row("Built-in", true, "44100", "256"),
    ]);
    assert_eq!(next_step(&devices), WizardStep::Advance);
}
