# `project.yaml` — format reference

Project-level I/O + per-input preset banks: model, parser, engine runtime,
format versioning, scenes and spillover. This is the only project file format:
there is no other extension and no chain-based project document.

## Document shape

A `project.yaml` file is YAML with a single top-level `project:` key:

```yaml
project:
  name: Studio                          # optional

  inputs:
    input-1:
      label: "Eu + filho (mesmo som)"   # optional
      sources:                          # = Vec<InputEntry> — NOT flattened
        - device_id: scarlett
          mode: mono                    # mono | stereo | dual_mono (per source)
          channels: [0]
        - device_id: scarlett
          mode: mono
          channels: [1]
      bank:                             # index -> preset name (gaps allowed)
        1: clean
        2: drive
      active-preset: 2                  # an index present in `bank`
      active-scene: 1                   # 1..=8
      routing: [out-1]                  # names of `outputs` entries

  outputs:
    out-1:
      label: "PA L"                     # optional
      device_id: scarlett
      mode: stereo                      # mono | stereo
      channels: [0, 1]

  presets:                              # shared pool, processing-only, no I/O
    clean:
      blocks: []
    drive:
      blocks: []

  midi:                                  # optional, ADR 0003
    bindings:                            # what each controller event does in THIS rig
      - source: { kind: note_on, channel: 1, note: 60 }
        command: ApplyRigNav
        args: { chain: "rig:guitar", kind: { StepPreset: -1 } }
```

## Model

