//! Responsibility: drives the endpoint checklist overlay.
//!
//! #328 (spec §5.3). Opening lists the node's endpoints (`endpoint_rows`); a
//! toggle goes through `set_endpoint_enabled`; the rows are refreshed in place
//! from the chain afterwards, so an unchecked endpoint stays listed, unchecked.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, Global, Model, ModelRc, Timer, VecModel};

use domain::AudioDeviceDescriptor;
use project::block::path_letter;
use project::endpoint_disables::EndpointNode;

use crate::chain_graph_ids::{resolve_node, NodeRef};
use crate::chain_graph_wiring::chain_at;
use crate::endpoint_checklist_items::{endpoint_rows, EndpointRow};
use crate::endpoint_toggle::set_endpoint_enabled;
use crate::graph_gesture_actions::{GestureError, RowsTarget};
use crate::helpers::set_status_error;
use crate::state::ProjectSession;
use crate::{AppWindow, ChainGraphOverlayState, ChannelOptionItem, ProjectChainItem};

pub(crate) struct EndpointChecklistWiringCtx {
    pub(crate) project_session: Rc<RefCell<Option<ProjectSession>>>,
    pub(crate) project_chains: Rc<VecModel<ProjectChainItem>>,
    pub(crate) input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub(crate) output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub(crate) toast_timer: Rc<Timer>,
}

struct Checklist {
    ctx: EndpointChecklistWiringCtx,
    items: Rc<VecModel<ChannelOptionItem>>,
}

pub(crate) fn checklist_items(rows: &[EndpointRow]) -> Vec<ChannelOptionItem> {
    rows.iter()
        .enumerate()
        .map(|(index, row)| ChannelOptionItem {
            index: index as i32,
            label: row.label.as_str().into(),
            selected: row.enabled,
            available: true,
        })
        .collect()
}

pub(crate) fn checklist_title(node: &EndpointNode) -> String {
    match node {
        EndpointNode::Input => rust_i18n::t!("title-endpoints-input"),
        EndpointNode::Output => rust_i18n::t!("title-endpoints-output"),
        EndpointNode::PathOutput(leaf) => {
            rust_i18n::t!("title-endpoints-path", path = path_letter(leaf.path))
        }
    }
    .to_string()
}

/// The node's rows as they are now, or `None` when the node is gone.
fn current_rows(
    checklist: &Checklist,
    chain_index: i32,
    node_id: &str,
) -> Option<(EndpointNode, Vec<EndpointRow>)> {
    let chain = chain_at(&checklist.ctx.project_session, chain_index)?;
    let Some(NodeRef::Endpoints(node)) = resolve_node(&chain, node_id) else {
        return None;
    };
    let borrowed = checklist.ctx.project_session.borrow();
    let registry = borrowed.as_ref()?.io_bindings.borrow().clone();
    let rows = endpoint_rows(&chain, &registry, &node);
    Some((node, rows))
}

fn toggle(
    checklist: &Checklist,
    chain_index: i32,
    node_id: &str,
    row: i32,
    enabled: bool,
) -> Result<(), GestureError> {
    let inputs = checklist.ctx.input_chain_devices.borrow();
    let outputs = checklist.ctx.output_chain_devices.borrow();
    let rows = RowsTarget {
        model: &checklist.ctx.project_chains,
        inputs: &inputs,
        outputs: &outputs,
    };
    set_endpoint_enabled(
        &checklist.ctx.project_session,
        usize::try_from(chain_index).map_err(|_| GestureError::NoSuchChain)?,
        node_id,
        usize::try_from(row).map_err(|_| GestureError::NotApplicable)?,
        enabled,
        &rows,
    )
}

pub(crate) fn wire(window: &AppWindow, ctx: EndpointChecklistWiringCtx) {
    let checklist = Rc::new(Checklist {
        ctx,
        items: Rc::new(VecModel::default()),
    });
    let state = ChainGraphOverlayState::get(window);
    state.set_checklist_items(ModelRc::from(checklist.items.clone()));
    {
        let (weak, checklist) = (window.as_weak(), checklist.clone());
        state.on_open_checklist(move |chain_index, node_id| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let Some((node, rows)) = current_rows(&checklist, chain_index, &node_id) else {
                return;
            };
            checklist.items.set_vec(checklist_items(&rows));
            let state = ChainGraphOverlayState::get(&window);
            state.set_checklist_chain_index(chain_index);
            state.set_checklist_node(node_id);
            state.set_checklist_title(checklist_title(&node).into());
            state.set_checklist_open(true);
        });
    }
    {
        let (weak, checklist) = (window.as_weak(), checklist.clone());
        state.on_checklist_toggled(move |chain_index, node_id, row, enabled| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            match toggle(&checklist, chain_index, &node_id, row, enabled) {
                Ok(()) => {}
                Err(GestureError::Failed(err)) => set_status_error(
                    &window,
                    &checklist.ctx.toast_timer,
                    &rust_i18n::t!("error-graph-action", err = err),
                ),
                Err(other) => log::warn!("[endpoint-checklist] toggle ignored: {other:?}"),
            }
            // Same rows in the same order: update in place, the list never jumps.
            if let Some((_, fresh)) = current_rows(&checklist, chain_index, &node_id) {
                for (index, item) in checklist_items(&fresh).into_iter().enumerate() {
                    if index < checklist.items.row_count() {
                        checklist.items.set_row_data(index, item);
                    }
                }
            }
        });
    }
}
