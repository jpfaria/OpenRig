# Architecture

## Layers

Dependencies point inward: the core knows nothing about Slint, audio drivers or file formats, so it is tested without them and every frontend (GUI, MCP, MIDI, console, render, a future gRPC server) drives the same core.

- **Domain and project model** (`domain`, `project`) — IDs, value objects, chains, blocks, parameters, validation. No I/O.
- **Application** (`application`) — the command bus: every state change is a `Command`; `State`/`Event`/`SideEffect` have no Slint dependency.
- **Engine** (`engine`) — the real-time audio graph: one runtime per input stream, blocks processed within the callback deadline.
- **Blocks and backends** (`block-*`, `nam`, `ir`, `lv2`, `vst3-host`, `plugin-loader`) — the models and the engines that run them.
- **Adapters and infrastructure** (`adapter-*`, `infra-*`) — frontends, audio devices (cpal), YAML and filesystem.

## Crates

| Group | Crate | What it is |
|---|---|---|
| Core | `domain` | IDs, value objects, I/O bindings, mixer strips |
| | `project` | Project model: chains, blocks, split rules, block catalog |
| | `application` | Command bus, dispatcher, read bus, persistence worker |
| | `engine` | Real-time runtime: chain runtimes, segments, splits, taps, offline render |
| Blocks | `block-core` | `BlockProcessor`, `AudioChannelLayout`, `ParameterSet`, instrument constants |
| | `block-preamp`, `block-amp`, `block-cab`, `block-gain`, `block-delay`, `block-reverb`, `block-mod`, `block-dyn`, `block-filter`, `block-wah`, `block-pitch` | One block type each, with its native models |
| | `block-body`, `block-util`, `block-full-rig`, `block-routing` | Block types with no native models |
| | `block-ir`, `block-nam` | The generic IR and NAM loaders |
| Backends | `nam` | Neural Amp Modeler (vendored C++ core via `cpp/nam_wrapper`) |
| | `ir` | Partitioned FFT convolution |
| | `lv2` | LV2 host |
| | `vst3-host` | VST3 host |
| | `plugin-loader` | Discovers and loads plugin packages (manifests, grids, captures) from the bundled `plugins/source` tree and the user root |
| | `feature-dsp` | Tuner pitch detection, spectrum FFT, metronome, tone descriptors |
| Adapters | `adapter-gui` | The desktop app (Slint, `ui/`) |
| | `adapter-mcp` | MCP server (library, `rmcp`, Streamable HTTP) on the live instance via `application::bridge` — see [mcp.md](mcp.md) |
| | `adapter-midi` | MIDI controllers: devices, learn, mappings, feedback |
| | `adapter-console`, `adapter-console-rig` | Terminal frontend; headless rig |
| | `adapter-render` | Offline render, binary `openrig-render` — see [render.md](render.md) |
| | `adapter-server`, `adapter-vst3` | Reserved; no product yet |
| | `ui-openrig` | UI types shared across frontends |
| Infrastructure | `infra-cpal` | Audio devices and streams (cpal; JACK on Linux) |
| | `infra-yaml` | Project and preset YAML |
| | `infra-filesystem` | Config, paths, MIDI profiles, I/O bindings on disk |
| Tools | `tone-calibrate` | Offline calibration of the tone analysis limits (`openrig-tone-calibrate`) |

Notes on the core crates:

- `application` — `Command`/`Event`, `LocalDispatcher`, `bridge` (the `Send`↔`!Send` bridge for MCP/gRPC), `PublishingDispatcher` (event fan-out), `command_schema` (tool schema per variant), `persist_worker` (disk writes run on a dedicated worker thread — handlers serialize in memory and enqueue; `persist_worker::flush()` is the durability barrier used on shutdown and in save→reload round-trips; dispatch never blocks the calling thread on I/O), `app_config_persist` (`config.yaml` writes bind the destination path at dispatch time and hand it to the worker — the worker never re-resolves `$HOME` at write time, so a HOME-swap test can't leak fixtures onto the user's real config).
- `engine` — `ChainRuntimeState`, `process_input_f32` / `process_output_f32`, lock-free graph rebuild via `update_chain_runtime_state`. Fast path: `set_block_enabled` flips `FadeState` on the live `BlockRuntimeNode`, so a per-block toggle never rebuilds the chain. The public surface is re-exported through `engine::runtime::*`.
- `infra-cpal` — `ProjectRuntimeController` owns the per-chain cpal streams. `upsert_chain` is the rebuild path. Switching a chain off kills every stream it owns, open or still being activated (`kill_chain_streams`), so nothing lands later and plays a chain the screen shows as off; switching it on is a fresh activation. `set_block_enabled` forwards into the engine's block-level fast path.
- `adapter-render` — binary `openrig-render` and lib `adapter_render::render()`. Loads a project, decodes an input WAV, drives `engine::offline::render_chain` (the same `RuntimeProcessor::process_buffer` as the realtime callback) and writes the output WAV atomically. No Slint, no MIDI; one chain, deterministic. `Command::RenderChain` routes to `application::render_handler::run`, which calls `adapter_render::render()`: the dependency goes application → adapter-render, and the render pipeline lives only in `adapter-render`.

