# MIDI/OSC + BLE-MIDI Adapter — Design (#22)

**Date:** 2026-05-18
**Issue:** #22
**Branch:** `feature/issue-22` (from `feature/issue-165` — sibling, co-evolves with #165/#436)
**Status:** Approved design — pending implementation plan

Realizes Phase 3 of `2026-04-23-command-dispatch-architecture-design.md`. This spec
**reuses the thread bridge introduced by #165** rather than inventing its own transport
plumbing — MIDI is just another `Command` producer on the existing bus.

---

## Goal

Let physical and wireless controllers — MIDI footswitches, expression pedals, BLE-MIDI
devices (M-Vave Chocolate, iPad apps, modern wireless footswitches), OSC apps — drive the
**same `Command`s the GUI uses**, on the same running OpenRig instance. Hands-free preset
switching, bypass toggling and parameter rides while playing.

## Non-goals (v1)

- Mapping editor UI inside OpenRig — hand-edited `midi-map.yaml` for v1.
- Per-project mapping files — one global map for v1.
- MIDI **output** (sending MIDI to other devices) — input only.
- MIDI feedback to controller LEDs (events fan out over the bridge already; wiring LED
  feedback to a specific controller is a follow-up).
- Auth beyond what the local machine implies.

---

## Context (what already exists — do not rebuild)

The command bus (#295) and the **thread bridge** (#165) are done on the `feature/issue-165`
line this branch sits on:

- `crates/application/src/command.rs` — `Command` enum, 30 variants, `Serialize +
  Deserialize + JsonSchema`.
- `crates/application/src/bridge.rs` — `CommandBridge::submit(cmd) -> oneshot` (`Send`,
  cloneable, held by the transport thread) + `BridgeDrain::drain(dispatcher, cap)` (owned
  by the frontend thread, non-blocking, called per tick). **This is exactly what MIDI
  needs.** No new bridge.
- `crates/application/src/publishing_dispatcher.rs` — `PublishingDispatcher` decorator;
  every dispatched `Command`'s `Vec<Event>` fans out on an `EventSink`. Future LED feedback
  consumes this for free.
- `crates/adapter-mcp` — the **structural template**: a frontend-agnostic library crate
  that receives only the bridge handle and runs its protocol on its own thread.
- Frontend wiring pattern: the `--mcp` opt-in already constructs the bridge, wraps the
  dispatcher with `PublishingDispatcher`, spawns the adapter thread, and drains per tick.
  `--midi` reuses the **same** drain point.

Conclusion: #22 is `adapter-mcp` with the protocol layer swapped. The bridge, the
dispatch path, the event fan-out, and the per-tick drain are all reused unchanged.

---

## Architecture

### Principle

A **frontend** (GUI or console) owns the application, the `LocalDispatcher`, the audio
engine and the event loop. The MIDI adapter is **not a frontend and not a launch mode** —
it is an optional input source that *attaches* to a running frontend and feeds the same
command bus, identically to MCP.

```
            ┌─────────────── frontend process (GUI or console) ───────────────┐
 player ──▶ │  GUI event loop  ──▶ LocalDispatcher ──▶ ProjectSession           │
            │        │                    ▲                                     │
            │        │ drain (per tick)   │ dispatch()                          │
            │   ┌────┴──────────────────────────────────────────────────────┐  │
 M-Vave ──▶ │   │  adapter-midi  (own thread, midir USB/BLE-MIDI source)     │  │
 Chocolate  │   │   MIDI msg ─▶ binding lookup ─▶ Command ─▶ bridge.submit() │  │
 (BLE)      │   └────────────────────────────────────────────────────────────┘  │
            └──────────────────────────────────────────────────────────────────┘
```

### Crate

New **library** crate `crates/adapter-midi` (mirrors `adapter-mcp`). Frontend-agnostic:
knows nothing about Slint, the console or the engine. Receives only the `CommandBridge`
handle and its config (mapping file path, MIDI port selection).

### Dependencies

- `midir` — cross-platform MIDI input. Abstracts **USB-MIDI and BLE-MIDI** behind one
  input interface (CoreMIDI on macOS speaks to BLE-MIDI devices like the M-Vave Chocolate
  once paired at the OS level — no extra code path).
- `serde_yaml` (already in the workspace) — mapping file parsing.
- `rosc` — **behind a `osc` Cargo feature, off by default**. Not in the v1 build path.

### MIDI source → Command translation

1. The daemon opens the configured input port via `midir` on its own thread.
2. Each inbound message (`Note On/Off`, `CC`, `Program Change`) is matched against the
   loaded bindings.
3. A match builds the typed `Command` (deserializing `args`, applying any `scale`).
4. `bridge.submit(cmd)` — the frontend drains and dispatches on its own thread, exactly as
   an MCP tool call does. The daemon does not await the result for fire-and-forget control
   (a footswitch does not block on a reply); the oneshot is dropped.

**Zero audio-thread impact** by construction: the adapter only injects `Command`s into the
existing bus. No DSP/routing/I/O code is touched. Real-time invariants 1–10 hold exactly
as they do for MCP.

### Mapping file

Single global file at the per-OS config dir, resolved by the same path logic the rest of
OpenRig uses (no hardcoded paths):

- macOS: `~/Library/Application Support/OpenRig/midi-map.yaml`
- Windows: `%APPDATA%\OpenRig\midi-map.yaml`
- Linux: `~/.config/OpenRig/midi-map.yaml`

`source` is **internally tagged on `kind`** (serde_yaml does not support the
externally-tagged map form cleanly; the `kind:` form is also closer to the original
Phase-3 sketch). `chain`/`block` are the project's **string ids** (`ChainId(String)` /
`BlockId(String)`), not ordinals — the `0`/`1` in the original sketch was illustrative.

```yaml
# Selects the input device by case-insensitive substring. Omit → system default.
input: Chocolate

bindings:
  # Footswitch A → toggle bypass
  - source: { kind: note_on, channel: 1, note: 60 }
    command: ToggleBlockEnabled
    args: { chain: "<chain-id>", block: "<block-id>" }

  # Footswitch B → save the project (unit command, no args)
  - source: { kind: program_change, program: 5 }
    command: SaveProject

  # Expression pedal CC 7 → sweep a parameter, 0..127 → 0.0..100.0
  - source: { kind: cc, channel: 1, controller: 7 }
    command: SetBlockParameterNumber
    args: { chain: "<chain-id>", block: "<block-id>", path: gain }
    scale: { min: 0.0, max: 100.0 }
```

- `command` is the `Command` variant name; `args` is its `schemars`-derived shape. The
  same `JsonSchema` that powers MCP tools **validates the mapping at load time** via the
  new `application::command_schema::command_from_variant` (single source of truth for
  "(name, args) → Command", shared with — and the canonical home for — what MCP does).
- `scale` is a **top-level** binding field (not inside `args`), `cc`-only, linear for v1
  (`log` is a follow-up). The scaled value is written into the arg named `into` (default
  `value`); a `cc` without `scale` passes the raw 0..=127 as `value`.
- Validation builds every binding's `Command` (injecting a probe value for scaled
  bindings) at load; an unknown command or schema-mismatched args is a hard error — the
  daemon refuses to start rather than silently ignoring bindings (no silent failure).

