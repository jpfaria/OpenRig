//! Wiring for the first-run audio-setup wizard navigation callbacks.
//!
//! Two callbacks: `on_go_to_output_step` (validates that at least one input
//! is selected before advancing to the output step) and `on_go_to_input_step`
//! (returns to the input step and clears any toast).

use std::rc::Rc;

use slint::{ComponentHandle, Timer, VecModel};

use crate::audio_devices::selected_device_settings;
use crate::helpers::{clear_status, set_status_error, set_status_warning};
use crate::{AppWindow, DeviceSelectionItem};

pub(crate) struct AudioWizardCtx {
    pub input_devices: Rc<VecModel<DeviceSelectionItem>>,
    pub toast_timer: Rc<Timer>,
}

pub(crate) fn wire(window: &AppWindow, ctx: AudioWizardCtx) {
    let AudioWizardCtx {
        input_devices,
        toast_timer,
    } = ctx;

    {
        let weak_window = window.as_weak();
        let input_devices = input_devices.clone();
        let toast_timer = toast_timer.clone();
        window.on_go_to_output_step(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            match selected_device_settings(&input_devices, "input") {
                Ok(devices) if !devices.is_empty() => {
                    clear_status(&window, &toast_timer);
                    window.set_wizard_step(1);
                }
                Ok(_) => {
                    set_status_warning(
                        &window,
                        &toast_timer,
                        "Selecione pelo menos um input antes de continuar.",
                    );
                }
                Err(error) => {
                    set_status_error(&window, &toast_timer, &error.to_string());
                }
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let toast_timer = toast_timer.clone();
        window.on_go_to_input_step(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            clear_status(&window, &toast_timer);
            window.set_wizard_step(0);
        });
    }
}