## Read bus: `LiveSource` + `application::read::resolve`

Writes cross every frontend the same way, through `dyn CommandDispatcher`
(`crates/application/src/dispatcher.rs`) — the GUI holds it as
`Rc<dyn CommandDispatcher>` (`adapter-gui/src/state.rs`), never a concrete
`LocalDispatcher`. Reads use the mirror abstraction: `application::read::resolve`
(`crates/application/src/read.rs`) is the `QueryKind` matcher every FRONTEND
calls, fed a `ReadContext` that borrows the project/rig/io-bindings, the
dispatcher, and one `&dyn LiveSource` (`crates/application/src/live_source.rs`).
No frontend keeps a second copy of the match — see
`adapter-gui/src/mcp_query_resolver.rs` and `adapter-console/src/main.rs`'s
`console_resolve`. Neither path holds a concrete dispatcher, and neither
answers a read itself: `mcp_query_resolver.rs` still names the runtime handle,
but only to hand it to `GuiLiveSource` (see the guard at the end of this
section).

It is not, however, the only matcher in the process. `CommandBridge::query`
(`crates/application/src/bridge.rs`) tries `resolve_off_frontend` FIRST, and
that is a second exhaustive `match` which answers 11 of the 20 kinds — the five
catalog/paths kinds plus `ProjectYaml`, `Ids`, `ListChainPresets`,
`ListProjectPresets`, `GetBlockParams` and `ChainQualityReport` — without ever
reaching `read::resolve`. Those kinds are derivable from the
published `snapshot` (or from process-global catalogs), so serving them inline
keeps an MCP client off the frontend tick's queue. The cost is a real, known
divergence, recorded in the table below: the fast path carries its own "no rig
attached to the session" literals instead of `read::NO_RIG_ATTACHED`, and
`ProjectYaml` answers from `snapshot::latest()` on that road and from the live
`Project` on the frontend road. Anything runtime- or GUI-coupled (`Devices`,
`ChainMeters`, `ChainLoopers`, the analyzers, `DiLoopState`, `ChainLatency`,
`ChainToneReport`, `MetronomeState`) still queues for the frontend and goes
through `read::resolve`, as does everything before the first snapshot exists.

`LiveSource` covers everything that only exists inside a frontend's own audio
runtime — chain meters, tuner, spectrum, DI loop, loopers, a chain's real
sample rate, the block errors the audio thread reported, the backend's health,
the device list. A frontend implements ONLY the methods it actually hosts; every
other method keeps the trait's default `None`. `None` means "not hosted",
never "hosted but empty" — `read::resolve` is the one place that turns an
unhosted read into the documented empty shape (an empty `Vec`, `running:
false`, `SILENT_DBFS` rows); it is not the frontend's job to invent that
payload, and it is not an error. `adapter-console`'s `ConsoleLiveSource` is
the reference for a frontend that hosts almost nothing: it answers
`devices()` and the per-chain rate half of `chain_loopers()` (it drives the
engine directly and needs a real rate to report), and leaves every other
method at the default.

Two methods carry a tri-state instead of the plain `Option`: `devices()` and
`chain_loopers()` both return `Option<Result<…>>` — not hosted / hosted-but-
failed / hosted. `None` ⇒ not hosted (the resolver answers the documented
empty shape); `Some(Ok(_))` ⇒ hosted and resolved; `Some(Err(_))` ⇒ hosted
but this call failed (a dead audio host, an unresolvable chain rate), and
that failure keeps its error instead of degrading into an empty answer or a
fabricated value. A chain's sample rate in particular is never invented:
a stopped GUI or a console with no device for a chain reports
the failure, not a hardcoded 48 kHz.

