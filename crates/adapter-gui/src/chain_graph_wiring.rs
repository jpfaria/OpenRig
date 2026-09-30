//! Responsibility: connects the chain graph's gestures to their handlers.
//!
//! #328 (spec §5.1). Thin by design: each `ChainGraphBridge` callback resolves
//! its chain row, hands the node id or anchor to the module that owns the rule
//! (`graph_click`, `graph_anchor`, `graph_gesture_actions`, `chain_graph_drop`,
//! `chain_graph_drag`) and shows a refusal as a toast. Wired once from
//! `desktop_app_block_wiring::wire_all`.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, Global, Timer, VecModel};

use domain::ids::BlockId;
use domain::AudioDeviceDescriptor;
use project::chain::Chain;

use crate::chain_block_lists::side_index;
use crate::graph_anchor::{insert_target, parse_anchor, InsertTarget};
use crate::graph_click::{click_action, ClickAction};
use crate::graph_gesture_actions::{
    drop_node, remove_node, remove_split, toggle_node, GestureError, RemoveOutcome, RowsTarget,
};
use crate::helpers::set_status_error;
use crate::state::ProjectSession;
use crate::{AppWindow, ChainGraphBridge, ChainGraphOverlayState, ProjectChainItem};

pub(crate) struct ChainGraphWiringCtx {
    pub(crate) project_session: Rc<RefCell<Option<ProjectSession>>>,
    pub(crate) project_chains: Rc<VecModel<ProjectChainItem>>,
    pub(crate) input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub(crate) output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub(crate) toast_timer: Rc<Timer>,
}

pub(crate) fn wire(window: &AppWindow, ctx: ChainGraphWiringCtx) {
    let ctx = Rc::new(ctx);
    wire_clicks(window, &ctx);
    wire_edits(window, &ctx);
    wire_drags(window, &ctx);
    wire_split_removal(window, &ctx);
}

/// A snapshot of the chain at a row, released before any flow re-borrows it.
pub(crate) fn chain_at(
    session: &Rc<RefCell<Option<ProjectSession>>>,
    chain_index: i32,
) -> Option<Chain> {
    let borrowed = session.borrow();
    let session = borrowed.as_ref()?;
    let project = session.project.borrow();
    project
        .chains
        .get(usize::try_from(chain_index).ok()?)
        .cloned()
}

fn with_rows<T>(ctx: &ChainGraphWiringCtx, f: impl FnOnce(&RowsTarget<'_>) -> T) -> T {
    let inputs = ctx.input_chain_devices.borrow();
    let outputs = ctx.output_chain_devices.borrow();
    f(&RowsTarget {
        model: &ctx.project_chains,
        inputs: &inputs,
        outputs: &outputs,
    })
}

fn report(window: &AppWindow, ctx: &ChainGraphWiringCtx, result: Result<(), GestureError>) {
    match result {
        Ok(()) | Err(GestureError::NoMove) => {}
        Err(GestureError::Failed(err)) => set_status_error(
            window,
            &ctx.toast_timer,
            &rust_i18n::t!("error-graph-action", err = err),
        ),
        Err(other) => log::warn!("[chain-graph] gesture ignored: {other:?}"),
    }
}

fn wire_clicks(window: &AppWindow, ctx: &Rc<ChainGraphWiringCtx>) {
    let (weak, ctx) = (window.as_weak(), ctx.clone());
    ChainGraphBridge::get(window).on_node_clicked(move |chain_index, node_id| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let Some(action) = chain_at(&ctx.project_session, chain_index)
            .and_then(|chain| click_action(&chain, &node_id))
        else {
            return;
        };
        match action {
            ClickAction::SelectRow(row) => {
                window.invoke_select_chain_block(chain_index, row as i32)
            }
            ClickAction::OpenPathBlock { path, index } => ChainGraphBridge::get(&window)
                .invoke_open_path_block(
                    chain_index,
                    path.split.0.as_str().into(),
                    side_index(&path.side),
                    index as i32,
                ),
            ClickAction::OpenSplitEditor { split } => ChainGraphOverlayState::get(&window)
                .invoke_open_split_editor(chain_index, split.0.as_str().into(), 0),
            ClickAction::OpenMixerEditor { split } => ChainGraphOverlayState::get(&window)
                .invoke_open_split_editor(chain_index, split.0.as_str().into(), 1),
            ClickAction::OpenChecklist => {
                ChainGraphOverlayState::get(&window).invoke_open_checklist(chain_index, node_id)
            }
        }
    });
}

