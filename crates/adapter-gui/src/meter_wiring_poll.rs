//! Meter-polling timer lifecycle (issue #792 split from meter_wiring.rs).
//!
//! `start_meter_polling` sets up the ~15 Hz Slint timer and, per tick, refreshes
//! tap subscriptions and drains readings; the per-chain row update is factored
//! into `refresh_chain_meter_row` so neither is a monolith. The pure compute +
//! signature helpers it drives stay in `meter_wiring` (imported via the glob).

use std::cell::RefCell;
use std::collections::HashMap;

use domain::ids::ChainId;
use project::project::Project;

use crate::meter_wiring::*;
use crate::state::ProjectSession;

/// Lifecycle wiring: starts a Slint Timer that, at the meter poll rate, picks
/// up the current chain list from the project session, ensures every
/// chain has its meter taps subscribed, polls them, and writes the
/// per-chain peak dBFS into the matching `ProjectChainItem` rows of
/// the `project_chains` VecModel. Timer is leaked (lives for the
/// app's lifetime, like the other polling timers).
pub fn start_meter_polling(
    project_runtime: std::rc::Rc<std::cell::RefCell<Option<infra_cpal::ProjectRuntimeController>>>,
    project_chains: std::rc::Rc<slint::VecModel<crate::ProjectChainItem>>,
    project_session: std::rc::Rc<std::cell::RefCell<Option<crate::state::ProjectSession>>>,
) {
    use slint::TimerMode;
    // #715: ~15 Hz, not 30 Hz. The per-frame work of this timer (draining taps,
    // rebuilding the meter rows, and the Slint re-render it triggers) is memory
    // traffic that competes with the audio worker on the shared cache and
    // evicts the NAM weights → cold-cache inference → late buffer → crackle.
    // Halving the rate halves that contention. 15 Hz is still smooth for level
    // meters. (RING_CAPACITY 4096 still covers a 15 Hz drain: 48 kHz / 15 ≈
    // 3200 samples per tick.)
    const TICK: std::time::Duration = std::time::Duration::from_millis(METER_POLL_TICK_MS);
    const RING_CAPACITY: usize = 4096; // 15 Hz poll @ 48 kHz ⇒ ~3200 samples per drain

    let store = new_meter_store_per_stream();
    // Per-chain "runtime layout" signature snapshot from the previous
    // tick. The signature changes on any state that tears down +
    // rebuilds the chain's runtime (toggle, rig-nav preset/scene
    // switch, block add/remove). Chains whose signature did NOT change
    // keep their stable ring handles — no re-subscription, no flicker.
    let last_signature: std::rc::Rc<RefCell<HashMap<ChainId, u64>>> =
        std::rc::Rc::new(RefCell::new(HashMap::new()));
    // Issue #670: previous-tick xrun count per chain, so the overload
    // indicator lights when the audio callback missed a NEW deadline since
    // the last poll (and a warning is logged once per transition).
    let last_xruns: std::rc::Rc<RefCell<HashMap<ChainId, u64>>> =
        std::rc::Rc::new(RefCell::new(HashMap::new()));
    let last_underruns: std::rc::Rc<RefCell<HashMap<ChainId, u64>>> =
        std::rc::Rc::new(RefCell::new(HashMap::new()));
    // #661: bundled DI loop ids are static for the session (the di-loops
    // directory is scanned once on project load by `replace_project_chains`);
    // cache them here so the per-tick source-list refresh never hits the
    // filesystem.
    let bundled_di_loop_ids = crate::di_loop_ui_sources::bundled_di_loop_ids();
    let timer = slint::Timer::default();
    timer.start(TimerMode::Repeated, TICK, move || {
        let session_borrow = project_session.borrow();
        let Some(session) = session_borrow.as_ref() else {
            return;
        };
        let project = session.project.borrow();
        let chain_ids: Vec<_> = project.chains.iter().map(|c| c.id.clone()).collect();
        // #808: the DI output select lists the chain's bound outputs (offline, no
        // device needed), so refresh EVERY chain — active or not — else a DI-only
        // chain (never enabled) shows no output select.
        crate::di_output_options::apply_di_outputs_to_rows(
            &project_chains,
            &project,
            &session.io_bindings.borrow(),
        );
        let rt_borrow = project_runtime.borrow();
        // Detect chains whose runtime-layout signature changed since
        // the last tick. Signature mixes project bits (enabled, per
        // block id+enabled) with the engine's current stream count;
        // when the engine rebuilds the per-input runtimes (toggle
        // off→on, rig-nav preset switch) the count or enabled flag
        // flips and the store re-subscribes against the fresh rings.
        // Controller=None ⇒ the whole controller was dropped (last
        // chain toggled off); `detect_invalidations` wipes the cache
        // so the next online tick treats every chain as fresh — and
        // we also drop the store so we don't keep handing out rings
        // pointed at the dropped controller.
        let invalidate = detect_invalidations(
            &project.chains,
            rt_borrow.as_ref(),
            &mut last_signature.borrow_mut(),
        );
        let Some(controller) = rt_borrow.as_ref() else {
            store.borrow_mut().clear();
            return;
        };
        let make_streams = |cid: &ChainId| -> ChainMeterStreams {
            build_streams_from_taps(controller, cid, RING_CAPACITY)
        };
        refresh_subscriptions_lazy_per_stream(&store, &chain_ids, &invalidate, &make_streams);
        // Reclaim any orphan tap slots left behind after an invalidation
        // (rings dropped from the store free their consumer side, the
        // runtime sweeps).
        if !invalidate.is_empty() {
            controller.prune_dead_input_taps();
            controller.prune_dead_stream_taps();
        }
        // Aggregate per-stream readings to a single (max in, max out)
        // pair per chain so the existing single-bar UI keeps showing
        // *something*. The per-stream values are also forwarded into
        // `ProjectChainItem.stream_meters` for the multi-stream UI
        // surface.
        let per_stream = poll_per_stream(&store);
        let readings: Vec<(ChainId, f32, f32)> = per_stream
            .iter()
            .map(|(cid, streams)| {
                let max_in = streams
                    .iter()
                    .map(|s| s.in_dbfs)
                    .fold(engine::output_meter::SILENT_DBFS, f32::max);
                let max_out = streams
                    .iter()
                    .map(|s| s.out_dbfs)
                    .fold(engine::output_meter::SILENT_DBFS, f32::max);
                (cid.clone(), max_in, max_out)
            })
            .collect();
        for (cid, in_db, out_db_raw) in readings {
            refresh_chain_meter_row(
                &cid,
                in_db,
                out_db_raw,
                &project,
                session,
                controller,
                &project_chains,
                &per_stream,
                &bundled_di_loop_ids,
                &last_xruns,
                &last_underruns,
            );
        }
    });
    std::mem::forget(timer);
}

