# Arquitetura

## Crates principais

- `block-core` — `BlockProcessor`, `AudioChannelLayout`, `ParameterSet`, constantes de instrumento
- `block-preamp` / `block-amp` — preamp / amp completo
- `adapter-gui` — UI Slint (`ui/`)
- `adapter-mcp` — servidor MCP (biblioteca, `rmcp` 1.7.0, Streamable HTTP); liga na instância viva via `application::bridge`. Ver `docs/mcp.md` e `docs/superpowers/specs/2026-05-17-165-mcp-server-design.md`
- `application` — command bus: `Command`/`Event`, `LocalDispatcher`, `bridge` (ponte `Send`↔`!Send` p/ MCP/gRPC), `PublishingDispatcher` (fan-out de eventos), `command_schema` (schema de tool por variante)
- `engine` — `ChainRuntimeState`, `process_input_f32` / `process_output_f32`, lock-free graph rebuild via `update_chain_runtime_state`. Fast path: `set_block_enabled` (issue #522) flips `FadeState` on the live `BlockRuntimeNode` so a per-block toggle never rebuilds the chain. Public surface is re-exported through `engine::runtime::*`.
- `infra-cpal` — `ProjectRuntimeController` owns the per-chain CPAL streams. `upsert_chain` is the full rebuild path; `pause_chain` + the fast-path resume branch inside `upsert_chain_modal` (issue #522) keep the runtime + streams alive across chain toggles via `set_draining()` so re-enable is O(1). `set_block_enabled` forwards into the engine for the matching block-level fast path.
- `adapter-render` — headless offline render console (issue #552). Binary `openrig-render` and lib `adapter_render::render()`. Loads a project, decodes an input WAV, drives `engine::offline::render_chain` (same `RuntimeProcessor::process_buffer` as the realtime callback), writes the output WAV atomically. No cpal, no Slint, no MCP, no MIDI — single-chain, deterministic, used by the audio-validation pipeline (`openrig-tone-analyzer` skill, OpenRig-claude#8). Standalone — never linked into `adapter-gui`. See `docs/render.md`.
- `nam` — Neural Amp Modeler
- `asset-runtime` — `EmbeddedAsset`, `materialize()`

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
