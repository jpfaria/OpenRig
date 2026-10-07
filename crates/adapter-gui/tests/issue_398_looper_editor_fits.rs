//! #398 — the waveform editor's card clips what it holds, so a button row
//! wider than the card would lose its last buttons: hidden and unclickable.
//! Long labels (a long translation, or none at all) must shrink the text
//! buttons instead, keeping every button inside the card.
//!
//! Own test binary: it switches the process to the untranslated catalog,
//! whose labels are the long message ids.

use adapter_gui::LooperEditorHarness;
use i_slint_backend_testing::ElementHandle;
use slint::ComponentHandle;

#[test]
fn long_labels_keep_every_editor_button_inside_the_card() {
    i_slint_backend_testing::init_no_event_loop();
    let w = LooperEditorHarness::new().unwrap();
    slint::select_bundled_translation("").unwrap();
    w.show().unwrap();

    // fit, trim, crop, cut, play, undo, redo, close
    let areas: Vec<_> = ElementHandle::find_by_element_id(&w, "EditorButton::area").collect();
    assert_eq!(areas.len(), 8, "every button is on screen");

    // Close sits on the bar, 12 px in from the card's right edge.
    let close = &areas[7];
    let card_right = close.absolute_position().x + close.size().width + 12.0;
    for (i, a) in areas.iter().enumerate() {
        let right = a.absolute_position().x + a.size().width;
        assert!(
            right <= card_right,
            "button {i} ends at {right}, past the card's edge at {card_right}"
        );
        assert!(a.size().width >= 28.0, "button {i} keeps a clickable width");
    }
}
