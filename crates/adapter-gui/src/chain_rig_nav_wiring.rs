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

use crate::chain_rig_nav::rig_nav_rows;
use crate::helpers::set_status_error;
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
        })
        .collect();
    window.set_chain_rig_nav(ModelRc::new(VecModel::from(items)));
}

fn apply_switch(
    window: &AppWindow,
    ctx: &ChainRigNavCtx,
    chain_index: i32,
    preset_slot: Option<usize>,
    scene: Option<usize>,
) {
    let mut session_borrow = ctx.project_session.borrow_mut();
    let Some(session) = session_borrow.as_mut() else {
        return;
    };
    let Some(rig) = session.rig.clone() else {
        return;
    };

    // chain index → synthetic chain id → rig input name.
    let (chain_id, input) = {
        let proj = session.project.borrow();
        let Some(chain) = proj.chains.get(chain_index as usize) else {
            return;
        };
        let Some(name) = chain.id.0.strip_prefix("rig:") else {
            return;
        };
        (chain.id.clone(), name.to_string())
    };

    let rebuilt = engine::rig_runtime::switch_and_project_input(
        &mut rig.borrow_mut(),
        &input,
        preset_slot,
        scene,
    );
    let Some(new_chain) = rebuilt else {
        set_status_error(
            window,
            &ctx.toast_timer,
            &rust_i18n::t!("error-invalid-chain"),
        );
        return;
    };

    // Swap the rebuilt chain in place (same id ⇒ index/alignment kept).
    {
        let mut proj = session.project.borrow_mut();
        if let Some(slot) = proj.chains.get_mut(chain_index as usize) {
            let was_enabled = slot.enabled;
            *slot = new_chain;
            slot.enabled = was_enabled;
        }
    }

    if let Err(e) = sync_live_chain_runtime(&ctx.project_runtime, session, &chain_id) {
        set_status_error(window, &ctx.toast_timer, &e.to_string());
    }
    replace_project_chains(
        &ctx.project_chains,
        &session.project.borrow(),
        &ctx.input_chain_devices.borrow(),
        &ctx.output_chain_devices.borrow(),
    );
    refresh_chain_rig_nav(window, session);
}

pub(crate) fn wire(window: &AppWindow, ctx: ChainRigNavCtx) {
    let ctx = Rc::new(ctx);
    {
        let weak = window.as_weak();
        let ctx = ctx.clone();
        window.on_switch_chain_preset(move |chain_index, slot| {
            if let Some(window) = weak.upgrade() {
                apply_switch(&window, &ctx, chain_index, Some(slot as usize), None);
            }
        });
    }
    {
        let weak = window.as_weak();
        let ctx = ctx.clone();
        window.on_switch_chain_scene(move |chain_index, scene| {
            if let Some(window) = weak.upgrade() {
                apply_switch(&window, &ctx, chain_index, None, Some(scene as usize));
            }
        });
    }
}
