# Audio configuration

## Stream model (CLAUDE.md invariants 4 / 5 / 10)

### Stream rules

1. **The internal bus is ALWAYS stereo.** A mono input becomes `Stereo([s, s])`
   right at the start through `to_stereo` (broadcast). There is no point
   inside the chain where the signal travels as mono on the bus.
2. **Each block declares a `ModelAudioMode`**, the layout it can process
   (`MonoOnly` / `DualMono` / `TrueStereo` / `MonoToStereo`).
3. **The block's wrapper inserts a conversion only when the block needs
   another layout.** A stereo bus + a block that takes stereo → no
   conversion.
4. **Output mode `mono`** is the only case that collapses back to one
   channel, through `mixdown(L, R)`. A stereo output passes straight through.

### Wrapper table

`dual_mono` and `stereo` inputs enter as stereo (no conversion). A `mono`
input does `to_stereo` (broadcast L=R=s) at the start and the bus stays
stereo from there on. Each block asks for the layout it can process, and
the wrapper inserts a conversion only when the stereo bus does not match
what the block accepts.

| `ModelAudioMode` | Wrapper before | Wrapper after | Block behaviour |
|---|---|---|---|
| `MonoOnly` | `to_mono` (mixdown L+R) | `to_stereo` (broadcast) | one instance processes the collapsed sample |
| `DualMono` | `to_dual_mono` (two independent monos) | `to_stereo` (a stereo pair) | two parallel instances, one per channel — independent `[L,R]` in, `[L_p, R_p]` out |
| `TrueStereo` | straight through (the bus is already stereo) | straight through | one instance sees a correlated `[L, R]` and processes stereo |
| `MonoToStereo` | (gets stereo; `to_mono` if the implementation needs a mono source) | straight through | the block returns stereo |

The wrapper per `ModelAudioMode` is chosen in `crates/engine/src/runtime_processor_model.rs`.

### Canonical pipeline

> **Model A:** an `InputBlock`/`OutputBlock` is a `{ model, io, endpoint }`
> reference to an I/O binding — it carries **no** device/channels. At activation
> `engine::runtime_endpoints::resolve_chain_io(chain, registry)` resolves each
> port to its concrete device endpoint (`device_id`/`mode`/`channels`) from the
> per-machine binding registry (`config.yaml` `io_bindings`). The pipeline below
> shows the resolved endpoint feeding the (unchanged) stereo bus.

```
Hardware → [InputBlock io/endpoint] --resolve_chain_io--> { device_id, mode, channels }
        ↓
  initial bus:
    mode mono      → Mono → to_stereo (broadcast L=R=s) → Stereo
    mode dual_mono → Stereo (meaning: two independent monos)
    mode stereo    → Stereo (meaning: a stereo pair)
        ↓
  for each block in the chain:
    [wrapper before] → adapts the bus to the layout the block accepts
    the block processes
    [wrapper after]  → back to the stereo bus
        ↓
  OutputBlock { device_id, mode, channels }
    mode stereo, ch [a, b]  → ch_a = L, ch_b = R
    mode mono,   ch [a]     → mixdown(L, R) → s, escreve ch_a
    mode mono,   ch [a, b…] → mixdown(L, R) → s, copied to ALL of them

  Mixdown:
    Average → (L + R) * 0.5  (default)
    Sum     →  L + R
    Left    →  L
    Right   →  R
```

> **Device opens at its NATIVE channel count.** When `mode = mono`
> with `channels = [a]` selects a single physical output on a hardware-stereo
> interface (Scarlett 2i2 etc.), the CPAL stream is still opened at the
> device's `default_output_config().channels()`. Opening such a device as
> 1-channel mono silences it on macOS / CoreAudio — the routing is the
> engine's job (`write_output_frame` writes the mixdown into `ch_a` of the
> interleaved buffer; other channels stay at zero).

### Examples

**1. Mono in + MonoOnly block + stereo out**
```
HW Mono(ch0) → to_stereo → [s,s] → to_mono → block_mono(s)→m → to_stereo
            → [m,m] → HW(ch0=m, ch1=m)
```

**2. Stereo in + DualMono block + stereo out**
```
HW Stereo(ch0,ch1) → [L,R] → to_dual_mono → block_dm(2 instances):
       L→L_p, R→R_p → to_stereo → [L_p,R_p] → HW(ch0=L_p, ch1=R_p)
```

**3. Mono in + TrueStereo block (chorus) + stereo out**
```
HW Mono(ch0) → to_stereo → [s,s] → block_ts(L=R=s)→[L',R']
            → HW(ch0=L', ch1=R')
```

**4. Mono in + MonoOnly + TrueStereo + stereo out**
```
HW Mono(ch0) → to_stereo → [s,s] → to_mono → block_mono→m → to_stereo
            → [m,m] → block_ts→[L',R'] → HW(ch0=L', ch1=R')
```

**5. Mono in + MonoOnly block + mono out**
```
HW Mono(ch0) → to_stereo → [s,s] → to_mono → block_mono→m → to_stereo
            → [m,m] → mixdown=m → HW(ch0=m)
```

**6. Mono in + TrueStereo block (fil4, a stereo LV2) + mono out**

When every output of a mono input is mono, the engine runs the segment on a
mono bus (`project::chain::processing_layout`: mono blocks skip the
mono→stereo→mono round-trip). A true-stereo block on that bus gets the bus
broadcast to both of its inputs (`AudioProcessor::StereoFromMono`) and the bus
is stereo from there on; the mono output takes the mixdown at the end. The rule
lives in `project::chain::bus_layout_after`, shared by the engine and
`validate_project`.
```
HW Mono(ch0) → m → [m,m] → block_ts→[L',R'] → mixdown → HW(ch0)
```

### Parallel streams

- Each InputBlock = one FULLY isolated parallel stream (its own runtime,
  no shared buffer / lock / route / tap).
- Several InputBlocks / OutputBlocks → summing is the backend's job
  (cpal / JACK). The engine NEVER mixes streams together.
- A solo input passes at unity in any combination (input mode × output
  mode), pinned in `crates/engine/src/volume_invariants_tests.rs`.

### Isolation is by stream identity

- Every selection or grouping of runtimes on the I/O path (`slots_for_*`,
  output routing, taps, DI, meters) uses the identity of the stream/device —
  never the sample rate, never "every runtime that matches". The same rate on
  two devices cross-mixes, and a mis-resolved rate groups the wrong runtimes
  (underrun, cross-talk). An output serves only its own stream's runtime, a
  tap reads only its own, a rebuild rebuilds only its own.
- Isolation includes CPU time. A playing stream must not raise another
  stream's latency: a loop/DI pipeline in the same RT time-constraint class as
  a live chain's audio callback couples the two through the clock, even with
  separate buffers. The fix is separating priority/class, never "make the
  other one spend less CPU" — that hides the coupling until N streams bring
  it back.
- Isolation includes the device's own properties. The buffer size is a
  device-level property on CoreAudio: a stream that opens a device with a
  buffer size other than the project's re-sizes it under every stream already
  running there (underrun bursts on the live chain, and a HAL deadlock when a
  stream starts while an input callback is reallocating for the new size).
  So a chain's streams and an isolated loop/DI playback resolve their device
  through the same resolver, with the project's `device_settings` for that
  device.

### Why these rules (invariants 4 / 5 / 10)

- **4 — Isolation between streams.** Each InputBlock has its own
  runtime, buffer and state. Touching one does not affect another. The
  final mix is the audio driver's job.
- **5 — Stereo bus inside.** A mono input becomes `Stereo([s, s])` from
  the first block on. Blocks always see `[L, R]`. How to leave (mono /
  stereo) is the OutputBlock's decision alone.
- **10 — Per-stream volume is IMMUTABLE.** Nothing in the engine
  attenuates the signal pre-emptively "to avoid clipping". The output
  limiter (`tanh` at the end) handles that. A solo input passes at unity
  in any input mode × output mode combination (pinned by
  `volume_invariants_tests.rs`).

### Split-mono fan-out

A special case: `mode: mono` with **more than one channel** in `channels`
(`channels: [0, 1]`). The engine creates **one sibling stream per
channel** — each runs the whole chain in parallel, reading a different
physical channel. Useful for two guitars on the same interface with the
same preset, without duplicating the chain.

For **a single mono source** (one guitar), use `channels: [N]` only (N =
the physical channel the source comes in on). Several channels turn the
fan-out on, which is probably not what you want.

Pinned acceptance (`volume_splitmono_preset_tests.rs`, g01..g04):

- `solo` (signal only on ch0, ch1 silent) → output peak = signal peak (UNITY).
- `dual` below the limiter knee (signal on ch0 + ch1) → a plain sum.
- `dual` above the knee → `tanh(sum)`.
- the mono → stereo bus broadcast is symmetric (L = R).

`split_mono_sibling_count` is structural metadata; its scale multiplier
**MUST stay at 1.0** until an opt-in auto-mix feature exists with the
owner's explicit approval.

### Virtual DI loop (per-chain, ephemeral)