Rate parity: the latency probe (the sonar badge and
`openrig://chains/{id}/latency`) asks `LiveSource::chain_sample_rate` before it
falls back to the dispatcher's `engine_sr`. `engine_sr` mirrors a RUNNING
stream and goes back to `REFERENCE_SAMPLE_RATE` when the rig stops, so asking
it first would report DSP latency measured at 48 kHz for a stopped rig on a
44.1 kHz interface. The GUI answers the new door the same way it answers
`chain_loopers`: the controller's rate while something runs, otherwise the rate
the chain's own devices resolve to. Neither source is ever a guess — a frontend
that cannot resolve one answers `None`.

Meter parity: `GuiLiveSource::chain_meters` (`adapter-gui/src/gui_live_source.rs`)
reads the same `ProjectChainItem` rows the IN/OUT bars are bound to — never a
second poll of the audio taps — so the screen and any other reader of the
same chain cannot disagree, and nothing extra runs on the audio path. Only
reduced readings (dBFS, note/cents, band levels, looper position/state) ever
cross the `LiveSource` boundary; no raw PCM buffer or stream handle does.

### What a chain ROW reads

The meter tick redraws each chain row from three per-chain readings the project
cannot answer: `chain_loopers` (what the loops are doing, and the rate they
count at), `chain_di_loop` (is the DI's dedicated stream playing, and its OWN
peaks) and `chain_runtime` (`live` + `xruns` + `underruns`).
All three carry the chain's identity: there is no pooled "the rig's xruns", and
a row shows its own stream's health or nothing. `GuiLiveSource::di_loop` (the
whole project, for MCP) and `ChainRowLiveSource::chain_di_loop` (one chain, for
the tile) go through the same helper, so the screen and the transport cannot
drift.

### A block's diagnostic stream

A utility block may publish a small table of already-reduced entries (`key` /
`value` / `text` / `peak`) from its worker thread, which its editor panel
renders. That is a plain reading, not a tap, so it is
`LiveSource::block_stream` (impl `gui_live_source::BlockStreamLiveSource`),
tri-shaped like the rest of the trait: `None` ⇒ nothing hosted (the panel keeps
what it shows), `Some(vec![])` ⇒ hosted and this block publishes nothing (the
panel goes inactive). The detached editor, the inline drawer and the compact
view all read it through the seam.

## Subscription seam: `AudioTaps`

`LiveSource` returns finished values and a `Command` cannot return a ring, so
neither can express the third thing a frontend needs: a **standing tap** it
opens once and polls on its own tick. That is
`application::audio_taps` (`crates/application/src/audio_taps.rs`):

- `TapPoint` names WHAT is tapped, by the identity of the stream that produces
  it — `StreamInput { chain, stream }`, `StreamOutput { chain, stream }`,
  `InputChannels { chain, input, channels, .. }`. There is no way to express
  "every runtime at this rate" or "all that match" (`CLAUDE.md` LAW).
- `AudioTaps::subscribe` returns an `Arc<dyn AudioTap>`: the subscription. It is
  `Send + Sync` (a capture may be handed to a worker) while the authority that
  issues it is frontend-local.
- `AudioTap::poll_peak_dbfs` is the REDUCED reading — a finished number, what
  the meters use, implementable over any transport.
  `AudioTap::drain_channel` hands out the raw window and **defaults to
  nothing**: samples are an in-process affordance. A frontend that implements
  only the reduced method is complete, not broken.

The consumers that need raw windows — the tuner (YIN), the spectrum (FFT) and
the Tone Doctor — run next to the audio and publish their RESULTS through
`LiveSource::tuner` / `LiveSource::spectrum` and the `DiagnoseChainTone`
command, so a remote frontend is served finished readings and the PCM never has
to travel. Both poll methods CONSUME the window they report: a tap is
single-consumer, as the underlying SPSC ring always was.

The GUI's implementation is `GuiAudioTaps` in `adapter-gui/src/runtime_taps.rs`
— the only module that turns a `TapPoint` into a `ProjectRuntimeController`
subscription. It wraps the very rings the consumers used to hold directly, so
the audio thread's work is unchanged.

## Write bus: `RuntimeControl`

