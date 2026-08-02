# Decouple the UI from the engine — one live-read provider, one resolver (#127, folds #831)

**Status:** design approved (2026-07-29)
**Issue:** #127 (supersedes #831)
**Related:** #42 (gRPC transport), #43 (Flutter remote), #829 (MCP parity), spec `2026-04-23-command-dispatch-architecture-design.md`

## Problem

The *write* axis is already decoupled: every state change is a `Command`, dispatched through the
`CommandDispatcher` trait, and `adapter-mcp` (#829) proves a second frontend can drive the app
without touching UI code.

The *read* axis is not:

1. **The GUI owns concrete types.** `crates/adapter-gui/src/state.rs` holds `Rc<LocalDispatcher>`,
   and wiring functions take `&application::local_dispatcher::LocalDispatcher` in their signature
   (e.g. `di_loop_wiring.rs`). Nothing in the UI can be pointed at a different implementation.
2. **`infra_cpal` leaks into the UI.** `ProjectRuntimeController` appears in 46 non-test files of
   `adapter-gui` (135 occurrences; ~45 real calls over ~20 methods: `poll_stream`,
   `subscribe_*_tap`, `sample_rate`, `stream_count`, `set_output_muted`, `set_io_bindings`,
   `set_block_enabled`, `upsert_chain` / `remove_chain` / `sync_project`, …). The UI depends on the
   audio backend directly, so a UI running anywhere but this process is impossible.
3. **Every frontend re-implements the read bus** (#831). `adapter-gui/src/mcp_query_resolver.rs`
   and `adapter-console/src/main.rs` each carry a full `QueryKind` match, each decides the payload
   shape, and they have already drifted — the console writes a literal `-120.0` for a silent meter
   while the GUI uses `engine::output_meter::SILENT_DBFS`.

(1)–(3) are one gap: there is no single abstraction for "live state a frontend hosts", so the UI
reaches into the backend and each transport re-derives the answers.

## Goals

- No concrete dispatcher or `infra_cpal` type in UI wiring signatures.
- One place in the core that turns a `QueryKind` into its payload, for every transport.
- A frontend supplies only what genuinely needs its live state, through one narrow trait.
- Zero behavior change on the desktop: same numbers on screen, same latency, same audio path.

## Non-goals

- gRPC / remote transport itself — that stays in #42. This design only makes it possible.
- Any change to the audio thread, DSP, routing, or stream isolation.
- UI/UX changes.

## Architecture

```
Slint UI ──► dyn CommandDispatcher ──► Command bus ──► engine
   ▲                                       │
   └── read ── application::read::resolve(QueryKind, ReadContext) ◄── dyn LiveSource
                                                                       (GUI: runtime taps,
                                                                        console: nothing)
```

### 1. `dyn CommandDispatcher` in the GUI

`ProjectSession.dispatcher` becomes `Rc<dyn CommandDispatcher>`; wiring functions take
`&dyn CommandDispatcher`. Two reads currently only `LocalDispatcher` offers — `engine_sr()` and
`tone_report_json()` — move onto the `CommandDispatcher` trait (defaults preserve today's answers
for implementations that hold no such state).

### 2. `LiveSource` — the one live-read trait

New in `application`. It returns domain types; **serialization stays in the core**, so no adapter
can invent a shape. Every method defaults to `None`, meaning "this frontend hosts no such source":

```rust
pub trait LiveSource {
    fn chain_meters(&self) -> Option<Vec<ChainMeterReading>> { None }
    fn chain_health(&self, chain: &ChainId) -> Option<ChainHealth> { None }
    fn tuner(&self) -> Option<TunerReadings> { None }
    fn spectrum(&self) -> Option<SpectrumReadings> { None }
    fn di_loop(&self) -> Option<DiLoopState> { None }
    fn chain_loopers(&self, chain: &ChainId) -> Option<LooperSnapshot> { None }
    fn devices(&self) -> Option<Vec<AudioDeviceDescriptor>> { None }
    fn sample_rate(&self) -> Option<u32> { None }
}
```

Prior art in this repo: `LocalDispatcher::attach_tone_doctor_input` already injects a live provider
into the core instead of letting the adapter answer.

**PCM never crosses the trait.** Taps stay where the samples are; `LiveSource` exposes readings that
are already reduced (dBFS, note/cents, band levels) — the same shape #829 established, and the only
shape a remote transport can carry.

**Meter parity.** The GUI implementation reports the values from the last meter tick (the ones the
IN/OUT bars are drawing), not a second poll of the taps. Screen and transport therefore cannot
disagree, and no extra work lands on the audio path.

### 3. One resolver in the core

```rust
pub struct ReadContext<'a> {
    pub project: &'a Project,
    pub rig: Option<&'a RigProject>,
    pub io_bindings: &'a [IoBinding],
    pub dispatcher: &'a dyn CommandDispatcher,
    pub live: &'a dyn LiveSource,
}

pub fn resolve(kind: &QueryKind, ctx: &ReadContext<'_>) -> Result<String, String>;
```

The match lives here once, exhaustive by compilation. `adapter-gui` and `adapter-console` delete
their copies and call `resolve`. A frontend that lacks a source no longer enumerates kinds: the
resolver emits the documented empty shape (silent meters use `engine::output_meter::SILENT_DBFS`,
never a literal), so every resource stays addressable with the same JSON on every transport.

### 4. Runtime control becomes `Command`s

The control calls the UI still makes on `ProjectRuntimeController` (`set_block_enabled`,
`set_output_muted`, `set_io_bindings`, `upsert_chain` / `remove_chain` / `sync_project`,
`schedule_chain_activation`) become `Command` variants applied by the dispatcher. They are already
concentrated in `runtime_lifecycle.rs` and `project_ops::sync_project_dirty`, which keeps this
contained. After this step `ProjectRuntimeController` appears only in `runtime_lifecycle.rs` (which
owns the runtime) and in the GUI's `LiveSource` implementation — never in a wiring signature.

## Phases

1. `dyn CommandDispatcher` through the GUI; dispatcher-state reads onto the trait.
2. `LiveSource` + `GuiLiveSource` + `read::resolve`; both adapters call it; duplicated matches
   deleted. **Closes #831.**
3. Remaining runtime control → `Command`s; `infra_cpal` out of wiring signatures.
4. Docs: `docs/architecture.md` (the read bus), `docs/mcp.md` (one shape per resource).

Each phase is independently green and pushable.

## Testing

TDD red-first, as the repo requires — the first commit is the failing parity test.

- **Transport parity (the #831 verification):** drive every `QueryKind` through `resolve` twice —
  once with a `LiveSource` that hosts nothing (console-shaped), once with a fake that hosts
  everything — and assert both return the same shape, differing only in live values. A new variant
  cannot be answered by one transport and refused by another.
- **No literals:** silent meters assert against `engine::output_meter::SILENT_DBFS`.
- **Meter parity:** the value `LiveSource::chain_meters` reports equals the value written to the
  chain row in the same tick.
- Existing GUI wiring tests keep passing against `dyn CommandDispatcher` (they already build a
  `LocalDispatcher`), which is the regression net for "no behavior change".
- Real-hardware battery (`OPENRIG_HW_TESTS=1`) before the final push: latency, xruns and stream
  isolation unchanged.

## Risks

- **Dynamic dispatch on a poll path.** Reads happen at UI tick rate, never on the audio thread; the
  audio path is untouched. If any read shows up in a profile, it is cacheable per tick.
- **Phase 3 touches many files.** Mechanical, but it is where a behavior regression could hide —
  hence the wiring tests and the hardware battery before the final push.
- **`LiveSource` growing into a god-trait.** It only carries reads a frontend genuinely hosts;
  anything derivable from the project or dispatcher state stays a pure helper in `application`.
