# MCP read parity for block/plugin parameters (#572)

**Status:** draft (rewritten 2026-05-27 after recon — the original
two-phase plan was based on a false premise; see "What this spec used
to say" at the bottom).
**Issue:** [#572](https://github.com/jpfaria/OpenRig/issues/572)
**Date:** 2026-05-27
**Author:** brainstorm with @jpfaria

## Problem

MCP (and any future transport: gRPC, remote) can already **set** block
parameters via `Command::SetBlockParameter{Number,Bool,Text,File}` and
`Command::SelectBlockParameterOption` — these are auto-derived into MCP
tools (`set_block_parameter_number`, `select_block_parameter_option`,
etc.) by `crates/application/src/command_schema.rs`. But there is no way
for any transport to **discover**:

- Which parameters a given plugin (catalog model) exposes.
- Which parameters a given placed block instance currently has.
- The schema of each parameter (kind, range, options, default, unit,
  widget, group).
- The current value of each parameter on a placed block.

Without that, an MCP / gRPC client can only drive parameter mutations it
already knows about by name and type — there is no introspection. The
preset / tone-builder workflow needs to know which knobs are reachable
before deciding which `set_block_parameter_*` to call.

## What already exists (the part the first draft of this spec missed)

The descriptor infrastructure is **already complete** in
`crates/block-core/src/param/`:

- `schema::ModelParameterSchema { effect_type, model, display_name,
  audio_mode, parameters: Vec<ParameterSpec> }` — catalog-level schema
  per block model.
- `schema::ParameterSpec { path, label, group, widget, unit, domain,
  default_value, optional, allow_empty }` — per-param schema with all
  metadata.
- `schema::ParameterDomain { Bool | IntRange | FloatRange | Enum | Text
  | FilePath }` — the value space.
- `schema::ParameterWidget { Knob | Toggle | Select | FilePicker |
  TextInput | MultiSlider | CurveEditor }` — UI rendering hint.
- `schema::ParameterUnit { None | Decibels | Hertz | Milliseconds |
  Percent | Ratio | Semitones }` — display unit.
- `descriptor::BlockParameterDescriptor` — per-block-instance descriptor
  carrying schema + `current_value`.
- `ParameterSpec::materialize(block_id, effect_type, model, audio_mode,
  current_value) -> BlockParameterDescriptor` — the exact bridge from
  "catalog schema" to "live block param".

All of these already derive `Serialize, Deserialize` — JSON is free.

The GUI is already a pure consumer:
`crates/adapter-gui/src/block_editor_param_items.rs` does
`use project::param::{ParameterDomain, ParameterSpec, ParameterWidget};
spec.domain → BlockParameterItem`. Zero schema literals live in
`adapter-gui`.

The bridge from "model id string" to `ModelParameterSchema` is
`project::block::schema_for_block_model(...)` (consumed by the editor
today). The bridge from "(chain, block) placed instance" to a list of
materialized `BlockParameterDescriptor` lives in the project crate
(exact path to be confirmed in the implementation phase via grep on
`materialize`).

**Conclusion:** the architectural concern is already satisfied. Schema
metadata is centralised in `block-core::param`, the GUI consumes it.
The only work this issue needs is the MCP transport binding.

## Goals

1. Add `QueryKind::GetPluginParams { plugin_id }` — returns the model's
   `ModelParameterSchema` as JSON.
2. Add `QueryKind::GetBlockParams { chain, block }` — returns the list
   of materialized `BlockParameterDescriptor` for the placed block, as
   JSON (schema + `current_value`).
3. Wire two MCP resources (`openrig://plugins/{id}/params`,
   `openrig://chains/{cid}/blocks/{bid}/params`). **No MCP tools.**
   `adapter-mcp::tools::tools()` auto-derives from `Command` variants
   exclusively (`parity_guard_every_command_variant_is_a_tool` invariant)
   — there is no read-side equivalent today, and there should not be
   without an analogous `query_schema` system. MCP read access for
   parameters lives entirely on the resource path.
4. Adapter-console parity test arm for both new variants (same pattern
   used for `ListChainPresets` / `ListProjectPresets`).
5. Zero regression to existing setter Commands, the GUI's block editor,
   or any audio-thread invariant (`CLAUDE.md` invariantes 1–10).

## Non-goals

- gRPC adapter wiring (the new `QueryKind` variants feed it automatically
  whenever the gRPC adapter exists; nothing transport-specific in this
  issue).
- New parameter kinds, widgets, units, or domains beyond what already
  exists in `block-core::param::schema`.
- Refactoring `ModelParameterSchema` / `ParameterSpec` /
  `BlockParameterDescriptor` field names or shapes. Whatever they
  serialize to today is what we expose. If a field is internal-only,
  we may project a thinner view — but no rename / restructure here.
- Changing the mutation path or the on-disk shape of `Project`.
- Exposing setter parity (already exists via the 5
  `set_block_parameter_*` Commands).
- A persistent disk-side plugin parameter schema cache.

## Design

### New `QueryKind` variants

Add to `crates/application/src/bridge.rs::QueryKind`:

```rust
/// #572: full parameter schema for one plugin (catalog-level).
/// No placed instance required. Resolved via
/// `project::block::schema_for_block_model(plugin_id)` and serialized
/// to JSON. Empty / unknown plugin id → error.
GetPluginParams { plugin_id: String },

/// #572: list of materialized `BlockParameterDescriptor` for one
/// placed block (schema + current value per param). Resolved by
/// iterating the placed block's `ParameterSpec`s in the project's
/// model schema and calling `ParameterSpec::materialize(...)` with
/// the current value from `Project`.
GetBlockParams {
    chain: domain::ids::ChainId,
    block: domain::ids::BlockId,
},
```

### Resolvers

`crates/application/src/query.rs` gets two new pure functions following
the existing `list_plugin_catalog` / `get_plugin` / `find_plugins`
pattern:

```rust
pub fn get_plugin_params(project: &RigProject, plugin_id: &str)
    -> Result<String, String>;

pub fn get_block_params(
    project: &RigProject,
    chain: &ChainId,
    block: &BlockId,
) -> Result<String, String>;
```

Both serialize via `serde_json::to_string(...)` against the existing
types. Each is independently unit-tested with the same red-first
discipline as the other `query` functions.

### JSON shape

Default approach: **let the existing `Serialize` impls speak.** That
gives MCP clients the exact same wire shape the rest of the system uses
(no parallel schema to maintain). Concretely, the responses are
roughly:

`get_plugin_params` (the `ModelParameterSchema` direct serialization):

```json
{
  "effect_type": "preamp",
  "model": "british_70s",
  "display_name": "British 70s",
  "audio_mode": "stereo",
  "parameters": [
    {
      "path": "drive",
      "label": "Drive",
      "group": null,
      "widget": "Knob",
      "unit": "none",
      "domain": { "FloatRange": { "min": 0.0, "max": 10.0, "step": 0.1 } },
      "default_value": 5.0,
      "optional": false,
      "allow_empty": false
    }
  ]
}
```

`get_block_params` (a `Vec<BlockParameterDescriptor>` serialization,
wrapped in `{ "params": [...] }` to leave room for future top-level
metadata without breaking clients):

```json
{
  "params": [
    {
      "id": "chain:abc:block:def::drive",
      "block_id": "chain:abc:block:def",
      "effect_type": "preamp",
      "model": "british_70s",
      "audio_mode": "stereo",
      "path": "drive",
      "label": "Drive",
      "group": null,
      "widget": "Knob",
      "unit": "none",
      "domain": { "FloatRange": { "min": 0.0, "max": 10.0, "step": 0.1 } },
      "default_value": 5.0,
      "current_value": 7.2,
      "optional": false,
      "allow_empty": false
    }
  ]
}
```

If the existing serialization carries fields that are noisy for an MCP
client (e.g., very long file-path extension lists, or
`audio_mode`-only metadata that we already expose elsewhere), the
implementation phase may add a thin projection type. Default is "no
projection" — fewer types, less drift.

### Transport bindings

`crates/adapter-mcp`:

- `resources.rs`: two new resource templates,
  `openrig://plugins/{id}/params` and
  `openrig://chains/{cid}/blocks/{bid}/params`. Each gets a `parse_*`
  helper (matched **before** the broader `URI_PLUGIN_PREFIX` /
  `URI_CHAIN_PRESETS_TEMPLATE` arms so the `/params` suffix is not
  swallowed).
- `tools.rs`: **no change**. The current `tools()` function is a 1:1
  auto-derivation from the `Command` enum (each `Command` variant
  becomes one MCP tool via `command_schema`). There is no equivalent
  for `QueryKind` reads, and the parity test
  `parity_guard_every_command_variant_is_a_tool` enforces that count.
  Reads live exclusively on the resource path. If a future issue
  introduces a `query_schema`-style read-side derivation, both
  `get_plugin_params` and `get_block_params` would gain tools as a
  side effect — out of scope here.

### Adapter-console parity

The same test that on develop guards new `QueryKind` variants
(`ListChainPresets`, `ListProjectPresets`, etc.) gets two new arms
covering `GetPluginParams` and `GetBlockParams`. That keeps the
adapter-console guard rail in sync.

Note: on `develop` today `cargo build -p adapter-console` already fails
because the `ListChainPresets` / `ListProjectPresets` arms were never
backported from `feature/issue-548`. That is **not** in scope for #572,
but the new arms this issue adds must not make it worse — confirm
adapter-console builds clean after the parity arms land (which they
will, because the parity arms are explicitly what fixes the missing
coverage).

## Done criteria

- `cargo test -p application` — green, including the two new resolver
  unit suites.
- `cargo test -p adapter-mcp` — green, including the parity test arm
  for both new variants.
- `cargo build -p adapter-console` — clean (covering the
  pre-existing-on-develop gap as a side effect of adding the parity
  arms).
- `cargo build --workspace` — clean, zero new warnings
  (`CLAUDE.md` "zero warnings").
- Manual MCP smoke (locally): `get_plugin_params {plugin_id: "<known>"}`
  returns the catalog schema; `get_block_params {chain_id, block_id}`
  returns schema + current after one `set_block_parameter_number`
  round-trip.
- `docs/architecture.md` (or the appropriate MCP section in
  `docs/architecture.md` / `docs/cli.md`) mentions the two new tools
  and resources in the same commit as the implementation, per
  `CLAUDE.md` "doc no mesmo commit".

## Implementation order (suggested)

One issue, one branch (`feature/issue-572`), small commits — each one
red-first per `docs/testing.md`:

1. Locate the existing bridge functions:
   `project::block::schema_for_block_model(...)` and the "materialize
   descriptors for a placed block" helper (exact name to confirm via
   grep). If the second helper does not exist as a single function
   today, add it to `project` first (red-first), tested in isolation —
   it is the same loop the block editor already does, factored.
2. Add `QueryKind::GetPluginParams { plugin_id }` + resolver
   `get_plugin_params` + unit tests. Wire the frontend's
   `serve_queries` arm. No transport wiring yet.
3. Wire `openrig://plugins/{id}/params` MCP resource (no tool —
   see "Transport bindings"). Adapter-console parity arm. End-to-end
   smoke.
4. Add `QueryKind::GetBlockParams { chain, block }` + resolver
   `get_block_params` + unit tests.
5. Wire `openrig://chains/{cid}/blocks/{bid}/params` MCP resource
   (no tool, same reason). Adapter-console parity arm. End-to-end smoke.
6. Docs update (architecture.md / architecture-mcp section) +
   `gh issue comment` per push.

## Risks / open questions

- **Existing serialization fidelity.** The "let `Serialize` speak"
  default depends on the existing `serde` attributes producing a wire
  shape that is reasonable for agent clients. If, e.g., `ParameterDomain`
  serializes as `{"FloatRange":{...}}` (Rust enum default) we may want
  to project to a flatter shape. Decide during implementation, not
  before — first commit shows the raw output, then we decide.
- **`schema_for_block_model` ownership.** The function lives in the
  `project` crate today. The new resolver needs read-only access to
  it through whatever the frontend already exposes (no new
  cross-crate types). Confirm during step 1.
- **Param-id `path` stability.** `ParameterSpec.path` is what
  `Command::SetBlockParameter*` accepts as `path`. The resolver
  output uses the same field — agents can chain
  `get_block_params` → `set_block_parameter_number` without
  translation. Verify against `local_dispatcher_block_param.rs`
  during step 4.

## What this spec used to say

The original draft proposed a two-phase plan — Phase 1 "extract a new
`ParameterDescriptor` type from `adapter-gui` into `domain`", Phase 2
the actual MCP queries. Four commits were landed on this branch and
reverted (see git log for the revert SHAs) once recon on
`crates/block-core/src/param/` showed that:

- `BlockParameterDescriptor`, `ParameterSpec`, `ModelParameterSchema`,
  `ParameterDomain`, `ParameterWidget`, `ParameterUnit`,
  `ParameterOption` all already exist in `block-core::param`.
- `block_editor_param_items.rs` is already a pure consumer of those
  types — no schema literals there.

The architectural concern that drove the two-phase plan ("tela não tem
regra de negócio") was correct in general but already satisfied in
practice for parameters. The recon failure was on me — should have
`ls`-ed `block-core/src/param/` before writing the original spec.

## References

- `CLAUDE.md` — invariantes, leis de arquitetura, gitflow.
- `docs/development/gitflow.md` — workspace, branches, comentários em
  issue.
- `docs/testing.md` — red-first TDD discipline.
- `crates/application/src/bridge.rs` — `QueryKind` enum.
- `crates/application/src/query.rs` — existing resolvers (the new ones
  follow this pattern).
- `crates/application/src/command_schema.rs` — MCP tool auto-derivation
  pattern from the `Command` enum (read-side equivalent does not exist;
  each `QueryKind` arm in adapter-mcp is hand-written, same as today).
- `crates/block-core/src/param/schema.rs` — `ModelParameterSchema`,
  `ParameterSpec`, `ParameterDomain`, `ParameterWidget`,
  `ParameterUnit`, `ParameterOption`.
- `crates/block-core/src/param/descriptor.rs` — `BlockParameterDescriptor`.
- `crates/block-core/src/param/builders.rs` — `float_parameter`,
  `bool_parameter`, `enum_parameter`, `text_parameter`,
  `file_path_parameter`, etc. — how each model declares its specs.
- `crates/adapter-gui/src/block_editor_param_items.rs` — current GUI
  consumer (untouched by this issue).
- `crates/adapter-mcp/src/{tools,resources}.rs` — transport bindings.
