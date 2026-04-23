# Command Dispatch Architecture Design

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Decouple OpenRig's UI from its domain by routing every interaction through a typed `CommandDispatcher`. Enable headless operation, remote control over gRPC, MIDI/OSC/BLE-MIDI controllers, and a scriptable CLI — all consuming the same fine-grained `Command`/`Event` vocabulary.

---

## Problem

Today `crates/adapter-gui/src/lib.rs` (~9.5k lines) wires every Slint `on_*` callback (knob change, model swap, bypass, device picker, file picker) directly to `ProjectSession` mutations. There is no abstraction layer between UI and domain. Consequences:

- **No headless mode.** Engine and GUI are baked into one binary. Cannot run engine on Orange Pi and control from a tablet.
- **No external controllers.** A footswitch, expression pedal, or BLE-MIDI device cannot drive the same operations the GUI does.
- **No scripting.** Show automation, preset switching from external software, integration with QLab/Ableton — none possible.
- **Hard to test.** Interaction logic only runs inside a live Slint window.
- **Coupling.** Any new transport would re-invent its own command shape.

---

## Solution

A single, internal command bus. Every state change in OpenRig is expressed as a typed `Command`. Every observable change is emitted as a typed `Event`. Anything that wants to drive the rig (the GUI, gRPC, MIDI, CLI) produces `Command`s. Anything that wants to react to the rig (the GUI, remote clients, MIDI feedback) consumes `Event`s.

Rolled out in 4 phases, one GitHub issue each:

