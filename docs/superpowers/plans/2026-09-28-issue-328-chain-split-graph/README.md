# #328 Chain split and GraphView chain editor — plan index

**Spec:** `docs/superpowers/specs/2026-09-28-issue-328-chain-split-graph-design.md` · **Issue:** #328 · **Branch:** `feature/issue-328` · **Workspace:** `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328` (a `git clone`, never a worktree, never the main folder).

## Parts and order

Run the parts strictly in this order, one task at a time. A part never consumes anything a later part produces.

| # | File | Scope (spec) | Consumes | Tasks |
|---|---|---|---|---|
| 1 | `part-1-model.md` | Model + persistence: `SplitBlock`, knobs, rules, walkers, YAML v2, `EndpointDisables` on `RigInput`/`Chain`, checklist applied in `resolve_chain_ports` (§1, §2) | — | 1–12 |
| 2 | `part-2-commands.md` | Commands + MCP: `path` addressing, `SplitCommand`, `SetChainEndpointEnabled`, save-time pruning, validate, 103 tools (§3) | Part 1 | 1–7, **7b**, 8–10, **10b**, 11 |
| 3 | `part-3-engine-mix.md` | Engine Split → Mix inside one segment, processor latency, path alignment, walkers, hardware battery (§4.1, §4.3) | Part 1 | 0–3, 3b, 4–19, 19b, 20–21 |
| 4 | `part-4-engine-y.md` | Engine Y → A/B: `SegmentPaths`, per-output path sets, empty-node handling, `chain_plays`, stream signature, Linux/JACK (§4.2, §1.3 runtime, §5.3 engine side) | Parts 1, 3 | 1–11 |
| 5 | `part-5-graphview.md` | GraphView component: file split, wheel/zoom, `ParallelEnd::Fan`, node kinds, anchors, drop resolution, cards, callbacks (§5.2, §5.1 component side) | — | 1–11 |
| 6 | `part-6-chain-row.md` | Chain row integration: id addressing, `chain_graph_adapter`, ChainRow graph, gestures, picker entries, Split chip, split/mixer editors, endpoint checklist, README, owner checklist (§5.1, §5.3, §5.4, §8) | Parts 1, 2, 5 | 0–16 |

Dependency notes:
- Part 5 is independent and could move earlier, but builds are serialized, so the order above is the one to follow.
- Part 4 relies on Part 3's `RuntimeProcessor::Split` (it builds a Y split per segment as a neutral Split → Mix through Part 3's builder).
- Part 6's owner checklist (Task 16) covers audio behaviour from Parts 3–4, so the owner validates only after Part 6.

## Execution rules (every part)

- Work only in the solver clone; every git command is `git -C <solver> …`; stage explicit paths, never `git add -A`; never `git worktree`.
- **One cargo process at a time**, always `nice -n 19 cargo … -j 2`. No parallel builds, no scratch clones, reviewers run sequentially.
- Production code only through the Edit/Write tools (no sed/regex migrations). TDD red-first: see the assertion fail (paste the FAILED line) before production code; a test that passes on arrival is labelled a characterization pin.
- `crates/engine/src/volume_invariants_tests.rs` is never edited. Zero allocation/lock/syscall/I/O on the audio thread. One file, one responsibility, `//! Responsibility:` / `// Responsibility:` header, caps `.rs` 600 / `.slint` 500, routers under 100 lines.
- **Commit + push after every task.** Before EVERY push, run the push gate: `cargo fmt --all -- --check`, `nice -n 19 cargo test --workspace -j 2`, `nice -n 19 cargo build --workspace -j 2` (zero warnings), `VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates`. If the remote moved: `pull --rebase`, re-run the gate, push.
- **After every push:** `gh issue comment 328 --repo jpfaria/OpenRig` with the short hash, the files and the tests that ran.
- Commit messages: `feat(#328): …` in English, **no `Co-Authored-By` trailer** (`.claude/skills/openrig-code-quality/SKILL.md:446`, `docs/development/gitflow.md:39`). All six parts now follow this.
- UI tasks: invoke `claude-plugin:ux-ui`, `slint:slint` and `slint-best-practices` before the first `.slint` line; render with `tools/slint-render` and look at the PNG; prove every click, overlay and drag with an `i-slint-backend-testing` test before the push; no `PopupWindow`; SVG icons only; every new string in all nine catalogs.
- Hardware battery (`OPENRIG_HW_TESTS=1`, idle machine) is run for Part 3 Task 20; the JACK code of Part 4 Task 10 compiles only on the Linux CI.
- Docs change in the same commit as the behaviour (spec §8).

