//! Compact chain view: the header's chain-admin forwarders and (#787) the
//! per-block parameter tabs + drag geometry.
//!
//! The admin actions (preset, scene, rename, load, save-as, configure, latency
//! probe, volume) simply re-invoke the matching callback on the main
//! `AppWindow`, which already owns the wiring — the compact view stays a pure
//! projection with no second dispatch path to keep in sync. Split out of
//! `compact_chain_callbacks.rs`, which was over the 500-line cap.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, Model, ModelRc, Timer, VecModel};

use crate::compact_block_layout::{slot_index_at, slot_y};
use crate::compact_block_tabs::set_active_group;
use crate::compact_block_view::build_compact_blocks;
use crate::state::ProjectSession;
use crate::{AppWindow, CompactChainViewWindow};

/// Heights of the compact rows currently on screen, in order — the geometry the
/// drop slot and its indicator are computed from.
fn compact_row_heights(win: &CompactChainViewWindow) -> Vec<f32> {
    win.get_compact_blocks()
        .iter()
        .map(|it| it.row_height)
        .collect()
}

/// Re-project the compact model for `chain_index` from the live project.
fn refresh(
    win: &CompactChainViewWindow,
    session: &Rc<RefCell<Option<ProjectSession>>>,
    chain_index: i32,
) {
    let session_borrow = session.borrow();
    if let Some(session) = session_borrow.as_ref() {
        let blocks = build_compact_blocks(&session.project.borrow(), chain_index.max(0) as usize);
        win.set_compact_blocks(ModelRc::from(Rc::new(VecModel::from(blocks))));
    }
}

pub(crate) fn wire(
    window: &AppWindow,
    compact_win: &CompactChainViewWindow,
    chain_index: i32,
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
) {
    {
        let weak_main = window.as_weak();
        let weak_compact = compact_win.as_weak();
        let project_session = project_session.clone();
        compact_win.on_switch_chain_preset(move |slot| {
            let Some(m) = weak_main.upgrade() else {
                return;
            };
            m.invoke_switch_chain_preset(chain_index, slot);
            // #667: the switch mutates the project via the main window, but the
            // compact view owns a separate `compact_blocks` model the main path
            // never touches — re-project it here, or the tone changes while the
            // block list stays on the old preset.
            if let Some(cw) = weak_compact.upgrade() {
                refresh(&cw, &project_session, chain_index);
            }
        });
    }
    // #659: the preset bank dropdown's search runs against this window's own
    // `PresetPicker` global (each Slint window owns its globals), so wire it here
    // too — without it the compact view's finder would never populate.
    crate::chain_rig_nav_wiring::wire_preset_picker_search(compact_win);
    // #749: same for the DI loop source dropdown.
    crate::di_source_picker_wiring::wire_di_source_picker_search(compact_win);
    {
        let weak_main = window.as_weak();
        compact_win.on_switch_chain_scene(move |s| {
            if let Some(m) = weak_main.upgrade() {
                m.invoke_switch_chain_scene(chain_index, s);
            }
        });
    }
    {
        let weak_main = window.as_weak();
        compact_win.on_rename_chain_preset(move |name| {
            if let Some(m) = weak_main.upgrade() {
                m.invoke_rename_chain_preset(chain_index, name);
            }
        });
    }
    {
        let weak_main = window.as_weak();
        compact_win.on_load_chain_preset(move || {
            if let Some(m) = weak_main.upgrade() {
                m.invoke_configure_chain_preset(chain_index);
            }
        });
    }
    {
        let weak_main = window.as_weak();
        compact_win.on_save_chain_preset_as(move || {
            if let Some(m) = weak_main.upgrade() {
                m.invoke_save_chain_preset(chain_index);
            }
        });
    }
    {
        let weak_main = window.as_weak();
        compact_win.on_configure_chain(move |ci| {
            if let Some(m) = weak_main.upgrade() {
                m.invoke_configure_chain(ci);
            }
        });
    }
    {
        let weak_main = window.as_weak();
        compact_win.on_probe_chain_latency(move |ci| {
            if let Some(m) = weak_main.upgrade() {
                m.invoke_probe_chain_latency(ci);
            }
        });
    }
    {
        let weak_main = window.as_weak();
        compact_win.on_chain_volume_changed(move |ci, v| {
            if let Some(m) = weak_main.upgrade() {
                m.invoke_chain_volume_changed(ci, v);
            }
        });
    }

    // #787: which parameter tab a block shows is view state, so switching it only
    // re-projects the compact model — no Command, nothing reaches the project.
    {
        let weak_compact = compact_win.as_weak();
        let project_session = project_session.clone();
        compact_win.on_select_block_parameter_group(move |_ci, bi, gi| {
            let Some(cw) = weak_compact.upgrade() else {
                return;
            };
            let blocks = cw.get_compact_blocks();
            let Some(block) = (0..blocks.row_count())
                .filter_map(|i| blocks.row_data(i))
                .find(|it| it.block_index == bi)
            else {
                return;
            };
            let Some(group) = block.parameter_groups.row_data(gi.max(0) as usize) else {
                return;
            };
            set_active_group(block.block_id.as_str(), group.as_str());
            refresh(&cw, &project_session, chain_index);
        });
    }
    // #787: rows have their own heights, so the drop slot and its indicator come
    // from the row geometry instead of a fixed 112px stride.
    {
        let weak_compact = compact_win.as_weak();
        compact_win.on_slot_at(move |y| {
            weak_compact
                .upgrade()
                .map(|cw| slot_index_at(&compact_row_heights(&cw), y))
                .unwrap_or(0)
        });
    }
    {
        let weak_compact = compact_win.as_weak();
        compact_win.on_slot_y(move |slot| {
            weak_compact
                .upgrade()
                .map(|cw| slot_y(&compact_row_heights(&cw), slot.max(0) as usize))
                .unwrap_or(0.0)
        });
    }
}