A `Command` is the only way to change state, so a state change that has to
reach the audio runtime needs a door the DISPATCHER can knock on — otherwise
the GUI applies it in its own callback and the same command over MCP/gRPC/MIDI
emits its event and changes nothing audible. That door is
`application::runtime_control::RuntimeControl`
(`crates/application/src/runtime_control.rs`), the write-side mirror of
`LiveSource`. The frontend that hosts the runtime implements it and hands the
dispatcher a shared instance via
`CommandDispatcher::attach_runtime_control`; a transport that owns no audio
attaches nothing and every method keeps its default no-op — the command still
succeeds and still reports its event.

The GUI's implementation is `GuiRuntimeControl`, in
`adapter-gui/src/runtime_lifecycle.rs` — the one module that owns the
controller. Current doors: `set_output_muted` (rig-wide, the tuner's mute),
`set_io_bindings`, `set_block_enabled` (the in-place fade toggle — never a
stream restart), `sync_chain`, the teardown pair `stop_project_runtime`
(rig-wide) / `remove_chain` (one chain), the device-settings pair
`apply_device_settings` (make the MACHINE's drivers adopt the new rate — body
in `adapter-gui/src/runtime_devices.rs`) / the whole-graph `sync_project`, the DI
stream trio `arm_di_stream` / `disarm_di_stream` / `refresh_di_stream`, the
metronome's `start_metronome` / `stop_metronome` / `set_metronome_settings` /
`refresh_metronome_output`, the analyzers' `set_tuner_running` /
`set_spectrum_running`, and the looper's `create_looper` / `remove_looper` /
`looper_transport` / `set_looper_param` / `set_looper_input` /
`set_looper_output` / `export_chain_loops`. The bodies of the DI and metronome
families live in `adapter-gui/src/runtime_pipelines.rs` (both are INDEPENDENT
pipelines, invariant #4, and share the two rules below where the chain doors do
not) — together with `ensure_runtime`, the lazy creation only a START may
run; the looper bodies live in `adapter-gui/src/runtime_loopers.rs`, the two
teardowns in `adapter-gui/src/runtime_teardown.rs`, and the analyzers' in
`adapter-gui/src/runtime_analyzers.rs`.

**The analyzers are on the bus too.** `SetTunerEnabled` /
`SetSpectrumEnabled` apply the door in the handler: the session that subscribes
to the taps and runs YIN / the FFT is built there, not in the GUI's POWER
callback, so the `toggle_tuner` / `toggle_spectrum` footswitch slots in
`adapter-midi`, an MCP client and the window all power the same analyzer; the
windows only render the row model the analyzer hands them, re-bound on every
rebuild. Neither door may wake audio: an analyzer reads
what is already sounding, so over a stopped rig it subscribes to nothing and the
reading stays empty.

Three doors are NOT reached through a command handler:
`apply_finished_rebuilds` and `reconnect_audio` (the frontend's own poll tick,
below) and `reconcile_chain_loopers` (the meter tick's per-chain looper
reconcile: slots, the recording drain, the playback streams). They are on the
same trait because they are still writes to the runtime, and the module that
performs them must reach it through this seam like everyone else. The
consequence of `reconcile_chain_loopers` not being a `Command` is worth naming:
the frontend that hosts the audio must keep ticking for a RECORD started from
ANY transport to actually capture.

Two rules the DI trio makes explicit:

- **Isolation** (`CLAUDE.md` LAW): every door that addresses a stream carries
  that stream's identity — the whole `Chain` for an arm (it resolves that
  chain's `di_output` and renders through a copy of its graph), the `ChainId`
  for a disarm. Never a group, never "every chain at this rate".
- **Only an arm may start audio.** `arm_di_stream` runs `ensure_runtime`
  first, because the DI is an independent pipeline that must play with no
  chain enabled — the runtime is a precondition of the thing the user
  asked to hear. `sync_chain`, `disarm_di_stream` and `refresh_di_stream` must
  never create a controller: a sync, a stop or an output pick is not a request
  to hear anything, and audio starting behind the user's back from any
  transport is the failure mode. `refresh_di_stream` is a separate door from
  `arm_di_stream` for exactly this reason — it follows a change (the picked
  output; the device rate) only for a stream that is ALREADY
  sounding.

`attach_engine_sr` is where a device-rate change becomes a refresh: it stores
the new rate and asks the control to re-arm each chain whose loop is playing,
so a loop resampled to the old rate never drags on against a rebuilt runtime.

### Stopping, rebuilding and deleting

Three teardown/rebuild sequences are applied by the command handler, so the
same command from any transport changes what is heard:

| door | command that applies it | what a remote client can do |
|---|---|---|
| `stop_project_runtime` | `ProjectCommand::StopProjectRuntime`, and `ProjectCommand::CloseProject` | **stop the rig** — a client that can START audio (enable a chain, play a DI) can also silence it, and a `CloseProject` closes every stream |
| `sync_project` | `SettingsCommand::SaveAudioSettings` | change the device settings and have them take effect |
| `remove_chain` | `ChainCommand::RemoveChain` | delete a chain and hear it stop |

Three rules worth naming:

- **`remove_chain` is NOT `sync_chain` for a chain that is gone.** The
  lookalike validates the WHOLE project first, so an unrelated invalid chain
  would abort the teardown and leave a deleted chain sounding; it is also a
  different sequence (device resolve, activation scheduling). The delete
  removes the stream the command already removed from the project, and nothing
  else — a delete must never touch a neighbour's runtime (invariant #4).
- **`sync_project` is whole-project, never rate-grouped.** It walks the chains
  the PROJECT names, one at a time, each against its own resolved devices. It
  exists because the device settings apply to every device at once, which no
  per-chain sync can express. "Whole project" is an explicit list of chain
  identities, never a filter over live runtimes by sample rate (`CLAUDE.md`
  LAW).
- **Neither may start audio.** A stop, a delete and a device-settings rebuild
  all follow what is already running; with no controller there is nothing to do
  and the rig stays stopped. Only an arm may wake audio (above).

Every teardown ends by re-publishing the engine sample rate, so "nothing
running" reads as the reference rate instead of the rate of a device that is no
longer open.

The three modules that OPEN a project (`project_file_dialog_wiring`,
`recent_projects_wiring`, and `chain_rig_nav_wiring`'s external-event drain)
still have to hand a freshly built session's dispatcher this frontend's runtime
control. They do it through `runtime_lifecycle::RuntimeAttach`, a capability
whose handle is private and whose only operation is `to_session` — so a wiring
module can wire the seam up without being able to reach through it.

### The looper's runtime half is the dispatcher's

A loop lives in the controller-owned looper store, not in the project: the
project only remembers that a looper EXISTS and where its knobs are. Every
`LooperCommand` therefore has a runtime half, and the command handler applies
it — never the GUI callback that dispatched — so a looper driven from a
footswitch or an MCP client records like one driven from the panel.

Each door is handed the chain AFTER
the project half ran, and each ends by reconciling THAT chain's isolated
playback stream, so a closed loop sounds and a removed one goes quiet on the
user's action instead of on the next ~15 Hz meter tick. `PlayStop` travels
whole: only the store knows whether that one button means play or stop, so
resolving it in a frontend would give each transport its own answer.

**Only an add or a transport START may wake audio.** The panel arms REC
only against a live store, so `create_looper` and a Record / Play / PlayStop
`looper_transport` run `ensure_runtime` first; Stop, Clear, Undo, Redo, the
knobs and the endpoint picks never may — silencing is not a reason to open a
device.

**The recorded PCM moves as a HANDLE, and never through a reading.**
`export_chain_loops` returns `Arc<engine::LoopPcm>` per loop, each carrying the
rate it was recorded at, and `ProjectCommand::SaveProject` writes them as the
wav sidecars under `<project>.loops/` before it serializes the project, so a
save issued over MCP/gRPC writes the loops too. The door is tri-state on purpose:
`None` means no store is hosted (the rig is stopped) and every saved pointer
must be left alone, which is NOT the same as "nothing was recorded". The
restore is deliberately not a `Command`: nobody asks for it, it is a
precondition of a controller existing, so it hangs off runtime creation
(`runtime_loopers::restore_chain_loops`) where every transport passes.

The read half is `LiveSource::chain_loopers`: the loops' transport state and
the rate they are counted at. The looper panel redraws from it
(`gui_live_source::LooperLiveSource`) and MCP serves the same values — no PCM
ever crosses that seam.

### The metronome's state is the dispatcher's

The click's SETTINGS live in the dispatcher, not in the GUI, so a MIDI
footswitch bound to `toggle_metronome` or an MCP `set_metronome_enabled` is
heard like the window's own knob: `application::metronome_state::MetronomeControlState`
(settings, chosen output endpoint, POWER, tap history, and the `config.yaml`
they persist to), attached per session by `adapter-gui/src/state.rs` and read
back as a `MetronomeSnapshot` through `CommandDispatcher::metronome_snapshot`.
A handler therefore validates the value, stores it, persists it and applies it
through `RuntimeControl` — once, for every transport. The GUI window renders
that snapshot (`metronome_events::render_settings`) and translates knob indices
to command keys (`metronome_view.rs`); it holds no metronome state at all.

`MetronomeSettings` stays in `feature-dsp` (`engine::metronome_state`
re-exports it) rather than being duplicated in `application`: it is six scalars
plus two enums with no DSP state and no device knowledge, `application` already
depends on `feature-dsp`, and a second copy would put the `Subdivision` /
`Timbre` vocabulary in two places. The chosen output travels as an opaque
endpoint KEY — `application` never learns which device or channels it names;
the frontend that owns the audio host resolves it.

The read half is `LiveSource::metronome`: where the click is in the bar, and
whether it is sounding, as the audio side reports it. The GUI's beat lamps
sample it on a timer (a phase, never a queue of beat events) and MCP serves the
same values at `openrig://metronome`.

### The poll tick, split by what it does

`adapter-gui/src/desktop_app_polling.rs` runs two timers for the life of the
app, split by what each one IS:

- **Reads** — `LiveSource::block_errors` and `LiveSource::audio_health`,
  implemented by `gui_live_source::HealthLiveSource`. An error reading carries
  the CHAIN whose runtime raised it (`ProjectRuntimeController::poll_errors`
  drains each runtime's own queue and tags it), so a failure is never an
  anonymous entry from "some stream".
- **Writes** — `RuntimeControl::apply_finished_rebuilds` (the
  control worker builds a live edit off-thread and the tick swaps it into the
  live slot — a mutation, whatever the controller's `poll_` name suggests) and
  `reconnect_audio` (tear down and re-open after the backend died). Bodies in
  `adapter-gui/src/runtime_health.rs`.

Neither write is a `Command`. A tick is nobody's request: dispatching it would
put an event on the bus five times a second for work the frontend queued for
itself. A reconnect changes no project state either — it re-opens the devices
THIS machine lost, the same judgement `ensure_runtime` gets. So an MCP/gRPC
client cannot pump another frontend's rebuild queue or force its reconnect.

`block_errors` is also deliberately not a `QueryKind`: the read DRAINS the
engine's queue, so it can only ever have one consumer — a second transport
polling it would take the toasts away from the window.

### What is deliberately NOT on the bus

Every state change is born a `Command` and every shared read is a `QueryKind`.
The three seams carry a few things that are neither, and each is a decision, not
an omission:

| thing | what it is | why it is not on the bus |
|---|---|---|
| `runtime_pipelines::ensure_runtime` | frontend-local | nobody asks for "bring the audio up": it is the precondition of an arm, a looper add or a transport START, and each of those IS a command. On its own it would be audio starting behind the user's back |
| `runtime_lifecycle::RuntimeAttach` | a capability the frontend holds | it only wires this frontend's seams onto a freshly opened session's dispatcher; its handle is private and its one operation is `to_session` |
| `application::audio_taps::AudioTaps` | a capability the frontend holds | a subscription is not a value — a `Command` cannot return one and a `QueryKind` cannot keep one open. A remote frontend implements the seam against its own transport |
| `RuntimeControl::apply_finished_rebuilds` / `reconnect_audio` / `reconcile_chain_loopers` | writes on the frontend's own tick | a tick is nobody's request, and a reconnect changes no project state. Consequence, stated: the frontend that hosts the audio must keep ticking for a RECORD started from ANY transport to capture |
| `CommandBridge::resolve_off_frontend` (`bridge.rs`) | a second `QueryKind` matcher | the non-blocking path: 11 of the 20 kinds are derivable from the published snapshot or from process-global catalogs, so an MCP client gets them inline instead of queueing behind the frontend tick. A known divergence, not a second bus: it carries its own no-rig literals instead of `read::NO_RIG_ATTACHED`, and `ProjectYaml` answers from `snapshot::latest()` there and from the live `Project` on the frontend road. Merging the two resolvers is its own change |
| `LiveSource::block_errors` | a DRAINING read | exactly one consumer is possible; a second transport polling it would take the window's toasts. Sharing it needs a non-destructive shape first, which is a design and not a rename |
| `audio_devices::invalidate_device_cache` (called by `project_settings_wiring.rs`, `device_refresh_apply.rs`) | frontend-local | it drops this frontend's cached ENUMERATION so the next refresh sees hardware that was just plugged in. It changes no project state and answers no question a remote client asked — it is the read path of the device pickers, the `ensure_runtime` / `reconnect_audio` judgement again |
| `runtime_devices::start_jack_in_background` (called by `compact_chain_block_handlers.rs`, Linux) | frontend-local | it is a non-blocking PRE-WARM with a progress toast, not the thing that makes audio work: `ProjectRuntimeController::ensure_jack_servers` already starts the server the chain needs, so a chain enabled over MCP gets jackd started by the runtime. Putting it on the bus would publish "this machine's daemon is booting" as project state |
| `LiveSource::audio_health` / `chain_runtime` / `block_stream` | non-destructive reads | each could honestly become a `QueryKind`; none has, because publishing one means choosing which numbers a remote client sees for a client that has not asked. `chain_di_loop` is not new surface at all — it is the per-chain half of the `openrig://di` arm, through the same helper |

### The guard: the UI may not name the backend

`crates/adapter-gui/src/no_infra_cpal_in_wiring_tests.rs` is that invariant as a
test. A module that names `infra_cpal::ProjectRuntimeController` has neither
door — it reaches the engine directly, which is exactly how a capability ends up
working in the GUI and silently doing nothing over MCP/gRPC — so the modules
allowed to name it are an explicit list, pinned in BOTH directions: a module
outside the list that names the backend fails as an offender, and a listed
module that no longer names it fails as stale. The list can therefore only
shrink; it is a ratchet, not a graveyard, and every entry carries its
justification in the test's ledger.

The entries are all of one kind. They OWN or CONSTRUCT the runtime
handle — `runtime_lifecycle.rs` (creates it and hosts `GuiRuntimeControl`),
`runtime_pipelines.rs`, `runtime_teardown.rs`, `runtime_loopers.rs`,
`runtime_taps.rs` and `runtime_health.rs` (the door bodies, split off as the
first file hit its line cap), and `desktop_app.rs` (allocates the shared
`Rc<RefCell<Option<..>>>` once) — or they read it as the frontend's `LiveSource`
(`gui_live_source.rs` and `mcp_query_resolver.rs`, which builds it). A new name
on that list is a regression, not growth.

The guard is about the CRATE, not only that handle. `AudioDeviceDescriptor` — an id, a name and a channel
count — appears in wiring signatures, so it lives in
`domain::AudioDeviceDescriptor`, where a remote or Flutter frontend can use it
without linking CPAL: `infra-cpal` produces it (`list_devices`,
`list_input_device_descriptors`, `list_output_device_descriptors`) and
deliberately does not re-export it, so there is one name for it and it is the
neutral one. The device operations a frontend genuinely owns are concentrated in
two modules — `audio_devices.rs` (enumeration and its cache) and
`runtime_devices.rs` (driver reconfiguration, and starting jackd on Linux) — and
the guard now rejects the identifier `infra_cpal` itself in any other module,
whatever it names in it.

A sibling test pins the behavioural half: `sync_live_chain_runtime` — the
sequence that resolves devices, schedules activation and rebuilds a chain's
DSP — has exactly ONE caller, `GuiRuntimeControl::sync_chain`, so the GUI and
MCP/MIDI reach the audio by the same road. All
three assertions run against the source with `//` comments stripped, so no prose
can satisfy them or trip them.

## Chain split runs inside one segment

`RuntimeProcessor::Split` is a node of the segment like any block. It holds
every path of the split, one buffer per path (preallocated to
`SEGMENT_FRAME_CAPACITY`; a larger callback is processed in chunks of that
size, so it never grows), one alignment delay line per path and the knobs as
atomics loaded once per callback. The mix is DSP inside that node — never a sum
of two segments or two runtimes — so each segment of a chain runs its own split
and stream isolation holds by construction. Every path but the first runs on a
lane of its own: a realtime thread spawned at build and parked between
callbacks, released with an atomic store plus an unpark and joined by a short
spin then a park, so one core never carries every path of a split and a
waiting thread uses no CPU.

A Y is not a node of its own: per output segment the builder shapes it into a
Split → Mix whose mixer passes at unity the paths that lead to that output's
leaves, with the other paths left empty at level zero (`split_segment_view`,
see [audio-config.md](audio-config.md) → "Y outputs"). Splits nest at any
depth, so one segment may run several split nodes. Every Y output runs its own
copy of everything on the way to its leaves — one pipeline per output, never a
shared pre-Y runtime. The project-side lookups (`project::block::split_lookup`)
never assume a chain holds a single split: they find one by its end (Mix or Y).

Files: `runtime_split.rs` routes `runtime_split_builder.rs` (model → node),
`runtime_split_state.rs` (what a split keeps), `runtime_split_process.rs` (one
callback), `runtime_split_lanes.rs` (one thread per extra path),
`runtime_split_mix.rs` (pure per-sample math), `runtime_split_align.rs` (delay
line), `runtime_split_knobs.rs` (atomics), `runtime_split_latency.rs` (path
latency, from each processor's `latency_samples()`) and `runtime_split_walk.rs`,
which the read-only node walkers (bypass mirror, offline faulted list, probe
summary) use to see inside the paths; the block toggle descends into the paths
itself because it mutates them.

## Generated model registry

Each block crate's `build.rs` (e.g. `crates/block-preamp/build.rs`) scans
`src/*.rs` for `MODEL_DEFINITION` and generates `generated_registry.rs`. A new
native model is a new `.rs` file with `pub const MODEL_DEFINITION` — see
[creating-blocks.md](development/creating-blocks.md).

`PreampModelDefinition` (`crates/block-preamp/src/registry.rs`) has `id`,
`display_name`, `brand`, `backend_kind`, `schema`, `validate`, `asset_summary`,
`build`, `supported_instruments` and `knob_layout`. Public lookups:
`preamp_display_name`, `preamp_brand`, `preamp_type_label`. Brand, type and
display name live only in Rust, never in YAML. Plugin models (NAM, IR, LV2,
VST3) are not in these registries: `plugin-loader` reads them from the plugin
packages.

## Assets

```
assets/brands/{brand}/logo.{svg,png}   one per catalog brand id; single colour on transparent, tinted by BrandLogo (colorize) — only Vox keeps its colours;
                                       every folder must be mapped in ui/components/brand_logo.slint (pinned by tests/issue_956_brand_logos_wired.rs)
assets/models/{model}.svg              panel artwork of a native model, wired in ui/components/block_panel_brand_strip.slint
assets/blocks/metadata/{lang}.yaml     description, license and homepage per model
assets/blocks/thumbnails/{type}/       block thumbnails (_default.png per type)
```

A brand logo is never drawn inside the artwork of the gear.

## Block editor panel

Every block, native or plugin, is edited in the same panel, rendered from the
model's parameter specs by the block editor's grid. A native model can place its
knobs over its artwork with `knob_layout` and recolour the panel through its
crate's `model_visual.rs`.

### EQ curve: which sample rate it is drawn at

The EQ / curve-editor preview is a filter response, so it depends on the
sample rate — near Nyquist the same knobs draw a different curve at 44.1 kHz
than at 48 kHz. The rate comes from `CommandDispatcher::engine_sr()`, which the
runtime lifecycle keeps in lock-step with the live device
(`runtime_lifecycle::sync_engine_sr_from_runtime`, called on every runtime start,
re-sync and teardown). While a rig is running the curve is drawn at the rate
the device actually negotiated — never a constant.

With **nothing running** the rate is `local_dispatcher::REFERENCE_SAMPLE_RATE`
(48 kHz): the block editor can be open with audio stopped, and the curve is
then illustrative. This is the sanctioned no-device value, not a live-path
assumption. All three teardowns put the rate back to the reference, so a curve
is never drawn at the rate of a device that is no longer open:
`stop_project_runtime` (leaving to the launcher, opening or creating another
project), `sync_live_chain_runtime` (disabling the last chain), and
`remove_live_chain_runtime` (deleting it). The last two drop the controller
when nothing is left running — no chain, no pending activation, no armed DI —
so a DI playing with no chain keeps its runtime and its rate. The
consequence for a stopped 44.1 kHz rig is deliberate: the curve redraws at the
reference the moment the streams close.

### Detached editor: one wiring for add and edit

Outside inline (fullscreen/touch) mode, the block editor opens as a detached
`BlockEditorWindow` built per-block by `block_editor_window_setup::create_and_wire`.
Both flows use it: editing an existing block passes `block_index: Some(i)`; adding
a new block passes `block_index: None` (add-mode — no auto-persist, no stream
timer, "add" confirm label, and the block is created only on save, where
`persist_block_editor_draft` inserts when the index is `None`). Because both share
this one setup, the parameter tabs render identically on add and edit — there
is a single detached-editor wiring.
