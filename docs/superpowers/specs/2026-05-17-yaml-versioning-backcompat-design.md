# YAML versioning + backward-compat (project.openrig & preset) — #450

**Goal:** Make project and preset YAML forward/backward-compatible: an explicit
`version` field, transparent legacy→rig migration on load, and a conversion
path from standalone legacy presets into `RigPreset` (the missing "+ presets"
half of #450).

## Root cause

Today neither `project.openrig` nor preset YAML carries a version. Legacy vs
new is guessed structurally; legacy projects never migrate on open; standalone
legacy presets (`ChainBlocksPreset`) have **no** path into `RigPreset`. Every
future schema change is fragile.

## Decisions (locked with user)

1. Explicit `version: u32` at document top-level for both formats.
2. Transparent migration on load (legacy `.yaml` → writes sibling
   `project.openrig` + `.bak`, runs as rig; idempotent — reuses existing
   `migrate_legacy_project_file`).

## Single source of truth

`crates/project/src/rig.rs`:
- `pub const PROJECT_FORMAT_VERSION: u32 = 1;`
- `pub const PRESET_FORMAT_VERSION: u32 = 1;`

## Format

```yaml
version: 1
project:
  name: ...
```

- Missing `version` ⇒ defaults to `1` (already-written pre-version files keep
  loading — current shape *is* v1).
- `version > CURRENT` ⇒ hard error (old binary refuses a newer file cleanly
  instead of silently dropping fields). Clear message: upgrade OpenRig.
- `version < CURRENT` ⇒ staged in-memory upgrade (none yet for v1; framework
  in place).

## Conversion (pure, project crate)

`RigPreset::from_legacy_blocks(blocks: Vec<AudioBlock>, volume: f32) -> RigPreset`
— blocks preserved bit-identical & in order, volume preserved exact,
`scene_params = []`, `scenes = {}`. Audio identical to the legacy preset.

## infra-yaml

- `RigProjectFile { version (default), project }`; serialize writes
  `version: CURRENT`.
- `PresetYaml.version` (default); save writes it.
- `load_legacy_preset_as_rig(path) -> (String /*name*/, RigPreset)`.
- `load_project_any(path) -> RigProject`: try new format (version-checked);
  else treat as legacy chain YAML and migrate transparently to a sibling
  `project.openrig` (+ `.bak`, idempotent, validated).

## Audio safety

No audio-thread/engine change. Blocks and `f32` volume preserved exact ⇒
`volume_invariants`, `stream_isolation`, `rig_spillover` unaffected (run as
guard).

## Tasks (TDD, RED→GREEN)

1. `RigPreset::from_legacy_blocks` + constants (project) — pure conversion test.
2. project.openrig `version`: round-trips, missing⇒1, `>CURRENT`⇒Err,
   existing golden round-trips unchanged.
3. preset `version`: missing⇒1, `>CURRENT`⇒Err, save writes it.
4. `load_legacy_preset_as_rig`: legacy preset file → RigPreset, blocks+volume
   preserved, scenes empty, name from name|id.
5. `load_project_any`: new loads; legacy transparently migrates (sibling
   `.openrig` + `.bak`); idempotent; doesn't clobber valid target;
   `version > CURRENT` rejected.

Docs updated same commit: `docs/project-openrig-format.md` (#449) +
`docs/...migration` (#450).
