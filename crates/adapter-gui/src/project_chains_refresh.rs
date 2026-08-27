//! Responsibility: rebuilds the chain rows the screen is bound to.

use crate::chain_block_item::chain_block_item_from_block;
use crate::chain_endpoint_labels::chain_endpoint_label;
use crate::project_view_tooltips::{chain_inputs_tooltip, chain_outputs_tooltip};
use crate::ui_state::{chain_io_chip_label_from_bindings, chain_routing_summary};
use crate::ProjectChainItem;
use domain::AudioDeviceDescriptor;
use infra_filesystem::IoBinding;
use project::block::AudioBlockKind;
use project::project::Project;
use slint::{ModelRc, SharedString, VecModel};
use std::rc::Rc;

pub(crate) fn replace_project_chains(
    model: &Rc<VecModel<ProjectChainItem>>,
    project: &Project,
    input_devices: &[AudioDeviceDescriptor],
    output_devices: &[AudioDeviceDescriptor],
    io_bindings: &[IoBinding],
) {
    let items = project
        .chains
        .iter()
        .enumerate()
        .map(|(index, chain)| {
            // Latency starts at 0 (badge hidden). The sonar probe populates
            // `latency_ms` with a measured value for up to 10 s when the
            // user clicks the probe button on the chain card.
            let latency_ms = 0.0_f32;
            ProjectChainItem {
                instrument: chain.instrument.clone().into(),
                title: chain
                    .description
                    .clone()
                    .unwrap_or_else(|| {
                        rust_i18n::t!("default-chain-name", n = index + 1).to_string()
                    })
                    .into(),
                subtitle: chain_routing_summary(chain, io_bindings).into(),
                enabled: chain.enabled,
                block_count_label: {
                    let effect_block_count = chain
                        .blocks
                        .iter()
                        .filter(|b| {
                            !matches!(
                                &b.kind,
                                AudioBlockKind::Input(_) | AudioBlockKind::Output(_)
                            )
                        })
                        .count();
                    if effect_block_count == 1 {
                        "1 block".into()
                    } else {
                        format!("{} blocks", effect_block_count).into()
                    }
                },
                input_label: {
                    let binding_name = chain_io_chip_label_from_bindings(chain, io_bindings, true);
                    if binding_name.is_empty() {
                        // #716: device endpoints resolve from the binding
                        // registry (never from block `entries`).
                        let (resolved_inputs, _) =
                            engine::runtime_endpoints::resolve_chain_io(chain, io_bindings);
                        let input_chs: Vec<usize> = resolved_inputs
                            .iter()
                            .flat_map(|e| e.channels.iter().copied())
                            .collect();
                        chain_endpoint_label("In", &input_chs).into()
                    } else {
                        binding_name.into()
                    }
                },
                input_tooltip: chain_inputs_tooltip(chain, project, input_devices, io_bindings)
                    .into(),
                output_label: {
                    let binding_name = chain_io_chip_label_from_bindings(chain, io_bindings, false);
                    if binding_name.is_empty() {
                        // #716: device endpoints resolve from the binding
                        // registry (never from block `entries`).
                        let (_, resolved_outputs) =
                            engine::runtime_endpoints::resolve_chain_io(chain, io_bindings);
                        let output_chs: Vec<usize> = resolved_outputs
                            .iter()
                            .flat_map(|e| e.channels.iter().copied())
                            .collect();
                        chain_endpoint_label("Out", &output_chs).into()
                    } else {
                        binding_name.into()
                    }
                },
                output_tooltip: chain_outputs_tooltip(chain, project, output_devices, io_bindings)
                    .into(),
                latency_ms,
                volume: chain.volume.round() as i32,
                // Issue #496: meters default to SILENT until the GUI
                // timer subscribes & polls (engine::output_meter).
                meter_in_dbfs: engine::output_meter::SILENT_DBFS,
                meter_out_dbfs: engine::output_meter::SILENT_DBFS,
                // #771: the DI meter row starts silent; the timer fills it
                // from the isolated playback's own peaks while the DI plays.
                di_meter: crate::StreamMeter {
                    in_dbfs: engine::output_meter::SILENT_DBFS,
                    out_dbfs: engine::output_meter::SILENT_DBFS,
                },
                // #771: the DI panel's output select — the chain's bound
                // output endpoints + the persisted pick.
                di_loop_outputs: {
                    let (labels, _) =
                        crate::di_output_options::output_labels_and_index(chain, io_bindings);
                    ModelRc::from(Rc::new(VecModel::from(
                        labels
                            .into_iter()
                            .map(SharedString::from)
                            .collect::<Vec<_>>(),
                    )))
                },
                di_output_selected_index: crate::di_output_options::output_labels_and_index(
                    chain,
                    io_bindings,
                )
                .1,
                // Issue #670: no overload until the meter timer observes
                // xruns from the running audio callback.
                audio_overload: false,
                // Per-stream meter slots. When the chain is enabled the length
                // matches the number of resolved input endpoints (one stream
                // per input runtime in the engine, per invariant #4); the
                // timer fills the live values, defaulting to SILENT here so the
                // UI renders the right number of (silent) bars on first paint.
                // When disabled the length is 0 (#750: the live graph hides).
                stream_meters: {
                    // #750: the per-stream graph is a LIVE surface — render
                    // ZERO rows while the chain is disabled so nothing shows
                    // until it is enabled. When enabled, one stream per
                    // resolved input endpoint (#716: from the binding registry,
                    // not per block `entries`), min 1 so an enabled-but-
                    // unresolved chain still shows a row.
                    // #85: one row per STREAM — a (input × output) pipeline —
                    // so a mid `Input`/`Output` port shows its own bar instead
                    // of hiding inside the chain's single input row.
                    let stream_count: usize = if chain.enabled {
                        crate::meter_wiring::project_stream_count(chain, io_bindings).max(1)
                    } else {
                        0
                    };
                    let model: Rc<VecModel<crate::StreamMeter>> = Rc::new(VecModel::default());
                    for _ in 0..stream_count {
                        model.push(crate::StreamMeter {
                            in_dbfs: engine::output_meter::SILENT_DBFS,
                            out_dbfs: engine::output_meter::SILENT_DBFS,
                        });
                    }
                    ModelRc::from(model)
                },
                blocks: {
                    // #85 / model A (#716): every block in the chain is a row.
                    // The head input and tail output are not blocks anymore —
                    // they come from the chain's E/S bindings and are drawn as
                    // fixed chips — so a mid `Input`/`Output`/`Insert` port the
                    // user placed must be visible. Hiding "the first Input and
                    // the last Output" swallowed exactly that row.
                    log::info!(
                        "[replace_project_chains] chain[{}] '{}' UI blocks:",
                        index,
                        chain.description.as_deref().unwrap_or("")
                    );
                    for (real_idx, b) in chain.blocks.iter().enumerate() {
                        log::info!(
                            "[replace_project_chains]   real_index={} kind={}",
                            real_idx,
                            b.model_ref()
                                .map(|m| format!("{}/{}", m.effect_type, m.model))
                                .unwrap_or_else(|| "io/insert".to_string())
                        );
                    }
                    ModelRc::from(Rc::new(VecModel::from(
                        chain
                            .blocks
                            .iter()
                            .enumerate()
                            .map(|(real_idx, b)| {
                                let mut item = chain_block_item_from_block(b);
                                item.real_index = real_idx as i32;
                                item
                            })
                            .collect::<Vec<_>>(),
                    )))
                },
                // #614: starts false; the meter timer updates it at ~30 Hz
                // via `ChainRuntimeState::has_di_loop`. Populated here so
                // the struct is complete on first render.
                di_loop_playing: false,
                // #614: enumerate bundled loop ids (stems under
                // <data-root>/assets/di-loops/) then append the
                // "Choose file…" sentinel. If the directory is missing or
                // empty (Task 8 ships the first loops), only the sentinel
                // appears so the user can still pick a WAV file.
                di_loop_sources: {
                    let bundled_ids = crate::di_loop_ui_sources::bundled_di_loop_ids();
                    let refs: Vec<&str> = bundled_ids.iter().map(|s| s.as_str()).collect();
                    let entries = crate::di_loop_ui_sources::build_di_loop_sources(&refs);
                    ModelRc::from(Rc::new(VecModel::from(
                        entries
                            .into_iter()
                            .map(SharedString::from)
                            .collect::<Vec<_>>(),
                    )))
                },
                di_loop_selected_index: -1, // #661: refreshed by meter timer
                // #323: the looper rows and the header tint start empty and
                // are refreshed by the meter timer from the live runtimes.
                // #323: build the looper rows from the chain's PERSISTED
                // config so a reopened project shows its loopers immediately.
                // The meter timer overlays the live state (position, layers,
                // recording…) on top for a chain that has a running stream;
                // without this seed a chain with no live stream showed an
                // empty panel and the user re-clicked Add until the config hit
                // the 8-looper cap.
                loopers: ModelRc::from(Rc::new(VecModel::from(
                    // #323 phase 2: the meter tick fills preset_index (needs the
                    // rig); the initial seed has no bank ⇒ every loop "follows".
                    crate::looper_view::looper_items_from_config(chain, io_bindings, &[]),
                ))),
                looper_active: false,
                looper_input_options: {
                    let (inputs, _) =
                        project::binding_discovery::chain_endpoint_labels(chain, io_bindings);
                    ModelRc::from(Rc::new(VecModel::from(
                        inputs
                            .into_iter()
                            .map(SharedString::from)
                            .collect::<Vec<_>>(),
                    )))
                },
                looper_output_options: {
                    let (_, outputs) =
                        project::binding_discovery::chain_endpoint_labels(chain, io_bindings);
                    ModelRc::from(Rc::new(VecModel::from(
                        outputs
                            .into_iter()
                            .map(SharedString::from)
                            .collect::<Vec<_>>(),
                    )))
                },
                // #323 phase 2: filled by the meter tick (needs the rig's bank);
                // the initial seed is empty ⇒ the picker shows just "follow".
                looper_preset_options: ModelRc::default(),
            }
        })
        .collect::<Vec<_>>();
    model.set_vec(items);
}
