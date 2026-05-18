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

```yaml
# Selects the input device by substring match; first match wins. Omit to use the
# system default input.
input: "Chocolate"

bindings:
  # Footswitch A → toggle bypass on chain 0 / block 2
  - source: { kind: midi, channel: 1, note: 60, action: note_on }
    command: ToggleBlockEnabled
    args: { chain: 0, block: 2 }

  # Footswitch B → load a preset
  - source: { kind: midi, program_change: 5 }
    command: LoadProject
    args: { path: presets/clean.yaml }

  # Expression pedal CC 7 → sweep a parameter, 0..127 mapped linearly to 0.0..100.0
  - source: { kind: midi, channel: 1, cc: 7 }
    command: SetBlockParameterNumber
    args:
      chain: 0
      block: 1
      path: gain
      scale: { type: linear, min: 0.0, max: 100.0 }
```

- `command` is the `Command` variant name; `args` is its `schemars`-derived shape. The
  same `JsonSchema` that powers MCP tools **validates the mapping at load time** — one
  source of truth for "what a command needs", no hand-written validation in the adapter.
- `scale` applies only to continuous sources (`cc`, pitch bend). `{ type: linear }` for
  v1; `log` is a follow-up.
- Parse errors are reported once at load with file/line context; the daemon refuses to
  start on an invalid map rather than silently ignoring bindings (no silent failure).

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
