# MIDI profiles — design spec (issue #548)

**Status:** brainstorm complete, awaiting plan.
**Date:** 2026-05-26.
**Owner:** João Paulo Faria.

## Problem

Today OpenRig has a single MIDI binding map (`midi-bindings.yaml`)
that the user must hand-edit to control the rig with a footswitch /
knob box / expression pedal. There is no notion of "this is a
Chocolate Plus, plug it in and it works": each user reinvents the
mapping. Worse, the schema only supports `note_on` / `cc`; very
common controllers (the M-Vave Chocolate family) send Program Change
out of the box.

## Goal

Ship a per-controller **profile** system so that:

1. The user plugs a known controller and the matching factory profile
   is one click away — no manual YAML editing.
2. Multiple controllers can be active simultaneously (8 Chocolates, an
   FCB1010, an expression pedal), each with its own profile.
3. Users build custom mappings in-app with a **MIDI Learn** flow, not
   by editing YAML by hand.
4. The schema covers every standard MIDI message type the listed
   controllers actually emit.

## Non-goals

- Writing configuration **back** to controllers (e.g. generating
  CubeSuite `.fcp` files). Out of scope for v1. The user configures
  the controller with the vendor app; OpenRig only reads.
- Auto-detecting "what model is plugged in" by port name. Profile
  selection is **explicit** (user activates from a list). The optional
  `source:` filter is a runtime safety net, not a discovery mechanism.

## Concept

### Profile file

A **profile** is a pair shipped in `assets/midi-profiles/`:

- `<name>.yaml` — declarative bindings.
- `<name>.md` — human-readable map (table of `bank/switch → action`).

`<name>` encodes model + mode of the controller, e.g.
`chocolate_plus_program_change_a` (M-Vave Chocolate Plus in CubeSuite
"Program change A" mode). Future modes get sibling files:
`chocolate_plus_advanced_custom.yaml`, etc.

User-created custom profiles live under
`~/.local/share/openrig/midi-profiles/` (macOS path follows the
existing project convention; see `CLAUDE.md` Cross-platform rules).

### YAML format

```yaml
name: "Chocolate Plus — Program Change A (factory)"
source: "FootCtrlPlus"      # optional substring of the MIDI port name
description: |
  M-Vave Chocolate Plus in CubeSuite "Program change A" mode.
  4 switches × 32 banks = 128 PCs, channel 1.
  A = PC 4(n-1), B = 4(n-1)+1, C = 4(n-1)+2, D = 4(n-1)+3.
bindings:
  - when: { kind: ProgramChange, channel: 1, program: 0 }
    do:   prev_preset
  - when: { kind: ProgramChange, channel: 1, program: 1 }
    do:   next_preset
  - when: { kind: ControlChange, channel: 1, controller: 7 }
    do:   chain_volume
```

Rules:

- `kind` uses MIDI 1.0 standard names: `NoteOn`, `NoteOff`,
  `ControlChange`, `ProgramChange` (room to grow to `PitchBend`,
  `Aftertouch`).
- Value field name follows the message type: `note` for Note On/Off,
  `controller` for CC, `program` for Program Change.
- Wildcards: `program: any` / `controller: any` / `note: any` —
  matches any value. The value is then exposed to the slot (e.g. the
  Jump Preset slot uses the program byte as the target preset index).
- `do` is the **slot name** from the 20-action catalog (see below).
  Slot names are snake_case, validated against the catalog at parse
  time.
- No placeholders, no embedded code. All "active chain / active block
  / scale CC to range" logic lives in **slot code**, not YAML.

### Slot catalog (V1 = 20 slots, frozen)

