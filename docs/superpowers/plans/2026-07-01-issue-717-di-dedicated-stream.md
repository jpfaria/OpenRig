# DI Loop as a Dedicated Isolated Stream — Implementation Plan (#717)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** An armed DI loop plays on its own isolated runtime (a copy of the chain's block graph), routed to a per-chain chosen output, with its own on-screen graph + meters — never on the guitar's stream.

**Architecture:** Add a second, input-less runtime per chain built only while the DI is armed. It reads the DI buffer, runs an independent copy of the chain's blocks, and writes to the chosen output route at that output's rate (reusing #749's per-output resample). The guitar runtime is untouched (invariant #4). The chosen output is persisted per-chain in `project.openrig`.

**Tech Stack:** Rust (engine, application, infra-cpal, domain, project, adapter-gui), Slint (UI), cpal (audio backend).

**Design spec:** `docs/superpowers/specs/2026-07-01-issue-717-di-dedicated-stream-design.md` (owner-approved).

## Global Constraints

- Audio-thread invariants (CLAUDE.md): zero alloc/lock/syscall/IO on any callback; zero xruns/dropouts; stream isolation (#4) — the DI runtime shares no buffer/route/tap/DSP state with the guitar runtime; internal stereo.
- TDD red-first is mandatory: every production change is preceded by a test that FAILS first (watch the assertion fail, not just a compile error). Create `.claude/.red-first-unlocked` only after seeing the RED; the guard re-arms on commit.
- Every state-changing operation is a `Command`; every GUI read is a `Query` — MCP/gRPC parity tests stay green.
- Repo content in English; chat pt-BR. New `@tr` keys → `.pot` + all locale `.po` in the same commit.
- Docs updated in the same commit as the behaviour (`docs/screens.md`, `docs/audio-config.md`).
- Work only under `.solvers/issue-717/`; branch `feature/issue-717`; push after each commit; assign milestone `v0.1.1` at PR/close.
- UI work: invoke `ui-ux-pro-max` + `slint` + `slint-best-practices` and render headless before claiming a layout done.

---

### Task 1: SPIKE — how an input-less DI runtime is clocked (resolves the crux)

This task decides the DI-stream driver, which the engine tasks depend on. It is investigation + a pinned decision, not a paper step — it ends with a passing test that fixes the chosen behaviour.

**Files:**
- Read: `crates/infra-cpal/src/stream_builder.rs`, `crates/infra-cpal/src/controller.rs`, `crates/infra-cpal/src/controller_taps.rs`, `crates/engine/src/runtime.rs`, `crates/engine/src/runtime_state.rs`
- Create (decision record): `docs/superpowers/specs/2026-07-01-issue-717-di-stream-clock-decision.md`
- Test: `crates/infra-cpal/tests/issue_717_di_dedicated_runtime.rs`

**Interfaces:**
- Produces: the chosen mechanism — **(a)** output-device-driven generation, or **(b)** input-less runtime on a virtual/duplex clock — plus the API the later tasks build on (e.g. `ProjectRuntimeController::arm_di_stream(chain, DiPcm, output_endpoint)` / `disarm_di_stream(chain)`), named in the decision record.

- [ ] **Step 1: Write the failing behavioural test** — arming the DI must NOT put the loop on the guitar runtime; a separate DI runtime must exist and carry the loop.

```rust
// crates/infra-cpal/tests/issue_717_di_dedicated_runtime.rs
// Build a chain runtime (guitar) + arm a DI stream. Assert:
//  - the guitar runtime has NO di_loop (has_di_loop() == false)
//  - a distinct DI runtime exists for the chain and carries the loop
#[test]
fn di_arms_on_a_separate_runtime_not_the_guitar_runtime() {
    // (built against the arm_di_stream API decided in this task)
}
```

- [ ] **Step 2: Run it — expect FAIL** (`arm_di_stream` unresolved / today the loop lands on the guitar runtime).

Run: `cargo test -p infra-cpal --test issue_717_di_dedicated_runtime`
Expected: FAIL (assertion or unresolved API).

- [ ] **Step 3: Investigate + record the decision.** Read the streaming files; write `2026-07-01-issue-717-di-stream-clock-decision.md` stating mechanism (a) or (b), why it holds the audio-thread invariants (no alloc/lock/IO on callback; no drift/xruns), and the exact `arm_di_stream`/`disarm_di_stream` signatures the next tasks consume.

- [ ] **Step 4: Implement the minimal driver + arm/disarm** to make the test pass (separate runtime built on arm, torn down on disarm; guitar runtime untouched).

- [ ] **Step 5: Run — expect PASS.** `cargo test -p infra-cpal --test issue_717_di_dedicated_runtime`

- [ ] **Step 6: Commit.** `git add crates/infra-cpal/... docs/superpowers/specs/2026-07-01-issue-717-di-stream-clock-decision.md && git commit`

> Gate: the mechanism and API names below (`arm_di_stream`, `disarm_di_stream`) are FINALISED by this task's decision record. If a signature differs, update Tasks 4–7's calls to match — same behaviour, corrected names.

---

### Task 2: Persisted per-chain DI output field (data model)

**Files:**
- Modify: `crates/project/src/chain.rs` (add the field to `Chain`)
- Test: `crates/project/tests/` (round-trip serde of a chain with the field)

**Interfaces:**
- Produces: `Chain.di_output: Option<DiOutputRef>` where `DiOutputRef` names one of the chain's bound output endpoints (exact shape — endpoint name vs. index into the chain's output bindings — chosen against `crates/domain/src/io_binding.rs`; default `None` ⇒ chain main output).

- [ ] **Step 1: Write the failing test** — a `Chain` with `di_output = Some(...)` round-trips through the project (YAML) serialize→deserialize unchanged; a legacy chain without the field deserializes to `None`.
- [ ] **Step 2: Run — expect FAIL** (unknown field). `cargo test -p project di_output`
- [ ] **Step 3: Add the field** (`#[serde(default, skip_serializing_if = "Option::is_none")]`) so existing `.openrig` files are unaffected.
- [ ] **Step 4: Run — expect PASS.**
- [ ] **Step 5: Commit.**

---

### Task 3: `Command::SetChainDiLoopOutput` + Event + dispatcher handler (parity)

**Files:**
- Modify: `crates/application/src/command.rs` (new variant), `crates/application/src/event.rs` (new event), `crates/application/src/local_dispatcher_di_loop.rs` (handler), `crates/application/src/local_dispatcher.rs` (route)
- Test: `crates/application/tests/` (dispatch sets the chain field + emits event; unknown chain → Err), parity test stays green.

**Interfaces:**
- Consumes: `Chain.di_output` (Task 2).
- Produces: `Command::SetChainDiLoopOutput { chain: ChainId, output: DiOutputRef }` → `Event::ChainDiLoopOutputChanged { chain }`.

- [ ] **Step 1: Write the failing test** — dispatching `SetChainDiLoopOutput` mutates the chain's `di_output` and emits `ChainDiLoopOutputChanged`; a missing chain returns `Err`.
- [ ] **Step 2: Run — expect FAIL.** `cargo test -p application set_chain_di_loop_output`
- [ ] **Step 3: Add the variant + event + handler**, following the `SetChainDiLoopSource` precedent.
- [ ] **Step 4: Run — expect PASS**, and `cargo test -p adapter-mcp` parity (tool count == command variants).
- [ ] **Step 5: Commit.**

---

### Task 4: Route the DI stream to the chosen output at that output's rate

**Files:**
- Modify: `crates/infra-cpal/src/controller_taps.rs` (arm resolves `Chain.di_output` → the endpoint's route + rate; reuse #749 `DiPcm::to_loop_at`)
- Test: `crates/infra-cpal/tests/issue_717_di_dedicated_runtime.rs` (extend)

**Interfaces:**
- Consumes: `arm_di_stream` (Task 1), `Chain.di_output` (Task 2).

- [ ] **Step 1: Write the failing test** — a chain with two bound outputs at different rates; arming with `di_output = second` puts the DI runtime on the second output's route AND at its rate (loop length matches that rate; guitar untouched).
- [ ] **Step 2: Run — expect FAIL** (arm ignores `di_output`, defaults to main).
- [ ] **Step 3: Implement** the output resolution in the arm path.
- [ ] **Step 4: Run — expect PASS.**
- [ ] **Step 5: Commit.**

---

### Task 5: adapter-gui wiring — arm/disarm the DI stream (not the guitar segment)

**Files:**
- Modify: `crates/adapter-gui/src/di_loop_wiring.rs` (play/stop call `arm_di_stream`/`disarm_di_stream` with the chain's `di_output`, replacing the guitar-segment `set_chain_di_loop` path)
- Test: `crates/adapter-gui/tests/issue_614_di_loop_wiring.rs` / new (arming leaves the guitar runtime's `has_di_loop()` false)

**Interfaces:**
- Consumes: `arm_di_stream`/`disarm_di_stream` (Task 1), `di_loop_for_chain` (existing DiPcm store), `Chain.di_output` (Task 2).

- [ ] **Step 1: Write the failing test** — `play_chain_di_loop` arms the dedicated DI runtime and leaves the guitar runtime unarmed.
- [ ] **Step 2: Run — expect FAIL** (today it arms the guitar runtime).
- [ ] **Step 3: Rewire play/stop** to the dedicated-stream API.
- [ ] **Step 4: Run — expect PASS.**
- [ ] **Step 5: Commit.**

---

### Task 6: DI output select in the panel (UI)

**Files:**
- Modify: `crates/adapter-gui/ui/components/di_loop_panel.slint` (add the reusable Select for outputs), the panel globals/wiring, translations
- Test: headless interaction test (picking an output dispatches the command intent) + `tools/slint-render` PNG

**Interfaces:**
- Consumes: `Command::SetChainDiLoopOutput` intent (Task 3), the chain's bound output endpoints (list source).

- [ ] **Step 1: Invoke `ui-ux-pro-max` + `slint` + `slint-best-practices`.**
- [ ] **Step 2: Write the failing interaction test** — the output select lists the chain's bound outputs; picking one fires the output-selected callback with that endpoint.
- [ ] **Step 3: Run — expect FAIL.**
- [ ] **Step 4: Add the second Select** (reuse the existing component) + wire the callback; add `@tr` keys to `.pot` + all `.po`.
- [ ] **Step 5: Run — expect PASS; render the PNG and verify layout.**
- [ ] **Step 6: Commit.**

---

### Task 7: Dedicated DI-stream graph + own meters (owner requirement)

**Files:**
- Modify: `crates/application/src/bridge.rs` (or the QueryKind home) — expose the DI runtime's input/output meters as a `Query` (read parity)
- Create: a Slint DI-stream graph component (IN=DI source → block copy → OUT=chosen output, own meters), shown only while armed
- Modify: the Chains screen to render it while the DI plays
- Test: query returns the DI runtime's meters; headless render of the DI graph

**Interfaces:**
- Consumes: the DI runtime (Task 1), its meters.

- [ ] **Step 1: Invoke `ui-ux-pro-max` + `slint`; write the failing Query test** — a `QueryKind` for the DI stream's meters returns values while armed, `None`/empty when not.
- [ ] **Step 2: Run — expect FAIL.**
- [ ] **Step 3: Add the QueryKind + serve it** (MCP resource parity).
- [ ] **Step 4: Build the DI graph component**, bound to the DI meters, visible only while armed; render the PNG and verify.
- [ ] **Step 5: Run tests + render — expect PASS.**
- [ ] **Step 6: Commit.**

---

### Task 8: Docs + wrap-up

**Files:**
- Modify: `docs/screens.md` (DI = dedicated stream + its own graph + output select), `docs/audio-config.md` (DI is a separate isolated runtime, no longer input-substitution)
- Real-hardware: `OPENRIG_HW_TESTS=1` battery — guitar + DI running together, assert no xruns.

- [ ] **Step 1: Update docs** in the same spirit as the behaviour.
- [ ] **Step 2: Run the HW battery** on an idle machine; confirm zero xruns/underruns with both streams live.
- [ ] **Step 3: Commit; open PR (milestone v0.1.1, closes #717) when the owner asks.**

---

## Self-Review

- **Spec coverage:** dedicated runtime (T1,4,5) · block copy (T1 — the DI runtime builds the chain's blocks) · chosen persisted output among bound endpoints (T2,3,4,6) · own graph + meters (T7) · guitar untouched/simultaneous (T1,5 assertions) · parity (T3,7) · docs/i18n (T6,8). Covered.
- **Placeholders:** the only deferred detail is the clock mechanism — deliberately Task 1's spike output, not a hidden TODO.
- **Type consistency:** `arm_di_stream`/`disarm_di_stream`, `Chain.di_output`/`DiOutputRef`, `Command::SetChainDiLoopOutput`/`Event::ChainDiLoopOutputChanged` are used consistently; Task 1's decision record is the single source for the arm API and later tasks adjust calls to its final signatures.
