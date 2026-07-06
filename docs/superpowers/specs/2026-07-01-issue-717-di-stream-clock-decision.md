# #717 spike decision — how the dedicated DI stream is clocked

Resolves the open question in the design spec (`2026-07-01-issue-717-di-dedicated-stream-design.md`). Grounded in a read of `crates/infra-cpal/src/{stream_builder,controller,live_runtime,active_runtime,slot_processing,io_topology}.rs` and `crates/engine/src/runtime.rs`.

## Current model (why there is no input-less path today)

Processing is driven by the **input device's cpal callback**; the **output
device's callback** independently drains it. They are decoupled by a per-route
**SPSC ring** (elastic-sized, `elastic::compute_elastic_targets_for_chain`) —
two device clocks, no single "runtime clock".

- Producer: input callback → `process_input_buffer` → `process_input_f32`,
  which pushes mixed frames into each output route's SPSC ring
  (`runtime.rs:264-270`). On macOS (#670) the heavy graph DSP runs in a
  dedicated per-stream `dsp_worker` (`stream_builder.rs:175`), NOT inline in the
  audio callback.
- Consumer: output callback → `process_output_buffer` →
  `process_output_f32_mixed` pops the ring and sums each isolated runtime the
  output serves (`slots_for_output_stream`, same-rate only) — the one legal
  cross-stream mix point (invariant #4), then the limiter.

The existing DI loop (#614) and the latency beep (#723) both generate audio
*inside* `process_input_f32`, clocked by the guitar's input callback and riding
the guitar runtime/stream/meters. That is the #717 complaint.

## Decision: Candidate B — dedicated worker-clocked DI runtime, drained by the chosen output

Replace the input-DEVICE producer with a **worker producer**, reuse the rest of
the pipeline unchanged:

1. **Isolated DI runtime.** Build a separate `ChainRuntimeState` for the chain
   via `build_chain_runtime_state(&chain_copy, output_rate, &[buf], &registry)`
   from a **copy** of the chain's blocks + the chosen output binding. Feed it the
   loop with `set_di_loop(Some(pcm.to_loop_at(output_rate)))` (reuses #749) and
   the existing `SegmentFeed::Loop` path — no engine change.
2. **Producer clock.** A self-clocked worker modeled on `dsp_worker::spawn`
   steps the DI runtime and fills its SPSC route ring. This is the only
   genuinely new surface.
3. **Consumer.** The chosen output device's existing output stream drains the DI
   runtime as one more isolated slot (`slots_for_output_stream` /
   `process_output_f32_mixed`) — backend mix, zero changes to the drain path.
4. **Lifecycle.** `arm_di_stream(chain, pcm)` builds+starts the worker and
   registers the slot; `disarm_di_stream(chain)` stops the worker and unregisters.
   It **must not touch the guitar runtime** (it replaces the guitar-injection
   `arm_di_loop_per_output_stream`). Store `(slot, worker handle)` in a controller
   `di_streams: HashMap<ChainId, …>`; `di_stream_active` reads it.

**Why B over A.** Candidate A (produce inside the output callback) would run NAM
inline in an audio callback — the exact #670 anti-pattern `dsp_worker` exists to
avoid — unless the DI graph is pre-rendered off-thread, which loses live
editing. B keeps everything RT-safe (#8), fully isolated (#4), and live-editable
by reusing the worker pattern the codebase already trusts.

**Fallback A** (pre-render the block-copy off-thread, read a cursor in the output
callback as an isolated slot) is smaller and needs no new thread, but the DI
graph is not live-editable — revisit only if live-edit of the DI path is dropped.

## API this fixes (consumed by plan Tasks 4–7)

- `ProjectRuntimeController::arm_di_stream(&self, chain_id: &ChainId, pcm: Arc<DiPcm>)`
- `ProjectRuntimeController::disarm_di_stream(&self, chain_id: &ChainId)`
- `ProjectRuntimeController::di_stream_active(&self, chain_id: &ChainId) -> bool`

(Output selection — plan Task 4 — extends `arm_di_stream` to resolve the chain's
`di_output` to the chosen output binding + rate.)
