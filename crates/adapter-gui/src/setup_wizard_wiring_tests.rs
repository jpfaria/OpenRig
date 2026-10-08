//! The wizard buttons, pressed on the real window, walk the steps the pure
//! rules decide and close the wizard on the last one.

use std::cell::RefCell;
use std::rc::Rc;

use super::{audio_saved, show_step, wire, SetupWizardCtx};
use slint::ComponentHandle;

use crate::{AppWindow, SetupWizardBridge};

fn bridge(window: &AppWindow) -> SetupWizardBridge<'_> {
    window.global::<SetupWizardBridge>()
}

fn wired(step: i32) -> AppWindow {
    i_slint_backend_testing::init_no_event_loop();
    let window = AppWindow::new().expect("window");
    wire(
        &window,
        SetupWizardCtx {
            project_session: Rc::new(RefCell::new(None)),
            app_config: Rc::new(RefCell::new(infra_filesystem::AppConfig::default())),
        },
    );
    window.set_show_setup_wizard(true);
    show_step(&window, step);
    window
}

#[test]
fn next_on_the_language_step_shows_the_audio_step() {
    let window = wired(0);
    bridge(&window).invoke_next();
    assert_eq!(bridge(&window).get_step(), 1);
    assert!(!bridge(&window).get_step_skippable());
}

#[test]
fn next_on_the_io_step_shows_the_last_step() {
    let window = wired(2);
    bridge(&window).invoke_next();
    assert_eq!(bridge(&window).get_step(), 3);
    assert!(bridge(&window).get_step_last());
    assert!(bridge(&window).get_step_skippable());
}

#[test]
fn back_on_the_io_step_shows_the_audio_step() {
    let window = wired(2);
    bridge(&window).invoke_back();
    assert_eq!(bridge(&window).get_step(), 1);
}

#[test]
fn next_on_the_last_step_closes_the_wizard() {
    let window = wired(3);
    bridge(&window).invoke_next();
    assert!(!window.get_show_setup_wizard());
}

#[test]
fn a_successful_audio_save_moves_the_wizard_to_the_io_step() {
    let window = wired(1);
    audio_saved(&window);
    assert_eq!(bridge(&window).get_step(), 2);
}

#[test]
fn an_audio_save_outside_the_wizard_moves_nothing() {
    let window = wired(1);
    window.set_show_setup_wizard(false);
    audio_saved(&window);
    assert_eq!(bridge(&window).get_step(), 1);
}