A chain's DI loop plays on its **own isolated, streamed runtime** — it never replaces or rides the guitar's live input. Arming resolves the chain's persisted output choice (`Chain.di_output` → one of its bound output endpoints; absent → the main output), builds a fresh copy of the chain's block graph REDUCED to that output's binding (so the loop feeds the route the chosen output drains) on a `di-stream` worker, and parks a ring-backed playback on that output's cell immediately — a 75 s loop starts in milliseconds. The worker steps the runtime paced by **ring backpressure**: it only produces what the output callback consumed, so the output device clock IS the DI clock — no drift by construction — and the callback only pops frames and sums (zero allocation/locks/DSP, invariant #8). Decoding, resampling (per-output rate) and all block DSP happen off the audio thread. The guitar runtime, its meters, and every other output are untouched (isolation invariant #4); a device-rate change re-arms at the new rate. A live edit (a param change or a block toggle) re-renders the DI **gaplessly**: the playback that is sounding keeps playing while the new render is built and pre-rolled off-thread, and the incoming worker takes the output's cell over mid-loop — at exactly the loop position the listener reaches (`DiPlayback::play_pos`, `set_di_loop_pos`) — so the edit lands with neither a silent gap nor a restart of the take. The outgoing playback is retired off the audio thread, never dropped by the callback (invariant #8). **The render never shares the callbacks' scheduling class.** In the Mach time-constraint policy the chain's audio thread runs in, the two would compete for the same slots and every playing loop would raise the LIVE stream's latency. The render runs one precedence step below the default, outside the real-time class entirely; the ring's backpressure and the ~100 ms pre-buffer absorb the preemption that costs. This is the isolation LAW applied to CPU time — the pipelines do not compete, rather than the render being made cheaper. The **source** choice stays runtime-only; only the **output** choice persists, inside the chain in `project.yaml` (ADR 0003).  **Insert blocks are bypassed on this runtime.** An Insert splits a chain into segments whose tail is fed by the external gear's RETURN, and this runtime is stepped by an output-only worker that feeds segment 0 alone — a bound insert would leave every later segment with no input and the playback would reach the output as silence. The isolated copy disables them, the same way an insert whose E/S does not resolve is bypassed rather than allowed to silence the chain; the send/return itself stays live on the guitar runtime, untouched. See the **Virtual DI loop** entry under **Chains** in `docs/screens.md` for UI details.

### Per-chain looper

Each `ChainRuntimeState` owns a `LooperBank` — up to 8 loopers, each up to 60 s of stereo at that runtime's **live** sample rate. The bank sits inside `ChainProcessingState` (the audio thread already holds `&mut` to it) and is driven by a lock-free op queue, the same pattern as the block-toggle fast path: the control thread never takes the `processing` lock, it pushes a `LooperOp` and the audio thread drains it inside the section it already owns.

**Where it sits in the signal path.** The loopers run at the chain input, on the chain's FIRST segment only (a chain's loop material is heard exactly once, no matter how many segments share the callback). They record the dry frame that feeds the segment — after the DI substitution, so recording works while monitoring a DI — and SUM their playback into it, unlike the DI loop which REPLACES the frame. The loop therefore runs through the entire block graph and follows every live edit.

**Memory and the RT contract.** Layer buffers are allocated by the control thread and handed to the audio thread inside the op; buffers a looper is done with (cleared, undone past the ring, refused) go back through a return queue and are dropped on the GUI tick. The audio thread never allocates, locks or frees (invariant #8) — pinned by `looper_record_overdub_and_undo_do_not_allocate` in `audio_alloc_invariant_tests`. Playback sums the audible layers on read (one multiply-add per layer per channel per frame), which is what makes undo/redo O(1): they move a counter instead of re-mixing 60-second buffers off-thread.

**Speed reaches playback through the source rate.** A loop plays on the isolated stream, which sources the store's mixdown. The armed `DiPcm` is built with the take's rate scaled by the factor (`looper_playback_pcm`), which is exactly the classic behaviour — the read cursor steps by the factor and the pitch follows it, no time-stretch — and the speed is part of the re-arm key, so changing it on a playing loop re-renders instead of being ignored.

**Two transport scopes.** A row's play (or stop) is for hearing ONE loop. The panel's global play/stop — `LooperAction::PlayAll` / `StopAll`, so MIDI and MCP reach them too — moves every loop on THAT chain at once, which is what starting a take locked to the same bar needs. The global scope skips what it cannot start: a looper with no take, one still recording or overdubbing (its take would be cut short), and one switched OFF (a disabled looper keeps its recording and its routing — it just sits the transport out). Another chain's loops are never touched.

**One loop timeline for the whole project.** Every looper, on every chain, follows ONE cycle kept by the `LooperStore` (`looper_store_sync.rs`, helpers in `loop_sync.rs`). The first REC pressed over silence sets the anchor — the top of the cycle, in host nanoseconds — and the first take's length is the time between its two presses, not the frames the meter tick happened to drain. Every later take, on any chain, is rounded to whole cycles of the shortest loop already recorded (under half a cycle rounds up to one) and keeps recording until that many frames arrived. The audio thread stamps the input tap with the host capture time of its first sample (`InputTap::first_capture_ns`, fed from the input callback's timestamp through the DSP worker), so the take is rotated to put the top of the cycle at frame 0 — a REC pressed mid-cycle lands where it was played. Closing a take never restarts the loops already playing. Playback is placed on the same clock: a cold arm starts at the loop position the timeline is at (`cold_start`), the output callback holds a timed playback silent until its start instant, starts it on the exact frame inside the buffer that contains it, and drops frames it owes when it starts late or runs dry (`di_playback_timing.rs`). A loop plays back exactly as long as its take — the isolated stream folds no seam crossfade into it (`DiPcm::without_seam_crossfade`), since a playback a few frames short would walk off the cycle every turn. Play from silence restarts the timeline 300 ms ahead so every loop started together begins at the top together; play while anything sounds joins the running timeline. Timed playback needs the stream timestamps on the host clock, so it is on where they are (macOS); elsewhere loops start at once and only the take lengths are synced.

**Isolation.** A bank belongs to exactly ONE runtime. A chain served by several parallel runtimes gets one bank per runtime, each recording its own input with its own buffers — two audio threads never touch the same memory, and a chain-level status reads whichever runtime actually holds material. An off-thread rebuild carries the banks over (`adopt_taps_from`), so a live edit does not wipe a recorded loop; a rebuild that CHANGED the sample rate drops them instead of replaying frames at the wrong speed.