/// Header state polling — copies the current rig_nav row, meter values, volume,
/// DI state and chain title from the main window's models into the compact view,
/// so its header stays in sync with the chains screen (the source of truth).
pub(crate) fn start_header_poll(
    window: &AppWindow,
    compact_win: &CompactChainViewWindow,
    ci: usize,
) {
    {
        let weak_compact = compact_win.as_weak();
        let weak_main_for_poll = window.as_weak();
        let header_timer = Timer::default();
        header_timer.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_millis(80),
            move || {
                let (Some(cw), Some(mw)) = (weak_compact.upgrade(), weak_main_for_poll.upgrade())
                else {
                    return;
                };
                use slint::Model;
                let chains = mw.get_project_chains();
                let nav_model = mw.get_chain_rig_nav();
                if let Some(row) = chains.row_data(ci) {
                    cw.set_meter_in_dbfs(row.meter_in_dbfs);
                    cw.set_meter_out_dbfs(row.meter_out_dbfs);
                    cw.set_stream_meters(row.stream_meters.clone());
                    cw.set_volume(row.volume);
                    cw.set_chain_enabled(row.enabled);
                    cw.set_chain_title(row.title.clone());
                    // #613: mirror the measured latency the sonar probe
                    // wrote onto the main chains row, so the badge shows
                    // inside the compact view (not only on the list).
                    cw.set_latency_ms(row.latency_ms);
                    // #614: mirror DI loop playing state and source list.
                    cw.set_di_loop_playing(row.di_loop_playing);
                    cw.set_di_loop_sources(row.di_loop_sources.clone());
                    // #717: mirror the selected source too — without it the
                    // compact panel opens with nothing picked and hides the
                    // play/stop button.
                    cw.set_di_loop_selected_index(row.di_loop_selected_index);
                    // #771: mirror the output select too.
                    cw.set_di_loop_outputs(row.di_loop_outputs.clone());
                    cw.set_di_output_selected_index(row.di_output_selected_index);
                    // #771: the DI meter row shows the isolated playback's
                    // OWN levels (row.di_meter, fed from di_playback_peaks)
                    // — never a mirror of the chain meters.
                    cw.set_di_graph_meter(row.di_meter);
                }
                if let Some(nav) = nav_model.row_data(ci) {
                    cw.set_rig_nav(nav);
                }
            },
        );
        // Park the timer on the open_compact_window slot's
        // lifetime so it stops when the window closes.
        std::mem::forget(header_timer);
    }
}