| YAML | Rust | Notes |
|---|---|---|
| `project` | `RigProject` | `crates/project/src/rig.rs` |
| `inputs.<name>` | `RigInput` | keyed map; `BTreeMap` ⇒ deterministic order |
| `inputs.<name>.sources[]` | `Vec<InputEntry>` | reused 1:1 from the existing block model — `mode` is **per source**, never flattened to one device/channel (invariant #4, multi-source) |
| `inputs.<name>.bank` | `BTreeMap<usize, String>` | index → preset name; gaps allowed |
| `inputs.<name>.active-preset` | `usize` | index into `bank`, **not** a name (same preset reused across inputs) |
| `inputs.<name>.active-scene` | `usize` | `1..=8` |
| `inputs.<name>.disabled_endpoints` | `EndpointDisables` | #328 graph checklists: `{ inputs, outputs, path_a_outputs, path_b_outputs }`, each a list of `{ io, endpoint }` (binding id + endpoint name) left out of that node. Absent = every endpoint checked; no version bump. |
| `outputs.<name>` | `RigOutput` | `label` + flattened `OutputEntry` |
| `presets.<name>` | `RigPreset` | `blocks: Vec<AudioBlock>` — processing only |
| `presets.<name>.blocks[].kind: !Split` | `SplitBlock` | Chain split: `{ end: mix \| y, params, paths: [[blocks], …] }`, at least two paths. Path blocks are full `AudioBlock`s with their own ids. |
| `midi.bindings[]` | `RigProjectMidi.bindings` | optional, ADR 0003. `Source`/`Scale`/`Binding` data types live in `crates/project/src/midi.rs`. When present, replaces the system fallback (`midi-bindings.yaml`) at resolve time; the controller (`input:`) always comes from the system `midi-profile.yaml`. Absent → resolver falls back to system file → shipped default. |

### Edit capture: scene diff vs preset base

`write_back_processing_blocks` (run by `Command::CaptureRigEdits` and the
save path) captures edits made on the projected chain back into the active
preset. **Float** param edits become the active scene's f32 override
(Helix Snapshot rule — the key is auto-marked in `scene-params`).
**Non-float** params (Bool/Int/String — e.g. a NAM noise-gate toggle)
cannot live in the f32 scene diff: they are written into the preset base
`blocks`, shared by every scene, so they survive save+reload.

### Model swap keeps the scenes

Changing a block's model keeps its id and position, so it is **not** a
structural edit. `write_back_model_swaps` (`crates/project/src/rig_model_swap.rs`)
runs first in the capture: the swapped base block takes the new model and
keeps its own `enabled`, and only the scene overrides / `scene-params`
entries of that block whose parameter the new model does not have are
dropped. Every other scene, bypass, override, the other blocks' base values
and `active-scene` survive. `ReplaceBlockModel` mirrors the swap into the rig
right away and re-resolves the live block through the active scene; the
block editor's `OverwriteBlock` path gets the same treatment on the next
capture (scene switch or save).

### Chain split

A preset may hold any number of `Split` blocks (`kind: !Split`), and a split may sit inside a path at any depth. Blocks before a split are shared by every path; for `end: mix` the blocks after it are shared again after the mixer. `paths` lists the paths in order (A, B, C, …), at least two. The split and mixer knobs live in `params`, keyed by the 0-based path index (see `docs/blocks-catalog.md` → Chain split, which also lists the rules a split must follow).

A Mix, then a Y: two amps summed, then one output with a cab and one without (abridged: each block's `params` are left out):

```yaml
version: 2                                # a Split in the file writes version 2
project:
  presets:
    dual-amp:
      id: dual-amp
      name: DUAL AMP
      blocks:
      - id: rig:guitar:block:mix
        enabled: true
        kind: !Split
          end: mix
          params: { values: { mix_pan_0: -50.0, mix_pan_1: 50.0 } }  # plus the other split knobs
          paths:
          - - id: rig:guitar:block:amp-1
              kind: !Core { effect_type: amp, model: blackface_clean }
          - - id: rig:guitar:block:amp-2
              kind: !Core { effect_type: preamp, model: american_clean }
      - id: rig:guitar:block:y
        enabled: true
        kind: !Split
          end: y
          params: { values: { level_to_0: 100.0, level_to_1: 100.0 } }  # plus the other split knobs
          paths:
          - - id: rig:guitar:block:cab        # path A → its outputs (FRFR): with a cab
              kind: !Core { effect_type: cab, model: american_2x12 }
          - []                                # path B → its outputs (a real cab): no cab
```

Each Y leaf feeds every output endpoint of the chain's E/S that the input's `disabled_endpoints.path_outputs` does not leave out: one entry per leaf with something unchecked, `{ split: <split id>, path: <index>, disabled: [...] }`. Files that still carry `path_a_outputs` / `path_b_outputs` load them onto the chain's Y.

Chain preset files write the split as `type: split` with `end`, `params` and `paths`. Path blocks carry no id on disk and load as `<split id>::p<path>:<i>`. Files that hold `a` and `b` instead of `paths` load them as paths 0 and 1, and their `_a` / `_b` knob keys as `_0` / `_1`. A path block this machine cannot load is dropped with a warning and the rest of the split is kept.

Scenes, edit capture and model swaps reach the blocks inside the paths exactly like top-level blocks: a path block's scene keys are `<its id>.<param>`, the split's own knobs are `<split id>.<param>` (float knobs per scene, `split_mode` / `mix_polarity_<i>` / `mix_master_sum` preset-wide). Swapping a path block's model keeps every scene (the rule below applies inside paths). Adding, removing or moving a block inside a path is a structural edit and follows the rule below like a top-level one: the split keeps its own base knobs, every block the preset already had keeps its base, and every scene survives except the entries of blocks that are gone.

### Structural edits keep the scenes; an insert belongs to its preset

Adding, removing or reordering blocks (the insert included) is structural:
`replace_preset_blocks_if_structural` (`crates/project/src/rig_write_back.rs`)
rewrites the preset's block list to follow the chain, but a block the preset
already had (same id, same model) keeps its base, not the scene-applied live
copy. Every scene and `scene-params` entry survives except those of blocks
that are gone; the per-scene diff then runs as usual for the active scene.
Before this, any structural edit replaced the base with the live chain and
cleared every scene.

An `Insert` is stored in the preset's own `blocks`, so a preset switch takes
it from the preset being loaded (position and scene-applied `enabled`), never
from the chain that was live. Only the chain's `Input`/`Output` ports carry
over a switch (`merge_preserved_ports`, `crates/application/src/local_dispatcher_rig.rs`).
Before this, one preset's insert was carried into every preset of the bank,
and removing it from one preset removed it from all of them.

## Validation

`RigProject::validate() -> Result<(), String>` is run by `parse_rig_project`
and rejects:

1. a `bank` slot naming a preset absent from `presets`;
2. `active-preset` not present as a key in that input's `bank`;
3. `active-scene` outside `1..=8`;
4. a preset containing an `Input`/`Output` block (presets are processing-only);
5. per-input source channel conflicts — delegates to
   `InputBlock::validate_channel_conflicts` (same `(device, channel)` used by
   two sources of the same input);
6. a `routing` target not naming an `outputs` entry;
7. a preset breaking the split rules of #328: two Mix splits, two Y splits, a Y before a Mix, a split/select/input/output/insert inside the path of any split, or anything but a port after a Y split.

Cross-input capture exclusivity is **not** validated statically: a project
may freely hold many inputs sharing a `(device, channel)` tap (a library of
alternative configs). The rule that two inputs sharing a tap cannot be
**active at the same time** (isolation invariant #4) is enforced by the
engine at runtime, not by `validate()`.

## Parser API (`infra-yaml`)

| Fn | Purpose |
|---|---|
| `parse_project(&str) -> Result<RigProject>` | parse + version-check + validate |
| `serialize_project(&RigProject) -> Result<String>` | deterministic serialize (stamps `version`) |
| `load_project_file(&Path) -> Result<RigProject>` | read + parse + validate |
| `save_project_file(&Path, &RigProject)` | serialize + write (creates dirs) |
| `load_legacy_preset_as_rig(&Path) -> Result<(String, RigPreset)>` | convert a standalone legacy preset file into a `RigPreset` |

Round-trip (`parse → serialize → parse → serialize`) is byte-deterministic
because every map is a `BTreeMap`.

## Engine runtime

`engine::rig_runtime` bridges the model to the audio engine without changing
the audio-thread contract:

- `rig_to_chains(&RigProject) -> Vec<Chain>` — each input + its active preset +
  routed outputs is projected onto one synthetic legacy `Chain`
  (`Input(sources)` → preset blocks → `Output(routing)`), distinct `ChainId`
  `rig:<input>` per input.
- `RigRuntime::build(project, sample_rate)` — brings up one **fully isolated**
  runtime per input via the existing `RuntimeGraph::upsert_chain` (invariant
  #4: no shared buffer/lock/route/tap; mixing stays in the backend),
  **skipping** any input whose `(device, channel)` tap is already held by an
  earlier-enabled input (deterministic by input name).
- Enabled state is **in-memory only**, never persisted to the file:
  - `RigRuntime::enable_input(name)` — activates an input at runtime; errors
    if any of its taps is already used by an active input (free it first).
  - `RigRuntime::disable_input(name)` — tears down that input's runtime and
    frees its taps for another input.
  - `RigRuntime::is_enabled(name)` — current activation state.
  A project may freely *define* many tap-sharing inputs (a library of
  configs); only the *active set* must be tap-disjoint, enforced here — not
  by `validate()`. `switch_preset`/`switch_scene` require the input active.
- `RigRuntime::switch_preset(input, idx)` — rebuilds **only that input's**
  chain. Same I/O signature ⇒ the proven in-place lock-free update path: the
  `Arc<ChainRuntimeState>` is preserved, the new pipeline is built off the
  brief swap lock, and the existing per-segment cosine fade-in keeps the
  switch click-free. Other inputs are untouched. Switching presets also
  resets `active_scene` to `1` — scenes are per-preset, so carrying the
  previous preset's scene index over would leak a phantom scene into the
  new preset on the next `write_back_processing_blocks` call.

Transport-agnostic (no Slint/cpal in `engine`); the host wires the resulting
`RuntimeGraph` to its backend.

## Spillover

A preset/scene switch retains the **previous** pipeline as a decaying
`OutgoingTail` so its delay/reverb tail rings out in parallel while the new
pipeline fades in. SPSC-safe: the old pipeline is fed silence and summed into
the segment's own `frame_buffer` *before* the single per-route push (one
producer per ring preserved); built off the audio thread; equal-power
fade over `SPILLOVER_FRAMES` then dropped. Reached via
`ProjectRuntimeController::upsert_chain_spillover` →
`RuntimeGraph::upsert_chain_spillover` →
`update_chain_runtime_state_spillover`; the bank/scene navigator uses it on
every switch. `None` ⇒ behaviour byte-identical to the in-place path.

Gated by `rig_spillover` golden (retains-then-drops + non-spillover
byte-identical) plus `volume_invariants`/`stream_isolation`/
`audio_signal_integrity` all green.

## In-memory chain model → rig conversion

`project::migrate::migrate_legacy_project(&Project) -> RigProject` is a pure,
deterministic (⇒ idempotent) transform:

Chains are **grouped by capture source**. The source key is the list of
`(device, mode, channels)` of a chain's input entries, **mono-normalized**
(a `mono` entry only taps one physical channel, so `mono [0,1]` ≡
`mono [0]`). Every chain on the same source becomes a preset in **one
input's bank** — one guitar with many songs ⇒ one input + N presets.

| Legacy `Chain`s | `RigProject` |
|---|---|
| chains with the same source key | one `inputs["input-{M}"]` (first-seen order) |
| each such chain, in chain order | a bank slot `1..N`; `active-preset 1`, `active-scene 1` |
| normalized input entries of the group's first chain | `input.sources` (multi-source preserved) |
| `output_blocks` deduped by `(device, mode, channels)` | `outputs["output-{K}"]` (first-seen); each input's `routing` = union of its chains' outputs |
| blocks minus `Input`/`Output`, order preserved | `presets[name].blocks` |
| `chain.volume` | `presets[name].volume` (audio unchanged, invariant #10) |
| `chain.description` slug, else `preset-{N}` (uniquified) | preset name (shared pool) |

No preset is lost (`presets.len() == chains.len()`, each in a bank slot) and the
result always passes `validate()`. Deterministic ⇒ idempotent.

## Format versioning + backward-compat

Both `project.yaml` and standalone preset files carry an explicit
top-level `version:`. A document is written with the lowest version that can
hold it: `project::rig::{PROJECT_FORMAT_VERSION, PRESET_FORMAT_VERSION}` (`1`),
or `project::format_version::SPLIT_FORMAT_VERSION` (`2`) when a preset holds a
`Split` (#328). This build reads up to `MAX_READABLE_FORMAT_VERSION` (`2`); an
older build refuses a version 2 file with its "newer than this build" error
instead of failing inside serde:

```yaml
version: 1
project: { ... }
```

- **Missing `version`** ⇒ a pre-version file; its shape *is* v1, so it loads
  unchanged (older files keep working).
- **`version > CURRENT`** ⇒ refused with a clear "newer than this build"
  error instead of silently dropping unknown fields (an old binary will not
  corrupt a newer project).
- **`version < CURRENT`** ⇒ staged in-memory upgrade (no upgrades exist for
  v1 yet; the hook is in `parse_project`).

Legacy standalone presets convert via `load_legacy_preset_as_rig` (blocks +
volume preserved bit-identical ⇒ audio unchanged; no scenes/scene-params ⇒
behaves as one Default scene).