| # | Slot name | Group | Acts on | Backing Command(s) |
|---|---|---|---|---|
| 1 | `toggle_tuner` | App | global | `SetTunerEnabled` (toggle) |
| 2 | `toggle_output_mute` | App | global | `SetOutputMuted` (toggle) |
| 3 | `toggle_spectrum` | App | global | `SetSpectrumEnabled` (toggle) |
| 4 | `prev_chain` | Chain nav | active chain ← | new Command (see Phase 0) |
| 5 | `next_chain` | Chain nav | active chain → | new Command |
| 6 | `toggle_active_chain_enabled` | Chain | active chain | `ToggleChainEnabled { chain: active }` |
| 7 | `toggle_compact_view` | Chain | active chain | new Command (#436 audit) |
| 8 | `prev_preset` | Rig nav | active chain | `ApplyRigNav { chain: active, kind: StepPreset(-1) }` |
| 9 | `next_preset` | Rig nav | active chain | `ApplyRigNav { chain: active, kind: StepPreset(+1) }` |
| 10 | `prev_scene` | Rig nav | active chain | `ApplyRigNav { chain: active, kind: StepScene(-1) }` |
| 11 | `next_scene` | Rig nav | active chain | `ApplyRigNav { chain: active, kind: StepScene(+1) }` |
| 12 | `jump_preset_n` | Rig nav | active chain | `ApplyRigNav { chain: active, kind: Preset(value) }` (value from PC / CC) |
| 13 | `jump_scene_n` | Rig nav | active chain | `ApplyRigNav { chain: active, kind: Scene(value) }` |
| 14 | `prev_block_1` | Block nav | active block | new Command |
| 15 | `next_block_1` | Block nav | active block | new Command |
| 16 | `prev_block_2` | Block nav | active block | new Command |
| 17 | `next_block_2` | Block nav | active block | new Command |
| 18 | `toggle_active_block_enabled` | Block | active block | `ToggleBlockEnabled { chain: active, block: active }` |
| 19 | `chain_volume` | Continuous | active chain | `SetChainVolume { chain: active, value: cc_scaled }` |
| 20 | `block_param_numeric` | Continuous | active block | `SetBlockParameterNumber { chain: active, block: active, path: <first numeric>, value: cc_scaled }` |

"Acts on active X" means the slot reads `SelectionState` from the
bridge to determine which chain/block to target. Slots do not take
explicit ids in their YAML form — that is the whole point of the
catalog.

### Active state

A new struct lives in the bridge:

```rust
pub struct SelectionState {
    pub active_chain: Option<ChainId>,
    pub active_block: Option<BlockId>, // belongs to active_chain
}
```

The GUI writes to it whenever the user selects a chain or a block on
the Chains screen (or compact view, or block editor). MIDI slots read
from it. A new `QueryKind::Selection` exposes the same state to
MCP/gRPC (paridade query, per `openrig-code-quality` LAW).

### Pipeline (MIDI in → action)

1. A MIDI message arrives at any open port:
   `{ port_name, kind, channel, data1, data2 }`.
2. For each **active profile**:
   - If the profile has a `source:`, skip unless `port_name` contains
     the substring.
   - For each binding, match `kind`, `channel`, and the per-kind value
     (or wildcard `any`).
   - On match: look up the named slot (compile-time registry of 20
     functions). The slot:
     - Reads `SelectionState` for active chain/block (if it needs
       them).
     - Reads the raw value (`cc.value`, `pc.program`, `note.velocity`)
       if needed (for jumps, for CC scaling).
     - Scales CC to the parameter's `min..max` if it's a continuous
       slot.
     - Builds a fully-typed `Command` and calls `dispatcher.dispatch`.
3. A message that matches bindings in 2 active profiles dispatches
   both actions (no exclusivity enforcement; rare in practice).

### Multiple active profiles

The user activates profiles from the Settings/MIDI screen. The set of
active profiles is stored on the **project** (so a session on stage
loads with the right controllers wired up), not on the system —
following ADR 0003 ("if the project moves to another machine, does
this value go with it?" → yes).

### Custom profiles

`[Customize]` next to a factory profile:

1. Clones the factory YAML to
   `~/.local/share/openrig/midi-profiles/<name>-custom.yaml`.
2. Opens the editor (Settings/MIDI/Editor panel).
3. The editor shows the bindings as a table: `When → Do`. Each row has
   `[Edit] [Delete] [Learn]`.
4. `[Learn]`: the next incoming MIDI message fills the `When` fields
   (kind, channel, data1) for that row.
5. `Do` is a dropdown of the 20 slot names (with i18n labels).
6. `Save` validates against the schema and writes the YAML.

### Skill for profile authoring

A new skill `openrig-midi-profile-builder` in the `openrig` plugin
guides the agent through:

- Asking the model, source name, and CubeSuite (or vendor app) mode.
- Optionally instructing the user to capture messages via MIDI
  Monitor / `receivemidi` and pasting the log; the skill parses it and
  proposes a `bindings:` block.
- Mapping each captured `(kind, channel, value)` to one of the 20
  catalog slots through 1 question per row.
- Generating `<name>.yaml` + `<name>.md` in `assets/midi-profiles/`,
  validating against the schema.

## Phases (each phase = 1 PR, red-first)

| # | Deliverable | Touches | Blocked by |
|---|---|---|---|
| 0 | Audit 20 slots × `Command` enum. List which Commands are missing (active nav, toggle Compact View, step block). Output is a checklist in this spec / issue. | docs only | — |
| 1 | `SelectionState` in `application::bridge` + GUI sync + `QueryKind::Selection`. | `application`, `adapter-gui`, `adapter-mcp` | — |
| 2 | `MidiProfile` schema + YAML parser + validation against the slot catalog. New `kind: ProgramChange` support in source. | `adapter-midi` | — |
| 3 | Implement the 20 slots (red-first), in 5 groups: 3a App (1-3), 3b Chain nav (4-7), 3c Rig nav (8-13), 3d Block (14-18), 3e Continuous (19-20). | `adapter-midi`, possibly `application` (new Commands per Phase 0) | 0, 1 |
| 4 | Match pipeline: multiple active profiles, `source:` substring filter, `any` wildcard. | `adapter-midi` | 2 |
| 5 | First shipped factory asset: `chocolate_plus_program_change_a.yaml` + `.md`. | `assets/midi-profiles/` | 2, 3 |
| 6 | UI Settings/MIDI: list factory + user profiles, Activate / Deactivate, View Map (.md). No editor yet. | `adapter-gui` (Settings screen), i18n catalogs (9 locales) | 4, 5 |
| 7 | Custom profile editor + MIDI Learn per row. | `adapter-gui`, `adapter-midi` (learn pipeline already partial in `learn.rs`), i18n | 6 |
| 8 | Skill `openrig-midi-profile-builder` in the openrig plugin. | external plugin repo | 2 (schema) |
| 9 | Docs sync: `docs/midi.md` rewrite, `docs/midi-profiles.md` new, `docs/midi-chocolate.md` update, READMEs in 3 languages if tagline changes. | `docs/`, READMEs | all |

## Invariants — what cannot regress

- Audio thread isolation: MIDI dispatch runs on the GUI/control
  thread, never on the audio thread. No new Mutex/lock/syscall on the
  audio path.
- Latency round-trip: unchanged. MIDI message → command → effect is
  not on the audio hot path.
- Stream stereo invariants (CLAUDE.md §5): untouched.
- Volume invariants pinned in `volume_invariants_tests.rs`: untouched.
  `chain_volume` slot dispatches the existing `SetChainVolume`
  command; it does not bypass the volume rules.
- Cross-platform: factory profiles ship bundled (no install path
  hardcoded). User-profile directory uses platform-appropriate paths
  via the existing config layer.

## Test strategy

- Phase 1: `SelectionState` round-trip tests (set chain, read it; set
  block, read it; clear chain clears block; GUI selection event ↔
  bridge state).
- Phase 2: YAML parsing tests — every kind, every wildcard,
  unknown-slot rejection, schema-incompatible value rejection. Golden
  factory profile loads cleanly.
- Phase 3: per-slot unit test red-first. Each slot has at least one
  test that builds the expected `Command` from a sample MIDI message
  and a fixed `SelectionState`.
- Phase 4: pipeline integration test — two active profiles, four
  ports, messages routed by `source:`. Wildcard binding gets value
  from the message.
- Phase 5: smoke test loading the shipped Chocolate Plus profile and
  exercising one binding from each kind (PC, CC if present).
- Phase 6: i18n test (all new keys translated in en/pt/es minimum).
  Slint visual no-go test (existing pattern from #513).
- Phase 7: MIDI Learn unit test (mock MIDI input → expected `When`
  fields filled).

`#[ignore]` is forbidden per `openrig-code-quality` 10b. Fixtures live
under `crates/<x>/tests/fixtures/`.

## Open questions (resolve as phases land)

1. **`block_param_numeric` target**: V1 maps to the first numeric
   parameter of the active block's schema. UI may later add "pick
   which param" — out of v1 scope.
2. **Project-level vs system-level `active_profiles`**: per ADR 0003,
   "if the project moves to another machine, does this go with it?".
   Active profiles are project-level (yes). Profile **files** live
   system-side (factory bundled, user under
   `~/.local/share/openrig/midi-profiles/`).
3. **Migration** of the existing single `midi-bindings.yaml` resolved
   view (project → system fallback → shipped default): kept as the
   legacy path until Phase 6 lands the new UI; at that point the old
   path becomes "the default factory profile" auto-activated when no
   user activation exists. Detailed migration plan goes in the Phase 4
   PR description.

## Related work

- #181 — MIDI infrastructure (umbrella for input/routing/chain
  integration). This issue is a child.
- #326 — Expression pedal mapping (CC). Covered by slots 19/20 and
  custom profile.
- #436 — GUI→Command parity. Phase 0 audit cross-references this
  issue's checklist.
- ADR 0003 — system vs project config taxonomy.
- `docs/midi.md`, `docs/midi-chocolate.md`,
  `docs/midi-command-coverage.md` — current state; will be updated in
  Phase 9.
