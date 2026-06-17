# I/O Binding Registry — couple inputs↔outputs per stream

- **Issue:** #716
- **Date:** 2026-06-17
- **Status:** Design (approved in brainstorming)

## 1. Problem

A chain configures its input endpoints and output endpoints **independently**.
There is no way to express "audio captured on interface A exits only through
interface A, audio from interface B exits only through interface B".

Two failure modes today:

1. **Intra-chain bleed.** Every input stream of a chain writes to the
   chain-shared `output_routes` buffer, and every output block reads that same
   buffer. So with two inputs and two outputs in one chain, **every input
   bleeds into every output** (all-to-all).
2. **Cross-device delay.** Because of (1), audio captured on device A is forced
   out device B. A and B are independent clock domains, so the backend must
   buffer/resample between them — adding latency and risking drift/xruns. This
   is the delay the user wants to eliminate.

## 2. Goal

Introduce an explicit **I/O binding registry** so the user controls which inputs
reach which outputs. Binding inputs and outputs into the same group makes
"A→A, B→B" the natural, default expression, and makes cross-device bleed
**structurally impossible** rather than something to validate against.

Non-goals (YAGNI for this issue): node-based visual routing (#114), parallel
chain splits (#328), a mixer/aux-send (#184/#85/#344). Those may build on this
registry later but are out of scope here.

## 3. Current architecture (as-is)

- `Input`, `Output`, `Insert` are variants of `AudioBlockKind` inside
  `chain.blocks`. `blocks[0]` = InputBlock (fixed), `blocks[N-1]` = OutputBlock
  (fixed); extra inputs/outputs/inserts can be inserted in the middle.
- Each block holds `entries: [{ name, device_id, mode, channels }]` — the device
  endpoint is **embedded in the block, inside the project**.
- Per-entry stream isolation (invariant #4): every raw input entry owns its own
  isolated `ChainRuntimeState` (own `processing` mutex, `output_routes`,
  `input_taps`, scratch). `RuntimeGraph` is keyed by `(ChainId, entry group)`.
- Mixing happens **only** at the backend (cpal/JACK), summing per physical
  device. The engine never mixes streams.
- Device *settings* (sample rate, buffer, bit depth) already live per-machine in
  `config.yaml`; device *endpoints* (`device_id`) currently live in the project.

References: `docs/audio-config.md`, `CLAUDE.md` invariants #4/#5/#10,
`docs/adr/0003-system-vs-project-config.md`.

## 4. Proposed model

### 4.1 The registry (per-machine, `config.yaml`)

A new list of named I/O bindings. Each binding ("dupla") groups a set of input
endpoints and a set of output endpoints. They are typically same-interface (one
clock domain) but may span interfaces by user choice.

```yaml
io_bindings:
  - id: main                 # stable id, referenced by chains
    name: "Scarlett"
    inputs:  [{ name: In1,  device_id: "...", mode: mono,   channels: [0] }]
    outputs: [{ name: Out1, device_id: "...", mode: stereo, channels: [0,1] }]
  - id: cab_b
    name: "Interface B"
    inputs:  [{ name: In1,  device_id: "B", mode: mono,   channels: [0] }]
    outputs: [{ name: Out1, device_id: "B", mode: stereo, channels: [0,1] }]
```

**Scope decision (ADR 0003):** the registry references concrete
`device_id`/channels, which are machine-specific, so it lives in `config.yaml`
(system), next to device settings. Chains reference a binding by its stable
`id`. Moving a `.openrig` to another machine carries only the `id` reference;
the target machine re-resolves it against its local registry. This makes the
project **more** portable than today (where raw `device_id` is embedded in the
chain).

### 4.2 Chain blocks become ports

Input/Output blocks stop carrying device endpoints. Each becomes a *port* that
carries **one endpoint** and **references an I/O binding**:

```yaml
blocks:
  - { type: input,  io: main, endpoint: In1 }   # source port
  - { type: preamp, ... }
  - { type: output, io: main, endpoint: Out1 }  # destination port
```

- An **input port** carries one *source* endpoint (selected from its io's
  `inputs`) and references the io that provides the *destination*.
- An **output port** carries one *destination* endpoint (selected from its io's
  `outputs`) and references the io that provides the *source*.
- Head input (pos 0) and tail output (pos n) are the ordinary special cases.

### 4.3 Routing rule (unified, symmetric)

> A **stream** is spawned for each pair `(input port, output port)` that belongs
> to the **same I/O binding**, with the input port at or before the output port
> in block order. The stream reads the input port's source endpoint, runs **only
> the blocks strictly between the two ports**, and writes the output port's
> destination endpoint.

Consequences:

- **Isolation A→A / B→B is structural.** Ports of io A only pair with ports of
  io A. Input of io A can never reach output of io B — it is impossible to
  express, not merely validated.
- **"The stream rule does not change."** Each `(input,output)` pair is still its
  own isolated per-entry runtime; the only mix point is still the backend,
  summing streams that target the same physical output endpoint. What is new:
  (1) endpoints come from the registry, (2) a stream may start/end at block
  offsets, (3) pairing is scoped to the io.

### 4.4 Worked examples (chain `A B C D E`, io `XYZ`)

**Input in the middle** — io XYZ inputs `{ch1, ch2}`, output `ch3,4`. Ports:
head-input(ch1 @0), middle-input(ch2 @after A), tail-output(ch3,4 @end).

```
ch1 → A B C D E → ch3,4     (head-input  × tail-output)
ch2 →   B C D E → ch3,4     (middle-input × tail-output)
```

**Output in the middle** — io XYZ input `ch1`, outputs `{ch3, ch4}`. Ports:
head-input(ch1 @0), tail-output(ch3 @end), middle-output(ch4 @after C).

```
ch1 → A B C D E → ch3       (head-input × tail-output)
ch1 → A B C     → ch4       (head-input × middle-output)
```

The combinatorial pairing across one io's ports is what "a combinação pode gerar
vários streams" means.

## 5. Data model changes

| Where | Change |
|---|---|
| `config.yaml` (system) | New `io_bindings: [{ id, name, inputs:[endpoint], outputs:[endpoint] }]`. `endpoint = { name, device_id, mode, channels }`. |
| Chain block (project) | Input/Output blocks drop `entries`; gain `io: <id>` + `endpoint: <name>`. Insert block unchanged. |
| Engine runtime | `RuntimeGraph` keying extends to `(ChainId, io_id, input_endpoint, output_endpoint)`; the per-entry runtime body is unchanged. Block range `[inputPos+1 .. outputPos-1]` (exclusive of ports) defines the processed segment. |

The endpoint struct is the existing `{ name, device_id, mode, channels }`; it
simply **moves** from the chain block to the registry.

## 6. Command bus

New variants in `crates/application/src/command.rs` (system scope):

- `CreateIoBinding { id, name, inputs, outputs }`
- `UpdateIoBinding { id, name?, inputs?, outputs? }`
- `DeleteIoBinding { id }` (rejected if any chain references it, or offered with
  reassignment — see open question O3)

Reshape existing chain-IO commands to set the reference on a block instead of
embedding endpoints:

- `save_chain_io`, `save_chain_input_endpoints`, `save_chain_output_endpoints`
  → operate on `{ io, endpoint }` references.

All new commands flow GUI → `dispatcher.dispatch`; MCP and gRPC inherit the same
variants (parity — LAW 1). MCP tool surface gains
`create_io_binding/update_io_binding/delete_io_binding` and the reshaped
endpoint tools.

## 7. UI / screens

This change touches **many** GUI surfaces (grep over `crates/adapter-gui`). Every
one is listed so none is missed:

- **Data bridge** (`models.slint`, `ui_state.rs`, `state.rs`): Slint structs for
  bindings + a projector exposing them to all editors below.
- **Settings → System → "I/O bindings"** (new section, next to Audio interface):
  list/create/edit/delete bindings; per binding add/remove input+output
  endpoints (device + channels + mode pickers reused from `settings/audio.rs`).
- **Audio interface + audio wizard** (`settings/audio.rs`,
  `device_settings_wiring.rs`, `device_refresh_wiring.rs`,
  `audio_wizard_wiring.rs`): first-run/device-change flows create/update the
  `default` binding (O4) and re-resolve bindings on device hot-swap (#354).
- **Endpoint editor** (`chain_endpoint_editor.slint` +
  `chain_editor_{input,output,meta}_*_callbacks.rs`): Input/Output blocks replace
  device/channel/mode fields with an **I/O picker** + **endpoint picker** scoped
  to the chosen binding.
- **`chain_io_*` subsystem** (`chain_io_main/picker/save/fullscreen` +
  `chain_io_block_builders.rs`, `io_groups.rs`,
  `chain_{input,output}_groups_wiring.rs`): the fullscreen "configure I/O" flow
  and the input/output groups operate on binding references; groups are shown
  grouped by binding (this is where A→A vs B→B becomes visible). Helps #257.
- **Compact view + chain list** (`compact_chain_view.slint`,
  `compact_chain_block_handlers.rs`, `chain_row.slint`, `chain_chips.slint`,
  `project_view.rs`): "configure I/O" routes to the new pickers; rows/chips show
  the bound I/O name instead of raw device strings.
- **Insert editor** (`chain_insert_editor.slint`, `insert_wiring.rs`): send/return
  endpoints **stay raw** in this issue (inserts are a single-runtime send/return
  pipeline, not a binding-paired stream); task is to verify they don't regress.
- **Tuner / Spectrum** (`tuner_session.rs`, `spectrum_session.rs`): enumerate one
  tuner per active input port and one analyzer per active output endpoint under
  the new per-binding stream keys.
- **Windows / touch parity** (`app-window.slint`, `desktop_main.slint`,
  `touch_main.slint`, `secondary_windows_chain.slint`): wire the new components
  into both layouts.
- SVG icons only, translations refreshed (new `@tr` strings → run
  `extract-translations.sh` + fill all 9 catalogs in the same PR).

## 8. Migration (must preserve sound)

Old chains embed endpoints in blocks. On load (extend the existing YAML
auto-migration):

1. For each chain, collect its input endpoints and output endpoints.
2. Create **one** generated io binding holding all of them
   (`id` derived/deduped; identical bindings across chains are merged).
3. Rewrite the chain's input/output blocks to `{ io, endpoint }` references.

Because one io with multiple inputs+outputs pairs all-inputs × all-outputs, the
result is **all-to-all within that io == today's behavior**. Golden samples and
volume invariants therefore stay byte-identical. The *new* capability is the
user splitting into separate bindings to gain isolation.

Old `config.yaml` without `io_bindings` and old project YAML with embedded
`entries` both keep deserializing (back-compat per
`2026-05-17-yaml-versioning-backcompat-design.md`).

## 9. Invariants & red flags

- **#4 isolation** — preserved; each pair is an isolated runtime, mixing only at
  the backend. Cross-stream sharing would be a regression.
- **#5 stereo bus** — untouched.
- **#10 volume immutable** — `volume_invariants_tests.rs` must pass unchanged. If
  it breaks, the source is wrong, not the test.
- **Golden samples** within tolerance after migration.
- No alloc/lock/syscall/I/O on the audio thread (#8) — registry resolution
  happens at graph build time, off the audio thread.

## 10. Testing strategy (TDD red-first, mandatory)

Each item: write the test, watch it fail (RED), then implement.

1. **Isolation (core fix):** chain with io A (in/out on A) + io B (in/out on B);
   feed signal into A's input → B's output stays silent; and vice versa.
2. **Routing rule — input offset:** the input-in-the-middle example produces
   exactly the two expected streams (block range correctness).
3. **Routing rule — output offset:** the output-in-the-middle example.
4. **Migration single in/out:** old YAML → identical routing + golden samples.
5. **Migration multi in/out:** old all-to-all chain → one io → identical backend
   sum (volume invariants pass).
6. **`config.yaml` round-trip** of `io_bindings` (+ back-compat load of YAML
   without the field).
7. **Command/MCP parity** for `create/update/delete_io_binding` and the reshaped
   endpoint commands.
8. **Real-hardware battery** (`OPENRIG_HW_TESTS=1`) with two interfaces:
   confirm no cross-device path and no added latency vs. single-device baseline.

## 11. Open questions

- **O1 — endpoint identity:** reference io endpoints by `name` (human-stable) or
  by index? Proposal: by `name`, unique within a binding.
- **O2 — same physical channel in two bindings:** today a physical channel may
  only be enabled in one chain at a time (runtime-validated). Define the rule
  for a channel appearing in two bindings (likely: still one active owner at a
  time, validated at enable).
- **O3 — deleting a referenced binding:** reject vs. cascade vs. reassign.
  Proposal: reject with a clear error listing referencing chains.
- **O4 — default binding for a fresh project:** auto-create a "default" io from
  the system default input/output devices so new chains have something to
  reference.

## 12. Documentation to update (same PR)

`docs/audio-config.md` (I/O-as-blocks section + stream model), `docs/screens.md`
(Settings + Chain editor), `docs/cli.md` if any flag changes, `docs/adr/`
(consider a short ADR recording the registry scope decision), READMEs only if
user-facing behavior is described.
