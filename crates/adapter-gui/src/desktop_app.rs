//! Responsibility: starts the desktop app in the order its parts need.
//!
//! `run_desktop_app` is orchestration and nothing else: create the state
//! handles and the windows, hand each callback group to its own wiring
//! module, then enter the Slint event loop. The linear order below IS the
//! startup contract — a wiring module that runs before the model it reads
//! exists sees an empty window.

use anyhow::{anyhow, Result};
use infra_cpal::ProjectRuntimeController;
use infra_filesystem::FilesystemStorage;
use slint::{ComponentHandle, ModelRc, Timer, VecModel, Global};
use std::cell::RefCell;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::rc::Rc;
use ui_openrig::{AppRuntimeMode, InteractionMode, UiRuntimeContext};

use crate::audio_devices::{build_device_selection_items, mark_unselected_devices};
use crate::project_ops::{load_and_sync_app_config, resolve_project_paths};
use crate::state::{
    AudioSettingsMode, BlockEditorDraft, BlockWindow, ChainDraft, InsertDraft, ProjectSession,
    SelectedBlock,
};
use crate::{latency_probe, ChannelOptionItem, CompactChainViewWindow};

pub fn run_desktop_app(
    runtime_mode: AppRuntimeMode,
    interaction_mode: InteractionMode,
    cli_project_path: Option<PathBuf>,
    auto_save: bool,
    fullscreen: bool,
    mcp_addr: Option<SocketAddr>,
    midi_map: Option<crate::cli::MidiMapArg>,
) -> Result<()> {
    log::info!(
        "starting desktop app: runtime_mode={:?}, interaction_mode={:?}",
        runtime_mode,
        interaction_mode
    );
    crate::ui_watchdog::spawn();
    let context = UiRuntimeContext::new(runtime_mode, interaction_mode);
    let settings = FilesystemStorage::load_gui_audio_settings()?.unwrap_or_default();
    let needs_audio_settings =
        context.capabilities.can_select_audio_device && !settings.is_complete();
    let project_paths = resolve_project_paths();
    let loaded_config = load_and_sync_app_config()?;
    let resolved_paths = infra_filesystem::resolve_asset_paths(loaded_config.paths.clone());
    infra_filesystem::init_asset_paths(resolved_paths);
    // #693: warm the device cache off-thread so the first project open / IO
    // window never pays the ~2s CoreAudio enumeration on the GUI thread
    // (measured: the [ui-stall] 760ms at boot + the open delay).
    std::thread::Builder::new()
        .name("device-cache-warmer".into())
        .spawn(|| {
            let _ = infra_cpal::list_input_device_descriptors();
            let _ = infra_cpal::list_output_device_descriptors();
        })
        .ok();
    let vst3_sample_rate = settings
        .input_devices
        .first()
        .map(|d| d.sample_rate)
        .unwrap_or(48_000) as f64;
    crate::desktop_app_catalog::load(&project_paths, vst3_sample_rate);
    let app_config = Rc::new(RefCell::new(loaded_config));
    let project_session = Rc::new(RefCell::new(None::<ProjectSession>));
    let chain_draft = Rc::new(RefCell::new(None::<ChainDraft>));
    let insert_draft = Rc::new(RefCell::new(None::<InsertDraft>));
    let selected_block = Rc::new(RefCell::new(None::<SelectedBlock>));
    let block_editor_draft = Rc::new(RefCell::new(None::<BlockEditorDraft>));
    let project_runtime = Rc::new(RefCell::new(None::<ProjectRuntimeController>));
    // #127: the two capabilities the wiring modules get INSTEAD of the runtime
    // handle — installing the seam on a freshly opened session, and reading the
    // loopers' transport state (the same finished reading MCP gets). Neither
    // can start, stop or sync audio.
    let looper_live = crate::gui_live_source::looper_live_source(&project_runtime);
    // #127: the subscription seam. Every tap consumer (meters, tuner,
    // spectrum, Tone Doctor) asks THIS for a subscription by stream identity
    // instead of holding the audio backend.
    let audio_taps = crate::runtime_taps::gui_audio_taps(&project_runtime);
    // #127: the analyzers' lifecycle. The tuner and the spectrum are powered
    // by `SelectionCommand::SetTunerEnabled` / `SetSpectrumEnabled`, applied
    // through `RuntimeControl` — so a MIDI footswitch and an MCP client start
    // the same analyzer the window's POWER does. The windows only render.
    let analyzers = crate::runtime_analyzers::AnalyzerSessions::new(&project_session, &audio_taps);
    let runtime_attach = crate::runtime_lifecycle::RuntimeAttach::new(&project_runtime, &analyzers);
    // #127: the block editors' diagnostic-stream reading — the same seam MCP
    // reads through, so a panel never holds the audio backend for it.
    let block_stream_reads = crate::gui_live_source::block_stream_live_source(&project_runtime);
    let probe_windows = latency_probe::new_windows();
    let saved_project_snapshot = Rc::new(RefCell::new(None::<String>));
    let project_dirty = Rc::new(RefCell::new(false));
    let open_block_windows: Rc<RefCell<Vec<BlockWindow>>> = Rc::new(RefCell::new(Vec::new()));
    let inline_stream_timer: Rc<RefCell<Option<Timer>>> = Rc::new(RefCell::new(None));
    let open_compact_window: Rc<RefCell<Option<(usize, slint::Weak<CompactChainViewWindow>)>>> =
        Rc::new(RefCell::new(None));
    let audio_settings_mode = Rc::new(RefCell::new(AudioSettingsMode::Gui));
    // Start with empty device descriptors. Enumerating here would read
    // /proc/asound/cards (and transitively /proc/asound/card*/stream0), which
    // invokes the kernel snd-usb-audio proc handler and has been correlated
    // with vendor-firmware notifications that destabilize USB audio interfaces
    // on fragile xHCI controllers. Descriptors are populated lazily by
    // refresh_input_devices / refresh_output_devices when the user actually
    // opens a chain I/O editor or the Settings panel — i.e. when they
    // explicitly ask the app to look at the hardware.
    let input_chain_devices: Rc<RefCell<Vec<domain::AudioDeviceDescriptor>>> =
        Rc::new(RefCell::new(Vec::new()));
    let output_chain_devices: Rc<RefCell<Vec<domain::AudioDeviceDescriptor>>> =
        Rc::new(RefCell::new(Vec::new()));
    let preset_file_list: Rc<RefCell<Vec<std::path::PathBuf>>> = Rc::new(RefCell::new(Vec::new()));
    let crate::desktop_app_windows::DesktopWindows {
        window,
        project_settings_window,
        chain_insert_window,
        chain_port_window,
        tuner_window,
        spectrum_window,
        metronome_window,
        chain_editor_window,
        plugin_info_window,
    } = crate::desktop_app_windows::create()?;
    let port_draft: Rc<RefCell<Option<crate::state::PortDraft>>> = Rc::new(RefCell::new(None));
    let insert_send_channels = Rc::new(VecModel::from(Vec::<ChannelOptionItem>::new()));
    let insert_return_channels = Rc::new(VecModel::from(Vec::<ChannelOptionItem>::new()));
    // The analyzer owns each session; these are the same cells, for the reads
    // (`GuiLiveSource::tuner`, `openrig://tuner`) that answer from them.
    let tuner_session = analyzers.tuner_cell().clone();
    let spectrum_session = analyzers.spectrum_cell().clone();
    // #14/#127: the metronome's settings live in the dispatcher, restored from
    // the per-machine config by every session (`state::attach_metronome_state`).
    // What this window keeps is the read seam it draws the beat lamps from —
    // the same `LiveSource` an MCP client reads the click's position through.
    let metronome_live = crate::gui_live_source::metronome_live_source(&project_runtime);
    let metronome_timer = Rc::new(Timer::default());

    crate::desktop_app_language::wire(
        crate::desktop_app_language::LanguageWindows {
            window: &window,
            project_settings_window: &project_settings_window,
            chain_insert_window: &chain_insert_window,
            tuner_window: &tuner_window,
            spectrum_window: &spectrum_window,
            metronome_window: &metronome_window,
            chain_editor_window: chain_editor_window.clone(),
            plugin_info_window: plugin_info_window.clone(),
        },
        project_session.clone(),
    );
    // #712: System / Integrations master switches (MIDI adapter / MCP server).
    crate::settings::integrations::wire(
        &window,
        &project_settings_window,
        project_session.clone(),
        app_config.clone(),
    );
    // #716: System / I/O bindings editor.
    crate::settings::io_bindings::wire(
        &window,
        &project_settings_window,
        project_session.clone(),
        app_config.clone(),
        input_chain_devices.clone(),
        output_chain_devices.clone(),
    );
    let input_devices = Rc::new(VecModel::from(build_device_selection_items(
        &input_chain_devices.borrow(),
        &settings.input_devices,
    )));
    mark_unselected_devices(&input_devices, &settings.input_devices);
    let output_devices = Rc::new(VecModel::from(build_device_selection_items(
        &output_chain_devices.borrow(),
        &settings.output_devices,
    )));
    mark_unselected_devices(&output_devices, &settings.output_devices);

    // Initial AppWindow + ProjectSettingsWindow state + project-row VecModels
    // (extracted to desktop_app_init)
    let crate::desktop_app_init::InitialState {
        project_devices,
        project_chains,
        recent_projects,
        chain_input_device_options,
        chain_output_device_options,
        chain_input_channels: _chain_input_channels,
        chain_output_channels: _chain_output_channels,
    } = crate::desktop_app_init::populate_initial_window_state(
        &window,
        &project_settings_window,
        &context,
        &settings,
        auto_save,
        fullscreen,
        needs_audio_settings,
        &input_chain_devices,
        &output_chain_devices,
        &app_config,
        &input_devices,
        &output_devices,
    );

    // CLI auto-open (extracted to desktop_app_cli_open)
    crate::desktop_app_cli_open::try_auto_open(
        cli_project_path.as_ref(),
        &window,
        &project_session,
        &project_chains,
        &input_chain_devices,
        &output_chain_devices,
        &saved_project_snapshot,
        &project_dirty,
        &app_config,
        &recent_projects,
    );
    // #127: hand the session the CLI just installed this frontend's audio
    // runtime, so a runtime-control command issued before the first chain sync
    // still reaches the audio (it used to cold-start the runtime itself). Done
    // here rather than inside `try_auto_open` because this is the module that
    // owns the runtime handle; nothing in between reaches the audio.
    if let Some(session) = project_session.borrow().as_ref() {
        crate::runtime_lifecycle::attach_runtime_control(&project_runtime, &analyzers, session);
    }
    let crate::desktop_app_block_models::BlockEditorModels {
        block_type_options,
        block_model_options,
        filtered_block_model_options,
        block_model_option_labels,
        block_parameter_items,
        multi_slider_points,
        curve_editor_points,
        eq_band_curves,
    } = crate::desktop_app_block_models::init(&window);
    let block_editor_persist_timer = Rc::new(Timer::default());
    let toast_timer = Rc::new(Timer::default());
    crate::OverlayBridge::get(&window).set_toast_message("".into());
    crate::OverlayBridge::get(&window).set_toast_level("info".into());

    // Background polling timers (extracted to desktop_app_polling)
    // #127: the tick gets the two seams, never the runtime handle.
    let tick_reads = crate::gui_live_source::health_live_source(&project_runtime);
    let tick_writes =
        crate::runtime_health::polling_runtime_control(&project_runtime, &project_session);
    crate::desktop_app_polling::start(
        &window,
        toast_timer.clone(),
        tick_reads,
        Rc::clone(&tick_writes),
    );

    // Issue #496 / #32 / #36: per-chain IN/OUT dBFS meter polling.
    // ~30 Hz timer that subscribes new chains' input + stream taps
    // and writes peak dBFS into the matching ProjectChainItem rows.
    crate::meter_wiring::start_meter_polling(
        Rc::clone(&audio_taps),
        crate::gui_live_source::chain_row_live_source(&project_runtime),
        Rc::clone(&tick_writes),
        project_chains.clone(),
        project_session.clone(),
    );

    crate::SettingsBridge::get(&project_settings_window).set_project_devices(ModelRc::from(project_devices.clone()));
    crate::SettingsBridge::get(&window).set_project_devices(ModelRc::from(project_devices.clone()));
    project_settings_window.set_sample_rate_options(window.get_sample_rate_options());
    project_settings_window.set_buffer_size_options(window.get_buffer_size_options());
    project_settings_window.set_bit_depth_options(window.get_bit_depth_options());
    chain_insert_window.set_send_device_options(ModelRc::from(chain_output_device_options.clone()));
    chain_insert_window
        .set_return_device_options(ModelRc::from(chain_input_device_options.clone()));
    chain_insert_window.set_send_channels(ModelRc::from(insert_send_channels.clone()));
    chain_insert_window.set_return_channels(ModelRc::from(insert_return_channels.clone()));
    chain_insert_window.set_selected_send_device_index(-1);
    chain_insert_window.set_selected_return_device_index(-1);
    chain_insert_window.set_status_message("".into());
    // --- ChainInsertWindow callbacks (extracted to insert_wiring) ---
    // #85 — the mid-chain I/O port editor's own callbacks.
    crate::port_wiring::wire_port_window(
        &window,
        &chain_port_window,
        crate::port_wiring::PortWiringCtx {
            port_draft: port_draft.clone(),
            project_session: project_session.clone(),
            project_chains: project_chains.clone(),
            saved_project_snapshot: saved_project_snapshot.clone(),
            project_dirty: project_dirty.clone(),
            input_chain_devices: input_chain_devices.clone(),
            output_chain_devices: output_chain_devices.clone(),
            auto_save,
        },
    );
    crate::insert_wiring::wire(
        &window,
        &chain_insert_window,
        crate::insert_wiring::InsertWiringCtx {
            insert_draft: insert_draft.clone(),
            input_chain_devices: input_chain_devices.clone(),
            output_chain_devices: output_chain_devices.clone(),
            insert_send_channels: insert_send_channels.clone(),
            insert_return_channels: insert_return_channels.clone(),
            project_session: project_session.clone(),
            project_chains: project_chains.clone(),
            saved_project_snapshot: saved_project_snapshot.clone(),
            project_dirty: project_dirty.clone(),
            auto_save,
        },
    );
    crate::desktop_app_settings_wiring::wire(
        &window,
        &project_settings_window,
        crate::desktop_app_settings_wiring::SettingsWiringDeps {
            input_devices: input_devices.clone(),
            output_devices: output_devices.clone(),
            project_devices: project_devices.clone(),
            project_session: project_session.clone(),
            project_chains: project_chains.clone(),
            chain_input_device_options: chain_input_device_options.clone(),
            chain_output_device_options: chain_output_device_options.clone(),
            input_chain_devices: input_chain_devices.clone(),
            output_chain_devices: output_chain_devices.clone(),
            audio_settings_mode: audio_settings_mode.clone(),
            saved_project_snapshot: saved_project_snapshot.clone(),
            project_dirty: project_dirty.clone(),
            toast_timer: toast_timer.clone(),
            app_config: app_config.clone(),
            auto_save,
        },
    );

    // Seed the initial project-name and project-path-display from the session.
    {
        let sess = project_session.borrow();
        let name: slint::SharedString = sess
            .as_ref()
            .and_then(|s| s.project.borrow().name.clone())
            .unwrap_or_default()
            .into();
        let path: slint::SharedString = sess
            .as_ref()
            .and_then(|s| s.project_path.as_ref().map(|p| p.display().to_string()))
            .unwrap_or_else(|| "(unsaved)".into())
            .into();
        crate::SettingsBridge::get(&window).set_project_name(name.clone());
        crate::SettingsBridge::get(&window).set_project_path_display(path.clone());
        // Mirror onto the standalone settings window (#513): SettingsPage
        // reads project-name from project-name-draft, but the path is a
        // separate property that must be pushed independently.
        project_settings_window.set_project_name_draft(name);
        crate::SettingsBridge::get(&project_settings_window).set_project_path_display(path);
    }
    crate::desktop_app_project_wiring::wire(
        &window,
        &project_settings_window,
        crate::desktop_app_project_wiring::ProjectWiringDeps {
            project_paths: project_paths.clone(),
            app_config: app_config.clone(),
            recent_projects: recent_projects.clone(),
            project_session: project_session.clone(),
            project_chains: project_chains.clone(),
            project_devices: project_devices.clone(),
            runtime_attach: runtime_attach.clone(),
            chain_input_device_options: chain_input_device_options.clone(),
            chain_output_device_options: chain_output_device_options.clone(),
            input_chain_devices: input_chain_devices.clone(),
            output_chain_devices: output_chain_devices.clone(),
            audio_settings_mode: audio_settings_mode.clone(),
            saved_project_snapshot: saved_project_snapshot.clone(),
            project_dirty: project_dirty.clone(),
            preset_file_list: preset_file_list.clone(),
            toast_timer: toast_timer.clone(),
            auto_save,
            fullscreen,
        },
    );

    crate::desktop_app_topbar_wiring::wire(
        crate::desktop_app_topbar_wiring::TopBarWindows {
            window: &window,
            tuner_window: &tuner_window,
            spectrum_window: &spectrum_window,
            metronome_window: &metronome_window,
        },
        &project_session,
        &project_chains,
        &analyzers,
        crate::gui_live_source::chain_rate_live_source(&project_runtime, &project_session),
        &metronome_live,
        &metronome_timer,
        probe_windows.clone(),
    );
    // --- Back-to-launcher callback (extracted to back_to_launcher_wiring) ---
    crate::back_to_launcher_wiring::wire(
        &window,
        &project_settings_window,
        crate::back_to_launcher_wiring::BackToLauncherCtx {
            project_session: project_session.clone(),
            project_chains: project_chains.clone(),
            saved_project_snapshot: saved_project_snapshot.clone(),
            project_dirty: project_dirty.clone(),
            chain_editor_window: chain_editor_window.clone(),
            input_chain_devices: input_chain_devices.clone(),
            output_chain_devices: output_chain_devices.clone(),
            toast_timer: toast_timer.clone(),
        },
    );
    // --- Chain-level callback wirings (extracted to desktop_app_chain_wiring) ---
    crate::desktop_app_chain_wiring::wire_all(&crate::desktop_app_chain_wiring::ChainWiringDeps {
        window: &window,
        chain_draft: chain_draft.clone(),
        block_editor_draft: block_editor_draft.clone(),
        project_session: project_session.clone(),
        project_chains: project_chains.clone(),
        audio_taps: Rc::clone(&audio_taps),
        block_stream_reads: Rc::clone(&block_stream_reads),
        saved_project_snapshot: saved_project_snapshot.clone(),
        project_dirty: project_dirty.clone(),
        input_chain_devices: input_chain_devices.clone(),
        output_chain_devices: output_chain_devices.clone(),
        chain_input_device_options: chain_input_device_options.clone(),
        chain_output_device_options: chain_output_device_options.clone(),
        chain_editor_window: chain_editor_window.clone(),
        open_compact_window: open_compact_window.clone(),
        toast_timer: toast_timer.clone(),
        app_config: app_config.clone(),
        fullscreen,
        auto_save,
    });
    // --- Block-related callback wirings (extracted to desktop_app_block_wiring) ---
    crate::desktop_app_block_wiring::wire_all(&crate::desktop_app_block_wiring::BlockWiringDeps {
        chain_port_window: &chain_port_window,
        port_draft: port_draft.clone(),
        window: &window,
        chain_insert_window: &chain_insert_window,
        selected_block: selected_block.clone(),
        block_editor_draft: block_editor_draft.clone(),
        insert_draft: insert_draft.clone(),
        block_type_options: block_type_options.clone(),
        block_model_options: block_model_options.clone(),
        filtered_block_model_options: filtered_block_model_options.clone(),
        block_model_option_labels: block_model_option_labels.clone(),
        block_parameter_items: block_parameter_items.clone(),
        multi_slider_points: multi_slider_points.clone(),
        curve_editor_points: curve_editor_points.clone(),
        eq_band_curves: eq_band_curves.clone(),
        project_session: project_session.clone(),
        project_chains: project_chains.clone(),
        block_stream_reads: Rc::clone(&block_stream_reads),
        saved_project_snapshot: saved_project_snapshot.clone(),
        project_dirty: project_dirty.clone(),
        input_chain_devices: input_chain_devices.clone(),
        output_chain_devices: output_chain_devices.clone(),
        chain_input_device_options: chain_input_device_options.clone(),
        chain_output_device_options: chain_output_device_options.clone(),
        insert_send_channels: insert_send_channels.clone(),
        insert_return_channels: insert_return_channels.clone(),
        open_block_windows: open_block_windows.clone(),
        inline_stream_timer: inline_stream_timer.clone(),
        open_compact_window: open_compact_window.clone(),
        toast_timer: toast_timer.clone(),
        plugin_info_window: plugin_info_window.clone(),
        block_editor_persist_timer: block_editor_persist_timer.clone(),
        auto_save,
    });
    // Fullscreen inline chain editor callbacks — delegate to ChainEditorWindow
    // --- Chain editor delegation forwarders (extracted to chain_editor_forwarders_wiring) ---
    crate::chain_editor_forwarders_wiring::wire(&window, chain_editor_window.clone());
    // --- on_save_chain + on_cancel_chain (extracted to chain_save_cancel_callbacks) ---
    crate::chain_save_cancel_callbacks::wire(
        &window,
        crate::chain_save_cancel_callbacks::ChainSaveCancelCtx {
            chain_draft: chain_draft.clone(),
            project_session: project_session.clone(),
            project_chains: project_chains.clone(),
            saved_project_snapshot: saved_project_snapshot.clone(),
            project_dirty: project_dirty.clone(),
            input_chain_devices: input_chain_devices.clone(),
            output_chain_devices: output_chain_devices.clone(),
            toast_timer: toast_timer.clone(),
            auto_save,
        },
    );
    // --- Chain row callbacks (extracted to chain_row_wiring) ---
    crate::chain_row_wiring::wire(
        &window,
        crate::chain_row_wiring::ChainRowCtx {
            project_session: project_session.clone(),
            project_chains: project_chains.clone(),
            looper_live: looper_live.clone(),
            saved_project_snapshot: saved_project_snapshot.clone(),
            project_dirty: project_dirty.clone(),
            input_chain_devices: input_chain_devices.clone(),
            output_chain_devices: output_chain_devices.clone(),
            toast_timer: toast_timer.clone(),
            auto_save,
            pending_delete_chain_id: std::rc::Rc::new(std::cell::RefCell::new(None)),
        },
    );
    // #791: Tone Doctor's run/apply for the main chains page. Wired here
    // because its taps need the runtime handle, which this module owns —
    // `chain_row_wiring` used to forward it and now names no audio backend.
    crate::tone_doctor_compact_wiring::wire_main(
        &window,
        project_session.clone(),
        Rc::clone(&audio_taps),
        toast_timer.clone(),
    );
    // #614: DI loop file picker — separate module because chain_row_wiring
    // is forbidden from using rfd:: (issue #511).
    crate::di_loop_chooser_wiring::wire(&window, project_session.clone(), toast_timer.clone());
    crate::chain_rig_nav_wiring::wire(
        &window,
        crate::chain_rig_nav_wiring::ChainRigNavCtx {
            project_session: project_session.clone(),
            project_chains: project_chains.clone(),
            runtime_attach: runtime_attach.clone(),
            input_chain_devices: input_chain_devices.clone(),
            output_chain_devices: output_chain_devices.clone(),
            toast_timer: toast_timer.clone(),
            saved_project_snapshot: saved_project_snapshot.clone(),
            project_dirty: project_dirty.clone(),
            auto_save,
        },
    );
    crate::plugin_info_inline_wiring::wire(&window);
    // Ao fechar a janela principal, encerra todo o processo
    window.window().on_close_requested(|| {
        let _ = slint::quit_event_loop();
        slint::CloseRequestResponse::HideWindow
    });

    // Per-chain latency badge: measurement is taken synchronously by
    // `latency_probe::install_handler`; this timer only clears each
    // badge after its 10-second display window expires.
    let _latency_timer = latency_probe::install_expiry_timer(
        window.as_weak(),
        project_chains.clone(),
        probe_windows.clone(),
    );

    // Virtual keyboard: dispatch key events to the focused element
    // Virtual keyboard (extracted to virtual_keyboard_wiring)
    crate::virtual_keyboard_wiring::wire(&window);

    // ── MCP server (opt-in, --mcp[=addr]) ──────────────────────────────────
    // A complementary network server on the live instance: an agent drives
    // the same `ProjectSession` the user has open. The server runs on its own
    // thread (tokio); commands/queries cross the `!Send` boundary via the
    // bridge and are serviced here on the Slint event-loop thread (same place
    // GUI callbacks dispatch), so GUI and MCP share one project with no lock.
    // Bound for the whole `window.run()` so the timer keeps firing.
    let _mcp_drain_timer = match mcp_addr {
        Some(addr) => Some(crate::desktop_app_mcp::start(
            addr,
            &window,
            &project_session,
            crate::chain_rig_nav_wiring::ChainRigNavCtx {
                project_session: project_session.clone(),
                project_chains: project_chains.clone(),
                runtime_attach: runtime_attach.clone(),
                input_chain_devices: input_chain_devices.clone(),
                output_chain_devices: output_chain_devices.clone(),
                toast_timer: toast_timer.clone(),
                saved_project_snapshot: saved_project_snapshot.clone(),
                project_dirty: project_dirty.clone(),
                auto_save,
            },
            crate::desktop_app_mcp::McpDeps {
                project_runtime: project_runtime.clone(),
                tuner_session,
                spectrum_session,
            },
        )?),
        None => None,
    };

    // ── MIDI/BLE-MIDI controller adapter (opt-in, --midi[=PATH]) ───────────
    // Same complementary-input pattern as MCP; wiring extracted to keep this
    // file within the size cap. Bound for the whole `window.run()`.
    let _midi_drain_timer = match midi_map {
        Some(arg) => Some(crate::midi_adapter_wiring::wire(
            window.as_weak(),
            crate::chain_rig_nav_wiring::ChainRigNavCtx {
                project_session: project_session.clone(),
                project_chains: project_chains.clone(),
                runtime_attach: runtime_attach.clone(),
                input_chain_devices: input_chain_devices.clone(),
                output_chain_devices: output_chain_devices.clone(),
                toast_timer: toast_timer.clone(),
                saved_project_snapshot: saved_project_snapshot.clone(),
                project_dirty: project_dirty.clone(),
                auto_save,
            },
            arg,
        )?),
        None => None,
    };

    window.run().map_err(|error| anyhow!(error.to_string()))
}
