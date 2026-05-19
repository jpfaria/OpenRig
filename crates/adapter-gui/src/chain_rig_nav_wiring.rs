//! Wiring for the per-chain rig preset/scene selectors (#436 #1) on the
//! legacy chains screen. A switch mutates the retained `RigProject`,
//! re-projects that one input's synthetic chain, swaps it into the live
//! `Project` and re-syncs the runtime through the proven path (zero new
//! audio code). Then both the chains model and the rig-nav model are
//! refreshed. Disabled chains just update state (takes effect on enable).

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, ModelRc, Timer, VecModel};

use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};

use application::command::{ChainId, Command, RigNavKind};
use application::dispatcher::CommandDispatcher;
use application::event::Event;

use crate::chain_rig_nav::rig_nav_rows;
use crate::helpers::set_status_error;
use crate::project_ops::sync_project_dirty;
use crate::project_view::replace_project_chains;
use crate::state::ProjectSession;
use crate::sync_live_chain_runtime;
use crate::{AppWindow, ChainRigNav, ProjectChainItem};

pub(crate) struct ChainRigNavCtx {
    pub project_session: Rc<RefCell<Option<ProjectSession>>>,
    pub project_chains: Rc<VecModel<ProjectChainItem>>,
    pub project_runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
    pub input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub toast_timer: Rc<Timer>,
    // Marking the project dirty / autosaving after a switch — same
    // path every other edit wiring uses. Without this a preset/scene
    // switch or add was in-memory only ("salvei e não aconteceu nada").
    pub saved_project_snapshot: Rc<RefCell<Option<String>>>,
    pub project_dirty: Rc<RefCell<bool>>,
    pub auto_save: bool,
}

/// Refresh the `chain-rig-nav` model from the current session (no-op if
/// none). Single entry point for every project-open path — avoids the
/// borrow-dance being copy-pasted at each call site.
pub(crate) fn refresh_from_session(
    window: &AppWindow,
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
) {
    if let Some(session) = project_session.borrow().as_ref() {
        refresh_chain_rig_nav(window, session);
    }
}

/// Rebuild the `chain-rig-nav` model from the session's retained rig,
/// aligned 1:1 with the current synthetic chains. No rig ⇒ empty model
/// (every chain shows no selector).
pub(crate) fn refresh_chain_rig_nav(window: &AppWindow, session: &ProjectSession) {
    let rows = match &session.rig {
        Some(rig) => rig_nav_rows(&rig.borrow(), &session.project.borrow()),
        None => Vec::new(),
    };
    let items: Vec<ChainRigNav> = rows
        .into_iter()
        .map(|r| ChainRigNav {
            has_rig: !r.input.is_empty(),
            preset_labels: ModelRc::new(VecModel::from(
                r.preset_labels
                    .into_iter()
                    .map(slint::SharedString::from)
                    .collect::<Vec<_>>(),
            )),
            active_preset_index: r.active_index as i32,
            scene: r.scene as i32,
            scene_count: r.scene_count as i32,
        })
        .collect();
    window.set_chain_rig_nav(ModelRc::new(VecModel::from(items)));
}

/// #436: the GUI carries NO rig business logic. It resolves the
/// synthetic chain id, dispatches `Command::ApplyRigNav` (the
/// dispatcher owns the rig and does capture→apply→re-project→swap),
/// then refreshes the UI and the live runtime from the shared
/// `Project`. Non-rig / missing chain ⇒ no-op.
fn reproject(window: &AppWindow, ctx: &ChainRigNavCtx, chain_index: i32, kind: RigNavKind) {
    let events = {
        let mut session_borrow = ctx.project_session.borrow_mut();
        let Some(session) = session_borrow.as_mut() else {
            return;
        };

        let chain_id = {
            let proj = session.project.borrow();
            let Some(chain) = proj.chains.get(chain_index as usize) else {
                return;
            };
            if chain.id.0.strip_prefix("rig:").is_none() {
                return;
            }
            chain.id.clone()
        };

        // All business logic lives behind the Command now.
        match session.dispatcher.dispatch(Command::ApplyRigNav {
            chain: chain_id,
            kind,
        }) {
            Ok(events) => events,
            Err(_) => {
                set_status_error(
                    window,
                    &ctx.toast_timer,
                    &rust_i18n::t!("error-invalid-chain"),
                );
                return;
            }
        }
    };

    // Same screen + live-runtime refresh the MIDI/MCP drain uses — one
    // path, so a footswitch moves the screen exactly like a click.
    apply_events_to_ui(window, ctx, &events);
}

/// One screen + live-runtime refresh, driven by the events a drained
/// command produced. Both the GUI click path (`reproject`) and the
/// MIDI/MCP drain timers call this, so a footswitch moves the screen
/// exactly like the mouse. Empty events ⇒ no-op.
pub(crate) fn apply_events_to_ui(window: &AppWindow, ctx: &ChainRigNavCtx, events: &[Event]) {
    if events.is_empty() {
        return;
    }
    let session_borrow = ctx.project_session.borrow();
    let Some(session) = session_borrow.as_ref() else {
        return;
    };

    // Re-sync the live runtime for each chain a command touched (once).
    let mut synced: Vec<ChainId> = Vec::new();
    for chain_id in events.iter().filter_map(Event::chain) {
        if synced.iter().any(|c| c == chain_id) {
            continue;
        }
        synced.push(chain_id.clone());
        if let Err(e) = sync_live_chain_runtime(&ctx.project_runtime, session, chain_id) {
            set_status_error(window, &ctx.toast_timer, &e.to_string());
        }
    }
    replace_project_chains(
        &ctx.project_chains,
        &session.project.borrow(),
        &ctx.input_chain_devices.borrow(),
        &ctx.output_chain_devices.borrow(),
    );
    refresh_chain_rig_nav(window, session);
    sync_project_dirty(
        window,
        session,
        &ctx.saved_project_snapshot,
        &ctx.project_dirty,
        ctx.auto_save,
    );
}

pub(crate) fn wire(window: &AppWindow, ctx: ChainRigNavCtx) {
    let ctx = Rc::new(ctx);
    {
        let weak = window.as_weak();
        let ctx = ctx.clone();
        window.on_switch_chain_preset(move |chain_index, slot| {
            if let Some(window) = weak.upgrade() {
                reproject(&window, &ctx, chain_index, RigNavKind::Preset(slot));
            }
        });
    }
    {
        let weak = window.as_weak();
        let ctx = ctx.clone();
        window.on_switch_chain_scene(move |chain_index, scene| {
            if let Some(window) = weak.upgrade() {
                reproject(&window, &ctx, chain_index, RigNavKind::Scene(scene));
            }
        });
    }
}
