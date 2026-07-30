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
(`di_loop_wiring::sync_engine_sr_from_runtime`, called on every runtime start,
re-sync and teardown). While a rig is running the curve is drawn at the rate
the device actually negotiated — never a constant.

With **nothing running** the rate is `local_dispatcher::REFERENCE_SAMPLE_RATE`
(48 kHz): the block editor can be open with audio stopped, and the curve is
then illustrative. This is the sanctioned no-device value, not a live-path
assumption. Stopping a rig — leaving to the launcher, opening another project,
or disabling the last chain — puts the rate back to the reference, so a curve
is never drawn at the rate of a device that is no longer open (#127). The
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