## Coverage (spec → part / task)

| Spec | Requirement | Part / Task |
|---|---|---|
| §1.1 | `AudioBlockKind::Split(SplitBlock)`, `SplitEnd`, `label`, `is_routing`, `model_identity` over both paths | P1 T2 |
| §1.1 | One split per chain; paths hold processing blocks only; Y ends the chain (model + `rig_validate`) | P1 T4 |
| §1.1 | Same rules enforced at every command door, chain untouched on refusal | P2 T3, T4, T5, T6, T8 |
| §1.2 | Knob schema, ranges, Ampero defaults | P1 T1 |
| §1.2 | Knobs edited through `SetBlockParameter*` (MIDI, scenes, MCP parity) | P1 T2 (`params_mut`), P2 T1 |
| §1.3 | `EndpointDisables` / `EndpointRef { io, endpoint }` / `EndpointNode` | P1 T8 |
| §1.3 | `RigInput.disabled_endpoints` persisted, no version bump | P1 T9 |
| §1.3 | Projected onto `Chain`, captured back, kept by the chain editor | P1 T10; P2 T7 (SaveChain keeps it) |
| §1.3 | Discovered endpoints filtered before segments are built; unknown refs ignored | P1 T11 (P4 T2 verifies) |
| §1.3 | Unknown refs dropped on the next save | **P2 T7b (added by this review)** |
| §1.4 | Walkers: `find_block_recursive`, descriptors, `project_disable_unavailable`, `port_duplication`, exhaustive matches | P1 T2, T6 |
| §1.4 | `apply_scene`, `write_back_processing_blocks`, model swap write-back reach paths and split params | P1 T7 |
| §1.4 | `resolve_chain_ports` | P1 T11 |
| §1.4 | `Chain::input_blocks/output_blocks/has_io`, `duplicates_chain_binding` retains | Kept top-level by design (P1 "File Structure" note: ports are forbidden inside a path) |
| §2 | `kind: !Split`, `type: split`, nested ids `<split>::a:<i>` / `::b:<i>`, version 2 only with a split, v1 unchanged, path block that cannot load drops only itself | P1 T3 |
| §2 | `disabled_endpoints` serde default | P1 T9, T10 |
| §3 | `path: Option<PathRef>` on `AddBlock` / `InsertPrebuiltBlock` / `MoveBlock`, byte-identical path-less payloads | P2 T4, T5 |
| §3 | `with_block`, `RemoveBlock`, `MoveBlock` lookup recursive; read parity | P2 T1, T2, T3, T5 |
| §3 | `AddBlock` ids via `BlockId::generate_for_chain` | P2 T4 |
| §3 | `AddSplit`, `SetSplitEnd`, `RemoveSplit` and their refusals | P2 T6 |
| §3 | GUI confirmation when path B is not empty | P6 T11 |
| §3 | `SetChainEndpointEnabled` | P2 T7 |
| §3 | `validate.rs` walks both paths, layout at the mixer | P2 T9 |
| §3 | MCP `COMMAND_VARIANT_COUNT` 99 → 103, one tool per variant | P2 T6 (102), T7 (103) |
| §3 / §1.4 | Preset/scene switch brings the new preset's split | P1 T5 (fix), P2 T10 (end-to-end pin) |
| §3 / §1.4 | Model swap inside a path keeps scenes (rig and live block) | P1 T7, **P2 T10b (added by this review)** |
| §4.1 | `RuntimeProcessor::Split(SplitRuntimeState)`, new files, `b_buf`, knobs as atomics | P3 T6–T12, T14 |
| §4.1 | Per-callback math: split inputs, run paths, align, mix | P3 T6, T11 |
| §4.1 | `node_emits_mono_content` = stereo for a split | P3 T12 |
| §4.1 | Node reuse by id, block toggle, bypass mirror, `route_convolution`, probe/offline/tone-doctor walkers | P3 T13, T15, T16, T17, T18, T19, T14 (tone doctor), T19b (#779 VST3 live rebuild); P4 T7 |
| §4.1 | `audio_alloc_invariant_tests.rs` split case | P3 T14 (P4 T6/T10 for Y/JACK) |
| §4.2 | One segment per input × checked output, `ChainSegment.paths: SegmentPaths`, no segment when no path feeds | P4 T4, T5, T6 |
| §4.2 | Both paths on one output summed inside the segment, aligned | P4 T6 (via P3 builder) |
| §4.2 | `classify_output_routes` / `blocks_between` mapping | P4 T4 (`segment_paths::route_paths`; signatures kept) |
| §4.2 | `chain_structure_signature` includes path sets | P4 T8 |
| §4.2 | Linux/JACK pops every route (`cfg`-guarded) | P4 T10 |
| §4.3 | `latency_samples()` default 0; oversampler, IR, limiter, VST3, LV2 | P3 T1, T2 (gated), T3, T3b, T4, T5 |
| §4.3 | Delay only the shorter path, 16384 cap, clamp + log at build | P3 T7, T9, T10, T16, T18 |
| §5.1 | Graph content: input → shared → split → lanes → mixer → shared → output; Y lanes end in own outputs | P5 T3, T8; P6 T6 |
| §5.1 | I/O nodes show checked names; click opens checklist | P6 T5, T15 |
| §5.1 | Split and mixer nodes open their editors (schema-rendered knobs) | P5 T8; P6 T14 |
| §5.1 | Block card parity with `BlockChip` | P5 T7, T9; P6 T9 (selection) |
| §5.1 | "+" on each edge and lane end → picker with position and path | P5 T5, T10; P6 T4, T11 |
| §5.1 | Picker "Split → Mix" / "Y → A/B", hidden when a split exists | P6 T12 |
| §5.1 | Drag within a lane, across lanes, shared ↔ path → `MoveBlock { position, path }` | P5 T6, T10; P6 T4, T11 |
| §5.1 | Wheel scrolls the list, Cmd/Ctrl + wheel zooms, pan by drag | P5 T2 (pan exists since #435) |
| §5.2 | `ChainStage::Parallel { lanes, end }` with `ParallelEnd::{Merge, Fan}` | P5 T3 |
| §5.2 | Node kinds drive the card | P5 T4, T8 |
| §5.2 | `add-requested`, `remove-requested`, `bypass-toggled`, `node-dropped` | P5 T9, T10 |
| §5.2 | `graph_view.slint` split into card / wire / canvas | P5 T1 |
| §5.2 | Accessible label with `@tr` | P5 T8 |
| §5.2 | Id-based addressing replaces `real_index` and the index mismatch | P6 T2, T3 |
| §5.2 | `chain_graph_adapter.rs`; meter poll reuses the models | P6 T6, T7 |
| §5.3 | Root-level overlay, every endpoint listed and checked by default, toggle dispatches `SetChainEndpointEnabled` | P6 T5 (rows from `endpoint_candidates`), T15 |
| §5.3 | Unchecking every input/output of a node is allowed; no segment built | P4 T3, T9 |
| §5.4 | Graph replaces `ChainRowBlocks`; row height by lane count; `chain_row_*` tests unchanged | P6 T7, T8 |
| §5.4 | `app-window.slint` split first | P6 T1 |
| §5.4 | Compact/touch view: one "Split" chip opening the split editor | P6 T13 |
| §6 | Volume: split-free chains bit-identical, defaults at unity | All parts; P1 T12, P3 T14/T21 check the pin is untouched |
| §6 | Stream isolation (mix inside one segment; Y outputs separate segments) | P3 T14, P4 T6, T10 |
| §6 | Latency unchanged; CPU measured on the battery | P3 T7, T10, T20 |
| §6 | Mode II and 1-channel pan documented | P3 T14 (docs/blocks-catalog.md) |
| §7 | Model / YAML / Commands / Engine math / Engine structure / HW battery / GraphView model / Slint | P1 T2–T11 · P1 T3 · P2 T1–T10b · P3 T6, T11, T14 · P4 T4, T6 · P3 T20 · P5 T3, T4, T6 · P5 T9–T11, P6 T8, T9, T11, T14, T15 |
| §8 | `blocks-catalog.md`, `audio-config.md`, `screens.md`, `gui/graph-view.md`, `mcp.md`, README ×3, nine catalogs | P1 T1/T4/T11, P2 (mcp.md, midi.md), P2 T7b, P3 T14, P4 T2–T10, P5 T11, P6 T9/T12/T14/T15 |

| Owner decision | Part / Task |
|---|---|
| 1. Split anywhere; pre/post blocks shared | P1 T2, T4; P3 T14; P6 T6 |
| 2. One split per chain | P1 T4; P2 T3, T4, T6, T8; P6 T12 |
| 3. Mixer: level, pan, B polarity, master, master sum | P1 T1; P3 T6, T8 |
| 4. Y: each path ends at its own output node | P4 T4–T6; P5 T3 (Fan); P6 T6 |
| 5. Paths time-aligned before the sum | P3 T1–T5, T7, T9, T10, T16, T18 |
| 6. Graph replaces the strip; wheel scrolls, Cmd/Ctrl + wheel zooms | P5 T2; P6 T8 |
| 7. Drag between path A and path B | P2 T5; P5 T6, T10; P6 T4, T11 |
| 8. macOS, Windows, Linux/JACK | P4 T10 (`cfg`-guarded); P6 T16 item 11 (owner check on Linux/JACK) |
| 9. Checklist: every endpoint, checked by default, uncheck = that node only, stays listed | P1 T8, T11; P2 T7, T7b; P4 T2, T3, T9; P6 T5, T15 |

## Gaps fixed by the completeness review

1. **Commit trailer unified.** Parts 2, 3 and 4 added `Co-Authored-By`; Parts 1, 5 and 6 did not. The project rule (no trailer) now applies to all six parts.
2. **Part 3 pushed without the gate.** It ran the full gate only in its last task and posted no per-push comment. Every "Commit and push" step now runs the gate, pushes to `origin feature/issue-328` and posts `gh issue comment 328`.
3. **Part 2 imported `PathRef`/`PathSide` from the wrong module.** Part 1 puts them in `block/path_ref.rs`, so `project::block::split_block::{PathRef, PathSide}` would not compile. All imports and the pre-flight grep now use `project::block::{PathRef, PathSide}`.
4. **Part 2 Task 7 duplicated Part 1.**
   - It edited `rig_sync.rs` again (Part 1 Task 10 already captures the checklist), and that Edit anchor would no longer match.
   - It re-implemented `EndpointDisables::set_enabled`. It now calls Part 1's method.
   - The two capture tests are now labelled characterization pins, and the expected red is corrected.
5. **Part 2 Task 10 duplicated Part 1 Task 5** (`merge_preserved_ports`). Its red could never fail and its Edit anchor was already gone. It is now an end-to-end pin with no production edit.
6. **New Part 2 Task 7b: spec §1.3, "dropped on the next save".** No part called `retain_known`. The new task adds `project::endpoint_prune::prune_stale_endpoint_disables`, run by `CaptureRigEdits` when an E/S registry is attached. It is red-first and updates the docs.
7. **New Part 2 Task 10b: a model swap inside a path.** `mirror_model_swap_into_rig` searched the top level only, so a swapped path block lost the active scene's values. It now uses Part 1's recursive `find_block_mut`, red-first against the #986 fixture.
8. **Part 4 engine `lib.rs` budget.** Part 3 takes the file to 99 lines, so Part 4's `pub mod segment_paths;` also needs the `#[path]` fallback. The fallback now lives in `runtime_graph.rs`, with every path it rewrites listed.
9. **Stale text updated.**
   - Part 4's `EndpointRef` pre-flight now reflects that Part 1 defines `io`.
   - Part 2's references to "Part 4 (GUI)" now point to Part 6.
   - Part 2's known-gaps list marks the two items that are now closed.
   - Part 1's hand-off has a status note.
10. **Owner checklist gains item 11**, Linux/JACK path B audible (owner decision 8). Nothing local plays that code.

## Orchestrator decisions (2026-09-29) — these OVERRIDE the part files where they disagree

1. **Oversampler (P3 T0 Step 5, T2).** Do not ask the owner and do not edit the existing pin `latency_is_constant`. Keep `Oversampler2x::latency_samples()` as it is; add `Oversampler2x::round_trip_latency_samples()` returning the measured whole-sample round trip (15), with its own red-first test. The ring modulator (and any oversampled block) reports the round-trip value through `latency_samples()`. T2 is no longer gated.
2. **Old highlight tests (P6 T2).** Owner approved (2026-09-29): P6 T2 may rewrite the 3 highlight tests and delete the 3 `lib_tests.rs` tests that pin the old flat index mapping. No other existing test may be rewritten or deleted.
3. **Split bypass / lane drags / path adds (open items 3 and 4).** `is_routing()` is true only for a `Split` with `end: Y` (spec §1.1 updated). A `Split → Mix`'s `enabled` flag and path contents stay OUT of `chain_structure_signature`; only a Y's per-output path sets go in. Mix-split bypass, lane drags and path edits are in-place DSP rebuilds with zero stream reopen. Pin it with a test: toggling or editing a Mix split leaves `chain_structure_signature` unchanged. The Mix split's bypass goes through the fade path (`FadeState`), passing the pre-split bus through untouched.
4. **JACK (P4 T10, open item 5).** No summing of routes in our code. Each output route gets its own JACK output ports and is connected to its playback ports; when two routes land on the same playback port, JACK sums them (backend mixing, allowed by the isolation law). If this cannot be done inside P4 T10, stop and report instead of summing.
5. **Audio-thread allocation (open item 6).** Path B's buffer and every split scratch are preallocated at build to the engine's maximum callback size. A callback larger than the capacity is processed in capacity-sized chunks. Zero allocation, no exception. Pin with an alloc-invariant test that feeds a callback larger than the capacity.
6. **Split rules in one place (open item 7).** Part 2's command doors call Part 1's rule check (`validate_split_layout` / `validate_structure`); no second copy of the rules and one set of error texts.
7. **MIDI block cursor and index commands (open item 8).** Block navigation walks the flattened order: pre-split blocks, path A, path B, post-mix blocks. Selection from the graph uses the block id. Add a red-first test in P6 for next/previous across the split.
8. **Picker inside a path (open item 9).** Input, Output and Insert are filtered out of the add-block picker when the target is a path; fix the row→type mapping so filtering is possible. Red-first test in P6 T12.
9. **Mix ↔ Y in the GUI (open item 10).** The split editor gets a Mix / Y switch that dispatches `SetSplitEnd`; refused switches (post-mix blocks exist) show the command's error as a toast. Interaction test in P6 T14.
10. **`ConfigureChain` (open item 12).** Preserves `disabled_endpoints` (red-first test in P2). Loopers stay out of scope.
11. **Open items 11, 13, 14** are accepted as written: `ChannelPicker` checklist and hit-zone sizes go to the owner's visual checklist; split knob edits rebuild the split node off the audio thread with processors reused; plugin latency is covered by fake-processor unit tests, and the VST3/LV2 end-to-end reds run where such a plugin is installed.
12. **Upstream.** Before every push: `git fetch`, then merge whichever of `origin/release/v0.6.0` and `origin/develop` has commits the branch lacks, re-run the gate, then push.

## Open items (need the owner or a later decision; not fixed here)

1. **Part 3 Task 2, oversampler 7 → 15 samples.** It edits an existing test. The owner answers at Task 0 Step 5.
2. **Part 6 Task 2 rewrites 3 existing highlight tests and deletes 3 in `lib_tests.rs`.** The spec calls the old mapping a defect, but CLAUDE.md forbids rewriting tests to fit. The owner must confirm.
3. **Split bypass costs a full stream rebuild.** A `Split` is routing, so `chain_structure_signature` (`infra-cpal/src/io_topology.rs:108-109`) includes its `enabled` flag. Bypassing the split would cost the #967-class 2–3 s silence. Spec §1.1 requires `is_routing() == true`; the owner decides whether the split's flag should leave the signature, as the insert's did.
4. **Lane drags and path adds on a Split → Mix also rebuild the streams.** The split's `model_identity()` sits in the signature. This matches today's behaviour for top-level reorders; Part 3 Task 15's processor reuse applies only to DSP rebuilds.
5. **Part 4 Task 10 (JACK) sums the routes of one runtime that share channels, in our code.** The owner should confirm this reading of the isolation law.
6. **Part 3 audio-thread allocation.** Path B's buffer grows once on the audio thread if a device delivers more than 1024 frames. Today's segment buffer does the same, but invariant 8 says "no exception".
7. **The split rules are stated twice.** `ensure_split_rules` (P2) and `validate_split_layout` / `validate_structure` (P1) enforce the same rules with different error texts, so they can drift.
8. **The MIDI block cursor and flat-index commands** (`SelectChainBlock`, `SaveChainInputEndpoints { block_index }`) do not reach blocks inside a path.
9. **The block picker inside a path still lists Input, Output and Insert.** Part 2 refuses them with a toast.
10. **No GUI control for Mix ↔ Y (`SetSplitEnd`).** It is MCP-only; the spec does not require a GUI control.
11. **Visual review.**
    - The checklist uses `ChannelPicker`, not the preset-derived `Select`.
    - LED and × hit zones are under 44 px.
12. **`ConfigureChain` (MCP) resets the checklist**, as it already resets loopers.
13. **Split knob edits rebuild the split node off the audio thread** (processors reused, alignment history kept) instead of updating atomics in place. There is no amp reload and no gap, but it departs from the spec's wording.
14. **The VST3/LV2 latency end-to-end reds need a plugin that declares latency** installed on the machine.
