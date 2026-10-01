//! Responsibility: wires the DI panel's delete-take to the bus.
//! The trash on a saved take row of the DI panel.
//!
//! Dispatch only: `DeleteLooperTake` removes the file and unloads it from any
//! chain that holds it (the same command an MCP client sends). This module
//! drops the row from the panel that is open, and only once the core agreed.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{Command, LooperCommand};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel, Weak};

use crate::di_panel_take_removal::{without_take, DiPanelList};
use crate::helpers::set_status_error;
use crate::state::ProjectSession;
use crate::{AppWindow, CompactChainViewWindow, DiPanel};

type Session = Rc<RefCell<Option<ProjectSession>>>;

/// The main window's DI panel.
pub(crate) fn wire_main(window: &AppWindow, session: &Session, toast_timer: &Rc<slint::Timer>) {
    let session = session.clone();
    let toast_timer = toast_timer.clone();
    let window_weak = window.as_weak();
    window.global::<DiPanel>().on_delete_take(move |label| {
        let Some(window) = window_weak.upgrade() else {
            return;
        };
        if let Err(err) = delete_take(&window.global::<DiPanel>(), &session, &label) {
            set_status_error(&window, &toast_timer, &err);
        }
    });
}

/// A compact chain window's own DI panel; a refusal is the main window's toast.
pub(crate) fn wire_compact(
    compact_win: &CompactChainViewWindow,
    session: &Session,
    main_weak: Weak<AppWindow>,
    toast_timer: Rc<slint::Timer>,
) {
    let session = session.clone();
    let compact_weak = compact_win.as_weak();
    compact_win
        .global::<DiPanel>()
        .on_delete_take(move |label| {
            let Some(compact_win) = compact_weak.upgrade() else {
                return;
            };
            if let Err(err) = delete_take(&compact_win.global::<DiPanel>(), &session, &label) {
                if let Some(main_win) = main_weak.upgrade() {
                    set_status_error(&main_win, &toast_timer, &err);
                }
            }
        });
}

/// Dispatch the delete; on success drop the row from the open `panel`.
fn delete_take(panel: &DiPanel<'_>, session: &Session, label: &str) -> Result<(), String> {
    {
        let borrowed = session.borrow();
        let Some(session) = borrowed.as_ref() else {
            return Ok(());
        };
        session
            .dispatcher
            .dispatch(Command::Looper(LooperCommand::DeleteLooperTake {
                name: label.to_string(),
            }))
            .map_err(|e| e.to_string())?;
    }
    let after = without_take(
        DiPanelList {
            sources: panel.get_sources().iter().map(|s| s.to_string()).collect(),
            take_rows: panel.get_take_rows().iter().collect(),
            selected: panel.get_selected_index(),
            playing: panel.get_playing(),
        },
        label,
    );
    panel.set_sources(ModelRc::new(VecModel::from(
        after
            .sources
            .into_iter()
            .map(SharedString::from)
            .collect::<Vec<_>>(),
    )));
    panel.set_take_rows(ModelRc::new(VecModel::from(after.take_rows)));
    panel.set_selected_index(after.selected);
    panel.set_playing(after.playing);
    Ok(())
}

#[cfg(test)]
#[path = "di_take_delete_wiring_tests.rs"]
mod tests;