/// Refresh a single chain's meter row from the current engine readings.
/// Computes every per-field delta (aggregate peaks, per-stream rows, DI
/// playing/meter/sources/outputs, audio overload) and writes the row back
/// only when something changed — the timer's per-tick work for one chain,
/// factored out of `start_meter_polling` so neither is a monolith.
#[allow(clippy::too_many_arguments)]
fn refresh_chain_meter_row(
    cid: &ChainId,
    in_db: f32,
    out_db_raw: f32,
    project: &Project,
    session: &ProjectSession,
    controller: &infra_cpal::ProjectRuntimeController,
    project_chains: &slint::VecModel<crate::ProjectChainItem>,
    per_stream: &[(ChainId, Vec<StreamMeterReading>)],
    bundled_di_loop_ids: &[String],
    last_xruns: &RefCell<HashMap<ChainId, u64>>,
    last_underruns: &RefCell<HashMap<ChainId, u64>>,
) {
    use slint::Model;
    // Push readings into matching VecModel rows (rows are indexed
    // 1:1 with `project.chains`). The stream_tap reads the chain
    // signal BEFORE the audio callback applies the chain volume
    // slider — so the OUTPUT meter must compensate on the GUI
    // side, otherwise moving the volume knob never changes the
    // reading (user-reported in issue #496).
    let Some(idx) = project.chains.iter().position(|c| c.id == *cid) else {
        return;
    };
    let Some(mut row) = project_chains.row_data(idx) else {
        return;
    };
    let chain_volume = project.chains[idx].volume;
    let out_db = apply_chain_volume_db(out_db_raw, chain_volume);
    // Per-stream rows. Issue #532: the row length follows the
    // project's input-entry count, NOT the engine's transient
    // stream readings. Engine readings fill the project-owned
    // slots; missing entries stay SILENT. Without this clamp
    // a preset switch leaked extra rows into the footer and a
    // sibling toggle collapsed the rest to zero rows.
    let engine_streams: Vec<StreamMeterReading> = per_stream
        .iter()
        .find(|(c, _)| c == cid)
        .map(|(_, streams)| streams.clone())
        .unwrap_or_default();
    let project_streams =
        project_input_count(&project.chains[idx], &session.io_bindings.borrow());
    // #750: a disabled chain renders no per-stream rows. The timer
    // still visits it (a stale tap may report a tick after toggle-off),
    // so the `enabled` flag — not just the resolved count — gates the
    // row out; otherwise the `.max(1)` clamp keeps the graph stuck on.
    let per_stream_rows: Vec<crate::StreamMeter> = rebuild_stream_meters_row(
        &engine_streams,
        project_streams,
        chain_volume,
        project.chains[idx].enabled,
    );
    let stream_meters_changed = {
        let current = row.stream_meters.iter().collect::<Vec<_>>();
        current.len() != per_stream_rows.len()
            || current.iter().zip(&per_stream_rows).any(|(a, b)| {
                (a.in_dbfs - b.in_dbfs).abs() > 0.05 || (a.out_dbfs - b.out_dbfs).abs() > 0.05
            })
    };
    let aggregate_changed = (row.meter_in_dbfs - in_db).abs() > 0.05
        || (row.meter_out_dbfs - out_db).abs() > 0.05;
    // #614/#717: poll DI loop playing state so the chain-tile icon (and
    // the dedicated DI graph) reflect the engine's armed/disarmed state.
    // The DI now plays on its own dedicated stream, so "playing" is
    // di_stream_active — not the (removed) guitar-runtime injection.
    let di_playing_now = controller.di_stream_active(cid);
    let di_changed = row.di_loop_playing != di_playing_now;
    // #771: the DI meter row reads the isolated playback's OWN peaks
    // (maintained by the output callback's mix) — not the chain's.
    let di_meter_now = crate::di_meter::di_meter_from_peaks(
        controller.di_playback_peaks(cid),
        di_playing_now,
    );
    let di_meter_changed = (row.di_meter.in_dbfs - di_meter_now.in_dbfs).abs() > 0.05
        || (row.di_meter.out_dbfs - di_meter_now.out_dbfs).abs() > 0.05;
    // #661: re-derive the loaded source from the dispatcher so the
    // popup ComboBox (a) lists a user-chosen File as a labelled entry
    // and (b) highlights the active source when reopened (the popup is
    // re-instantiated on each show).
    let loaded_source = session.dispatcher.di_loop_source_for_chain(cid);
    let bundled_refs: Vec<&str> = bundled_di_loop_ids.iter().map(|s| s.as_str()).collect();
    let desired_sources = crate::di_loop_ui_sources::build_di_loop_sources_with_loaded(
        &bundled_refs,
        loaded_source.as_ref(),
    );
    let di_selected_now = loaded_source.as_ref().map_or(-1, |s| {
        crate::di_loop_ui_sources::di_loop_selected_index(&desired_sources, s)
    });
    let di_selected_changed = row.di_loop_selected_index != di_selected_now;
    let di_sources_changed = {
        let current: Vec<String> = row.di_loop_sources.iter().map(|s| s.to_string()).collect();
        current != desired_sources
    };
    // #771: keep the DI output select fresh — bindings and the
    // persisted pick (SetChainDiLoopOutput) both change under the
    // open panel.
    let (desired_outputs, di_output_selected_now) =
        crate::di_output_options::output_labels_and_index(
            &project.chains[idx],
            &session.io_bindings.borrow(),
        );
    let di_outputs_changed = {
        let current: Vec<String> = row.di_loop_outputs.iter().map(|s| s.to_string()).collect();
        current != desired_outputs
    };
    let di_output_selected_changed = row.di_output_selected_index != di_output_selected_now;
    // Issue #670: per-chain audio overload. Catch BOTH failure modes
    // the user hears as crackle — an xrun (the audio callback missed
    // its deadline) or an underrun (the output elastic buffer ran
    // empty because the producer didn't deliver in time). Either
    // lights the row's overload badge. Both counters are plain atomic
    // reads off the audio thread — no `processing` lock (issue #580).
    let cur_xruns = controller.chain_xrun_count(cid);
    let cur_underruns = controller.chain_underrun_count(cid);
    let prev_xruns = last_xruns.borrow().get(cid).copied().unwrap_or(0);
    let prev_underruns = last_underruns.borrow().get(cid).copied().unwrap_or(0);
    let overloaded = chain_overloaded(prev_xruns, cur_xruns)
        || chain_overloaded(prev_underruns, cur_underruns);
    last_xruns.borrow_mut().insert(cid.clone(), cur_xruns);
    last_underruns.borrow_mut().insert(cid.clone(), cur_underruns);
    // One concise warning only on the transition INTO overload (not
    // every event) so it never spams the log.
    if overloaded && !row.audio_overload {
        log::warn!(
            "audio overload on chain '{}': {} new xrun(s), {} new \
             underrun(s) — the rig is heavy for this buffer size",
            cid.0,
            cur_xruns.saturating_sub(prev_xruns),
            cur_underruns.saturating_sub(prev_underruns),
        );
    }
    let overload_changed = row.audio_overload != overloaded;
    if aggregate_changed
        || stream_meters_changed
        || di_changed
        || di_meter_changed
        || di_selected_changed
        || di_sources_changed
        || di_outputs_changed
        || di_output_selected_changed
        || overload_changed
    {
        row.meter_in_dbfs = in_db;
        row.meter_out_dbfs = out_db;
        if overload_changed {
            row.audio_overload = overloaded;
        }
        if di_changed {
            row.di_loop_playing = di_playing_now;
        }
        if di_meter_changed {
            row.di_meter = di_meter_now;
        }
        if di_sources_changed {
            row.di_loop_sources = slint::ModelRc::from(std::rc::Rc::new(slint::VecModel::from(
                desired_sources
                    .into_iter()
                    .map(slint::SharedString::from)
                    .collect::<Vec<_>>(),
            )));
        }
        if di_selected_changed {
            row.di_loop_selected_index = di_selected_now;
        }
        if di_outputs_changed {
            row.di_loop_outputs = slint::ModelRc::from(std::rc::Rc::new(slint::VecModel::from(
                desired_outputs
                    .into_iter()
                    .map(slint::SharedString::from)
                    .collect::<Vec<_>>(),
            )));
        }
        if di_output_selected_changed {
            row.di_output_selected_index = di_output_selected_now;
        }
        if stream_meters_changed {
            // #715: mutate the existing per-stream model IN PLACE when
            // the row COUNT is unchanged (the common case while playing:
            // only the dB values move every tick). Allocating a fresh
            // VecModel each tick adds allocator memory traffic that helps
            // evict the audio worker's NAM weights from the shared cache
            // (the crackle). Only allocate when the stream count changes.
            let reused = row
                .stream_meters
                .as_any()
                .downcast_ref::<slint::VecModel<crate::StreamMeter>>()
                .filter(|vm| vm.row_count() == per_stream_rows.len())
                .map(|vm| {
                    for (i, r) in per_stream_rows.iter().enumerate() {
                        vm.set_row_data(
                            i,
                            crate::StreamMeter {
                                in_dbfs: r.in_dbfs,
                                out_dbfs: r.out_dbfs,
                            },
                        );
                    }
                })
                .is_some();
            if !reused {
                let model = std::rc::Rc::new(slint::VecModel::default());
                for r in &per_stream_rows {
                    model.push(crate::StreamMeter {
                        in_dbfs: r.in_dbfs,
                        out_dbfs: r.out_dbfs,
                    });
                }
                row.stream_meters = slint::ModelRc::from(model);
            }
        }
        project_chains.set_row_data(idx, row);
    }
}
