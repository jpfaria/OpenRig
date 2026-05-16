//! #453 mount — wires the `BankSceneNavigator` Slint component to the pure
//! navigation core. The live GUI only has the legacy `Project`; we derive a
//! `RigProject` from it via the #450 migration, drive `BankSceneState`, and
//! render the selected input's preset chain through the #435 `GraphView`.
//!
//! Engine-level audio switching (executing `BankSceneEffect`) needs the
//! `RigRuntime` bound to the live audio backend — that is the remaining
//! umbrella integration; here the effects update the in-memory rig so the
//! UI is fully interactive and the GraphView reflects the selection.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, ModelRc, VecModel};

use project::block::AudioBlockKind;
use project::migrate::migrate_legacy_project;
use project::rig::RigProject;

use crate::bank_scene_render::render;
use crate::bank_scene_session::{BankSceneEffect, BankSceneEvent, BankSceneState};
use crate::graph_view_model::{
    self as gvm, default_palette, linear_chain_layout, BlockBlueprint, ChainStage, GridMetrics,
    NodeCategory,
};
use crate::state::ProjectSession;
use crate::{AppWindow, BankNavItem, GraphEdgeGeometry, GraphNode};
use infra_cpal::ProjectRuntimeController;
use project::block::AudioBlock;

pub(crate) struct BankSceneNavCtx {
    pub project_session: Rc<RefCell<Option<ProjectSession>>>,
    pub project_runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
    pub bank_nav_items: Rc<VecModel<BankNavItem>>,
    pub bank_chain_nodes: Rc<VecModel<GraphNode>>,
    pub bank_chain_edges: Rc<VecModel<GraphEdgeGeometry>>,
    /// Presentation state, rebuilt from the live project when the screen opens.
    pub state: Rc<RefCell<Option<BankSceneState>>>,
}

/// Recompute the live chain backing `input-{N}` from the rig's active
/// preset+scene and push it to the running audio via the proven in-place
/// lock-free `upsert_chain` (click-free; spillover is #454-T5). The chain's
/// own Input/Output blocks (per-machine device config) are preserved; only
/// the processing blocks are replaced. This is what makes a preset/scene
/// switch actually change the sound.
fn resync_live_audio(ctx: &BankSceneNavCtx, input_name: &str) {
    let Some(idx) = input_name
        .strip_prefix("input-")
        .and_then(|n| n.parse::<usize>().ok())
        .filter(|n| *n >= 1)
        .map(|n| n - 1)
    else {
        return;
    };
    let session_ref = ctx.project_session.borrow();
    let Some(session) = session_ref.as_ref() else {
        return;
    };
    let rig = migrate_legacy_project(&session.project.borrow());
    let Some(rig_input) = rig.inputs.get(input_name) else {
        return;
    };
    let processing: Vec<AudioBlock> = rig_input
        .bank
        .get(&rig_input.active_preset)
        .and_then(|name| rig.presets.get(name))
        .map(|p| p.apply_scene(rig_input.active_scene))
        .unwrap_or_default();

    let chain_clone = {
        let mut proj = session.project.borrow_mut();
        let Some(chain) = proj.chains.get_mut(idx) else {
            return;
        };
        let io: Vec<AudioBlock> = chain
            .blocks
            .iter()
            .filter(|b| matches!(b.kind, AudioBlockKind::Input(_) | AudioBlockKind::Output(_)))
            .cloned()
            .collect();
        let (inputs, outputs): (Vec<_>, Vec<_>) = io
            .into_iter()
            .partition(|b| matches!(b.kind, AudioBlockKind::Input(_)));
        let mut blocks = inputs;
        blocks.extend(processing);
        blocks.extend(outputs);
        chain.blocks = blocks;
        chain.clone()
    };

    if let Some(runtime) = ctx.project_runtime.borrow_mut().as_mut() {
        let proj = session.project.borrow();
        // #454-T5: spillover — previous preset/scene tail rings out.
        if let Err(e) = runtime.upsert_chain_spillover(&proj, &chain_clone) {
            log::error!("[bank-scene] live resync failed: {e}");
        }
    }
}