### Frontend wiring

Opt-in `--midi[=PORT]` flag mirrors `--mcp`. When present the frontend (a) builds the
bridge (or reuses the one `--mcp` built — single bridge, multiple producers), (b) ensures
the dispatcher is wrapped with `PublishingDispatcher`, (c) spawns the `adapter-midi`
thread, (d) drains the command channel each tick (the existing drain point — no second
drain). Absent the flag, zero overhead. Both `adapter-gui` and `adapter-console` are
wired, same as #165.

### Hot-reload (optional, spec'd, may slip)

Watch `midi-map.yaml`; on change, re-parse and atomically swap the binding table on the
daemon thread. Invalid reload keeps the previous table and logs the error. Not a v1
blocker.

## Implementation notes (as built)

- **Crate**: `crates/adapter-midi` — `message.rs` (raw bytes → `MidiMessage`, pure),
  `mapping.rs` (serde types + load + validate), `translate.rs` (`resolve`, pure),
  `daemon.rs` (`run_blocking`: `midir` open + listen, the only impure layer). `lib.rs`
  re-exports only. `midir` pinned at `0.11` (its `alsa` range `>=0.9,<0.12` resolves to
  cpal's `alsa-sys`, avoiding the `links = "alsa"` collision that `midir 0.10` causes).
- **Console** reuses the single existing `CommandBridge` (`cmd_bridge.clone()`); the
  existing loop already drains. **GUI**: the MCP wiring builds its bridge *inside* its
  own opt-in block, so `--midi` got a **parallel** bridge + drain `Timer`
  (`midi_adapter_wiring.rs`) rather than refactoring #165's MCP block (sibling-conflict
  risk, out of #22 scope). Two producers → two bridges → two drains, both dispatching on
  the frontend thread sequentially per tick — correct and still zero audio-thread impact.
  Unifying into one bridge when both flags are set is a deferred cleanup.
- **Hot-reload**: deferred (not in this cut).
- **Pre-existing clippy debt** in `block-core/dsp/hilbert_iir.rs` (`excessive_precision`)
  makes the absolute `validate.sh` clippy step fail for any adapter crate; untouched by
  #22 and ignored by the comparative quality-gate (not worsened).

---

## Testing

- Unit: mapping parser — valid maps, every `source` kind, `scale` math, and rejected
  invalid maps (unknown command, missing args, bad scale).
- Unit: MIDI message → `Command` translation against a fake in-memory MIDI source stubbed
  in front of the daemon (no real device, no `midir` port).
- Integration: fake source emits Note/CC/PC → assert the matching `Command` reaches a stub
  dispatcher through a real `CommandBridge`/`BridgeDrain` pair (reuses #165's bridge test
  harness).
- No GUI/Slint in any test (invariant: the screen has no business logic).
- Zero warnings; quality gate green before any push.

## Real-time / invariants

No DSP, routing, I/O or audio-thread code added or modified. MIDI is purely another
`Command` producer on the existing bus; the frontend-side drain is the same bounded,
non-blocking, frontend-thread drain #165 introduced. Invariants 1–10 preserved by
construction; no new `Mutex`/`RwLock`/log/I/O on the processing path.

## Documentation (same commit as code)

- `docs/midi.md` — enabling `--midi`, the `midi-map.yaml` format, a worked **M-Vave
  Chocolate** example (BLE pairing at the OS level + a 4-footswitch map), and the note
  that the adapter attaches to the running instance (the same instance the GUI/MCP use).
- Cross-reference: point Phase 3 of `2026-04-23-command-dispatch-architecture-design.md`
  at this spec.
- README (3 languages) — move #22 from "planned" once shipped.

## Risks

- **`midir` BLE-MIDI coverage varies by OS.** macOS CoreMIDI exposes paired BLE-MIDI
  devices as normal MIDI inputs (Chocolate works). Linux/ALOSA may need the device bridged
  by the OS first; documented, not worked around in code (cross-platform rule — no
  per-OS behavior change).
- **Mapping/`Command` drift.** Mitigated by validating `args` against the same
  `JsonSchema` MCP uses — a renamed `Command` field fails the map load loudly, and a
  schema-lock-style test guards the binding-to-command contract.
- **Bridge contention with MCP.** Both submit to one `CommandBridge`; the drain is already
  capped per tick. Confirm the cap accommodates a fast footswitch burst + agent traffic.

## Open items for the implementation plan

- Exact `--midi`/`--mcp` shared-bridge construction (one bridge when both flags present).
- `midir` port-selection ergonomics (substring match vs. exact vs. index).
- Where the binding-table type lives (in `adapter-midi`; `Command` arg validation reuses
  `application`'s schema — no duplicated domain logic).
- Hot-reload in or out of v1.
