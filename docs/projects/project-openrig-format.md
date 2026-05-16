# `project.openrig` — format reference

Project-level I/O + per-input preset banks (rig architecture, #436). Introduced
by #449 (model + parser only — no engine wiring, migration, UI or scenes yet).

The legacy chain-based project (`project::project::Project`) is **untouched**;
this is an additive model. Migration of legacy `chain.yaml` is #450.

## Document shape

A `project.openrig` file is YAML with a single top-level `project:` key:

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
      active-scene: 1                   # 1..=8 (scene structure itself is #454)
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
```

## Model

| YAML | Rust | Notes |
|---|---|---|
| `project` | `RigProject` | `crates/project/src/rig.rs` |
| `inputs.<name>` | `RigInput` | keyed map; `BTreeMap` ⇒ deterministic order |
| `inputs.<name>.sources[]` | `Vec<InputEntry>` | reused 1:1 from the existing block model — `mode` is **per source**, never flattened to one device/channel (invariant #4 / multi-source of #436) |
| `inputs.<name>.bank` | `BTreeMap<usize, String>` | index → preset name; gaps allowed |
| `inputs.<name>.active-preset` | `usize` | index into `bank`, **not** a name (same preset reused across inputs) |
| `inputs.<name>.active-scene` | `usize` | `1..=8` |
| `outputs.<name>` | `RigOutput` | `label` + flattened `OutputEntry` |
| `presets.<name>` | `RigPreset` | `blocks: Vec<AudioBlock>` — processing only |

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
6. a `routing` target not naming an `outputs` entry.

## Parser API (`infra-yaml`)

| Fn | Purpose |
|---|---|
| `parse_rig_project(&str) -> Result<RigProject>` | parse + validate from a string |
| `serialize_rig_project(&RigProject) -> Result<String>` | deterministic serialize |
| `load_rig_project_file(&Path) -> Result<RigProject>` | read + parse + validate |
| `save_rig_project_file(&Path, &RigProject)` | serialize + write (creates dirs) |

Round-trip (`parse → serialize → parse → serialize`) is byte-deterministic
because every map is a `BTreeMap`.

## Engine runtime (#451)

`engine::rig_runtime` bridges the model to the audio engine without changing
the audio-thread contract:

- `rig_to_chains(&RigProject) -> Vec<Chain>` — each input + its active preset +
  routed outputs is projected onto one synthetic legacy `Chain`
  (`Input(sources)` → preset blocks → `Output(routing)`), distinct `ChainId`
  `rig:<input>` per input.
- `RigRuntime::build(project, sample_rate)` — one **fully isolated** runtime
  per input via the existing `RuntimeGraph::upsert_chain` (invariant #4: no
  shared buffer/lock/route/tap; mixing stays in the backend).
- `RigRuntime::switch_preset(input, idx)` — rebuilds **only that input's**
  chain. Same I/O signature ⇒ the proven in-place lock-free update path: the
  `Arc<ChainRuntimeState>` is preserved, the new pipeline is built off the
  brief swap lock, and the existing per-segment cosine fade-in keeps the
  switch click-free. Other inputs are untouched.

Transport-agnostic (no Slint/cpal in `engine`); the host wires the resulting
`RuntimeGraph` to its backend.

## Spillover (#454-T5) — DONE

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

## Migration from legacy `chain.yaml` (#450)

`project::migrate::migrate_legacy_project(&Project) -> RigProject` is a pure,
deterministic (⇒ idempotent) transform:

| Legacy `Chain` (1-based index N) | `RigProject` |
|---|---|
| chain N | `inputs["input-{N}"]` |
| all `input_blocks` entries, flattened in order | `input.sources` (multi-source preserved) |
| `output_blocks` entries, deduped by `(device, mode, channels)` | `outputs["output-{M}"]` (first-seen) + `input.routing` |
| blocks minus `Input`/`Output`, order preserved | `presets[name].blocks` |
| `chain.volume` | `presets[name].volume` (audio unchanged) |
| `chain.description` slug, else `preset-{N}` (uniquified) | preset name; `bank{1: name}`, `active-preset 1`, `active-scene 1` |

No preset is lost (`presets.len() == chains.len()`, each in a bank slot) and the
result always passes `validate()`.

File orchestrator `infra-yaml::migrate_legacy_project_file(legacy, out)`:

- returns the existing target untouched if it is already a valid `RigProject`
  (idempotent — legacy not re-read, target not clobbered);
- backs the legacy file up to `<legacy>.bak` exactly once before writing;
- validates the migrated project before saving.

## Out of scope here (tracked elsewhere)

- Spillover — old preset/scene tail decaying in parallel (#454-T5; design locked in spec)
- CLI `--project` — #452
- UI project picker + bank/scene navigator — #453
