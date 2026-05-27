# MCP read parity for block/plugin parameters (#572)

**Status:** draft
**Issue:** [#572](https://github.com/jpfaria/OpenRig/issues/572)
**Date:** 2026-05-27
**Author:** brainstorm with @jpfaria

## Problem

MCP (and any future transport: gRPC, remote) can already **set** block parameters
via `Command::SetBlockParameter{Number,Bool,Text,File}` and
`Command::SelectBlockParameterOption` — these are auto-derived into MCP tools
(`set_block_parameter_number`, `select_block_parameter_option`, etc.) by
`crates/application/src/command_schema.rs`. But there is no way for any
transport to **discover**:

- Which parameters a given plugin (catalog entry) exposes.
- Which parameters a given placed block instance currently has.
- The schema of each parameter (kind, range, options, default).
- The current value of each parameter on a placed block.

Without that, an MCP / gRPC client can only drive parameter mutations it
already knows about by name and type — there is no introspection. The
preset/tone-builder workflow, in particular, needs to know which knobs are
reachable before deciding which `set_block_parameter_*` to call.

## Architectural constraint (the real hard part)

The parameter **descriptor** — the metadata that says "this knob is a number
0..10 default 5", "this is an option { A, B, C } default A", etc. — currently
lives in the GUI layer (`crates/adapter-gui/src/block_editor_param_items.rs`
builds Slint rows from per-block knowledge encoded there). That violates two
inegociáveis OpenRig laws (`CLAUDE.md`):

1. **"Tela não tem regra de negócio. Slint é dispatcher puro."**
2. **"Backend transport-agnostic."**

If a new MCP `QueryKind::GetPluginParams` resolver pulled the schema from the
GUI (or duplicated the rules inside `crates/adapter-mcp`), every new transport
(gRPC, remote control) would re-duplicate the same schema rules and they would
drift the moment one transport added a knob or changed a range. The setters
already *nasce* `Command` (single source of truth); the readers must read from
a matching single source.

**Conclusion:** the work is two phases. Phase 1 is the real architectural
change. Phase 2 (the actual MCP queries) becomes trivial once Phase 1 is done.

## Goals

1. A single domain-owned source of truth for block parameter descriptors.
2. The GUI consumes that source (no schema rules left in `adapter-gui`).
3. MCP exposes two new read tools/resources that read the same source.
4. Adapter-console parity test covers the two new `QueryKind` variants.
5. Zero regression to existing setter Commands, the GUI's block editor, or
   any audio-thread invariant (`CLAUDE.md` invariantes 1–10).

## Non-goals

- gRPC adapter wiring (the new `QueryKind` variants will feed it automatically
  whenever the gRPC adapter exists; nothing transport-specific in this issue).
- Adding new parameter kinds beyond what `ParameterValue` already supports
  (`Number`, `Bool`, `Text`, `Option`, `File`).
- Changing the mutation path or the on-disk shape of `Project`.
- Exposing setter parity (already exists via the 5 `set_block_parameter_*`
  Commands).
- A persistent disk-side plugin parameter schema cache.

## Phase 1 — extract `ParameterDescriptor` into `domain`

### New domain type

Add next to `domain::value_objects::ParameterValue`:

```rust
pub struct ParameterDescriptor {
    pub id: domain::ids::ParameterId,
    pub kind: ParameterKind,
    pub default: ParameterValue,
}

pub enum ParameterKind {
    Number { min: f32, max: f32, step: f32 },
    Bool,
    Text,
    Option { values: Vec<String> },
    File,
}
```

Invariants enforced by constructors (and tested red-first):

- `Number`: `min < max`, `step > 0`, `default` is `Number(v)` with `min <= v <= max`.
- `Option`: `values.len() >= 1`, `default` is `Text(v)` with `v` in `values`.
- `Bool`: `default` is `Bool(_)`.
- `Text`: `default` is `Text(_)`.
- `File`: `default` is `Text(_)` (path) or unset — to be settled by the
  current `PickBlockParameterFile` semantics during extraction.

### Source per block

Each block model exposes its descriptor list. Two viable shapes — to be
chosen in the implementation plan after a code read of the block registry:

- **(a)** A `BlockModel` trait method `fn descriptors(&self) -> &[ParameterDescriptor]`
  with each model returning a `'static` slice (cheap, deterministic).
- **(b)** The block registry stores descriptors next to the model id (data,
  not behavior — works if model construction is cheap and we want them
  introspectable without instantiating).

Shape (a) is the working assumption; (b) only if registry analysis shows
descriptors are already keyed by model id elsewhere.

### GUI becomes a consumer

`crates/adapter-gui/src/block_editor_param_items.rs` and any of the
`block_editor_*` siblings that today encode per-model schema rules must move
those rules into the model's `descriptors()` and read back through that
single source. No `min` / `max` / `options` literal stays in `adapter-gui`
after this phase.

Red-first per `docs/testing.md`:

- Add a domain unit test asserting each existing block model returns
  descriptors that match the rows the GUI used to build. The test fails
  before extraction (no `descriptors()` exists) — that is the red — then
  goes green as the extraction lands.
- Add a Slint-free GUI test (against the value layer in
  `block_editor_values.rs`) confirming that the rows the editor would render
  come from `descriptors()` and not from local literals.

### Done criteria for Phase 1

- `grep -rEn '\b(min|max|options|step)\s*[:=]' crates/adapter-gui/src` shows
  zero parameter-schema literals.
- All existing tests still pass (`cargo test --workspace`).
- GUI block editor still renders identical rows for every model (manual
  smoke + golden where applicable).
- Volume invariants and audio-thread invariants untouched (Phase 1 does not
  touch the audio path at all).

## Phase 2 — read parity via `QueryKind`

### New variants

Add to `crates/application/src/bridge.rs::QueryKind`:

```rust
/// #572: full parameter schema for one plugin (catalog-level).
/// No placed instance required. Resolved from the block model's
/// ParameterDescriptor list. JSON payload omits `current`.
GetPluginParams { plugin_id: String },

/// #572: schema + current value for one placed block instance.
/// JSON payload includes `current` per param, read from the Project.
GetBlockParams {
    chain: domain::ids::ChainId,
    block: domain::ids::BlockId,
},
```

### Resolvers

`crates/application/src/query.rs` gets two new pure functions following the
existing `list_plugin_catalog` / `get_plugin` / `find_plugins` pattern:

```rust
pub fn get_plugin_params(catalog: &PluginCatalog, id: &str)
    -> Result<String, String>;
pub fn get_block_params(project: &RigProject, chain: ChainId, block: BlockId)
    -> Result<String, String>;
```

Both serialize to the JSON shape below. Each is independently unit-tested
against fixtures with the same red-first discipline as the other `query`
functions on develop.

### JSON shape (uniform)

```json
{
  "params": [
    {
      "id": "drive",
      "kind": "number",
      "min": 0.0,
      "max": 10.0,
      "step": 0.1,
      "default": 5.0,
      "current": 7.2
    },
    {
      "id": "voice",
      "kind": "option",
      "options": ["A", "B", "C"],
      "default": "A",
      "current": "B"
    },
    {
      "id": "enabled",
      "kind": "bool",
      "default": true,
      "current": false
    },
    {
      "id": "ir_file",
      "kind": "file",
      "current": "/path/to.wav"
    }
  ]
}
```

Rules:

- `current` is **omitted** in `GetPluginParams` responses.
- `current` is **always present** in `GetBlockParams` responses for every
  param the block has (even if the user has not changed it from `default`).
- A param the block lacks (model mismatch, stale snapshot) is dropped from
  the response — never reported with `current: null`. The transport sees
  the truth of the current `Project` state, nothing more.
- Numeric values are JSON numbers (not strings), booleans are JSON booleans.

### Transport bindings

`crates/adapter-mcp`:

- `tools.rs`: two new tool definitions, `get_plugin_params { plugin_id }`
  and `get_block_params { chain_id, block_id }`. Names follow the existing
  `list_plugin_catalog` / `get_plugin` style (snake_case, no `mcp` prefix).
- `resources.rs`: two new resource templates,
  `openrig://plugin/{id}/params` and
  `openrig://chain/{cid}/block/{bid}/params`.

### Adapter-console parity

The same test that on develop guards new `QueryKind` variants
(`ListChainPresets`, `ListProjectPresets`, etc.) gets two new arms covering
`GetPluginParams` and `GetBlockParams`. That is the canary that future
`QueryKind` additions also need adapter-console coverage.

### Done criteria for Phase 2

- `cargo test --workspace` green (including the two new resolver suites and
  the adapter-console parity test).
- MCP smoke (manual): `get_plugin_params {plugin_id: "<known>"}` returns
  the expected schema; `get_block_params {chain_id, block_id}` returns
  schema + current after one `set_block_parameter_number` round-trip.
- Resources reachable: `openrig://plugin/<id>/params` and
  `openrig://chain/<cid>/block/<bid>/params` deliver the same JSON the
  tools return.
- Zero new warnings (`cargo build` clean per `CLAUDE.md`).

## Phasing / PR strategy

One issue, one branch (`feature/issue-572`), but two logical commit clusters
inside it. The PR description will call the phase boundary out explicitly so
review can read them in order:

1. Phase 1 commits — domain `ParameterDescriptor`, GUI extraction, tests.
2. Phase 2 commits — `QueryKind` variants, resolvers, MCP tools/resources,
   adapter-console parity test.

If Phase 1 churn turns out to be larger than expected during the
implementation-plan write-up, the plan may propose splitting Phase 1 into a
separate PR (still #572 issue, separate sibling PR) — the spec does not
prescribe that, the implementation plan will decide based on diff size.

## Risks / open questions

- **Block registry shape (Phase 1).** The `(a)` vs `(b)` decision above
  depends on what the block registry currently looks like. The
  implementation plan will read `crates/registry` (or equivalent) first.
- **File-kind defaults.** Need to confirm during extraction whether `File`
  has a meaningful default (probably no — the user picks at use time). If
  so, `default` may be optional on `ParameterKind::File`.
- **Param-id stability.** The JSON contract uses `id` strings; these must
  match what `Command::SetBlockParameter*` already accepts in `path`, so
  agents can chain `get_block_params` → `set_block_parameter_number`
  without translation. The implementation plan will verify this against
  the existing `path` parsing in `local_dispatcher_block_param.rs`.
- **Adapter-gui scope creep.** Phase 1 touches `block_editor_*` files; the
  extraction must stop at moving schema literals, not refactor unrelated
  rendering code (per `CLAUDE.md` "delete só o escopo literal pedido").

## References

- `CLAUDE.md` — invariantes, leis de arquitetura, gitflow.
- `docs/development/gitflow.md` — workspace, branches, comentários em issue.
- `docs/testing.md` — red-first TDD discipline.
- `crates/application/src/bridge.rs` — `QueryKind` enum.
- `crates/application/src/query.rs` — existing resolvers.
- `crates/application/src/command_schema.rs` — MCP tool auto-derivation
  pattern from the `Command` enum (read-side equivalent does not exist yet
  and is **not** introduced by this issue — each `QueryKind` arm in
  adapter-mcp is hand-written, same as today).
- `crates/adapter-mcp/src/{tools,resources}.rs` — transport bindings.
- `crates/adapter-gui/src/block_editor_param_items.rs` — current home of
  the schema literals to extract.
