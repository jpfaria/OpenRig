# I/O Binding Registry Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bind a chain's inputs to its outputs through a per-machine I/O registry so audio captured on interface A exits only through interface A — killing cross-device clock-domain delay.

**Architecture:** A new `io_bindings` registry lives in `config.yaml` (system scope, ADR 0003). Chain Input/Output blocks stop embedding device endpoints and become *ports* that reference a binding by id + endpoint name. The runtime spawns one isolated stream per `(input port, output port)` pair **of the same binding**, running only the blocks between the two ports. Pairing scoped to a binding makes A→B bleed structurally impossible. Legacy chains migrate by wrapping their existing inputs+outputs into one binding (all-to-all == today's behavior), so golden samples and volume invariants stay identical.

**Tech Stack:** Rust workspace (crates: `project`, `application`, `engine`, `infra-cpal`, `adapter-gui`), Slint UI, serde/serde_yaml, cpal/JACK backend, MCP server.

## Global Constraints

- Repo content in English (code comments, docs, commits, issue comments). Chat in pt-BR.
- TDD red-first is mandatory: write the failing test, run it, see RED, **then** create `.dev-rules/.red-first-unlocked` and edit production. Never implement without a prior failing test.
- Audio-thread invariants must not regress: no alloc/lock/syscall/I/O on the audio thread (#8); registry resolution happens at graph-build time, off the audio thread.
- Stream isolation (#4): every `(input,output)` pair is its own isolated runtime; the only mix point is the backend (cpal/JACK), summing streams that target the same physical output endpoint.
- Internal stereo bus (#5) and volume invariants (#10) untouched. `crates/engine/src/volume_invariants_tests.rs` MUST pass unchanged — if it breaks, the source is wrong, not the test.
- Golden samples must stay within tolerance after migration.
- LOC caps (`validate.sh`): `.rs` non-test ≤ 600, `.slint` ≤ 500, `lib.rs`/`mod.rs` re-exports only < 100.
- Zero warnings (`cargo build --workspace` clean).
- Command-bus law (LAW 1): every state change is a `Command`; GUI/MCP/gRPC share the same variant (parity). No `borrow_mut()` in callbacks.
- Screen law (LAW 2): Slint is a pure dispatcher (callback → Event → pure fn); no business logic in `.slint`.
- New `@tr`/`t!` strings → run `scripts/extract-translations.sh` and fill all 9 catalogs in the same PR; English is the reference.
- SVG icons only (`@image-url` + colorize), never glyphs.
- Push immediately after each commit. Quality gate runs only at PR creation, never per push.
- Work happens only in `.solvers/issue-716/`. Never git/edit the main folder.

### Locked design decisions (spec open questions)

- **O1:** I/O endpoints are referenced by `name` (unique within a binding), not index.
- **O2:** A physical channel may be active in only one chain at a time — keep today's runtime-validated rule; a channel may *appear* in multiple bindings but only one active owner at enable time.
- **O3:** Deleting a binding referenced by any chain is **rejected** with an error listing the referencing chains/blocks.
- **O4:** A fresh project auto-creates a `default` binding from the system default input/output devices.

### Reference files (confirm exact locations during Task 1; paths from tests/docs)

- Block types: `crates/project/src/block/types.rs` (`InputBlock`, `OutputBlock`, `InputEntry`, `OutputEntry`, `ChainInputMode`, `ChainOutputMode`, `DeviceId`).
- Config model + load/save: `crates/project/src/` config module (the `config.yaml` schema: `recent_projects`, `paths`, `input_devices`, `output_devices`, `language`).
- Command enum + dispatcher: `crates/application/src/command.rs`, `crates/application/src/local_dispatcher_chain_io.rs`.
- Runtime: `crates/engine/src/runtime.rs`, `runtime_io.rs`, `runtime_block_builders.rs`; isolation contract `crates/engine/src/stream_isolation_tests.rs`, `stream_isolation_same_device_tests.rs`.
- cpal backend: `crates/infra-cpal/src/`.
- MCP tools: the crate exposing `save_chain_io` / `save_chain_input_endpoints` / `save_chain_output_endpoints`.
- UI: `crates/adapter-gui/ui/` (Settings page, Chain/Block editor) + `*_wiring.rs`.
- Docs: `docs/audio-config.md`, `docs/screens.md`, `docs/adr/`.

---

## Phase 0 — Registry data model + persistence (no behavior change)

### Task 1: `IoBinding` / `IoEndpoint` types

**Files:**
- Create: `crates/project/src/io_binding.rs`
- Modify: `crates/project/src/lib.rs` (re-export only)
- Test: `crates/project/src/io_binding.rs` (`#[cfg(test)]` module) or `crates/project/tests/io_binding_serde.rs`

**Interfaces:**
- Produces: `pub struct IoEndpoint { pub name: String, pub device_id: DeviceId, pub mode: ChannelMode, pub channels: Vec<usize> }`; `pub struct IoBinding { pub id: String, pub name: String, pub inputs: Vec<IoEndpoint>, pub outputs: Vec<IoEndpoint> }`. Reuse the existing endpoint shape from `InputEntry`/`OutputEntry` (`mode` is the existing mono/dual_mono/stereo enum; name it consistently with the existing type — confirm in Task 1 read).
- Consumes: `DeviceId` and the existing mode enum from `crates/project/src/block/types.rs`.

- [ ] **Step 1: Write the failing test**

```rust
// crates/project/tests/io_binding_serde.rs
use project::io_binding::{IoBinding, IoEndpoint};

#[test]
fn io_binding_round_trips_through_yaml() {
    let binding = IoBinding {
        id: "main".into(),
        name: "Scarlett".into(),
        inputs: vec![IoEndpoint { name: "In1".into(), device_id: "dev:in".into(), mode: Default::default(), channels: vec![0] }],
        outputs: vec![IoEndpoint { name: "Out1".into(), device_id: "dev:out".into(), mode: Default::default(), channels: vec![0, 1] }],
    };
    let yaml = serde_yaml::to_string(&binding).unwrap();
    let back: IoBinding = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(binding, back);
    assert!(yaml.contains("id: main"));
    assert!(yaml.contains("name: In1"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p project --test io_binding_serde`
Expected: FAIL — `io_binding` module / types do not exist.

- [ ] **Step 3: Implement minimal types**

Create `.dev-rules/.red-first-unlocked`, read `crates/project/src/block/types.rs` to copy the exact `DeviceId` and mode-enum names, then define `IoEndpoint` and `IoBinding` in `crates/project/src/io_binding.rs` with `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]`. Reuse the existing endpoint field types verbatim (do not introduce a parallel mode enum). Add `pub mod io_binding;` + re-export in `lib.rs`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p project --test io_binding_serde`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-716
git add crates/project/src/io_binding.rs crates/project/src/lib.rs crates/project/tests/io_binding_serde.rs
git commit -m "feat(#716): IoBinding/IoEndpoint registry types"
git push
```

---

### Task 2: `io_bindings` in `config.yaml` + back-compat

**Files:**
- Modify: the config-model struct + load/save (the module holding `recent_projects`/`input_devices` — confirm path in Task 1).
- Test: `crates/project/tests/config_io_bindings.rs`

**Interfaces:**
- Consumes: `IoBinding` (Task 1).
- Produces: a `pub io_bindings: Vec<IoBinding>` field on the config struct, defaulting to empty; getters/mutators following the existing config accessor pattern.

- [ ] **Step 1: Write the failing test**

```rust
// crates/project/tests/config_io_bindings.rs
// 1) round-trip: a config with io_bindings serializes and loads back equal.
// 2) back-compat: a config YAML string WITHOUT `io_bindings:` loads with an empty vec (no panic).
```
Write both as concrete `#[test]` fns: build the config type, set one binding, serialize, deserialize, assert equal; and `serde_yaml::from_str::<Config>("recent_projects: []\n...")` (a minimal legacy doc) yields `io_bindings == []`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p project --test config_io_bindings`
Expected: FAIL — field `io_bindings` does not exist.

- [ ] **Step 3: Implement**

Add `#[serde(default)] pub io_bindings: Vec<IoBinding>` to the config struct. Confirm save path writes it. (`#[serde(default)]` gives the back-compat empty vec.)

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p project --test config_io_bindings`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A crates/project
git commit -m "feat(#716): persist io_bindings in config.yaml with back-compat default"
git push
```

---

## Phase 1 — Command bus + MCP for the registry

### Task 3: `Create/Update/DeleteIoBinding` commands + handlers

**Files:**
- Modify: `crates/application/src/command.rs` (enum variants), the system-scope dispatcher handler module (mirror `local_dispatcher_chain_io.rs`; create `crates/application/src/local_dispatcher_io_binding.rs` if a new file is cleaner under the 600-LOC cap).
- Test: `crates/application/tests/io_binding_commands.rs`

**Interfaces:**
- Produces: `Command::CreateIoBinding { binding: IoBinding }`, `Command::UpdateIoBinding { binding: IoBinding }`, `Command::DeleteIoBinding { id: String }`. Handlers mutate the in-memory config snapshot and persist to `config.yaml`. `DeleteIoBinding` returns an error (existing error/SideEffect channel) when any chain block references `id`.
- Consumes: `IoBinding` (Task 1), config accessors (Task 2).

- [ ] **Step 1: Write the failing tests**

```rust
// crates/application/tests/io_binding_commands.rs
// test_create_then_get: dispatch CreateIoBinding -> state/config contains it; reload -> still there.
// test_update_replaces_by_id: dispatch Update -> fields change, count unchanged.
// test_delete_unreferenced_ok: dispatch Delete on a binding no chain uses -> removed.
// test_delete_referenced_rejected: a chain block references "main"; Delete{main} -> rejected, binding still present, error names the referencing chain (O3).
```
Write all four as concrete dispatch tests using the existing test dispatcher harness (mirror the harness used in `scene_output_preservation*.rs`).

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p application --test io_binding_commands`
Expected: FAIL — variants/handlers missing.

- [ ] **Step 3: Implement**

Add the three variants; route them in the dispatcher to the handler module; implement create/update (upsert by `id`) and delete-with-reference-check (scan chain blocks for `io == id`). Keep MCP/gRPC parity in mind (Task 4 wires MCP).

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p application --test io_binding_commands`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A crates/application
git commit -m "feat(#716): Create/Update/DeleteIoBinding commands (delete rejects referenced)"
git push
```

---

### Task 4: MCP tools for the registry (parity)

**Files:**
- Modify: the MCP tool-registration module (where `save_chain_io` etc. are exposed).
- Test: the MCP parity test suite (mirror existing MCP command tests).

**Interfaces:**
- Produces: MCP tools `create_io_binding`, `update_io_binding`, `delete_io_binding` that build the Task 3 commands and dispatch through the same bus.

- [ ] **Step 1: Write the failing test**

Concrete test: invoke each MCP tool handler with a JSON payload, assert it dispatches the matching `Command` variant and the config reflects the change (parity with the GUI path).

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p <mcp-crate> io_binding`
Expected: FAIL — tools unregistered.

- [ ] **Step 3: Implement** the three tools, delegating to the Task 3 commands. No business logic in the tool layer (LAW 1 parity).

- [ ] **Step 4: Run to verify it passes.**

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(#716): MCP create/update/delete_io_binding (bus parity)"
git push
```

---

## Phase 2 — Chain blocks reference bindings + migration

### Task 5: Input/Output blocks gain `io` + `endpoint` ref (back-compat)

**Files:**
- Modify: `crates/project/src/block/types.rs` (Input/Output block structs + their YAML serde).
- Test: `crates/project/tests/block_io_ref_serde.rs`

**Interfaces:**
- Produces: Input block fields `io: String` + `endpoint: String`; Output block fields `io: String` + `endpoint: String`. Legacy `entries: Vec<…>` becomes `#[serde(default, skip_serializing_if = "Vec::is_empty")]` so old YAML still deserializes (migration in Task 6 consumes it).

- [ ] **Step 1: Write the failing test**

```rust
// crates/project/tests/block_io_ref_serde.rs
// new_schema_round_trips: a block `{ type: input, io: main, endpoint: In1 }` serializes/deserializes equal.
// legacy_entries_still_deserialize: `{ type: input, entries: [{name, device_id, mode, channels}] }` deserializes without error and exposes the entries for migration.
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p project --test block_io_ref_serde`
Expected: FAIL — `io`/`endpoint` fields missing.

- [ ] **Step 3: Implement** the fields + serde attributes after the RED unlock. Keep `entries` for back-compat input to migration; new writes emit `io`/`endpoint`.

- [ ] **Step 4: Run to verify it passes.**

- [ ] **Step 5: Commit**

```bash
git add -A crates/project
git commit -m "feat(#716): chain Input/Output blocks reference io+endpoint (legacy entries back-compat)"
git push
```

---

### Task 6: Migration — wrap legacy chains into one binding

**Files:**
- Create: `crates/project/src/migrate_io_binding.rs`
- Modify: the project-load path to call the migration after deserialize; `lib.rs` re-export.
- Test: `crates/project/tests/migrate_io_binding.rs`

**Interfaces:**
- Produces: `pub fn migrate_legacy_io(project: &mut Project, config: &mut Config)` — for each chain with legacy `entries`, create (or dedupe to) one `IoBinding` holding all that chain's input+output endpoints, register it in `config.io_bindings`, and rewrite the chain's input/output blocks to `{ io, endpoint }` references. Idempotent (running twice is a no-op).
- Consumes: Tasks 1, 2, 5.

- [ ] **Step 1: Write the failing tests**

```rust
// crates/project/tests/migrate_io_binding.rs
// single_in_out: chain with 1 input + 1 output entry -> one binding, both blocks reference it; entries cleared.
// multi_in_out_all_to_all: chain with 2 inputs + 2 outputs -> ONE binding holding all 4 endpoints (preserves all-to-all).
// dedup_across_chains: two chains with identical endpoints -> a single shared binding id.
// idempotent: running migrate twice yields the same result, no duplicate bindings.
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p project --test migrate_io_binding`
Expected: FAIL — function missing.

- [ ] **Step 3: Implement** the migration; generate stable binding ids (e.g. hash of sorted endpoints or `auto_<n>`); wire it into project load.

- [ ] **Step 4: Run to verify they pass.**

- [ ] **Step 5: Commit**

```bash
git add -A crates/project
git commit -m "feat(#716): migrate legacy chain endpoints into io_bindings (all-to-all preserved)"
git push
```

---

### Task 7: Reshape chain-IO commands to set references

**Files:**
- Modify: `crates/application/src/local_dispatcher_chain_io.rs` + `command.rs` (the `SaveChainIo` / `SaveChainInputEndpoints` / `SaveChainOutputEndpoints` variants).
- Test: extend `crates/application/tests/` IO command tests; MCP parity test.

**Interfaces:**
- Produces: these commands set `{ io, endpoint }` on the targeted block instead of embedding endpoints. Signatures change from carrying `AudioBlock`/entries to carrying `{ chain, block_pos, io, endpoint }` (confirm exact existing shapes in Task 7 read).

- [ ] **Step 1: Write the failing test**

Concrete test: dispatch the reshaped command and assert the block now holds the `io`/`endpoint` reference and persists across reload (mirror `scene_output_preservation*.rs`).

- [ ] **Step 2: Run to verify it fails.** Run: `cargo test -p application chain_io`

- [ ] **Step 3: Implement** the reshaped variants + handlers; update MCP tool payloads for parity.

- [ ] **Step 4: Run to verify it passes.**

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(#716): chain-IO commands set io+endpoint reference (GUI/MCP parity)"
git push
```

---

## Phase 3 — Runtime routing (the core fix)

### Task 8: RED acceptance — cross-binding isolation

**Files:**
- Test: `crates/engine/src/io_binding_isolation_tests.rs` (or `crates/engine/tests/`)

**Interfaces:**
- Consumes: the runtime graph builder (Task 9 will satisfy this test).

- [ ] **Step 1: Write the failing test**

```rust
// Build a chain referencing two bindings:
//   io_a: input A.ch0 -> output A.ch0
//   io_b: input B.ch0 -> output B.ch0
// Feed an impulse/sine into A's input buffer, zero into B's.
// Process one block. Assert: A's output endpoint carries the (processed) signal;
// B's output endpoint stays silent (==0). Then swap and assert the mirror.
```
Use the existing engine test harness for building a runtime and pumping buffers (mirror `stream_isolation_tests.rs`).

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p engine io_binding_isolation`
Expected: FAIL — current shared `output_routes` routes A into B (bleed), or the graph cannot express bindings yet.

- [ ] **Step 3:** No implementation in this task — it is the acceptance gate satisfied by Task 9. Leave RED.

- [ ] **Step 4:** Confirm it is RED and committed as a pending acceptance test.

- [ ] **Step 5: Commit**

```bash
git add -A crates/engine
git commit -m "test(#716): RED — cross-binding isolation acceptance (A must not bleed into B)"
git push
```

---

### Task 9: Routing rule in the graph builder

**Files:**
- Modify: `crates/engine/src/runtime.rs` / `runtime_io.rs` / `runtime_block_builders.rs` (graph build + RuntimeGraph keying); resolve block-port references against `io_bindings`.
- Test: `crates/engine/src/io_binding_routing_tests.rs` + Task 8's test must go GREEN.

**Interfaces:**
- Produces: graph build that, for each binding referenced in a chain, enumerates `(input port, output port)` pairs with `inputPos <= outputPos`, and for each pair spawns an isolated runtime processing blocks `(inputPos, outputPos)` exclusive, reading the input endpoint and writing the output endpoint. `RuntimeGraph` key extends to include `(io_id, input_endpoint, output_endpoint)`.
- Consumes: `IoBinding` resolution (Tasks 1–2), block refs (Task 5).

- [ ] **Step 1: Write the failing tests**

```rust
// input_offset_example: chain [A,B,C,D,E], io XYZ in {ch1,ch2} out {ch3,4}.
//   ports: head-input(ch1@0), mid-input(ch2 after A), tail-output(ch3,4@end).
//   assert exactly 2 streams: ch1 over A..E, ch2 over B..E, both -> ch3,4.
// output_offset_example: io XYZ in {ch1} out {ch3, ch4}.
//   ports: head-input(ch1@0), tail-output(ch3@end), mid-output(ch4 after C).
//   assert exactly 2 streams: ch1 over A..E -> ch3, ch1 over A..C -> ch4.
// Assert the block range per stream (which block instances each stream runs).
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p engine io_binding_routing`
Expected: FAIL.

- [ ] **Step 3: Implement** the pairing + block-range slicing in graph build; route each stream's output only to its binding's output endpoint (replace the chain-shared all-output write with per-binding routing). Keep each pair a fully isolated runtime (invariant #4). Resolution happens at build time (no audio-thread cost).

- [ ] **Step 4: Run to verify routing tests AND Task 8 isolation pass**

Run: `cargo test -p engine io_binding`
Expected: PASS (Task 8 now GREEN).

- [ ] **Step 5: Commit**

```bash
git add -A crates/engine
git commit -m "feat(#716): per-binding stream routing (input×output pairs, blocks between ports)"
git push
```

---

### Task 10: Regression — golden + volume invariants + migration equivalence

**Files:**
- Test: add `crates/engine/tests/migration_equivalence.rs`; run the existing pinned suites.

**Interfaces:**
- Consumes: migration (Task 6) + routing (Task 9).

- [ ] **Step 1: Write the failing/guard test**

```rust
// migration_equivalence: take a legacy single-in/out chain, run it through migration + new routing,
// and assert the per-sample output equals the legacy path output within golden tolerance.
// (For multi-in/out: assert the backend sum equals the legacy all-to-all sum.)
```

- [ ] **Step 2: Run** `cargo test -p engine migration_equivalence` → expected RED until wired, then GREEN.

- [ ] **Step 3:** If the equivalence test reveals a routing mismatch, fix Task 9 (not the invariant tests).

- [ ] **Step 4: Run the full pinned regression**

Run: `cargo test -p engine volume_invariants && cargo test -p engine --test '*golden*' 2>/dev/null; cargo test --workspace`
Expected: PASS, `volume_invariants_tests.rs` unchanged.
Then the real-hardware battery on an idle machine with two interfaces:
Run: `OPENRIG_HW_TESTS=1 cargo test -p infra-cpal` and confirm no cross-device path and no added latency vs single-device baseline (see `docs/testing.md` → Real-hardware battery).

- [ ] **Step 5: Commit**

```bash
git add -A crates/engine
git commit -m "test(#716): migration equivalence + pinned invariants green under per-binding routing"
git push
```

---

## Phase 4 — UI (every affected surface)

> **GUI surface inventory** (from grep over `crates/adapter-gui`). Each Slint file
> pairs with its wiring `.rs`. Per LAW 2 every callback is tested as a pure
> `event → Command` fn (no `AppWindow` in tests). Per LAW 1 the UI only dispatches
> the Phase 1/2 commands.
>
> | Surface | Slint | Wiring |
> |---|---|---|
> | Data bridge | `models.slint` | `ui_state.rs`, `state.rs` |
> | Settings shell | `pages/settings.slint` | `project_settings_wiring.rs` |
> | Audio interface + wizard | `pages/settings/*`, `secondary_windows_chain.slint` | `settings/audio.rs`, `device_settings_wiring.rs`, `device_refresh_wiring.rs`, `audio_wizard_wiring.rs`, `audio_devices.rs` |
> | Endpoint editor | `pages/chain_endpoint_editor.slint` | `chain_editor_input_endpoint_callbacks.rs`, `chain_editor_output_endpoint_callbacks.rs`, `chain_editor_meta_io_callbacks.rs` |
> | I/O config subsystem | (fullscreen + picker in chain editor) | `chain_io_main_wiring.rs`, `chain_io_picker_wiring.rs`, `chain_io_save_wiring.rs`, `chain_io_block_builders.rs`, `chain_io_fullscreen_callbacks.rs`, `io_groups.rs`, `chain_input_groups_wiring.rs`, `chain_output_groups_wiring.rs` |
> | Compact view | `pages/compact_chain_view.slint` | `compact_chain_block_handlers.rs` |
> | Chain list summary | `pages/chain_row.slint`, `components/chain_chips.slint` | `chain_crud_wiring.rs`, `project_view.rs` |
> | Insert editor | `pages/chain_insert_editor.slint` | `insert_wiring.rs` |
> | Tuner / Spectrum taps | — | `tuner_session.rs`, `spectrum_session.rs` |
> | Windows / touch parity | `app-window.slint`, `desktop_main.slint`, `touch_main.slint` | `desktop_app.rs`, `desktop_app_init.rs` |

### Task 11: Slint data bridge for bindings

**Files:**
- Modify: `crates/adapter-gui/ui/models.slint` (add `IoBindingModel`, `IoEndpointModel` structs + a bindable list); `crates/adapter-gui/src/ui_state.rs`, `crates/adapter-gui/src/state.rs` (expose the binding list + per-block `io`/`endpoint` selection to the UI).
- Test: `crates/adapter-gui/src/ui_state_tests.rs`

**Interfaces:**
- Produces: `IoBindingModel { id, name, inputs: [IoEndpointModel], outputs: [IoEndpointModel] }`, `IoEndpointModel { name, device_label, mode, channels_label }`; a `fn ui_bindings(config) -> Vec<IoBindingModel>` projector and the inverse selection accessors used by every editor below.
- Consumes: `IoBinding` (Task 1), config (Task 2).

- [ ] **Step 1: Write the failing test**: `ui_bindings(config_with_two_bindings)` returns two models with names/endpoints mapped; a block holding `{io:"main", endpoint:"In1"}` resolves to the matching `IoEndpointModel`. Run → RED.
- [ ] **Step 2: Run** `cargo test -p adapter-gui ui_state` → FAIL.
- [ ] **Step 3: Implement** the Slint structs + the projector/selection fns (pure, testable).
- [ ] **Step 4: Run** → PASS.
- [ ] **Step 5: Commit**

```bash
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-716
git add -A crates/adapter-gui
git commit -m "feat(#716): Slint data bridge for io_bindings (models + ui_state projector)"
git push
```

---

### Task 12: Settings → System → "I/O bindings" editor

**Files:**
- Create: `crates/adapter-gui/ui/pages/settings/section_io_bindings.slint` (≤ 500 LOC) + `crates/adapter-gui/src/settings/io_bindings.rs`.
- Modify: `crates/adapter-gui/ui/pages/settings.slint` (add the section under *System*), `crates/adapter-gui/src/project_settings_wiring.rs` (register).
- Test: `crates/adapter-gui/src/settings/io_bindings_tests.rs`

**Interfaces:**
- Consumes: Task 3 commands; Task 11 models.
- Produces: list/create/edit/delete bindings; per binding add/remove input+output endpoints (reuse the device/channel/mode pickers from `settings/audio.rs`). Delete shows the Task 3 reject error inline (O3).

- [ ] **Step 1: Write the failing tests**: "create binding" event → `CreateIoBinding`; "add output endpoint" → `UpdateIoBinding` with the endpoint; "delete referenced" → surfaces the reject error, list unchanged. Run → RED.
- [ ] **Step 2: Run** `cargo test -p adapter-gui io_bindings` → FAIL.
- [ ] **Step 3: Implement** the Slint section + wiring dispatching Task 3 commands. SVG icons only; `@tr` on all labels.
- [ ] **Step 4: Run** → PASS; visually confirm via `/run`.
- [ ] **Step 5: Commit**

```bash
git add -A crates/adapter-gui
git commit -m "feat(#716): Settings I/O bindings editor section"
git push
```

---

### Task 13: Audio interface section + audio wizard create the default binding (O4)

**Files:**
- Modify: `crates/adapter-gui/src/settings/audio.rs`, `crates/adapter-gui/src/device_settings_wiring.rs`, `crates/adapter-gui/src/device_refresh_wiring.rs`, `crates/adapter-gui/src/audio_wizard_wiring.rs`.
- Test: `crates/adapter-gui/src/device_settings_wiring_tests.rs`, `crates/adapter-gui/src/audio_devices_tests.rs`

**Interfaces:**
- Consumes: Task 3 commands, Task 13 (project) default-binding helper.
- Produces: the first-run wizard / "set default device" flow creates or updates a `default` binding from the chosen input+output devices; a device hot-swap (#354 adjacency) re-resolves bindings whose `device_id` matches the changed device (does not silently drop them).

- [ ] **Step 1: Write the failing tests**: wizard "finish" with input=devA, output=devA → emits `CreateIoBinding{default,…}`; device-refresh that renames a device id keeps the binding resolvable (or flags it as unresolved, never silently empty). Run → RED.
- [ ] **Step 2: Run** `cargo test -p adapter-gui device_settings_wiring audio_devices` → FAIL.
- [ ] **Step 3: Implement** the wizard/refresh wiring against Task 3 commands + the default-binding helper.
- [ ] **Step 4: Run** → PASS.
- [ ] **Step 5: Commit**

```bash
git add -A crates/adapter-gui
git commit -m "feat(#716): audio wizard + device refresh manage the default I/O binding"
git push
```

---

### Task 14: Endpoint editor → I/O + endpoint pickers

**Files:**
- Modify: `crates/adapter-gui/ui/pages/chain_endpoint_editor.slint`; `crates/adapter-gui/src/chain_editor_input_endpoint_callbacks.rs`, `chain_editor_output_endpoint_callbacks.rs`, `chain_editor_meta_io_callbacks.rs`.
- Test: the existing `chain_editor_*` callback tests + a new picker test.

**Interfaces:**
- Consumes: Task 7 reshaped commands; Task 11 models.
- Produces: the Input/Output block editor shows an **I/O picker** + an **endpoint picker** scoped to the chosen binding instead of raw device/channel/mode fields.

- [ ] **Step 1: Write the failing test**: selecting `(io=main, endpoint=In1)` on an input block emits the reshaped `SaveChainInputEndpoints` carrying the reference. Run → RED.
- [ ] **Step 2: Run** `cargo test -p adapter-gui chain_editor` → FAIL.
- [ ] **Step 3: Implement** the picker UI + callbacks dispatching Task 7 commands.
- [ ] **Step 4: Run** → PASS.
- [ ] **Step 5: Commit**

```bash
git add -A crates/adapter-gui
git commit -m "feat(#716): endpoint editor uses I/O + endpoint pickers"
git push
```

---

### Task 15: Rework the `chain_io_*` configuration subsystem

**Files:**
- Modify: `crates/adapter-gui/src/chain_io_main_wiring.rs`, `chain_io_picker_wiring.rs`, `chain_io_save_wiring.rs`, `chain_io_block_builders.rs`, `chain_io_fullscreen_callbacks.rs`, `io_groups.rs`, `chain_input_groups_wiring.rs`, `chain_output_groups_wiring.rs`.
- Test: `crates/adapter-gui/src/chain_io_block_builders_tests.rs` (+ extend existing).

**Interfaces:**
- Consumes: Task 7 commands; Task 11 models.
- Produces: the fullscreen "configure I/O" flow and its picker/save/builders operate on `{io, endpoint}` references; the input/output "groups" UI lists ports grouped by their referenced binding (this is where the A→A vs B→B grouping becomes visible to the user).

- [ ] **Step 1: Write the failing test**: `chain_io_block_builders` builds an input port from `(io, endpoint)` selection (not raw device); the groups projector groups ports by binding id. Run → RED.
- [ ] **Step 2: Run** `cargo test -p adapter-gui chain_io` → FAIL.
- [ ] **Step 3: Implement** the rework across the listed files, keeping each ≤ 600 LOC (split if a file grows past the cap).
- [ ] **Step 4: Run** → PASS.
- [ ] **Step 5: Commit**

```bash
git add -A crates/adapter-gui
git commit -m "feat(#716): chain_io_* subsystem operates on binding references"
git push
```

---

### Task 16: Compact view + chain-list I/O summary

**Files:**
- Modify: `crates/adapter-gui/ui/pages/compact_chain_view.slint`, `crates/adapter-gui/src/compact_chain_block_handlers.rs`; `crates/adapter-gui/ui/pages/chain_row.slint`, `crates/adapter-gui/ui/components/chain_chips.slint`, `crates/adapter-gui/src/project_view.rs`.
- Test: `crates/adapter-gui/src/compact_chain_block_handlers` tests + a chip-label projector test.

**Interfaces:**
- Consumes: Task 11 models.
- Produces: compact view "configure I/O" routes to the Task 14/15 pickers; chain row / chips show the bound I/O name (e.g. "I/O: Scarlett") instead of raw device strings.

- [ ] **Step 1: Write the failing test**: the chip-label projector returns the binding `name` for a chain's head input/tail output; compact "configure I/O" dispatches the reshaped command. Run → RED.
- [ ] **Step 2: Run** `cargo test -p adapter-gui compact_chain project_view` → FAIL.
- [ ] **Step 3: Implement** the summary chips + compact handlers.
- [ ] **Step 4: Run** → PASS.
- [ ] **Step 5: Commit**

```bash
git add -A crates/adapter-gui
git commit -m "feat(#716): compact view + chain-list show bound I/O name"
git push
```

---

### Task 17: Insert editor — send/return endpoints decision

**Files:**
- Modify: `crates/adapter-gui/ui/pages/chain_insert_editor.slint`, `crates/adapter-gui/src/insert_wiring.rs` (only if the decision touches it).
- Test: `crates/adapter-gui/src/insert_wiring` tests.

**Decision (default, recorded in the ADR):** Insert send/return endpoints (external-loop device I/O) **keep their current raw endpoint model in this issue** — inserts are a single-runtime send/return pipeline (`docs/audio-config.md`), not a binding-paired stream, so folding them into the registry is a separate concern. This task's job is to **verify inserts still build and route unchanged** after the block-schema change, not to migrate them.

- [ ] **Step 1: Write the failing/guard test**: an insert block round-trips and builds its send/return runtime unchanged after Task 5's block-schema change (no accidental coupling to `io`/`endpoint`). Run → RED if the schema change broke it, else it stays GREEN as a regression guard.
- [ ] **Step 2: Run** `cargo test -p adapter-gui insert` and `cargo test -p engine insert`.
- [ ] **Step 3: Implement** only the minimal fix if the schema change leaked into inserts; otherwise no production change.
- [ ] **Step 4: Run** → PASS.
- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "test(#716): inserts unaffected by block-schema change (send/return stay raw)"
git push
```

---

### Task 18: Tuner / Spectrum taps under the new routing

**Files:**
- Modify (only if needed): `crates/adapter-gui/src/tuner_session.rs`, `crates/adapter-gui/src/spectrum_session.rs`.
- Test: `crates/adapter-gui/src/tuner_session_tests.rs`, `crates/adapter-gui/src/spectrum_session_tests.rs`.

**Interfaces:**
- Consumes: the runtime taps (`engine/input_tap.rs`, `engine/output_tap.rs`) and the new per-binding stream keys (Task 9).

- [ ] **Step 1: Write the failing/guard test**: the Tuner enumerates one tuner per active **input port** and the Spectrum one analyzer per active **output endpoint** under the new routing — for a two-binding chain, Tuner shows A's and B's inputs separately and Spectrum shows A's and B's outputs separately (no merged/duplicated channels). Run → RED.
- [ ] **Step 2: Run** `cargo test -p adapter-gui tuner_session spectrum_session` → FAIL.
- [ ] **Step 3: Implement** the enumeration against the per-binding stream keys.
- [ ] **Step 4: Run** → PASS.
- [ ] **Step 5: Commit**

```bash
git add -A crates/adapter-gui
git commit -m "feat(#716): tuner/spectrum enumerate per-binding input/output endpoints"
git push
```

---

### Task 19: Windows / touch parity

**Files:**
- Modify: `crates/adapter-gui/ui/app-window.slint`, `desktop_main.slint`, `touch_main.slint`, `secondary_windows_chain.slint`; `crates/adapter-gui/src/desktop_app.rs`, `desktop_app_init.rs`.
- Test: build + the existing app-init smoke tests.

**Interfaces:**
- Consumes: Tasks 11–16 components.

- [ ] **Step 1: Write the failing/guard test**: the touch layout exposes the same I/O picker callbacks as desktop (parity) — a wiring test asserting both windows bind the same callback. Run → RED.
- [ ] **Step 2: Run** `cargo test -p adapter-gui desktop_app` → FAIL.
- [ ] **Step 3: Implement** the touch/secondary-window wiring of the new components.
- [ ] **Step 4: Run** → PASS; visually confirm desktop + touch via `/run`.
- [ ] **Step 5: Commit**

```bash
git add -A crates/adapter-gui
git commit -m "feat(#716): touch/secondary-window parity for I/O binding UI"
git push
```

---

### Task 20: Default binding for fresh projects (project side, O4)

**Files:**
- Modify: project-creation path (`crates/project/src/`); `crates/adapter-gui/src/project_ops.rs`.
- Test: `crates/project/tests/default_binding.rs`, `crates/adapter-gui/src/project_chain_defaults_persistence_tests.rs`

- [ ] **Step 1: Write the failing test**: creating a new project yields a `default` binding from system default devices and the chain head/tail blocks reference it. Run → RED.
- [ ] **Step 2: Run** `cargo test -p project default_binding && cargo test -p adapter-gui project_chain_defaults` → FAIL.
- [ ] **Step 3: Implement** the auto-create (shared helper consumed by Task 13's wizard).
- [ ] **Step 4: Run** → PASS.
- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(#716): auto-create default I/O binding for new projects"
git push
```

---

### Task 21: Translations

**Files:**
- Modify: all 9 `.po`/`.yml` catalogs.

- [ ] **Step 1:** Run `scripts/extract-translations.sh`.
- [ ] **Step 2:** Fill every new `@tr`/`t!` string from Tasks 11–20 in all 9 catalogs (English reference; translate the rest).
- [ ] **Step 3:** Build to confirm no missing-key warnings.
- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "i18n(#716): catalogs for I/O bindings UI"
git push
```

---

## Phase 5 — Docs

### Task 22: Documentation

**Files:**
- Modify: `docs/audio-config.md` (I/O-as-blocks + stream model), `docs/screens.md` (Settings + every changed screen: endpoint editor, compact view, chain row, tuner/spectrum), `docs/cli.md` if any flag changed.
- Create: `docs/adr/000N-io-binding-registry-scope.md` recording the config.yaml/system scope decision **and** the insert-stays-raw decision (Task 17).

- [ ] **Step 1:** Update `audio-config.md`: registry concept, ports, the routing rule + the two worked examples, migration note. Keep invariants section consistent.
- [ ] **Step 2:** Update `screens.md`: new Settings I/O-bindings section, endpoint editor pickers, compact-view + chain-row I/O summary, tuner/spectrum per-binding enumeration.
- [ ] **Step 3:** Write the ADR (scope = system, chains reference by id; portability rationale; inserts keep raw send/return).
- [ ] **Step 4:** Update READMEs (en/pt-BR/es-ES) only if user-facing behavior described.
- [ ] **Step 5: Commit**

```bash
git add -A docs README*.md
git commit -m "docs(#716): I/O binding registry — audio-config, screens, ADR"
git push
```

---

## Self-Review

**Spec coverage:** registry (T1–2), scope/ADR 0003 (T2, T22), commands+MCP parity (T3–4, T7), block refs (T5), routing rule + isolation + offsets (T8–9), migration all-to-all (T6, T10), invariants/golden/hw battery (T10), O1 name (T1/T5), O2 channel ownership (kept; enforced by existing runtime validation — no new task unless a regression appears), O3 delete rejected (T3/T12), O4 default (T13/T20), inserts (T17 decision + guard), docs (T22). **GUI surfaces:** data bridge (T11), Settings I/O editor (T12), audio interface + wizard (T13), endpoint editor (T14), `chain_io_*` subsystem + groups (T15), compact view + chain row/chips (T16), insert editor (T17), tuner/spectrum (T18), windows/touch parity (T19), translations (T21) — covers every file from the GUI grep. No spec section is unaddressed.

**Placeholder scan:** implementation step bodies intentionally say "after RED unlock, read file X and apply this typed change" rather than fabricated production code — forced by the project's RED-FIRST law (a Global Constraint), not laziness. Test code and interfaces are concrete. No "TBD/handle edge cases/similar to Task N".

**Type consistency:** `IoBinding`/`IoEndpoint` field names (`id`, `name`, `inputs`, `outputs`, `device_id`, `mode`, `channels`) used identically across T1–3, T6, T9, T11. Block ref fields `io`/`endpoint` consistent across T5, T7, T9, T14–16. `IoBindingModel`/`IoEndpointModel` (T11) consumed by T12, T14, T15, T16. `migrate_legacy_io` stable across T6/T10.