| Phase | Issue | Scope |
|-------|-------|-------|
| 1 | [#295](https://github.com/jpfaria/OpenRig/issues/295) | Internal command bus, `LocalDispatcher`, refactor of all GUI callbacks |
| 2 | [#296](https://github.com/jpfaria/OpenRig/issues/296) | gRPC transport, `--server` / `--client` modes, `RemoteDispatcher` |
| 3 | [#297](https://github.com/jpfaria/OpenRig/issues/297) | MIDI/OSC adapter (incl. BLE-MIDI), YAML mapping, daemon |
| 4 | [#298](https://github.com/jpfaria/OpenRig/issues/298) | `openrig-cli` over gRPC for scripting and automation |

Phase 1 is the foundation. Phases 2–4 each depend only on Phase 1 (and Phase 4 also on Phase 2).

---

## Shared Architecture

### Types

```rust
// Defined in a new module/crate (e.g. `crates/command-bus` or `crates/application/src/commands.rs`).

/// Every state change the UI or any controller can request.
/// Fine-grained: one variant per current Slint `on_*` callback.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum Command {
    SetBlockParameterNumber { chain: ChainId, block: BlockId, path: String, value: f64 },
    SetBlockParameterBool   { chain: ChainId, block: BlockId, path: String, value: bool },
    SetBlockParameterText   { chain: ChainId, block: BlockId, path: String, value: String },
    SelectBlockParameterOption { chain: ChainId, block: BlockId, path: String, index: usize },
    PickBlockParameterFile  { chain: ChainId, block: BlockId, path: String, file: PathBuf },
    ToggleBlockEnabled      { chain: ChainId, block: BlockId },
    ReplaceBlockModel       { chain: ChainId, block: BlockId, model_id: String },
    AddBlock                { chain: ChainId, kind: BlockKind, model_id: String, position: usize },
    RemoveBlock             { chain: ChainId, block: BlockId },
    MoveBlock               { chain: ChainId, block: BlockId, new_position: usize },
    SelectInputDevice       { chain: ChainId, block: BlockId, entry: usize, device_id: String },
    ToggleInputChannel      { chain: ChainId, block: BlockId, entry: usize, channel: u32, selected: bool },
    SelectOutputDevice      { chain: ChainId, block: BlockId, entry: usize, device_id: String },
    ToggleOutputChannel     { chain: ChainId, block: BlockId, entry: usize, channel: u32, selected: bool },
    SelectOutputMode        { chain: ChainId, block: BlockId, entry: usize, mode: ChannelMode },
    // …Insert send/return variants…
    SaveProject,
    LoadProject             { path: PathBuf },
    // Full enumeration emerges from auditing every `on_*` in `lib.rs`.
}

/// Every observable change emitted by the dispatcher.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum Event {
    ProjectMutated,
    ChainReloaded            { chain: ChainId },
    BlockParameterChanged    { chain: ChainId, block: BlockId, path: String },
    BlockEnabledChanged      { chain: ChainId, block: BlockId, enabled: bool },
    BlockReplaced            { chain: ChainId, block: BlockId },
    BlockAdded               { chain: ChainId, block: BlockId },
    BlockRemoved             { chain: ChainId, block: BlockId },
    DeviceChanged            { chain: ChainId, block: BlockId },
    ProjectLoaded,
    ProjectSaved,
    Error                    { message: String },
}

pub trait CommandDispatcher: Send + Sync {
    fn dispatch(&self, cmd: Command) -> Result<Vec<Event>>;
    /// Subscribe to all events emitted by this dispatcher (including those
    /// triggered by other clients in remote mode).
    fn subscribe(&self) -> EventStream;
}
```

### Implementations

- **`LocalDispatcher`** (Phase 1) — holds the `ProjectSession` directly, mutates in-process. The default for the bundled GUI.
- **`RemoteDispatcher`** (Phase 2) — serializes `Command` over gRPC, awaits acknowledgement, surfaces server-emitted `Event`s through the same `EventStream`.

The `CommandDispatcher` trait is the only abstraction the rest of OpenRig sees. Switching between local and remote is a single line at startup.

### Data flow

```
GUI (Slint on_* callback)  ─┐
MIDI/OSC daemon            ─┤
openrig-cli                ─┼──► CommandDispatcher.dispatch(cmd) ──► Vec<Event>
gRPC server (from clients) ─┤                                              │
                            └──► (LocalDispatcher mutates ProjectSession)  │
                                                                           ▼
                                              Subscribers (GUI, gRPC clients, MIDI feedback)
```

---

## Phase 1 — Internal Command Bus (Issue #295)

**Goal:** introduce `Command`, `Event`, `CommandDispatcher`, `LocalDispatcher`. Refactor every Slint callback to dispatch instead of mutate. Zero behavior change.

### Steps

1. Audit every `on_*` callback in `crates/adapter-gui/src/lib.rs` and produce the exhaustive `Command` enum.
2. Identify every state-change side effect that the GUI currently observes (e.g. "after this mutation, refresh the chain view") and codify them as `Event` variants.
3. Create the new module/crate. The location is a small design choice during execution: either a new `crates/command-bus` crate (cleaner boundary, easier to test in isolation) or a `commands` module inside the existing `application` crate (less ceremony). Recommend the new crate for clarity.
4. Implement `LocalDispatcher` by lifting the existing in-callback logic into per-`Command` handler functions.
5. Refactor each `on_*` callback to: build the right `Command`, call `dispatch.dispatch(cmd)`, then react to the returned `Event`s (typically: re-read state and update Slint properties).
6. All existing tests must pass unchanged.
7. Add new dispatcher-level tests that exercise each `Command` against a stub `ProjectSession` without spinning up Slint.

### Non-goals for Phase 1

- No undo/redo (deferred).
- No batching (one `Command` per dispatch call). Future optimization if needed.
- No event bus broker between subscribers — `LocalDispatcher` calls subscribers synchronously. Async fan-out lands with Phase 2 if needed.

### Risk and mitigation

- **Risk:** missing a callback during the audit, leaving a direct mutation in `lib.rs`. **Mitigation:** lint or grep gate in CI that fails on any `project_session.borrow_mut()` outside the dispatcher.
- **Risk:** the refactor touches 9.5k lines of `lib.rs` and conflicts with parallel work. **Mitigation:** announce the refactor; merge `develop` frequently; expect to land in one large PR rather than many small ones.

---

## Phase 2 — gRPC Transport (Issue #296)

**Goal:** allow the engine to run headless on one machine and the GUI on another, communicating over gRPC.

### Why gRPC

Decided up front to avoid bikeshedding mid-implementation:

- Strong schema via `.proto` — bindings auto-generate for any future client (Swift, Kotlin, Python scripts).
- Native bidirectional streaming (events fan out cleanly to multiple clients).
- Acceptable binary-size cost (we already ship NAM/IR runtime; tonic adds ~20MB but fits the bundle).
- Discoverable: any teammate can use `grpcurl` to inspect the server.

WebSocket+JSON was considered and rejected: weaker schema, manual validation, no auto-generated clients.

### Steps

1. Create `crates/adapter-grpc` with `tonic` + `prost` dependencies.
2. Define `proto/openrig.proto` mirroring `Command` and `Event`. Provide `From`/`Into` impls between protobuf types and the Rust enums.
3. Implement the server: bidirectional streaming RPC, one `stream Command` in, one `stream Event` out per connection.
4. Implement `RemoteDispatcher` (client side) implementing the `CommandDispatcher` trait from Phase 1.
5. Add CLI flags to the OpenRig binary:
   - `--server [PORT]` — runs the audio engine without a GUI; opens the gRPC port.
   - `--client URL` — runs the GUI without an engine; connects to the URL via `RemoteDispatcher`.
   - No flags = current behavior (`LocalDispatcher`, GUI + engine in one process).
6. Document the **Wi-Fi hotspot pattern** for venues without infrastructure network (see Operational Notes below).

### Latency target

LAN round-trip (command → event echo) under 30ms in the typical case. Measured and reported in the issue. If exceeded, the implementation must investigate (HTTP/2 framing overhead, Nagle, etc.) before declaring done.

### Threat model

This phase is **trusted-LAN only**. No authentication, no TLS. The intended deployment is a private Wi-Fi network controlled by the user (their own hotspot or home network). Hardening (mTLS, token auth) is a follow-up issue if/when needed. Document this clearly so users do not expose the gRPC port to the public internet.

### Bluetooth note

gRPC over BLE is **not** in scope. Bluetooth control happens via BLE-MIDI in Phase 3 (see Operational Notes for the rationale).

---

## Phase 3 — MIDI/OSC + BLE-MIDI Adapter (Issue #297)

**Goal:** physical and wireless controllers (footswitches, expression pedals, iPads, BLE-MIDI devices, OSC apps) drive the same `Command`s the GUI uses.

### Steps

1. Create `crates/adapter-midi` using a cross-platform MIDI crate (e.g. `midir`) that already abstracts USB-MIDI and BLE-MIDI behind one input interface.
2. Optionally include OSC support behind a Cargo feature flag (e.g. `rosc`).
3. Define a YAML mapping format at the per-OS config dir (`~/Library/Application Support/OpenRig/midi-map.yaml` on macOS, `%APPDATA%\OpenRig\midi-map.yaml` on Windows, `~/.config/OpenRig/midi-map.yaml` on Linux):
   ```yaml
   bindings:
     - source: { kind: midi, channel: 1, cc: 7 }
       command: SetBlockParameterNumber
       args:
         chain: 0
         block: 1
         path: gain
         scale: { type: linear, min: 0.0, max: 100.0 }
     - source: { kind: midi, channel: 1, note: 60, action: note_on }
       command: ToggleBlockEnabled
       args: { chain: 0, block: 2 }
   ```
4. Implement the daemon: subscribe to MIDI/OSC sources, look up bindings, build `Command`s, call `dispatcher.dispatch(cmd)`. Works against any `CommandDispatcher` (local or remote).
5. Optional: hot-reload the mapping file on change.

### Why BLE-MIDI lives in this crate

BLE-MIDI is the standard wireless protocol for music controllers (every modern footswitch, every iPad music app speaks it). Treating it as just another MIDI source means iPads, wireless footswitches, and BLE pedals work out of the box — no bespoke OpenRig protocol needed.

### Out of scope for this phase

- Mapping editor UI inside OpenRig (hand-edited YAML for v1).
- Per-project mapping files (one global map for v1).
- MIDI **output** (sending MIDI to other devices).

---

## Phase 4 — `openrig-cli` (Issue #298)

**Goal:** scriptable CLI client over gRPC. Enables show automation, integration with show-control software, and rapid debugging of the gRPC layer.

### Steps

1. Create binary crate `openrig-cli` in the workspace.
2. Implement subcommands:
   - `set <path> <value>` (e.g. `set chain.0.block.2.gain 70`)
   - `bypass <path>` (toggle a block's enabled state)
   - `load-preset <id>` (replace the active project)
   - `watch` (stream `Event`s, one per line)
   - `list chains` / `list blocks <chain>` (read state for path discovery)
3. Connection: `--server URL` flag, `OPENRIG_SERVER` env var, sensible default (`localhost:<port>`).
4. Path syntax: `chain.<idx>.block.<idx>.<param>` parsed and converted to the right `Command` variant.

### Out of scope

- Interactive REPL.
- Authentication (inherits Phase 2's trust model).
- Bash/zsh completion (follow-up).

---

## Operational Notes

### Wi-Fi hotspot pattern (Orange Pi)

For live use without infrastructure network, the Orange Pi must act as a Wi-Fi access point so phones/tablets/laptops can connect and reach the gRPC server. NetworkManager handles this:

```bash
nmcli device wifi hotspot ifname wlan0 ssid OpenRig password <chosen-password>
```

This is documented in the Phase 2 issue and in the OpenRig user docs once that phase ships. The hotspot setup is a one-time operation per Pi, not part of every boot.

### Why not gRPC over Bluetooth

gRPC requires HTTP/2, which requires TCP/IP. Bluetooth options were:

- **BLE GATT** — too low throughput for streaming events; would require a custom non-gRPC protocol.
- **Bluetooth Classic SPP/RFCOMM** — possible to tunnel TCP, but pairing UX is poor and mobile platform support is patchy.
- **BNEP/PAN** — emulates Ethernet over Bluetooth; iOS/Android support is essentially gone in 2025.

The chosen path: **Wi-Fi hotspot for gRPC, Bluetooth only for BLE-MIDI in Phase 3.** Covers the realistic use cases (tablet UI on hotspot, wireless footswitch via BLE-MIDI) without inventing a bespoke transport. A custom BLE protocol (for a hypothetical OpenRig companion app) is intentionally deferred.

---

## Testing Strategy

- **Phase 1:** every existing test must pass without modification. New dispatcher-level tests cover each `Command` against a stub `ProjectSession`. No GUI required.
- **Phase 2:** integration tests with the gRPC server in the same process as a `RemoteDispatcher` client; round-trip every `Command` variant. Latency benchmark documented.
- **Phase 3:** unit tests for the YAML mapping parser; integration tests with a fake MIDI source stubbed in front of the daemon.
- **Phase 4:** integration tests that boot the gRPC server and exercise each subcommand end-to-end.

All four phases preserve the project's zero-warnings rule.

---

## Out of Scope (across all phases)

- Undo/redo for `Command`s — possible follow-up; not part of this design.
- Authentication / TLS for gRPC — first cut is trusted-LAN. Harden later.
- Custom BLE protocol for a companion app — deferred until/unless that app exists.
- Mobile (iOS/Android) clients — only the existing Slint GUI is wired up. Mobile is a separate effort.
- Mapping editor UI — hand-edited YAML for v1 of MIDI.
- Per-project MIDI mapping — one global mapping file for v1.

---

## Cross-references

- Issue #295 — Phase 1, command bus
- Issue #296 — Phase 2, gRPC transport
- Issue #297 — Phase 3, MIDI/OSC + BLE-MIDI
- Issue #298 — Phase 4, `openrig-cli`