fn category_for(effect_type: &str) -> NodeCategory {
    match effect_type {
        "amp" | "preamp" | "full_rig" => NodeCategory::Amp,
        "gain" | "overdrive" | "distortion" | "fuzz" | "boost" | "drive" => NodeCategory::Drive,
        "delay" => NodeCategory::Time,
        "reverb" => NodeCategory::Reverb,
        "mod" | "modulation" | "chorus" | "flanger" | "phaser" | "tremolo" | "pitch" => {
            NodeCategory::Modulation
        }
        "dyn" | "dynamics" | "compressor" | "gate" | "limiter" => NodeCategory::Dynamics,
        "filter" | "eq" | "wah" => NodeCategory::Eq,
        "util" | "cab" | "ir" | "body" => NodeCategory::Util,
        _ => NodeCategory::Other,
    }
}

/// Build the GraphView models for the selected input's active preset chain.
fn graph_for_selected(
    rig: &RigProject,
    selected: Option<&str>,
) -> (Vec<GraphNode>, Vec<GraphEdgeGeometry>) {
    let Some(input) = selected.and_then(|s| rig.inputs.get(s)) else {
        return (Vec::new(), Vec::new());
    };
    let blocks = input
        .bank
        .get(&input.active_preset)
        .and_then(|name| rig.presets.get(name))
        .map(|p| p.apply_scene(input.active_scene))
        .unwrap_or_default();

    let mut stages: Vec<ChainStage> = vec![ChainStage::Single(BlockBlueprint::new(
        "in",
        "Input",
        NodeCategory::Input,
    ))];
    for b in &blocks {
        let (label, et) = match &b.kind {
            AudioBlockKind::Core(c) => (c.model.clone(), c.effect_type.as_str()),
            AudioBlockKind::Nam(n) => (n.model.clone(), "amp"),
            _ => (b.id.0.clone(), "other"),
        };
        let mut bp = BlockBlueprint::new(b.id.0.clone(), label, category_for(et));
        bp.bypass = !b.enabled;
        stages.push(ChainStage::Single(bp));
    }
    stages.push(ChainStage::Single(BlockBlueprint::new(
        "out",
        "Output",
        NodeCategory::Output,
    )));

    let (nodes, edges) = linear_chain_layout(&stages, GridMetrics::default());

    let palette: std::collections::HashMap<String, (slint::Color, slint::Color)> =
        default_palette()
            .iter()
            .map(|s| {
                let c =
                    |v: u32| slint::Color::from_rgb_u8((v >> 16) as u8, (v >> 8) as u8, v as u8);
                (s.category.to_string(), (c(s.fill), c(s.border)))
            })
            .collect();
    let neutral = (
        slint::Color::from_rgb_u8(0x8a, 0x94, 0xa2),
        slint::Color::from_rgb_u8(0x5c, 0x63, 0x6e),
    );
    let coords: std::collections::HashMap<&str, (f32, f32)> =
        nodes.iter().map(|n| (n.id.as_str(), (n.x, n.y))).collect();

    let sn = nodes
        .iter()
        .map(|n| {
            let (fill, border) = palette.get(n.category.as_str()).copied().unwrap_or(neutral);
            GraphNode {
                id: n.id.clone().into(),
                label: n.label.clone().into(),
                category: n.category.as_str().into(),
                fill,
                border,
                layout_x: n.x,
                layout_y: n.y,
                bypass: n.bypass,
                selected: false,
            }
        })
        .collect();
    let se = edges
        .iter()
        .filter_map(|e: &gvm::GraphEdge| {
            let f = coords.get(e.from_id.as_str())?;
            let t = coords.get(e.to_id.as_str())?;
            Some(GraphEdgeGeometry {
                from_id: e.from_id.clone().into(),
                to_id: e.to_id.clone().into(),
                from_x: f.0,
                from_y: f.1,
                to_x: t.0,
                to_y: t.1,
            })
        })
        .collect();
    (sn, se)
}

