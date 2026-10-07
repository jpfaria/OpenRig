//! Responsibility: wires the first-run setup wizard's buttons.

use std::cell::RefCell;
use std::rc::Rc;

use infra_filesystem::AppConfig;
use slint::ComponentHandle;

use crate::setup_wizard_step::{
    after_audio_saved, enters_audio_step, leaves_io_step, on_back, on_next, on_skip, SetupStep,
    WizardMove,
};
use crate::state::ProjectSession;
use crate::{AppWindow, SetupWizardBridge};

#[path = "setup_wizard_io_persist.rs"]
mod io_persist;

pub(crate) struct SetupWizardCtx {
    pub(crate) project_session: Rc<RefCell<Option<ProjectSession>>>,
    pub(crate) app_config: Rc<RefCell<AppConfig>>,
}

pub(crate) fn wire(window: &AppWindow, ctx: SetupWizardCtx) {
    let ctx = Rc::new(ctx);
    let press = |decide: fn(i32) -> WizardMove| {
        let weak = window.as_weak();
        let ctx = Rc::clone(&ctx);
        move || {
            if let Some(window) = weak.upgrade() {
                let step = window.global::<SetupWizardBridge>().get_step();
                apply(&window, &ctx, step, decide(step));
            }
        }
    };
    let bridge = window.global::<SetupWizardBridge>();
    bridge.on_next(press(on_next));
    bridge.on_back(press(on_back));
    bridge.on_skip(press(on_skip));
}

fn apply(window: &AppWindow, ctx: &SetupWizardCtx, step: i32, mv: WizardMove) {
    if leaves_io_step(step, &mv) && ctx.project_session.borrow().is_none() {
        io_persist::persist(ctx.app_config.borrow().io_bindings.clone());
    }
    if enters_audio_step(&mv) {
        crate::device_refresh_apply::refresh_now(false);
    }
    match mv {
        WizardMove::Goto(next) => show_step(window, next),
        WizardMove::SaveAudio => window.invoke_save_audio_settings(),
        WizardMove::Finish => window.set_show_setup_wizard(false),
        WizardMove::Stay => {}
    }
}

/// Show `step`, with the footer buttons that step allows.
pub(crate) fn show_step(window: &AppWindow, step: i32) {
    let Some(current) = SetupStep::from_index(step) else {
        return;
    };
    let bridge = window.global::<SetupWizardBridge>();
    bridge.set_step(step);
    bridge.set_step_skippable(current.skippable());
    bridge.set_step_last(current.is_last());
}

/// Called after the audio devices were saved: the wizard moves on only when
/// the save came from its own audio step.
pub(crate) fn audio_saved(window: &AppWindow) {
    let step = window.global::<SetupWizardBridge>().get_step();
    if let Some(next) = after_audio_saved(window.get_show_setup_wizard(), step) {
        show_step(window, next);
    }
}

#[cfg(test)]
#[path = "setup_wizard_shell_tests.rs"]
mod shell_tests;

#[cfg(test)]
#[path = "setup_wizard_ui_tests.rs"]
mod ui_tests;

#[cfg(test)]
#[path = "setup_wizard_wiring_tests.rs"]
mod wiring_tests;