**How a looper command gets there.** Every `LooperCommand` has a project
half (the dispatcher's: which loopers exist, where their knobs are) and a
runtime half (the store's). Both halves are the dispatcher's, so a looper
driven from a footswitch or over MCP behaves like one driven from the GUI: the
handler applies the store mutation through
`RuntimeControl` (`create_looper` / `remove_looper` / `looper_transport` /
`set_looper_param` / `set_looper_input` / `set_looper_output`) and the door ends
by reconciling that chain's isolated playback streams. `PlayStop` travels whole
— only the store knows whether that one button means play or stop. Adding a
looper and a Record / Play / PlayStop may bring the runtime up with no chain
enabled; nothing else may. The recorded audio leaves as an
`Arc<engine::LoopPcm>` handle through `export_chain_loops`, which
`ProjectCommand::SaveProject` writes into `<project>.loops/` — so a save issued
over MCP keeps the loops too. The way back in is `ProjectCommand::LoadProject`:
a project opens with every chain disabled, so no controller exists yet to hold
a loop, and the handler asks `RuntimeControl::restore_saved_loops` for the
restore, so opening over MCP gets it too; like the DI arm, that door may create
the audio runtime, and a disabled chain opens no stream. See
`docs/architecture.md` → "Write bus".

### Per-entry stream isolation

Every **raw input entry** of a chain owns its own isolated
`ChainRuntimeState` (CLAUDE.md invariant #4): its own `processing` Mutex,
`output_routes` (+ `ElasticBuffer`), `input_taps`, scratch. The
`RuntimeGraph` is keyed by `(ChainId, entry group)`; "chain" in the YAML
is only logical grouping.

- **Two devices**: one cpal stream per device, each bound
  to its own runtime; the shared output device sums them at the backend
  (the only mix point invariant #4 permits). On macOS each device's
  callback joins **its own** device's OS workgroup — resolved by the
  bound device's UID, never the system default: joined to another device's
  workgroup, the callback is co-scheduled with the wrong device's IO thread
  and underruns under CPU contention despite spare cores. The `dsp_worker`
  thread (which we own, unlike the C-owned cpal HAL callback thread) holds
  its membership in an RAII guard and **leaves** the workgroup before the
  thread exits: a chain rebuild tears the worker down and respawns it, and
  a thread that joined but exits without leaving crashes in libpthread's
  `_os_workgroup_tsd_cleanup`. The HAL callback thread cannot leave
  from another thread, so it keeps its membership for the process lifetime.
- **Two entries on ONE device**: Core Audio cannot open two
  streams on one device (the result is total silence), so
  the device keeps ONE cpal stream whose callback fans out to every
  per-entry runtime bound to that cpal index. On macOS each entry gets
  its own `dsp_worker` realtime thread, so a heavy entry cannot starve
  its sibling. State isolation is the contract; the hardware deadline of
  one device callback is inherently shared.
- **Split-mono siblings** (one entry, `mode: mono, channels: [a, b]`)
  stay in ONE runtime: the pinned volume invariants (g02/g03) require
  siblings to sum before the per-runtime limiter.
- **Insert chains** are a single runtime (the send/return pipeline spans
  cpal indices); **Linux/JACK** keeps the per-device grouping behind the
  `jack` cfg because the JACK-direct client binds one runtime.

Contract tests: `crates/engine/src/stream_isolation_tests.rs` +
`stream_isolation_same_device_tests.rs`; cpal binding in
`crates/infra-cpal/src/tests_regression.rs`.

**Live edits on a VST3 chain.** A live edit on a running chain normally
rebuilds its runtime off-thread and swaps it in — but a **fresh** build calls
the VST3 `createInstance` on the control worker while the audio thread is inside
the old instance's `process()`, and JUCE global state is not safe against that
concurrent instantiate-vs-process (SIGSEGV; no lock can cover that pairing,
since `process()` is RT and must not lock). So a chain containing a VST3
is instead updated **in place** (`engine::runtime::update_chain_runtime_state`
in `controller_offthread_live_rebuild.rs`): the live VST3 instance is reused (a
param change becomes `setParameter`, never a reload), mutated under the runtime's
processing lock. Non-VST3 chains keep the off-thread fresh rebuild — re-creating
a NAM/native block touches no shared JUCE state.

**An in-place edit does not restart a pipeline it keeps.** The in-place
update rebuilds every pipeline's state. A pipeline the edit keeps — same input,
same routes, and no block built fresh (its old nodes reused, or no blocks at
all) — continues the old pipeline's click-safe fade from where it stood at the
swap, instead of fading in again from silence. A pipeline with no blocks counts
as kept: with an insert as the chain's first block, each head feeds the send
with no block of its own, and a fade-in from silence there would go out to the
pedals and come back as a dip on every tail route. A pipeline that is new, or
builds a block fresh, still fades in; the spillover path still builds fresh and
fades in against the old pipeline's tail. Pinned by
`insert_bridge_isolation::insert_on_an_in_place_scene_edit_is_inaudible`.

**A live edit never clicks.** On both edit paths, toggling a block or switching
scene is a warmed-up crossfade of the same stream, with no change to latency or
to the steady-state sound:

- **In place (chains holding a VST3).** Every node the edit needs fresh is
  built first (`runtime_graph_prebuild`), while the live pipelines keep
  playing; the swap then only moves nodes, under one processing-lock section,
  so the audio never plays a pipeline emptied for the edit. An edit that needs
  a fresh VST3 keeps the quiesced path, and so does a `Select`.
- **A fresh node takes over from the node it replaces**
  (`runtime_node_handover`): it runs unheard for 512 frames (an IR's partition
  latency and onset, most of a NAM's receptive field) while the old node — or
  the dry input, when the block was off or new — keeps playing, then the two
  crossfade over 128 frames.
- **A block turned back on warms up first.** Its processor sat frozen while the
  block was off; it now runs unheard for the same 512 frames before its
  128-frame fade-in, instead of ramping its stale state in.
- **Fresh rebuild (chains without a VST3).** The new runtime is handed over in
  its slot (`slot_handover`): the old runtime keeps playing, fed the same input,
  while the new one warms up unheard for 1536 frames (its primed cushion, its
  fades from silence, its cold blocks), then every output crossfades over 256
  frames. The old runtime is released on the control side once no audio thread
  holds it.
- **The DSP worker waits for the swap.** The per-input worker retries
  the processing lock for up to one period, sleeping 20 µs between tries,
  instead of dropping the buffer; the device callback keeps its plain
  `try_lock`.

The new sound starts 11–40 ms after the edit (the warm-up); signal latency is
unchanged. Pinned by `infra-cpal` `issue_987_live_edit_click_tests` (a steady
tone scanned sample by sample for a step or a gap, the in-place cases with the
audio on its own thread and a real IR cab).

### I/O resolution from the binding registry (model A)

Device I/O is **never** stored in the chain/preset/scene/rig — it lives only
in the per-machine binding registry (`config.yaml` `io_bindings`, type
`domain::io_binding::IoBinding { id, name, inputs, outputs }` of
`IoEndpoint { name, device_id, mode, channels }`). A chain references bindings
via `Chain.io_binding_ids` (its start/end I/O, never persisted as blocks) plus
optional **mid** `Input`/`Output` blocks that each carry `{ io, endpoint }`.

At activation `engine::runtime_endpoints::resolve_chain_io(chain, registry)`
turns the chain + registry into the resolved input/output endpoints
(`device_id`/`mode`/`channels`) and feeds them to the **proven, unchanged**
engine build (`build_per_input_runtime_states` / `build_runtime_graph`, which
take a `registry: &[IoBinding]`). The engine still builds one isolated runtime
per input (invariant #4) and sums at the backend per physical output endpoint;
only the **source** of the device endpoints moved (binding, not block
`entries` — which are removed). Resolution happens off the audio thread.

**Input-conflict rule (activation).** Two ACTIVE chains may not share the same
`(device, channel)`; same device on different channels is fine; outputs may be
shared (many inputs may feed one output). The rule is between CHAINS: one chain
may read one capture point through several of its own E/S (one guitar, two
outputs); that is two isolated pipelines the backend feeds from the same tap,
not a conflict.
`input_conflicting_chains` (`runtime_endpoints.rs`) detects it;
`ProjectRuntimeController::sync_project` refuses to activate a conflicting chain
(first wins). The rig path enforces the same via `tap_conflict`
(`rig_runtime.rs`), and the command guard (`conflicting_input_channel`)
compares across chains too — the three detectors must keep agreeing, or a chain
plays after an enable and goes silent on the next project-wide rebuild.

**The rule is enforced at the command bus too.** The runtime skip above
is silent — on its own it would leave two "enabled" chains on one capture
point, both lit while only the first makes sound. Every door
into "enabled" REFUSES the change, with an error naming the contended
`(device, channel)` and the chain holding it:

| Door | Guard |
|---|---|
| `ToggleChainEnabled` | refuses when the target's inputs are already held |
| `AddChain` / `SaveChain` (create) | refuses an incoming `enabled: true` chain |
| `SaveChain` (upsert) / `ConfigureChain` | refuses moving an ENABLED chain onto a held tap |
| `SetChainIoBindings` | refuses re-binding an ENABLED chain onto a held tap |
| project load (dispatcher + GUI) | `disable_conflicting_chains` brings the later chain up disabled |

One detector serves all of them and the activation path:
`conflicting_input_channel` (names the holder) and `disable_conflicting_chains`
(load-time normalization) in `runtime_endpoints.rs`, both resolving through
`resolve_chain_io` — the same resolution the runtime skips on. A disabled chain
holds nothing, so binding it freely is allowed; the guard fires on enable. Both
GUI surfaces (chains screen row and compact view) toast the error.

Contract tests: `crates/engine/tests/issue_716_input_conflict.rs`
(conflict detector + skip decision); `crates/engine/tests/issue_833_input_channel_conflict.rs`
(named conflict + load normalization); `crates/application/tests/chain_enable_channel_conflict.rs`
(every command door); `crates/adapter-mcp/tests/issue_833_channel_conflict_over_mcp.rs`
(the same rejection over the MCP tool surface);
`crates/adapter-mcp/tests/issue_833_mcp_http_end_to_end.rs` (the same rules
driven end-to-end over Streamable HTTP — `tools/call` writes, `resources/read
openrig://ids` reads back);
`crates/project/tests/issue_716_chain_io_bindings.rs`
+ `issue_716_binding_discovery.rs` (`resolve_chain_ports`); golden +
`volume_invariants` + `stream_isolation` prove the resolved path is bit-exact
to the legacy entries path.

### Endpoint checklist

The input and output nodes of a chain's graph list every endpoint of the chain's own E/S bindings (`io_binding_ids`), checked by default. Unchecking one leaves that endpoint out of THAT node only: it stays listed, and nothing is removed from the E/S. It is chain configuration, not preset data — `RigInput.disabled_endpoints` in `project.yaml`, projected onto `Chain.disabled_endpoints` by `rig_to_chains` and captured back by `sync_synthetic_into_rig` (`project::endpoint_disables::EndpointDisables`: `inputs` and `outputs`, each a list of `{ io, endpoint }` — binding id plus endpoint name — and `path_outputs`, one `{ split, path, disabled }` entry per Y leaf that has something unchecked). Files saved before §11 with `path_a_outputs`/`path_b_outputs` load them as paths 0 and 1 of the chain's Y.

`resolve_chain_ports` applies it before anything else sees the chain's I/O, so an unchecked endpoint opens no stream, builds no segment and claims no capture tap:

- a head input is kept while the input node has it checked;
- a tail output is kept while the chain output node has it checked — or, on a chain with Y leaves (which has no chain output node), while any leaf's output node does;
- mid `Input`/`Output` ports are not on the checklist.

The input-conflict detectors agree on it: the chain-side ones resolve through `resolve_chain_ports`, and the rig-side `tap_conflict` skips a `RigInput`'s unchecked inputs itself. Two chains can therefore share one E/S, each playing the inputs the other leaves out. A ref to an endpoint the E/S no longer offers matches nothing and is ignored; the next save (`CaptureRigEdits`) drops it from `project.yaml` (`EndpointDisables::retain_known`, fed by `endpoint_candidates` of the chain's own bindings). Unchecking every input or every output of a node leaves that node with no port.

Unchecking **every** input (with no mid `Input`) or **every** output (with no
mid `Output`) is allowed: the chain simply has nothing to play. The engine does
not invent its legacy fallback endpoint for it (`effective_inputs` /
`effective_outputs` keep that fallback for a chain that selects no E/S), and
with no input it builds no segment at all — an insert's return must never stand
in for the missing input.

The stream layer treats such a chain as switched off: the graph, the input-tap
claims and every activation gate read one rule,
`engine::runtime_graph::chain_plays`. Its streams die like a switch-off,
it claims no input channel another chain wants, and it never fails the
activation of the other chains. The rig runtime (`RigRuntime::build` /
`enable_input`) reads the same rule, so its tap detector agrees.

In the chains screen (desktop), clicking a chain graph's input node — or its output node, or on a
chain with Y leaves a leaf's own output node — opens this checklist as a root-level panel: every input (or
output) endpoint of the chain's E/S, checked unless that node leaves it out. Each click dispatches
`SetChainEndpointEnabled` for that node and that endpoint and resyncs the chain; an unchecked
endpoint stays listed so it can be checked again. The node's label names its checked endpoints
(`None` when every one is off).

Contract tests: `crates/project/tests/issue_328_endpoint_discovery.rs`, `crates/engine/tests/issue_328_endpoint_disables.rs`, `crates/infra-cpal/src/io_topology_tests.rs` (`unchecking_an_input_endpoint_changes_the_bound_io_signature`).

### Y outputs

A chain with a Y split keeps the stream model above: one segment per (input ×
output) pair (`split_chain_into_segments`). A **leaf** is one path of a Y that
holds no further Y (`project::block::y_leaves`), at any depth. Every leaf has
its own output node; the segment of output `O` runs the shared blocks and then
the leaves whose output node has `O` checked — its `ChainSegment.paths`
(`segment_paths::route_paths`, `SegmentPaths::Only(leaves)`). Every leaf on one
output is **one** segment; they are summed inside it, time-aligned, never by
two segments on one route. An output no leaf checks is no port, so it has no
route and no segment. The input/output pairing is unchanged: a head input still pairs
only with its own E/S's outputs, so a leaf can only reach outputs of the E/S
whose input feeds it.

With an insert cutting the shared blocks, the insert's return feeds one
pipeline per distinct leaf set (routes that run the same leaves share it). A mid
`Output` after the insert rides only the first of them, so its route is still
written once.

The builder then shapes every Y, per segment, into the split that segment runs
(`split_segment_view`): a Split → Mix of only the paths leading to its leaves,
with a neutral mixer — each running path at unity, centred, not inverted,
master at unity, no sum — and every other path empty at level zero. One path:
the output carries exactly that path, never delayed (an empty path has no
latency). Several: their unity sum, time-aligned by the Split → Mix code. A Y
nested at any depth is shaped the same way; a Mix keeps its own knobs. The Y's
own knobs (mode, level into each path, balance) apply; its mixer knobs are
ignored, since a Y has no mixer. A bypassed Y passes the signal once to every
checked output. An offline render (no per-output routing) hears every leaf.

A route's convolution cushion counts only the paths its segment
runs: another leaf's cab does not deepen the cushion — and so the latency — of
an output it does not feed.

`chain_structure_signature` carries each output's leaf set, so checking or
unchecking a leaf on an output that stays open is a structural edit: the chain
gets brand-new streams, never an in-place knob-style rebuild. On
Linux/JACK the structure signature is not consulted; there the edit is
an in-place rebuild, which already runs the new leaf sets.

**Splits before and around a Y.** A Mix may sit before a Y, inside a Y path, or
contain a Y-free subtree; every split before a leaf is part of what that leaf's
outputs run. So the segment of every output runs every Mix and every block on
the way to its leaves; `split_segment_view` reshapes only the Ys. This is the
isolation law, one pipeline per output: nothing before a Y is shared between
the outputs' runtimes, so **the CPU cost of every block before a leaf counts
once per output that leaf feeds** (two outputs = two Mix passes, two amp
pairs). The app sets no cap. A Mix knob edit or a Mix bypass reaches every
output in place. A path-structure edit (adding, removing or reordering a path
or a path block, switching the end) changes the split's `model_identity`, which
enters `chain_structure_signature`, so it reopens the chain's streams like any
other structural edit.

On Linux with JACK-direct, one JACK client carries a chain's whole runtime.
Every output route of that runtime (`ChainRuntimeState::output_route_count`)
gets **its own set of output ports**, one per device channel
(`jack_route_ports`): route 0 keeps the historical `out_N` names, route `r`
registers `route<r>_out_N`, and each port is connected to
`system:playback_N`. The callback pops each route into its own ports; when two
routes write the same channel, JACK sums them at the playback port — our code
never adds two routes together. The callback used to pop route 0 only, which
left a second Y leaf's output — and any chain's second output or insert send — silent.
A single-output chain registers exactly the ports it always did. Adding or
removing an output changes the stream signature's output count, so the client
is rebuilt with the new port sets.

### Mid-chain ports

A port the user drops **between** effect blocks is not the chain's own I/O — it
is an extra endpoint at that exact position, and it starts (or ends) a stream of
its own.

**A stream is one `(input × output)` pair, and each one is an independent
pipeline.** One input and two outputs are two streams; two inputs and two
outputs are four. Which blocks a pipeline runs follows from the two positions:
everything between its input's entry point and its output's position
(`runtime_segments::blocks_between`). So the chain's MAIN pipeline — head input
to tail output — runs every block and ignores the ports entirely, while a
pipeline that ends at a mid `Output` simply has fewer blocks. `ChainPort.from_block`
is what tells a head port at offset 0 from a mid port at offset 0.

The price is explicit: a rig with a mid `Input` AND a mid `Output` runs its
preset four times. With a NAM + IR + reverb preset that measures ~1.4× the
callback deadline at buffer 64 (`issue_85_owner_interfaces_tap` reports the peak
load) — the app's own "the rig is heavy for this buffer size" warning applies.

**Their device belongs to this chain's streams.** An output device's stream
mixes only the runtimes listed for it in `output_devices_by_input_cpal`
(`chain_resolve_io_map.rs`) — the stream-isolation LAW. That map is built per
binding group, and a mid port's binding usually contributes no input (mid
`Output`) or no output (mid `Input`) to this chain, so both cases needed an
explicit pass: the mid outputs' devices are added to this chain's input streams,
and the chain's tail devices are added to each mid input's stream. Neither
widens isolation — only this chain's own runtimes are involved. HEAD inputs
still follow their own binding: a TEYUN in never exits a SCARLET out.

**A disabled port is no port at all.** `resolve_chain_ports` skips a mid
`Input`/`Output` block whose `enabled` is `false`, so it opens no stream and
claims no device route.

**A DI loop plays on every pipeline this input feeds.** Each pipeline owns its
own route, so a loop fed to segment 0 alone would leave a mid output silent the
moment the DI comes on (`InputProcessingState::plays_di_loop`, marked on the
build and the rebuild).

#### A port on another clock

Interfaces disagree: a Scarlett at 44.1 kHz and a TEYUN whose floor is 48 kHz.
The chain runs on ONE clock, so a route whose device runs on another needs three
things, and missing any one of them is audible:

1. **Conversion.** Each route carries its device's rate
   (`OutputRoutingState::sample_rate`) and the producer converts into it —
   Catmull-Rom, state in the callback scratch beside the single producer, no
   lock and no allocation past the first callback (`runtime_route_resample.rs`).
   A same-rate rig never enters this path and stays bit-identical.
2. **Following the real clock.** Two crystals drift. A fixed ratio bleeds the
   route dry at exactly that pace, so the ratio is trimmed by how far the
   route's cushion sits from its target — clamped to ±0.2 %, inaudible, and
   enough for any real crystal (`RouteResampler::track`).
3. **A deeper cushion.** The route is fed in converted bursts and drained by a
   device taking fixed buffers at its own callback times, which the OS bunches.
   A cross-rate route gets `CROSS_RATE_CUSHION` (3×) the lockstep depth, primed
   — and the LIVE REBUILD must keep it, or the tap starves again after the first
   block move (`route_cushion`, shared by build and rebuild).

The rebuild also has to KNOW the rate: it derives the per-device rates from the
live stream signature, which must include the **outputs**
(`device_rates_from_signature`). Taken from the inputs alone, a mid `Output` on
another interface loses its rate on the first live edit and stops being
converted — audible as "fine when it starts, horrible after I move a block".

**Persistence.** A mid port lives in the preset (`sync_synthetic_into_rig` keeps
it; only the chain's own head/tail I/O is filtered out) and re-pointing one is a
preset-level edit — a port has no params, so `write_back_processing_blocks`
writes its block kind, not a scene override. `RigProject::validate` judges a
port against the chains that actually **play that preset**: an E/S another chain
carries is an aux send, not a duplicate.

**Preset/scene switch.** The rig-nav rebuild (`merge_preserved_ports`) walks the
CURRENT chain and keeps every port — `Input`, `Output` and `Insert` — at
its own slot, feeding the rebuilt effects into the slots between them; a
scene's `bypass` on an `Insert` still reaches the chain, because the merged
insert takes `enabled` from the rebuilt (scene-applied) block with the same id.
Only the slot, E/S and endpoint come from the current chain, so a scene takes
the external amp in and out of the loop like it does any `Core`/`Nam` block.

**Metering.** The chain row draws one INPUT/OUTPUT pair per STREAM
(`meter_wiring::project_stream_count` → `engine::runtime_graph::chain_stream_count`),
which is also the unit the runtime indexes its per-stream taps by — so a mid
port has its own bar and its own clip indication.

Real-hardware evidence (`OPENRIG_HW_TESTS=1`, macOS): the battery plays a tone
through real CoreAudio streams and measures it on the other side of a loopback
(`issue_85_mid_output_reaches_its_device.rs`, `issue_85_mid_input_reaches_the_tail.rs`),
crosses two rates (`issue_85_mid_output_other_rate_real.rs`), carries a full
preset (`issue_85_heavy_rig_mid_output.rs`) and runs on the owner's own two
interfaces (`issue_85_owner_interfaces_tap.rs`).

### DSP worker per input stream (macOS)

The chain DSP does NOT run inside the CoreAudio input callback. The HAL
thread sleeps between cycles; heavy model working sets (NAM A2 weights)
cool down, and the cold-cache inference tail (~1.4 ms vs ~250 us hot)
can cross the 64-frame cycle — CoreAudio then drops input, heard as a
click. Pinned by
`crates/infra-cpal/tests/issue_670_real_streams_no_xruns.rs` (real
streams, real chain, DI-loop injection, 60 s: zero xruns, zero underruns).

The input callback only copies the buffer into a lock-free SPSC ring
(microseconds, invariant #8 clean) and a dedicated per-stream worker
(`crates/infra-cpal/src/dsp_worker.rs`) runs `process_input_buffer`:
preemptible realtime, computation budget sized to the real work, short
spin (a bounded ~35% of the period — it keeps the model weights hot
through the inter-buffer gap, killing the cold tail) then 100 us sleeps
when idle.

A split's paths do not run serially on that worker. Each path but the
first has its own lane: a thread spawned when the split is built
(`crates/engine/src/runtime_split_lanes.rs`), asleep between callbacks,
that runs its path while the worker runs the first one; the worker waits
for every lane, then mixes. Handing a path over is two atomic stores, no
lock, no allocation. A lane takes the realtime policy of the worker that
drives its split (`engine::worker_rt_policy`, set by the worker each time
it promotes itself), so a nested split's lanes inherit it too. Two NAM amps
in one Split → Mix used to put both models on one realtime thread (95% of
a core, heard as crackle); now each amp has a core.

What the xrun LED means under the worker: a late worker buffer that
catches up is absorbed by the ring + elastic and is NOT audible — it
feeds the load meter only. Audible damage is counted where it physically
happens: an elastic underrun (output starved) or a ring-overflow drop (a
gap in the played signal, counted as an xrun). On the non-F32 inline paths a
late callback IS damage (CoreAudio drops input) and is counted as one. The
worker covers the F32 input path only (the
macOS live path); the Linux/JACK backend is untouched.

Stuck latency after an output stall: a route on the chain's own
clock never drains a cushion that grew, so an output stream that misses a
few periods while the producer keeps pushing would keep that extra
latency forever — and play behind its sibling routes (two routes a few ms
apart = a "doubled" sound). Each route's `ElasticBuffer` watches
its lowest fill per ~186 ms window; the lowest floor of an underrun-free
window is the level the route proved it can hold, and a floor more than
32 frames above it is discarded at the next callback with a 32-frame
crossfade. Steady state never trims (bit-identical output).
`openrig://routes` reports `fill_frames` and `latency_trims` per route.

The level is never learned ABOVE the route's cushion target plus one
callback buffer. A chain starts its input stream before its output
stream, and every input period before the output's first callback pushes a
buffer nobody pops — the ring is full (its whole capacity, 2× the target)
by the time the first window closes, and taking that floor as the level
would keep the full capacity as latency until the chain is switched off and
on; the excess above the target is shed at the first clean window instead. A
structural scene switch brings up brand-new streams the same way — the input
runs ahead again, and live rebuilds reuse routes — and is shed the same way. A
cross-rate route is not guarded at all: its resampler servo owns the
level, and the two would fight, refill and cut, for the life of the route.

Nor is the level ever taken BELOW the cushion a route was primed with
plus one callback buffer. One clean window where the dsp-worker happened to
land just in time would otherwise become the level, and every later trim would
cut the primed route down to a single buffer — zero margin, so each late worker
buffer would be an underrun on the output callbacks whose phase sits near the
worker's finish time. This does not fire at all on a same-device route at 64
frames (target 64, prime 64, capacity 128: a trim would need a floor above the
capacity).

The level is never learned BELOW what the route needs at a callback start
either: the fill it rests on — the cushion it was born with (a
convolver-fed route is primed with its whole resting cushion), then wherever
its hand-off last landed it (see **Slack** below) — and never less than the
buffer the callback pops plus the one buffer of slack a convolver-fed route
keeps (see "How a route's cushion is sized" below). A window whose floor dips
below that saw a push that came late or never came — on the callback that
closes a window the ring can be empty while that callback's own underrun is
not counted yet — and it teaches the guard nothing; taking it as the level
would make every clean window after it cut the route's cushion back to it. A
late worker costs a route born with a cushion at most the buffers it did not
deliver, and nothing once it is back on time. A route born empty (no convolver
feeds it) rests where the producer's hand-off leaves it and keeps the stall
rule above that.

The prime-plus-one-buffer floor still holds on a route that
keeps no slack. A route that keeps slack is held by its own rest instead —
where its hand-off lands it: with a prime deeper than one buffer (32 or 128
frames) the prime-plus-one floor would sit one buffer above that rest and keep
a stalled cycle's buffer there as latency for good. At 64 frames both floors
are the same 128.

Where a route's missing audio went is counted, because "underruns
rising, `fill_frames` steady, `latency_trims: 0`" fits two different
causes: `dropped_frames` counts what the full ring discarded (the output was
not popping yet or stalled, or a producer that ran late caught up — between
drift-guard trims every frame is accounted for as primed + pushed = popped +
queued + dropped; trimmed frames leave the ring without being counted here),
and `input_busy_skips` counts input buffers `process_input_f32` dropped on a
failed `processing.try_lock()` (another thread holding it, or a poisoned
lock). Both are one relaxed atomic add on the loss branch only. Not yet
counted: the backlog a dsp-worker discards when it recovers from a
saturation spiral (it logs `saturation spiral`).

#### How a route's cushion is sized

**Every CoreAudio unit of one device runs on ONE HAL IO thread, input first and
output 0–5 µs later, every cycle.** The chain DSP runs on the per-input DSP
worker, so the output callback of a cycle can never see that cycle's input: one
device buffer is the hand-off, and one more is slack (see **Slack** below). A
duplex unit or inline DSP would remove the hand-off, but every chain on the
device would then share that one thread's deadline — a CPU-time isolation
violation — and inline DSP brings back the cold-cache tail the worker exists to
avoid; neither is used.

- **Target** (`infra-cpal/src/elastic.rs`, `elastic_targets`, ONE function
  for the cold build and the live rebuild, so a live edit never resizes an
  insert send differently from its cold build): the output device's buffer × a
  multiplier — ×1 for an Insert
  send; on macOS ×1 for a regular output fed only by an input on its own
  device; ×2 otherwise (×8 on JACK). A route is sized from ITS producers —
  the inputs of the segments that write it (`engine::route_clock::
  route_producers`) — never from every chain input: a tail fed by an insert
  return on another interface keeps ×2, and one stream's buffer size never
  raises another stream's latency. Never less than its producer's buffer:
  the producer pushes a whole input callback at once, and a ring of 2× a
  smaller cushion would drop it.
- **Prime** (`engine/src/route_cushion.rs`): a route fed by a convolver
  (decided per route in `route_convolution.rs` — an insert's send
  before the cab and a mid tap before the cab are not fed) is born with its
  cushion already filled, exactly its target: never above it, so the guard
  never cuts it (a prime above the target would cost a skip ~186 ms after
  every live edit and every DI render). A cross-rate route starts with
  its extra depth filled; other routes start empty.
- **Slack** (`engine/src/elastic_hand_off.rs`): a convolver-fed route
  on the chain's own clock keeps one device buffer queued behind the buffer
  each callback pops. On macOS the DSP worker hands a cycle's buffer over
  AFTER that cycle's output callbacks, so a route primed with exactly its
  one-buffer target would pop the whole prime at its first callback and from
  then on start every callback with only the buffer it pops: a worker
  one period late would be a gap. The route rests on its
  slack — the buffer it pops plus one buffer, or its own cushion when that is
  deeper — and its hand-off lands it there every time it resumes:
  - *At the start* it plays its prime until the producer's first buffer
    lands; the callback that finds it lands the route inside its band (from
    that rest up to its target plus the buffer it pops). Short, it
    plays silence and pops nothing once, so the next hand-off lands on top;
    long (its stream came up after the input ran ahead), it drops what queued
    above the band before anything was heard. Landing at the route's first
    callback instead, before anything is handed over, would leave a route whose
    output stream came up before the input — even by one cycle — on the bare
    hand-off, with no slack, for good. A producer that pushes before the output
    callback (inline DSP, a DI render) lands inside the band and rests there.
  - *After a gap* (a worker late by 2+ periods ran the ring dry, then handed
    everything over at once) the ring holds the rest plus the silence the gap
    played; the callback those buffers land on sheds exactly that silence,
    crossfaded and counted as one latency trim, never below the rest. Kept as
    latency until a guard window passed clean — never, while lateness keeps
    coming — each late worker would leave the route another buffer later.
    Short after a gap, it waits one callback more, counted with the gap.
  - Where it lands is its rest, and the drift guard takes it as its level
    without waiting for a clean window: under disturbances from the start no
    window closes clean and the guard would fall back to its cap. On such a
    route a clean window a whole buffer above the level is stuck latency at any
    buffer size: at 32 frames the guard's 32-frame tolerance is a whole
    buffer, and a stalled cycle or a late worker would leave the route 32–64
    frames late for good.

  Exactly one device buffer (+64 frames, ~1.45 ms at 64 frames and 44.1 kHz),
  whatever order the streams came up in; a producer that pushes
  before the output callback (inline DSP, JACK, a DI render) or a deeper
  cushion already rests there and gains nothing. The ring has room for twice
  that rest (4× the target), so an output that misses a callback leaves the
  buffer the worker handed over in the ring, where the guard sheds it with a
  counted crossfade: a ring as deep as the rest would refuse it — a jump in the
  audio with no counter. The room is not latency: whatever the route holds
  more than one device buffer past the ring it had without slack (twice its
  target) is shed at once, so however many output cycles stall, it is never
  more than one buffer later than a route without slack (192 frames at 64,
  where a ring without slack stops at 128 — by dropping the audio).
  The ring's room is not part of the route's rebuild posture (target and
  capacity), so a live edit that adds or removes a cab keeps the route it
  had, slack or not, instead of a fresh one with a gap; the next full build
  sizes it anew. **One buffer per signal path:** a guitar through an insert
  loop crosses two rings (the send, then the tail after the return), so an
  insert send never keeps slack — an IR before the insert would otherwise
  give the loop two buffers. With the insert on, one late worker pass still
  costs the send the buffer it did not deliver, and that gap goes out to the
  pedals and comes back through the return into Main — the tail's counters
  do not show it. A gap-free loop needs a second buffer on the send, which
  the send does not keep; the pin
  `infra-cpal` `with_the_insert_on_a_worker_one_period_late_costs_no_route_any_audio`
  stays red while it does not. A route no convolver feeds (the insert send, a
  chain without a cab) starts empty and rests at the hand-off; after a
  staggered start the send rests at its full ring, where a missed output
  callback still drops a buffer silently.
- **Another clock** (`route_clock.rs`): a route whose output device is not
  its producer's input device runs on another clock; at the same nominal
  rate the two drift and the ring slowly drains. There a convolver-fed route
  keeps a 512-frame cushion, which holds the real-streams battery clean
  (BlackHole in, MacBook speakers out); on the
  producer's own clock nothing drifts and the route stays lean.

Real-hardware proof: `infra-cpal/tests/issue_965_insert_on_the_owners_interface.rs`
(0 xruns / 0 underruns on a cold start, over a minute, after live edits and
after adding a cab live under full CPU load) and the DSP-worker real-streams
battery (`issue_670_real_streams_no_xruns.rs`, 0/0).

### Memory stays resident (macOS)

On a machine out of memory the kernel compresses pages nobody touched for a
while. A reverb walks its delay line once per loop, seconds apart, so its
pages get compressed between passes and the dsp-worker stalls decompressing
them: 1-4 ms buffers, the output underruns (`underruns == dropped_frames`).

When the engine starts (`ProjectRuntimeController::start*`, or
`build_streams_for_project` for the console / headless rig) one ordinary
thread, `memory-residency` (`infra-cpal/src/memory_residency_keeper.rs`),
wires the process's private writable memory with `mlock`
(`infra-cpal/src/memory_wiring.rs` talks to the kernel;
`memory_wiring_pass.rs` decides): only regions something already touched, at
most 256 MB each, each region once, and at most a quarter of the machine's RAM
in total — counting every region already wired, wherever it sits. Passes run
one at a time (`mlock` stacks a wire per call). What a pass leaves unwired —
over the budget, too large, refused by the kernel — is a warning in the log,
said when it changes (`memory_wiring_report.rs`).

A pass runs the moment a runtime's memory changes — a runtime goes live
(`LiveRuntimeSlot::new` / `publish`: start-up, off-thread rebuild) or a
running chain is updated in place (the VST3 live edit, a resync that keeps
the streams) — again 1 s later, once the new DSP has written the buffers it
allocated zeroed (a delay line has no pages until then), and every 5 s in
between. The periodic pass alone would let the kernel compress a new chain's
pages in the first seconds.

Wired pages are never compressed or swapped, so OpenRig keeps its working
set — ~1.2 GB for two guitars with NAM, a cab IR and two VST3 reverbs on two
outputs, ~1.5 GB with the app's UI — in RAM for as long as it runs; the rest
of the machine has that much less. Deliberate costs of wiring whole regions:
a region with one touched page is wired whole (thread stacks, a looper's
unused tail), and heap freed inside a wired region stays resident, so the
wired amount follows the session's peak, not its current use. No latency
changes. Linux and Windows: not done (not measured there).

Proof: `infra-cpal/tests/issue_980_owners_two_guitars_two_outputs.rs` on the
owner's interface.

### Chain enabled is runtime state, not persisted

`Chain.enabled` is in-memory state — the user switches a chain on and off
while the app runs. **It is NOT serialized to `project.yaml`**: chains always
load disabled and the user picks which to activate. That is why
`ChainYaml.enabled` has `skip_serializing`.

**Switching a chain off kills ALL of its streams.** The controller keeps an
in-memory chain → streams index (`ChainStreamRegistry`: open streams +
activations and rebuilds still being built). `upsert_chain` with
`enabled: false` calls `kill_chain_streams`, which reads the index and tears
everything down — streams, runtimes, slots and the receivers of builds in
flight — instead of pausing: an in-flight activation landing later would open
new streams for a chain the screen shows as off. A build that still arrives for
a chain outside the index is discarded. Switching back on is always a cold
activation. The DI and the loopers are pipelines of their own and only go away
with `remove_chain`.

**Switching the last chain off never waits for a build in flight.**
With nothing left running the frontend drops the controller, and with it the
`ControlWorker`. Its drop closes the job queue and detaches the thread; it never
joins it — joining would park the GUI for the whole build still running
(CoreAudio device resolve + NAM/IR load — seconds on a real rig) whenever the
switch-off lands while the chain's activation or a live rebuild is still
building. A result nobody waits for is discarded on the worker.

**A chain runtime is freed on the control worker, never on the frontend
thread.** A JUCE plugin created on the control worker (an LV2 such as
TAL-Filter-2) waits, on cleanup, for the message thread it was created on — run
that cleanup on the GUI thread and it waits forever. So every door that
lets go of a runtime (switch-off, `remove_chain`, the device-settings sync,
`stop`, the controller's own `Drop`) silences it, closes the streams, lets the
dsp workers exit, gathers everything still holding it (graph entries, live slots,
builds that already landed in a pending channel) and hands the bundle to the
worker, which drops it — the same rule the live rebuild follows for a
superseded runtime. Pinned by `controller_drop_nonblocking_tests.rs`.

A channel of a physical device can be enabled in **one** chain at a time.
Enabling the second **fails with an error** — the command is refused and the
chain stays disabled; see the "Input-conflict rule" above. A project that
already carries the invalid state opens with the later chain disabled.

## I/O and bindings (model A)

A chain's **start/end I/O comes from the binding registry**, selected via
`Chain.io_binding_ids`, and is **never persisted as blocks**. `chain.blocks`
holds only **effects** + optional, manually-inserted **mid** `Input` / `Output`
/ `Insert` blocks. `Input`/`Output`/`Insert` are variants of `AudioBlockKind`;
there are no separate I/O lists.

- The chain's main input/output are materialized from `io_binding_ids` at
  activation (head inputs at offset 0, tail outputs at the end) — not stored.
- Mid `Input`/`Output` blocks are `{ model, io, endpoint }` ports referencing a
  binding endpoint; they carry **no** device data (legacy `entries` removed).
- An `Insert` block is `{ model, io }`: one binding, send on its output and
  return on its input. Unbound (or bound to a binding this machine does not
  have) it is bypassed, never a segment boundary.
- Each input still spawns its own isolated parallel runtime; Output is a
  non-destructive tap; Insert splits the chain into segments (disabled = bypass).

**Switching an insert on or off never touches a stream.** The rule
lives in `engine::insert_cut`, in two halves:

- `insert_owns_streams` — a BOUND insert (both sides of its E/S resolve) owns a
  send and a return stream whether it is enabled or not. The streams
  infra-cpal opens, the stream signatures a live edit is compared against
  (`bound_io_signature`, the live stream signature, `chain_structure_signature`
  ignores an insert's enable flag) and the engine's endpoint shims — route and
  input indices — all follow it, so switching the insert never renumbers,
  opens or closes a stream.
- `insert_cuts_chain` — only an ENABLED bound insert cuts the chain's DSP into a
  send segment and a return segment. A disabled one is what it always was: the
  chain plays straight through it, every head paired with its own E/S's
  outputs, on the stereo bus, each E/S in its own isolated runtime. Its send
  route is never written (the send stream carries silence,
  and a chain with no output of its own never plays out of it) and nothing
  reads its return: that stream's callback returns before the runtime's
  processing lock (`fed_inputs`), so it never costs the guitar's callback a
  period.

So a footswitch press, the enable dot, or a scene/preset whose only change is
the insert reaches `schedule_chain_activation` as "same streams" and takes the
off-thread DSP rebuild every live edit takes — built on the control worker,
live within milliseconds. The one exception is a chain with several input
entries (several E/S, an E/S with two input endpoints, a mid `Input`): its
runtimes are one pipeline while the loop cuts it and one per entry while it
does not, so the switch regroups them — `chain_structure_signature` carries
the grouping and such a switch gets new streams, as before. A chain with one
input entry is one runtime either way and owns every one of its routes
(`switch_owned_routes`), so a route only the loop's cut writes — its send, a
tail only the return feeds — is already bound when the loop is switched on. On
Linux+JACK the same edit goes through the synchronous in-place update (the JACK
backend has no off-thread swap). A chain holding a VST3 is updated in place
instead; that update looks for each block's old node in every old segment, so
the blocks the cut moves between segments keep their processors (a VST3 is not
re-instantiated, a delay keeps its tail).

**With the loop on, each physical output plays the return once, and the chain
volume acts once.** While an insert cuts the chain, ONE pipeline — the
return — feeds the tail. Two E/S of a chain often end on the same physical
output (two guitars both playing Main `[0,1]`), so the chain has one
route there per guitar. With the loop off that is right: each guitar is its
own pipeline on its own route, and the device sums them. The return is one
signal, so it writes only the first tail route on each physical output (same
device, same channels — `engine::insert_return_routes`); nothing writes the
other E/S's route on that output in this state, so the runtime never builds it;
its stream stays open and holds no runtime, because only a chain that
is one runtime in EVERY insert state (one input entry) owns routes it does not
write (`switch_owned_routes`) — two E/S regroup onto new streams when the loop
switches, so the loop's runtime owns only what it writes. Written to every tail
route, the return would play once per E/S on a shared output: +6.02 dB with the
two routes in step, a comb one buffer wide when they rest a buffer apart. The
chain volume is the chain's OUTPUT level: it scales the chain's outputs and
never an insert send (`OutputRoutingState::applies_chain_volume`), so 50 %
moves Main −6.02 dB with the loop on or off (scaling the send and then the tail
would be −12.04 dB through a unity loop). Pinned by the two-head rig suite
(`issue_979_two_head_rig_tests.rs` and its `insert_return_stacking`,
`volume_and_gain` and `issue_body_topology` children). Two E/S outputs that
overlap only partly (`[0,1]` and `[1,2]`) or list the same channels in another
order are not the same output by this rule; the return writes both.

**Switching a chain on, and hearing a rebuilt chain.** A chain's
devices are looked up by id through `infra_cpal::device_lookup`: a walk of the
host's device list remembers every device it passes, so the next lookup of any
of them is a single property query confirming the handle still names that id
(an unplugged/replugged device fails it and is looked up again; a device-list
refresh forgets everything; ASIO keeps the old walk, since holding a driver
keeps it loaded). Switching a chain on costs the stream open, not a walk of the
whole device list per endpoint. A runtime the control worker
rebuilt off-thread (a scene/preset switch, an insert switch, a live edit) is
swapped in by `rebuild_install_timer` every 5 ms instead of on the 200 ms
error-poll tick, so it is heard as soon as it is built.

**A mid port is a normal block.** It is a row in the chain like any effect
— the head input and tail output are chips drawn from the bindings, not rows —
and it survives a project load. Loading drops only the legacy leftovers that
point at a binding the chain ALREADY carries (that duplicate starves the
device); a port pointing at another E/S, or not yet pointed anywhere, is the
user's and stays.

**A mid `Output` emits the signal at ITS OWN position.** It taps the bus
right where it sits — only the blocks BEFORE it have run — while the chain keeps
flowing through the blocks after it down to the tail output. Nothing is cut and
no DSP runs twice: the tap is a copy of the segment bus at that point, so a
`Cab → [Output] → Delay → Reverb` chain sends the un-delayed cab signal to that
endpoint and the full chain to the tail. Both routes get the same click-safe
rebuild fade. A disabled mid `Output` keeps its (silent) route and emits
nothing, like any other disabled block.

### I/O binding registry

The binding holds the concrete device endpoint (device id, mode, channels); the
chain (and preset/scene/rig) carry only stable binding `id` references. This
keeps `project.yaml` files portable — moving them to another machine re-resolves the
ids against that machine's local `config.yaml` registry.

`config.yaml` schema (system scope):

```yaml
io_bindings:
  - id: main                # stable id referenced by chains
    name: "Scarlett"
    inputs:
      - { name: In1, device_id: "coreaudio:...", mode: mono, channels: [0] }
    outputs:
      - { name: Out1, device_id: "coreaudio:...", mode: stereo, channels: [0,1] }
  - id: cab_b
    name: "Interface B"
    inputs:
      - { name: In1, device_id: "B", mode: mono, channels: [0] }
    outputs:
      - { name: Out1, device_id: "B", mode: stereo, channels: [0,1] }
```

Chain block YAML using ports:

```yaml
chains:
  - description: guitar 1
    instrument: electric_guitar
    blocks:
      - { type: input,  io: main, endpoint: In1, enabled: true }
      - { type: preamp, model: marshall_jcm_800_2203, enabled: true, params: { volume: 70, gain: 40 } }
      - { type: insert, model: external_loop, enabled: true, io: fx_loop }
      - { type: delay,  model: digital_clean, enabled: true, params: { time_ms: 350, feedback: 40, mix: 30 } }
      - { type: output, io: main, endpoint: Out1, enabled: true }
```

An **insert references one E/S binding** (`io`), like every other chain
reference: the SEND goes out that binding's OUTPUT and the RETURN comes back on
its INPUT, so a single pick wires the whole loop and the `project.yaml` stays
portable. An insert whose binding does not resolve on this machine is
**bypassed** — the chain flows straight through it — instead of splitting
at an endpoint that was never opened. See ADR 0004.

**The loop usually shares the guitar's interface, and that is the normal case**
(a Synergy on OUT 5 / IN 4 of the same box). cpal opens ONE input stream per
DEVICE and binds every runtime fed by it, so the insert return takes the
cpal index of its DEVICE — from the same first-seen map the regular inputs use —
and reads its own channel off that shared stream. A return with a stream index
of its own would name a stream nobody opens: the post-insert segment would never
be fed and the rig would go silent with the send still working. The return
keeps its own entry group either way: it is never summed with the input it
shares the device with (invariant #4).

### Legacy projects open UNBOUND (clean break)

There is **no device migration**. The block model no longer has an `entries`
field at all — `io`/`endpoint` are the only I/O fields, and they are required,
so a project's device routing is never inferred from old per-block device data.
A legacy chain (or a chain whose bindings are not configured on this machine)
opens **unbound**: it produces no runtime and plays no audio until the user
selects its I/O bindings in Settings → I/O. Loading still works: the project
keeps its effect/preset/scene structure, and the user re-selects bindings.

This is intentional: routing is binding-only, the registry (`config.yaml`) is
the single source of truth for I/O, and a project remains portable without
inventing device routing the user never confirmed on this machine.

Sample rates: 44.1/48/88.2/96 kHz. Buffer sizes: 32/64/128/256/512/1024. Bit depths: 16/24/32.

## Per-machine device settings (config.yaml)

To change audio device settings in the UI, open the **Settings screen** (top bar) and select the **System / Audio interface** section. Sample rate, buffer size, bit depth, and language are **per-machine**, not per-project. They persist to `config.yaml`:

- macOS: `~/Library/Application Support/OpenRig/config.yaml`
- Windows: `%APPDATA%\OpenRig\config.yaml`
- Linux: `~/.config/OpenRig/config.yaml`

Schema:

```yaml
recent_projects: [...]
paths: { ... }            # plugins_path and the other folders (Settings → Paths)
input_devices: [{ device_id, name, sample_rate, buffer_size_frames, bit_depth, ... }]
output_devices: [...]
language: pt-BR           # or en-US; null follows the OS
midi_devices: [...]
midi_enabled: false
mcp_enabled: false
io_bindings: [...]        # the I/O binding registry (see "I/O and bindings")
metronome: { ... }
mixer: { ... }
```

The struct is `AppConfig` (`crates/infra-filesystem/src/app_config.rs`). A legacy `gui-settings.yaml` is migrated into `config.yaml` on first launch and deleted — nothing to do by hand.

`load_project_session()` fills `project.device_settings` in memory. The project YAML **does not persist** `device_settings` (`skip_serializing`), but an old YAML that still has the field deserializes.

**Saving applies to the rig that is running.** `SettingsCommand::SaveAudioSettings` persists the values *and* re-opens the running graph, through `RuntimeControl::sync_project`: the new rate / buffer size / bit depth (and on Linux/JACK the server parameters) apply to every device the project names at once, which no per-chain sync can express. The door walks the chains the **project** names, one at a time, each against its own resolved devices — never a selection over live runtimes by sample rate (`CLAUDE.md` LAW) — and it never starts audio: a save on a stopped rig leaves it stopped.

## Metronome output stream

The metronome does **not** run inside any chain. It opens its **own** cpal output stream on the device the user picks, and the operating system sums it with whatever else that device is playing — the same shape as the DI, and invariant #4 applied literally:

```
guitar:     [in] -> [chain] -> [out dev A]  \
                                             > backend sums
metronome:            [click] -> [out dev A] /
```

What this buys, and why it is not negotiable:

- **Nothing was added to the guitar's audio path.** `process_output_f32`, `process_output_f32_mixed`, `runtime_process_segment` and the output limiter are untouched, so the metronome cannot regress latency, the volume invariants, or stream stability.
- **Neither side can break the other.** A chain rebuild, a live block edit or a chain failure cannot chop the click; a missing metronome device cannot stop guitar audio.
- **The click cannot leak.** It never enters a chain's buffers, so it stays out of that chain's meters, taps, and anything recorded off another output.

There is no ring and no worker thread, unlike the DI: the click is synthesized, so the callback renders it directly from a pre-allocated scratch buffer. Settings reach the callback through atomics versioned by a generation counter (`engine/metronome_state.rs`), so the audio thread neither locks nor allocates (invariant #8). The stream's sample rate comes from the resolved device config — never a constant.

Settings live in **system** config (`config.yaml`), per ADR 0003; `enabled` is not persisted, so the app always starts with the click off.

On the Linux JACK build the click opens its own JACK client instead of a cpal stream (see "Auxiliary outputs on JACK" below).

## Backing-track player output stream

The backing-track player is a third independent pipeline beside the chains and the metronome: its **own** output stream on the chosen endpoint's device, summed by the backend, with nothing added to the guitar's audio path.

```
guitar:     [in] -> [chain] -> [out dev A]  \
                                             > backend sums
player:  [worker] -> ring -> [out dev A]    /
```

- **Decoding, resampling and time-stretching run on a worker thread at normal priority**, never on the audio thread and never in the realtime class, so the player cannot take CPU time from a chain's callback. The file is decoded once (symphonia) and resampled once to the device rate; speed and pitch go through a pitch-preserving stretcher (signalsmith-stretch). At 1.0× and 0 semitones the track is copied untouched.
- The worker keeps a short queue ahead in a lock-free SPSC ring; the output callback only drains it, applies the level and short fades on start, pause and seek, and writes the endpoint's channels. Transport and settings reach both sides through atomics (`engine/player/shared.rs`): the callback neither locks nor allocates (invariant #8).
- An A–B loop wraps with a short equal-power crossfade, so the seam never clicks. The position the GUI shows is what was *heard*, not what the worker rendered ahead.
- A chain rebuild, a live block edit or a chain failure does not touch the player's stream; stopping the player closes it.

**Auxiliary outputs on JACK.** The metronome and the player open through one auxiliary-output opener (`infra-cpal/aux_output.rs`): a cpal output stream on cpal builds, and on the Linux JACK build a JACK client of its own with one port per target channel, connected to the matching `system:playback_N`, so JACK sums it at the playback port. The metronome used to be silent on JACK; it now uses the same opener.

## Global mixer gain

The global mixer has one fader + mute per **physical endpoint** configured in
`config.yaml` (direction, device, channels — deduplicated across bindings, so
an output declared in four bindings is one fader). On the audio path:

- **Where.** An input fader scales the live device frames right after
  `read_input_frame`, before the chain's blocks (loop and silence feeds are not
  scaled). An output fader scales the route's frames together with the chain
  volume, before the output limiter.
- **Isolation.** Each endpoint has one lock-free `AtomicU32` target
  (`engine/mixer_gains.rs`). A graph build hands each route / input pipeline an
  `Arc` to its endpoint's scalar; the audio thread only loads it. Nothing is
  summed or shared between streams — two chains on the same output each read
  the same control value independently.
- **No clicks, no latency.** A move glides linearly from the value the stream
  last played to the target across one callback (`engine/mixer_ramp.rs`), then
  holds. No buffering, so zero added latency.
- **Unity is bit-identical.** A fader at 0 dB that stays there skips the
  multiply entirely; `volume_invariants_tests.rs` is untouched.
- **Split-mono inputs.** A mono endpoint with N channels runs as N single-channel
  pipelines; the application writes the strip value to the whole group and to
  each channel key, and the engine stays an exact-key lookup.
- **Range.** -60 dB .. +12 dB, default 0 dB; the bottom of the fader and mute
  are silence (linear 0).

## One output route per stream

A chain on several bindings builds one runtime per binding. Each runtime owns
an output route **only for the outputs its own segments write** — never for
another binding's output — and each output device stream holds only the
runtimes that write that output: a route nobody writes would be popped empty
on every frame, counted as underruns and light the chain's overload LED. The
same rule keeps an insert's return from being built twice: with the loop on,
the return writes one route per physical output, so a second E/S's route on
that output is written by nothing and not built.

## Stepped-input detector

An input that arrives broken carries a step at the same position of every
device buffer (a buzz at `rate / buffer` Hz). The underrun and trim counters do
not move when that happens, so each pipeline watches its own input channels.

- **Measure** (`crates/engine/src/input_seam_detector.rs`). Per received
  channel, the mean |3rd difference| per position inside the buffer, folded
  over a 0.25 s window. A clean input reads a max/median of about 1.0–1.3; a
  window above 5 is stepped. Below -90 dBFS a window carries no signal and is
  not judged.
- **Trip and clear.** Four stepped windows in a row (1 s) trip the channel;
  four clean ones (1 s) clear it. A buffer size change restarts the count, and
  a buffer our own processing skipped is fed as a discontinuity, never as a
  seam. Real-time safe: no allocation, lock or blocking.
- **Per pipeline** (`runtime_input_seams.rs`). A runtime is marked stepped only
  while one of its own pipelines reads a tripped channel; another chain's
  input never marks it.
- **Restart** (`adapter-gui/src/stepped_input_tick.rs`, on the 2 s poll tick).
  A marked chain is switched off and on, alone, like the chain toggle. After
  an attempt that chain waits 30 s before the next one; other chains do not
  wait.
- **Mark on disk** (`adapter-gui/src/stepped_input_mark.rs`). Before every
  restart the evidence is written to `<user data>/incidents/stepped-input/`:
  `input-<k>.wav` (the last seconds of each input stream, every device
  channel), `cycles-<k>.csv` (callback timing), `device-<k>.json` and
  `device-after-<k>.json` (the device at the trip and 3 s after the restart)
  and `openrig.json` (the open streams and the chain's routes). The last 10
  marks are kept.

The restart is not a fix: it cuts the sound, and the mark is there to find the
cause.

## Multi-rate streams

Two interfaces running at **different sample rates at the same time, in the
same chain** — e.g. a 44.1 kHz interface and a 48 kHz interface — is supported.

**The rate is resolved per device, and validated per binding.** Each device
contributes the `sample_rate` saved for it in `config.yaml`; with nothing
saved, the device's own default rate is used. Every per-input runtime is then
clocked at the rate of *its own* input device — this follows directly from
invariant #4: one binding is one isolated stream, and two isolated streams
share no clock.

What that allows and forbids:

- **Across bindings — free.** Binding A at 44.1 kHz and binding B at 48 kHz in
  one chain is valid, and starts two independent streams.
- **Inside one binding — must match.** If a binding's inputs disagree, or its
  input and output disagree, activation **fails loudly** rather than starting:
  `chain '<id>' invalid: mismatched sample rates across inputs (<a> vs <b>)`,
  or `… across I/O (<a> vs <b>)`. A single isolated stream cannot resample
  internally, so this is deliberately an error and never a silent conversion.
  The message surfaces through whichever status/toast belongs to the action
  that triggered the sync.
- If the device does not support the chosen rate at the channel count the chain
  needs, activation fails with `no supported config for sample_rate=<r> with at
  least <n> channels`.

**Nothing is resampled between streams** — there is no sample-rate conversion
bridging two pipelines, by design. The one resampler in this area is the DI
loop, which is resampled *to* each output stream's own rate when armed, so a
loop armed at one rate never plays stretched on another.

A single-binding chain is one binding, one group, one rate.

Caveats:

- **JACK (Linux, `jack` feature) is unchanged and out of scope** — the JACK
  server imposes one rate for everything. Multi-rate applies to the cpal path:
  macOS, Windows, and Linux built without the `jack` feature.
- The full path needs two physical interfaces, so it cannot be proven headless.
  It is covered by the hardware battery (macOS): `OPENRIG_HW_TESTS=1 cargo test
  -p infra-cpal --release --test issue_736_multi_rate_streams`, which runs 30 s
  and requires zero xruns and zero underruns.

## JACK lifecycle (Linux only)

With the `jack` feature, OpenRig owns the JACK server's lifecycle. `JackSupervisor` (`crates/infra-cpal/src/jack_supervisor/`) is the single owner of every `jackd` it launches:

- **`ensure_server`** adopts a `jackd` that is already running when its config matches the desired one (sample rate, buffer, channels, periods), and restarts it on a mismatch; otherwise it spawns one.
- **Before spawning**, the card's mixer is set to unity — playback AND capture controls to `0dB unmute` via `amixer` (`alsa_mixer.rs`; best-effort, needs `alsa-utils`). Without PipeWire/Pulse nothing initializes the mixer: many USB interfaces come up attenuated on playback or with a boosted mic input that clips.
- **The spawn** (`live_backend.rs`) runs `jackd [--realtime -P <prio> | --no-realtime] -n <server> -d alsa -d hw:<card> -r <rate> -p <buffer> -n <periods> -i <in> -o <out>` (3 periods by default) with `JACK_NO_AUDIO_RESERVATION=1`, cleans stale files in `/dev/shm` and waits for the server socket.
- **Drop never kills `jackd`.** Switching every chain off drops the controller; switching one back on adopts the server still running, with no respawn and no audio gap. A final shutdown (app exit, test teardown) calls `shutdown_all` explicitly.
- The launcher's "start audio" goes through `start_jack_in_background` (`device_settings.rs`), whose server the later controller adopts.

A 2 s timer in adapter-gui (`health_timer` → `audio_health_tick.rs`) checks the audio health and announces a disconnect and the reconnect. Everything sits behind `#[cfg(all(target_os = "linux", feature = "jack"))]`.
