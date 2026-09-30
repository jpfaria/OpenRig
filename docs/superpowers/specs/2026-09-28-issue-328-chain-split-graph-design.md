# Chain split (Split → Mix, Y → A/B) and GraphView chain editor — design

**Issue:** #328 · **Milestone:** v0.6.0 · **Branch:** `feature/issue-328`
**Reference:** Hotone Ampero II manual, firmware 1.2.0, pp. 28–38 (chain types, split node, mixer node).
**Recon map:** issue #328, comment "#328 design map" (file:line map of model, engine, commands, UI and GraphView).

## Goal

Let one chain run two amps (or any two block lists) side by side, the way the Ampero II does:

- **Split → Mix.** Shared blocks → split → path A ∥ path B → mixer → shared blocks → chain output. Main use: amp A panned hard left, amp B panned hard right.
- **Y → A/B.** Shared blocks → split → path A → its outputs, path B → its outputs. No mixer.

Render every chain with the existing `GraphView` component (#435), which replaces the block strip in the main chains list. The input and output nodes of the graph open a checklist that disables inputs/outputs of the chain's own E/S.

## Non-goals

- The Ampero's other chain types. Parallel = two chains. Serial = one longer chain. A/B → Y = two chains feeding the same output (the backend mixes).
- More than one Split → Mix or more than one Y → A/B per chain, a Y before a Mix, or a split inside a path. The one combination a chain may hold is a Mix, then a Y (§9).
- Input, Output or Insert blocks inside a path.
- Adding endpoints from the graph. The chain's E/S config still defines which inputs and outputs exist.
- The node-graph patchbay of #784.

## Owner decisions (approved in chat, 2026-09-28)

1. The split can sit anywhere in the chain. Blocks before the split and after the mixer are shared by both paths. A Y → A/B is the last processing block (§1.1).
2. ~~One split per chain.~~ Amended 2026-09-30: at most one Split → Mix and at most one Y → A/B per chain; when both are present the Mix comes first (§9).
3. Mixer: level A/B, pan A/B, B polarity, master, master sum.
4. Y → A/B: each path ends at its own output node.
5. The two paths are time-aligned before they are summed.
6. The graph replaces each chain's block strip in the main chains list. The mouse wheel keeps scrolling the list; zoom is Cmd/Ctrl + wheel.
7. Blocks can be dragged between path A and path B.
8. Works on macOS, Windows and Linux/JACK.
9. Input/output checklist: lists every input (or output) of the chain's E/S, checked by default. Unchecking disables that endpoint for that node only; it stays listed.

## 1. Model (`crates/project`)

### 1.1 `SplitBlock`

A new variant `AudioBlockKind::Split(SplitBlock)` sits in `chain.blocks` / `RigPreset.blocks`, following the `Select` precedent (the only nested variant today). Blocks before it are the shared pre-chain. For Split → Mix, blocks after it are the shared post-chain. For Y → A/B, nothing may follow it.

```rust
pub struct SplitBlock {
    pub end: SplitEnd,            // Mix | Y
    pub params: ParameterSet,     // split + mixer knobs, see 1.2
    pub a: Vec<AudioBlock>,
    pub b: Vec<AudioBlock>,
}
pub enum SplitEnd { Mix, Y }
```

Rules, enforced in `split_block_methods.rs` (new) and `rig_validate`:
- At most one `Split` with `end: Mix` and at most one with `end: Y` per chain (§9).
- Path blocks may not be `Split`, `Select`, `Input`, `Output` or `Insert`. `Select` is excluded to keep nesting one level deep. Every split's paths are checked, the Mix's and the Y's.
- With `end: Y`, the `Split` must be the last processing block of the chain; only the chain's own `Input`/`Output` ports may follow it. The rule is judged from the Y's own position, so a Mix can only sit before the Y.
- `is_routing()` is true only for a `Split` with `end: Y`, because a Y's per-output path sets decide which streams exist. A `Split → Mix` is pure DSP inside one segment: bypassing it, editing its paths or dragging blocks between lanes rebuilds the DSP in place and never reopens streams (no #967-style silence). Only the Y per-output path sets enter `chain_structure_signature`; a Mix split's `enabled` flag and path contents stay out of it.
- `model_identity()` encodes `end` plus the ids and model identities of both paths, so a structural edit inside a path is detected by `replace_preset_blocks_if_structural`.

### 1.2 Split and mixer parameters

All knobs live in `SplitBlock.params` (a `ParameterSet`). They are edited through the existing `SetBlockParameter*` commands, which gives MIDI mapping, scenes and MCP parity for free. Defaults are the Ampero defaults.

| Key | Range | Default | Meaning |
|---|---|---|---|
| `split_mode` | `same` \| `dual_mono` | `same` | Ampero Mode I / Mode II |
| `level_to_a`, `level_to_b` | 0–100 | 100 | Linear gain into each path (`x/100`) |
| `balance_a`, `balance_b` | −50…+50 | 0 | Mode II only. Which input channel feeds the path: −50 = L only, 0 = (L+R)/2, +50 = R only. The fed signal is dual mono `[s, s]` |
| `mix_level_a`, `mix_level_b` | 0–100 | 100 | Mix only. Linear gain of each path into the mixer |
| `mix_pan_a`, `mix_pan_b` | −50…+50 | 0 | Mix only. Balance law: centre = unity on both sides; toward one side, the opposite side is attenuated linearly to 0 at ±50 |
| `mix_b_polarity` | `normal` \| `invert` | `normal` | Mix only. Multiplies path B by −1 |
| `mix_master` | 0–100 | 50 | Mix only. Output gain `x/100`. At the default, two identical paths sum to unity |
| `mix_master_sum` | bool | false | Mix only. Output becomes dual mono `L = R = (L+R)/2` |

The Ampero manual shows `BALANCE A/B` as one label; this design uses one balance per path so that path A can take L and path B can take R.

### 1.3 Endpoint checklist state (`RigInput`)

The E/S checklist is chain configuration, not preset data (the same preset can be reused by several inputs). It lives on `RigInput` next to `io_binding_ids`:

```rust
#[serde(default, skip_serializing_if = "EndpointDisables::is_empty")]
pub disabled_endpoints: EndpointDisables,

pub struct EndpointDisables {
    pub inputs: Vec<EndpointRef>,         // input node
    pub outputs: Vec<EndpointRef>,        // chain output node (chains without a Y split)
    pub path_a_outputs: Vec<EndpointRef>, // the Y split's path A output node
    pub path_b_outputs: Vec<EndpointRef>, // the Y split's path B output node
}
pub struct EndpointRef { pub io: String, pub endpoint: String }
```

Unknown refs (the endpoint was removed from the E/S) are ignored at runtime and dropped on the next save. The projection (`rig_to_chains`) filters the discovered endpoints with this set before segments are built.

### 1.4 Walkers that must recurse into paths

`find_block_recursive`, `resolve_chain_ports`, `Chain::input_blocks/output_blocks/has_io`, `apply_scene`, `write_back_processing_blocks`, the `duplicates_chain_binding` retain in `rig_projection.rs`, `rig_validate`, `project_disable_unavailable`, `port_duplication`, and every exhaustive `match` on `AudioBlockKind` (the compiler lists them).

`write_back_processing_blocks` and `apply_scene` must recurse into `SplitBlock.a/b` and write back `SplitBlock.params`. Without this, a knob change inside a path is lost on save.

## 2. Persistence (`crates/infra-yaml`)

- `project.openrig`: `kind: !Split { end, params, a, b }` comes from the derive.
- Chain presets and legacy files: add `type: split` to `AudioBlockYaml`, with nested ids following the Select scheme (`<split>::a:<i>`, `<split>::b:<i>`).
- Format version: write `version: 2` **only when the file contains a `Split`**. Split-free projects and presets stay at `version: 1`, so older builds keep opening them. An older build that opens a version 2 file refuses it with its existing "newer version" error instead of failing inside serde.
- `disabled_endpoints` is a new `#[serde(default)]` field. It does not need a version bump.

## 3. Commands (`crates/application`)

- `AddBlock`, `InsertPrebuiltBlock`, `MoveBlock`: new `#[serde(default)] path: Option<PathRef>` where `PathRef { split: BlockId, side: A | B }`. `None` = top level, so existing MCP and MIDI-map payloads stay valid. `MoveBlock` accepts a different `path` than the source, which is how blocks are dragged between A and B.
- `with_block`, `RemoveBlock` and the `MoveBlock` lookup search recursively.
- `AddBlock` ids switch to `BlockId::generate_for_chain`. The current `"{chain}:{kind}:{len}"` scheme can collide once blocks live in paths.
- New `AddSplit { chain, position, end }`. It creates an empty `Split` with default params. It is refused if the chain already has a split with that `end`, if a Mix would land after the Y, or if `end = Y` and processing blocks follow `position` (§9).
- New `SetSplitEnd { chain, split_id, end }`. It is refused when the chain would hold two splits of one end, and switching to `Y` is refused while post-split processing blocks exist.
- New `RemoveSplit { chain, split_id }`. Path A's blocks take that split's place and path B's blocks are removed; the chain's other split stays. The GUI asks for confirmation when B is not empty.
- New `SetChainEndpointEnabled { chain, node: Input | Output | PathAOutput | PathBOutput, io, endpoint, enabled }`.
- `validate.rs` walks both paths and checks the layout each path produces at the mixer.
- `COMMAND_VARIANT_COUNT` in `adapter-mcp` goes from 99 to 103. Each new variant gets its MCP tool.

## 4. Engine (`crates/engine`)

### 4.1 Split → Mix: DSP inside one segment

The mixer is DSP inside the segment's processing, never a sum of two segments or two runtimes (stream-isolation law).

- New `RuntimeProcessor::Split(SplitRuntimeState)` next to `Select`, built in a new `runtime_split_builder.rs`. `runtime_block_builders.rs` is at 520/600 lines and cannot grow.
- `SplitRuntimeState` holds `a` and `b` (`Vec<BlockRuntimeNode>`), a `b_buf: Vec<AudioFrame>` sized to the maximum callback size at build time, the two alignment delay lines (4.3), and the knobs as atomics (same pattern as `volume_pct_bits`) so knob moves do not rebuild.
- New `runtime_split_process.rs` (the per-callback math):
  1. Build path B's input in `b_buf` from the bus: Mode I = bus × `level_to_b`; Mode II = dual mono from `balance_b`. Apply path A's input to the bus in place the same way.
  2. Run `process_audio_block` over A (bus) and over B (`b_buf`).
  3. Delay the shorter path (4.3).
  4. Mix per sample: pan (balance law), level, B polarity, master, optional master sum. Result goes back to the bus.
- `node_emits_mono_content` treats `Split` as stereo output.
- Node reuse by id, block toggle (`runtime_block_toggle.rs`), bypass mirror, `route_convolution`, probe/offline/tone-doctor walkers recurse into both paths.
- Zero allocation, lock, syscall or I/O on the audio thread. `audio_alloc_invariant_tests.rs` gets a Split → Mix case.

### 4.2 Y → A/B: one segment per (input × output)

The engine already runs one segment per (input × output) pair (`split_chain_into_segments`). For a Y chain, the segment for output `O` runs the shared blocks (with a Mix before the Y, the whole Mix among them, §9), then every path whose output node has `O` checked. If both paths feed `O`, they are summed inside that segment at unity with alignment (same code as 4.1, with neutral mixer knobs). If no path feeds `O`, no segment is built for it.

- `ChainSegment` gains a `paths: SegmentPaths` field (`A`, `B` or `AB`). The builder expands the Y node into those paths.
- `classify_output_routes` and `blocks_between` map each output route to its path set.
- `chain_structure_signature` (`infra-cpal/src/io_topology.rs`) includes the per-output path sets, so checking a new output opens its stream.
- Linux/JACK: `jack_handlers.rs:256` pops output index 0 only, which would leave path B silent. Fix behind `cfg(all(target_os = "linux", feature = "jack"))`.

### 4.3 Path alignment

Summing two paths with different latency comb-filters, and worse with B inverted.

- `MonoProcessor` and `StereoProcessor` gain `fn latency_samples(&self) -> usize { 0 }`. It is implemented where latency exists: the 2× oversampler (7 samples), VST3 (`getLatencySamples`), LV2 (latency port). Built-in blocks with no look-ahead stay at 0.
- At build time the split sums each path's latency and preallocates a delay line on the shorter path for the difference, capped at 16384 samples. Above the cap, it clamps and logs at build time (never on the audio thread).
- The longer path is not delayed, so the chain's total latency does not change.

## 5. GraphView as the chain editor (`crates/adapter-gui`)

Required skills before touching `.slint`: `claude-plugin:ux-ui` and `slint-best-practices`. Every layout step is checked with a `tools/slint-render` PNG. Every click, overlay and drag is proven with an `i-slint-backend-testing` interaction test before push.

### 5.1 Graph content

Left to right: input node → shared blocks → split node → two lanes (A on top, B below) → mixer node → shared blocks → output node. For Y there is no mixer; each lane ends in its own output node. A chain with a Mix then a Y draws both stages in that order (§9).

- Input and output nodes show the checked endpoint names. A click opens the endpoint checklist (5.3).
- The split and mixer nodes are labelled, clickable nodes. A click opens the split editor or the mixer editor: a small panel with the knobs from 1.2, rendered from the parameter schema like any block editor.
- Block cards keep parity with today's `BlockChip`: icon, enabled LED, unavailable tint, MIDI markers, tooltip, selection.
- A "+" affordance on each edge and at the end of each lane opens the existing add-block picker with the right position and `path`. The picker gets "Split → Mix" and "Y → A/B" entries. "Split → Mix" is offered while the chain has no Mix, only at positions before its Y. "Y → A/B" is offered while the chain has no Y, only after its Mix and where nothing but the chain's ports follows (§9).
- Drag moves a block within a lane, across the split into the other lane, or between shared and path positions. The drop target resolves to a `MoveBlock { position, path }`.
- The mouse wheel is not captured, so the chains list keeps scrolling. Cmd (macOS) / Ctrl (Windows, Linux) + wheel zooms. Pan is by drag on empty canvas.

### 5.2 GraphView changes

- `ChainStage::Parallel` gets an `end: Merge | Fan` choice. `Fan` produces no `__merge_N` node and one terminal per lane. `linear_chain_layout` and the auto-layout handle both.
- New node kinds: `io_input`, `io_output`, `split`, `mixer`, alongside `block`. The kind drives the card, not `label == ""`.
- New callbacks: `add_requested(edge or lane end)`, `remove_requested(node)`, `bypass_toggled(node)`, `node_dropped(node, target)`. Existing callbacks stay.
- `graph_view.slint` is 420/500 lines. It is split before growing: `graph_node_card.slint`, `graph_wire.slint` and the canvas stay as separate files.
- The accessible label uses `@tr`.
- Node ids are `BlockId`s. The flat `real_index` mapping, and the existing mismatch between `ui_index_to_real_block_index` (identity) and `real_block_index_to_ui` (skips the first Input and last Output), are replaced by id-based addressing.
- A new adapter (`chain_graph_adapter.rs`) turns a `Chain` into `ChainStage`s and then into Slint `GraphNode`/`GraphEdgeGeometry`. The meter poll must reuse the models, not rebuild them each tick.

### 5.3 Endpoint checklist

- A root-level overlay, not a `PopupWindow` (PopupWindow content does not receive clicks, #749/#761). It reuses the app's single generic select/checklist component.
- It lists every input (or output) of the chain's E/S bindings, checked by default. Toggling one dispatches `SetChainEndpointEnabled`. Nothing is added or removed from the E/S.
- Unchecking every input or every output of a node is allowed; that node's segments simply are not built.

### 5.4 Where it lives

- The graph replaces `ChainRowBlocks` inside `ChainRow`. The row height is derived from the lane count: one lane for linear chains, two for a chain with any split (a Mix then a Y still draws two lanes, one stage after the other). The four `tests/chain_row_*.rs` tests that pin the current height for linear chains must stay green unchanged.
- `app-window.slint` is at 500/500. Any callback forwarded through it forces a split of that file first (behaviour-preserving).
- Compact view: every block is a row in signal order: each split, then its path A, then its path B, then what follows it. The split row has its icon, its kind (SPLIT MIX / SPLIT Y) and the split editor's knobs on the row; a Mix split adds a Mixer tab with the mixer's knobs. It has no model selector, and opening it opens the split editor. A path row carries an "A"/"B" badge on its type icon. Toggle, remove, parameters, model, open, insert and drag all act on that path's block. A drag only reorders within its own list. Touch view: each split shows as one "Split" chip that opens the split editor on that split.

## 6. Invariants and risks

- **Volume.** Chains without a split must produce bit-identical output; `volume_invariants_tests.rs` stays unchanged. A Split → Mix with default knobs and identical paths is unity.
- **Stream isolation.** The mix is DSP inside one segment. Y outputs are separate segments that each run the shared blocks.
- **Latency.** No chain gains latency. Alignment only delays the shorter path.
- **CPU.** A split costs path A + path B + one mix pass. With a Mix before a Y, every Y output segment runs its own Mix, so the Mix and every block before the Y cost once per Y output (§9). Measured on the hardware battery (`OPENRIG_HW_TESTS=1`) before and after.
- **Mode II** only makes sense with a stereo or dual-mono source placed before any mono block. With a mono source both balances give the same signal. This is documented, not an error.
- **Pan on a 1-channel output.** A mono route averages L/R, so pan has no audible effect there. Documented.
- **File caps.** New logic goes into new files, each with its `//! Responsibility:` / `// Responsibility:` header.

## 7. Tests (red first)

- Model: `SplitBlock` rules; `model_identity` changes on a nested edit; scene apply and write-back reach nested params.
- YAML: round trip of `!Split` and `type: split`; version 2 only when a Split exists; version 1 files load unchanged.
- Commands: `path` addressing; move across paths; `AddSplit`/`SetSplitEnd`/`RemoveSplit` rules; `SetChainEndpointEnabled`; MCP variant count 103.
- Engine math: unity at defaults with identical paths; hard-L/hard-R isolation (A only on L, B only on R); B polarity cancels identical paths; master sum; Mode II balance picks L or R; alignment removes a known offset.
- Engine structure: Y builds one segment per checked output with the right path set; both paths on one output sum inside the segment; no allocation after warm-up with a split.
- Hardware battery: dual amp L/R on the real interface, zero xruns.
- GraphView model: `Fan` layout, I/O/split/mixer node kinds, drop-target resolution.
- Slint: interaction tests for the checklist overlay, the split/mixer editors and drag across lanes; `slint-render` PNGs of a linear chain, a Split → Mix and a Y → A/B chain.

## 8. Docs (same commits as the code)

`docs/blocks-catalog.md` (Split block and knobs), `docs/audio-config.md` (Y outputs, endpoint checklist), `docs/screens.md` and `docs/gui/graph-view.md` (graph chain row), `docs/mcp.md` (new tools), README in the three languages (feature list), all nine translation files for new UI strings.

## 9. Mix followed by Y (amendment, owner-approved 2026-09-30)

Owner's use case: sum two amps, then send the sum to two outputs that differ only in the cab.

```
input → Split → Mix (A: amp 1 ∥ B: amp 2) → shared blocks → Y → A/B
                                                            ├─ A: cab IR → FRFR output
                                                            └─ B: (nothing) → SYN-5050 output
```

### 9.1 Rule

- A chain holds at most one Split → Mix and at most one Y → A/B.
- When both are present the Mix comes first and the Y is the last processing block: only the chain's own `Input`/`Output` ports may follow it (the same Y-last rule as §1.1).
- Still refused: two Mix, two Y, a Y before a Mix, a processing block after the Y, and any split inside a path.
- `validate_split_layout` (`crates/project/src/block/split_block_methods.rs`) counts the splits per end and judges the Y-last rule from the Y's own position. Its errors are `a chain holds at most one split of each end (a Mix, then a Y), found 2 Mix` (or `Y`) and `a Y split ends the chain, but block '<id>' follows it`. `RigProject::validate` and the command layer's `ensure_split_rules` check the paths of every split, not only the first one.

### 9.2 Model and lookups

- `crates/project/src/block/split_lookup.rs`: `splits` (every top-level split with its position, in chain order), `find_split` (the first one), `find_split_with_end` and `has_y_split`. No lookup assumes a single split.
- Whatever asks where the chain ends (tail feed, `resolve_chain_ports`, endpoint discovery and feeds) looks up the Y, not the first split. A Mix + Y chain therefore ends at the Y's two path output nodes and has no chain output node. `EndpointDisables.path_a_outputs` / `path_b_outputs` belong to the Y; `outputs` applies only to chains without a Y.
- Ids: path block ids carry their split's id (`<split id>::a:<i>` / `<split id>::b:<i>` on disk, `BlockId::generate_for_chain` for new blocks), so the paths of the two splits never collide. Every command names a split by its block id: `PathRef { split, side }`, `SetSplitEnd { split_id }`, `RemoveSplit { split_id }`, and the knobs are `SetBlockParameter*` on that id (MIDI, scenes and MCP included).
- YAML: `project.openrig`, chain presets and legacy project files keep both splits with distinct path ids. The format version stays `2`.

### 9.3 Commands

- `AddSplit` is refused when the chain already has a split with that `end`, when a Mix would land after the Y, or when a processing block would follow a new Y.
- `SetSplitEnd` is refused when the chain would hold two splits of one end. On a chain holding a Mix and a Y, neither can switch.
- `RemoveSplit` removes only the split it names. Removing the Mix puts its path A in its place, still before the Y. Removing the Y puts its path A at the end and keeps the Mix.

### 9.4 Engine

No new engine code. The segment of Y output `O` runs the blocks before the Mix, the Mix node (both paths and the mixer, DSP inside the segment, §4.1), the blocks between the Mix and the Y, and then the Y shaped by `split_segment_view` into the paths `O` checks (§4.2). `split_segment_view` reshapes only the Y and leaves the Mix unchanged.

- **CPU scales with the Y outputs.** Every Y output is its own segment, so each one runs its own copy of the Mix and of every block before the Y. This is the stream-isolation law (one pipeline per output), never a shared pre-Y runtime.
- A live Mix knob edit reaches every Y output, and so does switching the Mix on or off in place.
- Only the Y's per-output path sets enter `chain_structure_signature`. Editing the Mix never reopens streams, and changing a Y path set behind a Mix is structural like on a Y alone.
- With both splits in one segment the audio callback allocates nothing once warm.

Pre-existing gaps, also true with a single split and not changed by this amendment:

- `runtime_graph_prebuild.rs` walks the raw `chain.blocks`, where `model_ref()` is `None` for a `Split`. So every live edit prebuilds the Y from scratch, since its saved block never matches the reshaped block the live Y is built from. A Mix is also prebuilt from scratch when it changes. `build_nodes_for` never takes a prebuilt split, so that work is thrown away and the audio is not wrong. `is_vst3` looks at the split only, not at its paths, so a VST3 inside a split path can be instantiated by the prebuild while the live instance processes (the #779 crash risk).
- `runtime_graph_assemble.rs` and `runtime_graph_update.rs` collect stream handles from top-level nodes only, so a block inside a split path gets no handle.

### 9.5 GUI

- Each split is its own `Parallel` stage of the graph. `linear_chain_layout` names the n-th split (1-based, top-level order) `__split_n` and, for a Mix, its mixer `__merge_n`. `resolve_node`, the lane anchors, click and remove gestures, the split editor callback, the end switch and the compact rows all carry the split's block id.
- The picker offers "Split → Mix" while the chain has no Mix, only before its Y, and "Y → A/B" while it has no Y, only after its Mix where nothing but the chain's ports follows. A chain holding both offers neither.
- Removing the Y node removes only the Y (it asks first when its path B holds blocks) and keeps the Mix.
- The row keeps two lanes. The compact view lists each split row followed by its paths. The touch view shows one Split chip per split, and each chip opens the editor on its own split.

### 9.6 Tests

- Model: `crates/project/tests/issue_328_split_rules.rs` (`a_mix_then_a_y_is_accepted`, `a_block_after_the_y_is_refused_when_a_mix_comes_first`, `a_y_before_a_mix_is_refused`, `a_chain_holds_at_most_one_mix_and_one_y`, `a_rig_accepts_a_mix_then_a_y`, `a_rig_refuses_a_port_or_select_in_the_y_path_behind_a_mix`, `the_split_lookups_see_every_split`), `crates/project/src/endpoint_feeds_tests.rs` and `crates/project/tests/issue_328_endpoint_discovery.rs`.
- YAML: `crates/infra-yaml/tests/issue_328_split_yaml.rs` (project round trip, chain preset, legacy project file).
- Commands: `crates/application/src/local_dispatcher_split_tests.rs` and `crates/application/src/ld_split_path_tests.rs`.
- Engine: `crates/engine/src/issue_328_y_segments_tests.rs`, `crates/engine/src/issue_328_y_audio_tests.rs` (each Y output hears the Mix, then only its own path; a Mix knob edit and a Mix toggle reach every output), `audio_alloc_invariant_tests.rs` (`audio_callback_does_not_allocate_with_a_mix_then_a_y`), and `crates/infra-cpal/src/io_topology_tests.rs` (`a_y_path_set_change_behind_a_mix_is_a_structural_change`).
- GUI: the `mix_then_y_*` tests of `crates/adapter-gui/src/` (graph ids, anchors, clicks, gestures, picker, split chip, compact rows, split editor) on the fixture `[pre, Mix(ma | mb), mid, Y(ya | yb)]`.
