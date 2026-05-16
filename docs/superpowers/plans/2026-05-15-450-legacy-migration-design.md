# #450 — legacy `chain.yaml` → `project.openrig` migration — Plan

> Sub-issue de #436. Depende de #449 (modelo/parser, já na branch).

**Goal:** Pure, idempotent, lossless transform from the legacy chain-based
`Project` into a `RigProject`, plus a file orchestrator that backs up before
writing. No audio change (golden/volume invariants must pass).

**Architecture:**
- Pure transform → `crates/project/src/migrate.rs`:
  `migrate_legacy_project(&Project) -> RigProject`. Deterministic ⇒ idempotent.
- File orchestrator → `crates/infra-yaml`: load legacy, `.bak` backup
  (skip if present ⇒ idempotent), write `project.openrig`; skip if target
  already a valid `RigProject`.
- `RigPreset` gains `volume: f32` (default 100.0) so `Chain.volume` is carried
  — losing it would change master output gain (CLAUDE.md invariant #10/#2).

## Mapping (closed in #436)

| Legacy `Chain` (index N, 1-based) | `RigProject` |
|---|---|
| chain N | `inputs["input-{N}"]` |
| chain `input_blocks().entries` (flattened, in order) | `input.sources: Vec<InputEntry>` |
| chain `output_blocks().entries` | `outputs` (deduped by `(device,mode,channels)`, named `output-{M}` first-seen) + `input.routing` |
| chain blocks minus `Input`/`Output` (order preserved) | `presets[name].blocks` |
| `chain.volume` | `presets[name].volume` |
| `chain.description` slug, else `preset-{N}` (uniquified) | preset name; `bank{1: name}`, `active-preset 1`, `active-scene 1` |
| `project.name` | `RigProject.name` |

Invariants: no preset lost (`presets.len() == chains.len()`, every preset in a
bank slot); processing blocks bit-identical & in order ⇒ audio identical;
result always passes `RigProject::validate()`.

## Tasks

- [ ] T1 — TDD: add `RigPreset.volume`; `migrate_legacy_project` (RED→GREEN):
  per-chain input+preset+bank; no loss; I/O stripped; order/volume preserved;
  outputs deduped + routing; `validate()` ok; idempotent (`f(x)==f(x)`,
  `f(f-roundtrip)` stable); empty project → empty rig.
- [ ] T2 — infra-yaml file orchestrator + tests: backup `.bak` (idempotent),
  write target, skip when target already valid.
- [ ] T3 — docs (`project-openrig-format.md` migration section) + `./scripts/qa.sh`
  green + push + comment #450.