fn wire_edits(window: &AppWindow, ctx: &Rc<ChainGraphWiringCtx>) {
    let bridge = ChainGraphBridge::get(window);
    {
        let (weak, ctx) = (window.as_weak(), ctx.clone());
        bridge.on_add_requested(move |chain_index, anchor| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let place = chain_at(&ctx.project_session, chain_index).and_then(|chain| {
                parse_anchor(&anchor).and_then(|slot| insert_target(&chain, &slot))
            });
            match place {
                Some(InsertTarget {
                    position,
                    path: None,
                }) => window.invoke_start_block_insert(chain_index, position as i32),
                Some(InsertTarget {
                    position,
                    path: Some(path),
                }) => ChainGraphBridge::get(&window).invoke_start_path_insert(
                    chain_index,
                    path.split.0.as_str().into(),
                    side_index(&path.side),
                    position as i32,
                ),
                None => log::warn!("[chain-graph] no insert place for anchor {anchor}"),
            }
        });
    }
    {
        let (weak, ctx) = (window.as_weak(), ctx.clone());
        bridge.on_bypass_toggled(move |chain_index, node_id| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let result = with_rows(&ctx, |rows| {
                toggle_node(&ctx.project_session, chain_index as usize, &node_id, rows)
            });
            report(&window, &ctx, result);
        });
    }
    {
        let (weak, ctx) = (window.as_weak(), ctx.clone());
        bridge.on_remove_requested(move |chain_index, node_id| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let outcome = with_rows(&ctx, |rows| {
                remove_node(&ctx.project_session, chain_index as usize, &node_id, rows)
            });
            match outcome {
                Ok(RemoveOutcome::ConfirmSplit { name, split }) => {
                    let overlay = ChainGraphOverlayState::get(&window);
                    overlay.set_confirm_remove_split_chain_index(chain_index);
                    overlay.set_confirm_remove_split_id(split.0.as_str().into());
                    overlay.set_confirm_remove_split_name(name.into());
                    overlay.set_confirm_remove_split_open(true);
                }
                other => report(&window, &ctx, other.map(|_| ())),
            }
        });
    }
}

fn wire_drags(window: &AppWindow, ctx: &Rc<ChainGraphWiringCtx>) {
    let bridge = ChainGraphBridge::get(window);
    {
        let ctx = ctx.clone();
        bridge.on_node_dragged(move |chain_index, node_id, x, y| {
            crate::chain_graph_drag::move_node(
                &ctx.project_chains,
                chain_index as usize,
                &node_id,
                x,
                y,
            );
        });
    }
    {
        let ctx = ctx.clone();
        bridge.on_node_drag_ended(move |chain_index, _node_id| {
            if let Some(chain) = chain_at(&ctx.project_session, chain_index) {
                crate::chain_graph_drag::settle_nodes(
                    &ctx.project_chains,
                    chain_index as usize,
                    &chain,
                );
            }
        });
    }
    {
        // Asked on every drag move so the target "+" lights up (Part 5).
        let ctx = ctx.clone();
        bridge.on_resolve_drop_anchor(move |chain_index, node_id, x, y| {
            chain_at(&ctx.project_session, chain_index)
                .map(|chain| crate::chain_graph_drop::drop_anchor_id(&chain, &node_id, x, y))
                .unwrap_or_default()
                .into()
        });
    }
    {
        let (weak, ctx) = (window.as_weak(), ctx.clone());
        bridge.on_node_dropped(move |chain_index, node_id, anchor| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let result = with_rows(&ctx, |rows| {
                drop_node(
                    &ctx.project_session,
                    chain_index as usize,
                    &node_id,
                    &anchor,
                    rows,
                )
            });
            report(&window, &ctx, result);
        });
    }
}

fn wire_split_removal(window: &AppWindow, ctx: &Rc<ChainGraphWiringCtx>) {
    let (weak, ctx) = (window.as_weak(), ctx.clone());
    ChainGraphOverlayState::get(window).on_confirm_remove_split(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let overlay = ChainGraphOverlayState::get(&window);
        let chain_index = overlay.get_confirm_remove_split_chain_index();
        let split_id = BlockId(overlay.get_confirm_remove_split_id().to_string());
        let result = with_rows(&ctx, |rows| {
            remove_split(&ctx.project_session, chain_index as usize, &split_id, rows)
        });
        report(&window, &ctx, result);
    });
}
