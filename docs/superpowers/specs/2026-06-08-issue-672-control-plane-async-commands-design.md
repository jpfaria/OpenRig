# Issue #672 — Async control plane: heavy commands must not freeze the UI

## Problem

The control plane is single-threaded. `LocalDispatcher` is `!Send` and lives on the
frontend thread; every state-changing `Command` (GUI callback or MCP, the latter via
`application::bridge`) is drained per-tick and applied **inline on the frontend thread**
(`docs/mcp.md:135-139`).

Heavy commands run synchronously there. Activating a chain hits `upsert_chain` — a full
CPAL stream rebuild (build + teardown + runtime-graph allocation, `infra-cpal`). While
that runs, the Slint event loop is blocked and the whole app freezes. The block has two
expensive parts: (1) building the new runtime/streams, (2) **dropping the old runtime**
(stopping CPAL streams can join a backend thread).

This is **not** an audio-thread problem. The per-chain CPAL streams are already isolated
parallel RT threads (invariant #4) and keep playing during the freeze. The defect is the
control plane doing heavy work on the UI thread.

## Goal

A heavy command never blocks the frontend thread. The UI stays responsive while a chain
rebuild happens off-thread; the new runtime goes live via a cheap, lock-free hand-off.

Non-goal: making the audio path "concurrent". It stays dedicated RT.

## Architecture — two planes joined by lock-free hand-off

```
frontend thread (!Send)         control worker thread            audio RT threads (per chain)
─────────────────────────       ──────────────────────          ───────────────────────────
Command arrives ──► classify
   cheap ─────────────────────────────────────────────────────► apply inline (as today)
   heavy ─► build a Send BuildRequest
            enqueue ───────────► build new RuntimeHandle
                                  (alloc, CPAL streams)
                                  publish via atomic swap ──────► next callback reads new
                                  drop OLD RuntimeHandle here
            ◄── completion (bridge) ──┘
   update UI state (live/pending)
```

Three responsibilities, three owners:

1. **Audio plane (unchanged).** Dedicated RT threads, one isolated stream per chain.
   Never lock/alloc/drop. Reads the live runtime through a single atomic pointer load.
   Invariants 6/7/8 forbid moving this into any pool/async runtime.

2. **Control worker (new).** A single dedicated thread that owns all heavy runtime
   construction and destruction: build new CPAL streams + runtime graph, publish, and
   **drop the previous runtime on this thread**. Single thread (not a pool) so rebuilds
   serialize and we can coalesce stale rebuild requests for the same chain.

3. **Frontend thread (slimmed).** Keeps the `!Send` dispatcher as the single writer of
   `State`. For a heavy command it only computes a cheap `Send` `BuildRequest` and
   enqueues it; it never builds or drops a runtime. On worker completion (delivered
   through the existing `application::bridge` channel) it does a cheap UI-state update.

### Hand-off mechanism

- Live runtime per chain is held behind an atomic pointer the audio callback loads once
  per buffer (RCU / `arc-swap`-style swap, or a triple-buffer). The swap itself is
  wait-free for the reader; zero lock on the audio thread.
- The worker builds the new `RuntimeHandle` fully, then swaps the pointer. The audio
  thread observes the new runtime on its next callback with no torn state.
- The **old** `RuntimeHandle` is moved to and dropped on the worker thread after the swap,
  never on the audio or frontend thread.

### Why `!Send` is preserved

Only a `Send` `BuildRequest` (the data needed to construct a chain runtime — device
settings, block specs, IO) crosses to the worker. `State` and the dispatcher stay on the
frontend thread as the single writer. We move the expensive *build/drop*, not the state.

## Components / files (to confirm against code during implementation)

- `infra-cpal` — `ProjectRuntimeController::upsert_chain`: split into
  `build_chain_runtime(BuildRequest) -> RuntimeHandle` (pure, worker-runnable) and a cheap
  `publish(chain_id, RuntimeHandle)` (atomic swap + queue old for drop).
- New control-worker module (likely in `infra-cpal` or `application`): owns the worker
  thread, an inbound `Send` queue of `BuildRequest`, per-chain coalescing, and the drop
  graveyard.
- `application::bridge` — reuse for worker→frontend completion notifications.
- Command classification: a single source of truth marking which `Command` variants are
  "heavy" (trigger a rebuild) vs "cheap" (apply inline). Cheap path stays exactly as today,
  including the #522 toggle fast-path.

## Data flow (chain activate)

1. `Command::ToggleChainEnabled` (or equivalent) reaches the dispatcher on the frontend.
2. Classified heavy → frontend builds a `Send` `BuildRequest` from `State` (cheap), marks
   the chain `pending` in UI, enqueues to the worker. Returns immediately — UI not blocked.
3. Worker builds the new `RuntimeHandle`, swaps the live pointer, drops the old handle.
4. Worker posts completion via the bridge; frontend clears `pending` → `live`.

## Error handling

- Build fails on the worker → it keeps the previous runtime live (no swap) and posts an
  error `SideEffect` to the frontend. The chain keeps playing its old runtime; UI shows the
  error. No partial/torn state ever reaches the audio thread.
- Worker thread panic → supervised restart; surface a user-visible error. Audio threads are
  independent and unaffected.

## Scope

**In:** the chain-rebuild path (`upsert_chain`) as the first vertical slice — it is the
proven freeze and exercises the full worker + atomic-swap + drop-on-worker machinery.
Establish the infrastructure here, then classify the remaining heavy commands (IO change,
block insert/replace, plugin load) onto it in follow-ups.

**Out:**
- Any change to the audio-path concurrency model (stays dedicated RT).
- Coalescing/queueing semantics beyond "latest rebuild per chain wins".
- Reworking the `Command`/`Event`/`SideEffect` contract beyond making heavy application
  asynchronous + adding a completion notification.

## Invariant guardrails (must not regress)

- #4 isolation: no shared buffer/lock/route/tap between streams; each chain swaps its own
  runtime pointer independently.
- #6/#7/#8: audio thread does one atomic load per buffer — zero added lock/alloc/syscall;
  no jitter regression.
- #1 latency, #3 stream stability (zero new xrun/dropout/click), #9 golden samples,
  #10 volume invariants — all unchanged.

## Testing (RED-first)

1. **RED test first** — a test that drives a heavy command and asserts the frontend thread
   is not blocked for the duration of the build (e.g. the dispatcher tick returns within a
   small budget while a slow `build_chain_runtime` runs). Must fail against `develop`.
2. Audio continuity across a swap: no dropout/xrun; samples continuous (golden tolerance).
3. Old runtime dropped off the audio/frontend thread (assert drop happens on the worker).
4. Build-failure path: old runtime stays live, error surfaced, no swap.
5. Coalescing: rapid repeated rebuilds for one chain collapse to the latest.
6. Existing `volume_invariants_tests.rs` and golden tests stay green.

## Open question for the build-failure UX

While a heavy rebuild is in flight, should the chain show a transient **pending/loading**
state in the UI, or stay visually unchanged until the swap lands? Affects only the UI
state machine, not the threading design.
