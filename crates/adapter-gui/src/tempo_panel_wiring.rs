//! Responsibility: wires the tempo panel's preset-tempo actions to the bus.
//!
//! Dispatch only: `SetRigPresetBpm` stores (or clears) the tempo of the
//! chain's active rig preset — the same command an MCP client sends. The
//! screen then refreshes through the drain every other rig edit uses, so the
//! chip's accent and the panel's preset line follow. The panel's tempo and
//! lock controls belong to the metronome and are wired with it.

use std::rc::Rc;

use application::command::{ChainId, Command, SelectionCommand};
use slint::ComponentHandle;

use crate::chain_rig_nav_wiring::{apply_events_to_ui, ChainRigNavCtx};
use crate::helpers::set_status_error;
use crate::{AppWindow, TempoPanel};

/// Connect the panel's store and clear on the main window.
pub(crate) fn wire(window: &AppWindow, ctx: &Rc<ChainRigNavCtx>) {
    {
        let weak = window.as_weak();
        let ctx = ctx.clone();
        window
            .global::<TempoPanel>()
            .on_store_preset_bpm(move |chain_index, bpm| {
                if let Some(window) = weak.upgrade() {
                    set_preset_bpm(&window, &ctx, chain_index, Some(bpm));
                }
            });
    }
    {
        let weak = window.as_weak();
        let ctx = ctx.clone();
        window
            .global::<TempoPanel>()
            .on_clear_preset_bpm(move |chain_index| {
                if let Some(window) = weak.upgrade() {
                    set_preset_bpm(&window, &ctx, chain_index, None);
                }
            });
    }
}

/// Dispatch the store/clear for the chain at `chain_index`, then refresh.
fn set_preset_bpm(window: &AppWindow, ctx: &ChainRigNavCtx, chain_index: i32, bpm: Option<f32>) {
    let events = {
        let session_borrow = ctx.project_session.borrow();
        let Some(session) = session_borrow.as_ref() else {
            return;
        };
        let ids: Vec<ChainId> = session
            .project
            .borrow()
            .chains
            .iter()
            .map(|chain| chain.id.clone())
            .collect();
        let Some(cmd) = preset_bpm_command(&ids, chain_index, bpm) else {
            return;
        };
        match session.dispatcher.dispatch(cmd) {
            Ok(events) => events,
            Err(e) => {
                set_status_error(window, &ctx.toast_timer, &e.to_string());
                return;
            }
        }
    };
    apply_events_to_ui(window, ctx, &events);
}

/// The command that stores (`Some`) or clears (`None`) the preset tempo of
/// the chain at row `chain_index`; `None` when that row is gone.
pub(crate) fn preset_bpm_command(
    chain_ids: &[ChainId],
    chain_index: i32,
    bpm: Option<f32>,
) -> Option<Command> {
    let chain = chain_ids.get(usize::try_from(chain_index).ok()?)?.clone();
    Some(Command::Selection(SelectionCommand::SetRigPresetBpm {
        chain,
        bpm,
    }))
}

#[cfg(test)]
#[path = "tempo_panel_wiring_tests.rs"]
mod tests;
