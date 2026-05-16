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

> **Spillover (old preset's delay/reverb tail decaying across a switch)** is
> intentionally consolidated in **#454**, where it is the headline deliverable
> and uses the *same* swap mechanism — keeping the audio-thread-sensitive
> change in one golden/volume-gated place. #451's switch is click-free but
> does not yet preserve the previous preset's tail.

## Out of scope here (tracked elsewhere)

- Scenes + spillover (old-tail crossfade; `scenes`/`scene-params`) — #454 (same swap mechanism)
- Legacy `chain.yaml` migration — #450
- CLI `--project` — #452
- UI project picker + bank/scene navigator — #453
