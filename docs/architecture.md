# Arquitetura

## Crates principais

- `block-core` — `BlockProcessor`, `AudioChannelLayout`, `ParameterSet`, constantes de instrumento
- `block-preamp` / `block-amp` — preamp / amp completo
- `adapter-gui` — UI Slint (`ui/`)
- `adapter-mcp` — servidor MCP (biblioteca, `rmcp` 1.7.0, Streamable HTTP); liga na instância viva via `application::bridge`. Ver `docs/mcp.md` e `docs/superpowers/specs/2026-05-17-165-mcp-server-design.md`
- `application` — command bus: `Command`/`Event`, `LocalDispatcher`, `bridge` (ponte `Send`↔`!Send` p/ MCP/gRPC), `PublishingDispatcher` (fan-out de eventos), `command_schema` (schema de tool por variante), `persist_worker` (#693: side-effect disk writes run on a dedicated worker thread — handlers serialize in memory and enqueue; `persist_worker::flush()` is the durability barrier used on shutdown and in save→reload round-trips; dispatch never blocks the calling thread on I/O), `app_config_persist` (#731: `config.yaml` writes bind the destination path at dispatch time and hand it to the worker — the worker never re-resolves `$HOME` at write time, so a HOME-swap test can't leak fixtures onto the user's real config)
- `engine` — `ChainRuntimeState`, `process_input_f32` / `process_output_f32`, lock-free graph rebuild via `update_chain_runtime_state`. Fast path: `set_block_enabled` (issue #522) flips `FadeState` on the live `BlockRuntimeNode` so a per-block toggle never rebuilds the chain. Public surface is re-exported through `engine::runtime::*`.
- `infra-cpal` — `ProjectRuntimeController` owns the per-chain CPAL streams. `upsert_chain` is the full rebuild path; `pause_chain` + the fast-path resume branch inside `upsert_chain_modal` (issue #522) keep the runtime + streams alive across chain toggles via `set_draining()` so re-enable is O(1). `set_block_enabled` forwards into the engine for the matching block-level fast path.
- `adapter-render` — headless offline render console (issue #552). Binary `openrig-render` and lib `adapter_render::render()`. Loads a project, decodes an input WAV, drives `engine::offline::render_chain` (same `RuntimeProcessor::process_buffer` as the realtime callback), writes the output WAV atomically. No Slint, no MIDI — single-chain, deterministic, used by the audio-validation pipeline (`openrig-tone-analyzer` skill, OpenRig-claude#8). Standalone — never linked into `adapter-gui`. Since #576 it is also a dep of `application`: `Command::RenderChain` routes to `application::render_handler::run`, which calls `adapter_render::render()`. The dep direction (application → adapter-render) was inverted from the originally-declared (and unused) reverse dep — the orchestration itself stays in `adapter-render` so the binary keeps a single source of truth for the render pipeline. See `docs/render.md`.
- `nam` — Neural Amp Modeler
- `asset-runtime` — `EmbeddedAsset`, `materialize()`

## Read bus: `LiveSource` + `application::read::resolve`

Writes cross every frontend the same way, through `dyn CommandDispatcher`
(`crates/application/src/dispatcher.rs`) — the GUI holds it as
`Rc<dyn CommandDispatcher>` (`adapter-gui/src/state.rs`), never a concrete
`LocalDispatcher`. Reads use the mirror abstraction: `application::read::resolve`
(`crates/application/src/read.rs`) is the single `QueryKind` matcher every
transport calls, fed a `ReadContext` that borrows the project/rig/io-bindings,
the dispatcher, and one `&dyn LiveSource` (`crates/application/src/live_source.rs`).
No frontend keeps a second copy of the match — see
`adapter-gui/src/mcp_query_resolver.rs` and `adapter-console/src/main.rs`'s
`console_resolve`. For these two paths the UI holds neither a concrete
dispatcher nor the audio backend directly.

`LiveSource` covers everything that only exists inside a frontend's own audio
runtime — chain meters, tuner, spectrum, DI loop, loopers, the device list. A
frontend implements ONLY the methods for the sources it actually hosts; every
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
fabricated value. A chain's sample rate in particular is never invented
(issue #723): a stopped GUI or a console with no device for a chain reports
the failure, not a hardcoded 48 kHz.

Meter parity: `GuiLiveSource::chain_meters` (`adapter-gui/src/gui_live_source.rs`)
reads the same `ProjectChainItem` rows the IN/OUT bars are bound to — never a
second poll of the audio taps — so the screen and any other reader of the
same chain cannot disagree, and nothing extra runs on the audio path. Only
reduced readings (dBFS, note/cents, band levels, looper position/state) ever
cross the `LiveSource` boundary; no raw PCM buffer or stream handle does.

## Write bus: `RuntimeControl` (#127)

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
`set_io_bindings`, `set_block_enabled` (#522's in-place fade toggle — never a
stream restart), `sync_chain`, the teardown pair `stop_project_runtime`
(rig-wide) / `remove_chain` (one chain), the whole-graph `sync_project`, the DI
stream trio `arm_di_stream` / `disarm_di_stream` / `refresh_di_stream`, the
metronome's `start_metronome` / `stop_metronome` / `set_metronome_settings` /
`refresh_metronome_output`, and the looper's `create_looper` / `remove_looper` /
`looper_transport` / `set_looper_param` / `set_looper_input` /
`set_looper_output` / `export_chain_loops`. The bodies of the DI and metronome
families live in `adapter-gui/src/runtime_pipelines.rs` (both are INDEPENDENT
pipelines, invariant #4, and share the two rules below where the chain doors do
not); the looper bodies live in `adapter-gui/src/runtime_loopers.rs`, and the
two teardowns in `adapter-gui/src/runtime_teardown.rs`.

Two rules the DI trio makes explicit:

- **Isolation** (`CLAUDE.md` LAW): every door that addresses a stream carries
  that stream's identity — the whole `Chain` for an arm (it resolves that
  chain's `di_output` and renders through a copy of its graph), the `ChainId`
  for a disarm. Never a group, never "every chain at this rate".
- **Only an arm may start audio.** `arm_di_stream` runs `ensure_runtime`
  first, because the DI is an independent pipeline that must play with no
  chain enabled (#808) — the runtime is a precondition of the thing the user
  asked to hear. `sync_chain`, `disarm_di_stream` and `refresh_di_stream` must
  never create a controller: a sync, a stop or an output pick is not a request
  to hear anything, and audio starting behind the user's back from any
  transport is the failure mode. `refresh_di_stream` is a separate door from
  `arm_di_stream` for exactly this reason — it follows a change (the picked
  output, #771; the device rate, #669) only for a stream that is ALREADY
  sounding.

`attach_engine_sr` is where a device-rate change becomes a refresh: it stores
the new rate and asks the control to re-arm each chain whose loop is playing,
so a loop resampled to the old rate never drags on against a rebuilt runtime.

### Stopping, rebuilding and deleting (#127)

Three teardown/rebuild sequences used to be GUI function calls made right after
a command was dispatched, so the same command from any other transport changed
nothing audible:

| door | command that applies it | what a remote client can now do |
|---|---|---|
| `stop_project_runtime` | `ProjectCommand::StopProjectRuntime`, and `ProjectCommand::CloseProject` | **stop the rig.** Before this, a client could START audio (enable a chain, play a DI) and had no way to silence it; and a `CloseProject` over MCP left every stream open |
| `sync_project` | `SettingsCommand::SaveAudioSettings` | change the device settings and have them take effect — the save persisted the new rate/buffer and left the audio running on the old ones |
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
  and the rig stays stopped. Only an arm may wake audio (above, #808).

Every teardown ends by re-publishing the engine sample rate, so "nothing
running" reads as the reference rate instead of the rate of a device that is no
longer open (#723/Task 11).

The three modules that OPEN a project (`project_file_dialog_wiring`,
`recent_projects_wiring`, and `chain_rig_nav_wiring`'s external-event drain)
still have to hand a freshly built session's dispatcher this frontend's runtime
control. They do it through `runtime_lifecycle::RuntimeAttach`, a capability
whose handle is private and whose only operation is `to_session` — so a wiring
module can wire the seam up without being able to reach through it.

### The looper's runtime half moved to the dispatcher (#127)

A loop lives in the controller-owned looper store, not in the project: the
project only remembers that a looper EXISTS and where its knobs are. Every
`LooperCommand` therefore has a runtime half, and it used to be applied by the
GUI callback that had just dispatched (`looper_callbacks::dispatch_and_apply`),
with the external-event drain running a second, parallel copy for MCP/MIDI. A
looper driven from a footswitch or an MCP client mutated nothing — Record left
the loop `Empty`.

The `LooperCommand` handlers apply it now. Each door is handed the chain AFTER
the project half ran, and each ends by reconciling THAT chain's isolated
playback stream, so a closed loop sounds and a removed one goes quiet on the
user's action instead of on the next ~15 Hz meter tick. `PlayStop` travels
whole: only the store knows whether that one button means play or stop, so
resolving it in a frontend would give each transport its own answer.

**Only an add or a transport START may wake audio** (#808). The panel arms REC
only against a live store, so `create_looper` and a Record / Play / PlayStop
`looper_transport` run `ensure_runtime` first; Stop, Clear, Undo, Redo, the
knobs and the endpoint picks never may — silencing is not a reason to open a
device.

**The recorded PCM moves as a HANDLE, and never through a reading.**
`export_chain_loops` returns `Arc<engine::LoopPcm>` per loop, each carrying the
rate it was recorded at, and `ProjectCommand::SaveProject` writes them as the
wav sidecars under `<project>.loops/` before it serializes the project — the
GUI's Save callback used to do that, so a save issued over MCP/gRPC wrote a
project whose loops were never written. The door is tri-state on purpose:
`None` means no store is hosted (the rig is stopped) and every saved pointer
must be left alone, which is NOT the same as "nothing was recorded". The
restore is deliberately not a `Command`: nobody asks for it, it is a
precondition of a controller existing, so it hangs off runtime creation
(`runtime_loopers::restore_chain_loops`) where every transport passes.

The read half is `LiveSource::chain_loopers`: the loops' transport state and
the rate they are counted at. The looper panel redraws from it
(`gui_live_source::LooperLiveSource`) and MCP serves the same values — no PCM
ever crosses that seam.

### The metronome's state moved to the dispatcher (#127)

The metronome is the same story with a second half: the click's SETTINGS used
to live in the GUI too (`MetronomeSession`), so `Event::MetronomeEnabledChanged`
was applied only in the GUI's own knob path. A MIDI footswitch bound to
`toggle_metronome`, or an MCP `set_metronome_enabled`, flipped the mirror in
`SelectionState`, got its event, and nothing was ever heard.

The dispatcher now owns them: `application::metronome_state::MetronomeControlState`
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

## Registry auto-gerado

`crates/block-preamp/build.rs` (e equivalentes nos outros block-*) escaneia `src/*.rs` procurando `MODEL_DEFINITION` e gera `generated_registry.rs`. Novo modelo = criar `.rs` com `pub const MODEL_DEFINITION: PreampModelDefinition = ...`.

`PreampModelDefinition` (em `crates/block-preamp/src/registry.rs`) tem: `id`, `display_name`, `brand`, `backend_kind`, `schema`, `validate`, `asset_summary`, `build`. Funções públicas: `preamp_display_name`, `preamp_brand`, `preamp_type_label` (`"native" | "NAM" | "IR"`).

`component.yaml` só tem caminhos de assets e posições SVG (`svg_cx`, `svg_cy`). **NUNCA** colocar brand/type/display_name em YAML — sempre no Rust.

## Assets

```
assets/brands/{marshall,vox,native}/logo.svg   ← logos worldvectorlogo (Marshall: fill="currentColor"; Vox: #53ad99 + #d99346)
assets/amps/{brand}/{model}/controls.svg       ← painel completo (não criar panel.svg separado)
assets/amps/{brand}/{model}/component.yaml     ← caminhos de assets + svg_cx/cy
```

`controls.svg` usa o AC30 como template visual: viewBox 800×200, fundo escuro, círculos como âncoras de knob (`fill="#111" stroke="#505050"`). Controles editáveis têm `id="ctrl-xxx"`; não-editáveis usam `opacity="0.6"` sem id. **Logo do brand NUNCA dentro da imagem do equipamento.**

## BlockEditorPanel

Quando o bloco selecionado é `preamp`, o painel mostra `controls.svg` em vez de só sliders. Implementação em `crates/adapter-gui/ui/pages/project_chains.slint` (propriedades `is-preamp`, `selected-model-id`, ternary chain de `@image-url()` por compile-time). `amp` ainda não tem equivalente.

### EQ curve: which sample rate it is drawn at (#723, #127)

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
is never drawn at the rate of a device that is no longer open (#127):
`stop_project_runtime` (leaving to the launcher, opening or creating another
project), `sync_live_chain_runtime` (disabling the last chain), and
`remove_live_chain_runtime` (deleting it). The last two drop the controller
when nothing is left running — no chain, no pending activation, no armed DI —
so a DI playing with no chain keeps its runtime and its rate (#808). The
consequence for a stopped 44.1 kHz rig is deliberate: the curve redraws at the
reference the moment the streams close.

### Detached editor: one wiring for add and edit (#815)

Outside inline (fullscreen/touch) mode, the block editor opens as a detached
`BlockEditorWindow` built per-block by `block_editor_window_setup::create_and_wire`.
Both flows use it: editing an existing block passes `block_index: Some(i)`; adding
a new block passes `block_index: None` (add-mode — no auto-persist, no stream
timer, "add" confirm label, and the block is created only on save, where
`persist_block_editor_draft` inserts when the index is `None`). Because both share
this one setup, the #780 parameter tabs render identically on add and edit. The
old persistent-window wiring that add used to go through (and that never built
the tabs) was retired in #819 — there is now a single detached-editor wiring.