fn rebuild(ctx: &BankSceneNavCtx) {
    let rig: Option<RigProject> = ctx
        .project_session
        .borrow()
        .as_ref()
        .map(|ps| migrate_legacy_project(&ps.project.borrow()));
    let Some(rig) = rig else {
        ctx.bank_nav_items.set_vec(Vec::new());
        ctx.bank_chain_nodes.set_vec(Vec::new());
        ctx.bank_chain_edges.set_vec(Vec::new());
        *ctx.state.borrow_mut() = None;
        return;
    };
    let st = BankSceneState::from_project(&rig);
    let rows: Vec<BankNavItem> = render(&st)
        .into_iter()
        .map(|r| BankNavItem {
            input: r.input.into(),
            label: r.label.into(),
            active_preset: r.active_preset,
            active_scene: r.active_scene,
            bank_slots: ModelRc::from(Rc::new(VecModel::from(r.bank_slots))),
            selected: r.selected,
        })
        .collect();
    ctx.bank_nav_items.set_vec(rows);
    let (n, e) = graph_for_selected(&rig, st.selected_input.as_deref());
    ctx.bank_chain_nodes.set_vec(n);
    ctx.bank_chain_edges.set_vec(e);
    *ctx.state.borrow_mut() = Some(st);
}

fn dispatch(ctx: &BankSceneNavCtx, ev: BankSceneEvent) {
    let (rows, selected, affected): (Vec<BankNavItem>, Option<String>, Vec<String>) = {
        let mut guard = ctx.state.borrow_mut();
        let Some(state) = guard.as_mut() else { return };
        let effects = state.apply(ev);
        let affected: Vec<String> = effects
            .iter()
            .filter_map(|e| match e {
                BankSceneEffect::SwitchPreset { input, .. }
                | BankSceneEffect::SwitchScene { input, .. } => Some(input.clone()),
                _ => None,
            })
            .collect();
        let rows = render(state)
            .into_iter()
            .map(|r| BankNavItem {
                input: r.input.into(),
                label: r.label.into(),
                active_preset: r.active_preset,
                active_scene: r.active_scene,
                bank_slots: ModelRc::from(Rc::new(VecModel::from(r.bank_slots))),
                selected: r.selected,
            })
            .collect();
        (rows, state.selected_input.clone(), affected)
    };
    ctx.bank_nav_items.set_vec(rows);

    // Make the switch actually change the sound (proven lock-free upsert).
    for input in &affected {
        resync_live_audio(ctx, input);
    }

    // Re-render the selected input's chain in the GraphView.
    if let Some(ps) = ctx.project_session.borrow().as_ref() {
        let rig = migrate_legacy_project(&ps.project.borrow());
        let (n, e) = graph_for_selected(&rig, selected.as_deref());
        ctx.bank_chain_nodes.set_vec(n);
        ctx.bank_chain_edges.set_vec(e);
    }
}

pub(crate) fn wire(window: &AppWindow, ctx: BankSceneNavCtx) {
    let ctx = Rc::new(ctx);
    window.set_bank_nav_items(ModelRc::from(ctx.bank_nav_items.clone()));
    window.set_bank_chain_nodes(ModelRc::from(ctx.bank_chain_nodes.clone()));
    window.set_bank_chain_edges(ModelRc::from(ctx.bank_chain_edges.clone()));

    let w = window.as_weak();
    let c = ctx.clone();
    window.on_open_bank_scene_navigator(move || {
        let Some(win) = w.upgrade() else { return };
        rebuild(&c);
        win.set_show_project_chains(false);
        win.set_show_bank_scene_navigator(true);
    });

    let w = window.as_weak();
    window.on_close_bank_scene_navigator(move || {
        let Some(win) = w.upgrade() else { return };
        win.set_show_bank_scene_navigator(false);
        win.set_show_project_chains(true);
    });

    let c = ctx.clone();
    window.on_bank_scene_refresh(move || rebuild(&c));

    let c = ctx.clone();
    window.on_bank_select_input(move |i| dispatch(&c, BankSceneEvent::SelectInput(i.to_string())));
    let c = ctx.clone();
    window.on_bank_prev(move |_i| dispatch(&c, BankSceneEvent::BankPrev));
    let c = ctx.clone();
    window.on_bank_next(move |_i| dispatch(&c, BankSceneEvent::BankNext));
    let c = ctx.clone();
    window.on_bank_select_scene(move |_i, s| {
        dispatch(&c, BankSceneEvent::SelectScene(s.max(0) as usize))
    });
    window.on_bank_node_clicked(move |id| {
        log::info!("[bank-scene] node clicked: {id}");
    });
}
