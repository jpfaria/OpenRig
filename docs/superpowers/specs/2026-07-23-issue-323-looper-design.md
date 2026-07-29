# Issue #323 — Per-chain multi-layer looper (design)

Status: approved (owner brainstorm, 2026-07-23). Supersedes the open questions in the issue body; the decisions are also recorded as an issue comment.

## 1. What it is

A Boss RC-style looper, offered **per chain as a feature — not as a block**, exactly like the virtual DI loop (#614 / #717 / #771): a button on the chain bar opens a root-level panel that lists the chain's loopers, one row each, with its own transport.

A chain can hold **N loopers**, created in the panel. They are fully independent: any number can be recording, overdubbing or playing at the same time, each with its own length and cursor. Nothing is synchronised between them in v1.

## 2. Signal path

- A looper **records the chain's dry input** — the same frames the chain's first segment receives from the device, before any block.
- Playback is **summed into the chain input**, so the recorded material runs through the whole chain. Swapping the amp or a pedal changes how the loop sounds.
- Overdub records the live dry input again, so no processed signal ever re-enters the recording — there is no wet feedback path.

This is the same injection point the DI loop uses (`engine::runtime::process_input_f32` → `SegmentFeed`), with one difference: the DI loop **replaces** the device frames (`SegmentFeed::Loop`), while the looper **adds** to whatever the segment is being fed. The two compose: with the DI loop armed, the looper records and plays over the DI playback.

Like the DI loop (#699), a chain's looper output is injected **once per chain**, on the chain's first segment only.

### Channel layout

The looper stores stereo frames at the engine sample rate, matching invariant #5 (a stream is always stereo internally). A mono chain input is broadcast into both stored channels; playback into a mono segment sums back down at the injection point, never inside the buffer.

### Stream isolation (law)

A `LooperBank` belongs to exactly ONE `ChainRuntimeState`. No buffer, cursor, queue or atomic is shared with any other chain, any other stream or any other bank. Rebuilding one chain's runtime never touches another's loopers. Bank state survives a runtime rebuild the same way the DI loop does (`runtime_state_taps::adopt_from`), so live edits do not wipe a recorded loop.

## 3. Capacity and allocation

- Max loop length: **60 s**, stereo, at engine sample rate (≈ 23 MB per layer at 48 kHz).
- Undo history: **8 overdub layers** per looper.
- Layers are allocated **off the audio thread**, on demand: an empty looper costs no audio memory. A `Box<[f32]>` is allocated by the control thread when a record is armed and handed to the audio thread through a lock-free queue.
- **Nothing allocates, locks, or frees on the audio thread.** Retired layers (clear, undo past the ring, looper removed) are pushed back to the control thread through a return queue and dropped there.

## 4. Audio-thread architecture

```
ChainRuntimeState
└── loopers: LooperBank            // owned by this runtime alone
    ├── ops:     ArrayQueue<LooperOp>      // control → audio  (buffers in)
    ├── retired: ArrayQueue<Box<[f32]>>    // audio → control  (buffers out)
    └── slots:   [LooperSlot; N]           // inside ChainProcessingState
```

`LooperSlot` holds: the layer buffers (`Vec<Box<[f32]>>`, ≤ 8), the active-layer count, `len_frames`, `write_pos`, `read_pos`, `state`, and the per-looper gains (mix, decay). Transport state and cursors are mirrored in atomics on `LooperBank` so the GUI can read position/level without a lock.

The slots live inside `ChainProcessingState` — the audio thread already holds `&mut` to it inside its existing `processing.try_lock()` section — and control-side mutations are queued and drained there, the exact `drain_pending_block_toggles` pattern from #580. No new lock, no lock taken by the GUI.

Per callback, per looper, in `process_input_f32`:

1. Drain `ops` (arm record, stop, undo, redo, clear, install a fresh layer buffer, remove a looper).
2. **Playback** — sum the active layers at `read_pos` into the segment's input frame, scaled by mix (and by decay for older layers). Reading N layers is N fused multiply-adds per channel per frame; 8 layers × 2 ch × 128 frames ≈ 2 k FLOP per callback, flat and predictable.
3. **Record / overdub** — write the dry input frame into the current layer at `write_pos`.
4. Advance the cursors once per callback (wrapping modulo `len_frames`), on the chain's first segment only.

**Undo is O(1)**: it decrements the active-layer count. **Redo** re-increments it while no new overdub has been recorded over it; the first new overdub after an undo drops the redo tail (buffers returned via `retired`). Because playback sums the layers on read, there is no mix buffer to recompute and no off-thread re-mix — which is what makes undo instant and allocation-free.

- **Speed** (½×, 1×, 2×) is a cursor step of 0.5 / 1 / 2 with linear interpolation on read — no resampling, no allocation.
- **Reverse** is a negative cursor step.
- **Feedback decay** multiplies existing layers' contribution during overdub, applied at read time via a per-layer gain — no buffer rewrite.
- Recording that reaches `len_frames` (first layer: the 60 s ceiling) stops the recording and switches to playing; it never grows a buffer.

### Transport state machine (per looper)

```
Empty ──rec──▶ Recording ──tap──▶ Playing ⇄ Overdubbing
                                    │  ▲        (tap toggles)
                                    │  └── undo / redo
                              stop  ▼
                                  Stopped ──play──▶ Playing
                                    │
                                  clear
                                    ▼
                                  Empty
```

The first `Recording → Playing` tap freezes `len_frames` — the loop length for that looper's lifetime, until `clear`.

## 5. Control plane

Every state change is a `Command` (architecture law), so GUI, MIDI and MCP share one path:

- `AddChainLooper { chain }` / `RemoveChainLooper { chain, looper }`
- `SetChainLooperTransport { chain, looper, action }` — `Record | Play | Stop | Undo | Redo | Clear` (one variant, not six commands; MIDI maps a footswitch to an action)
- `SetChainLooperParam { chain, looper, param }` — mix, decay, speed, reverse

The dispatcher validates (chain exists, looper exists, layer budget) and emits `Event::ChainLooper*`; the adapter-gui wiring applies to the runtime, mirroring `local_dispatcher_di_loop.rs`. The MCP tool surface inherits the same commands for free (parity law).

### MIDI

The existing `adapter-midi` maps a footswitch/CC to `SetChainLooperTransport`. Documented in `docs/midi-command-coverage.md` alongside the other command mappings. A looper without a footswitch is unusable while playing, so this ships in v1.

## 6. Persistence

- **Parameters** (looper count, mix/decay/speed/reverse per looper) live in the chain inside `project.openrig`, next to `di_output`.
- **Audio** auto-saves with the project: each non-empty looper's mixdown is written as a dry wav under `<project>.loops/looper-<id>.wav` and reloaded when the project opens, restored as a single base layer (undo history is not persisted). Writing happens off the audio thread, on project save.

Rationale for storing the mixdown rather than the layers: the layers only exist to support undo within a session, and 8 × 23 MB per looper on disk per project is not a trade the feature earns.

## 7. UI

- A looper button on the chain bar, beside the DI headphones and the Tone Doctor button — SVG via `@image-url` + colorize, never a glyph (Orange Pi renders tofu).
- A root-level panel (an inline `Rectangle`, **not** a `PopupWindow` — its content is unclickable, confirmed in #749 and #761), rendered at the window root so the chain list cannot clip it.
- **Stacked list, one row per looper**: transport (rec / play-stop), undo, clear, a level control and a position/length bar. `+` adds a looper, `×` removes one.
- State is signalled by colour **and** label — red recording, blue playing, grey empty — never colour alone.
- The finer parameters (speed, reverse, decay) live in a per-row expander, so a collapsed row stays large enough for touch on the Orange Pi.
- Selects reuse the single shared Select component (one-select law), never a new dropdown.

## 8. Testing

TDD red-first throughout; every item below is a test that fails before the code exists.

- **Engine, offline** — record → play returns the recorded frames; overdub sums; undo removes exactly the last layer; redo restores it; a new overdub after undo drops the redo tail; clear empties; speed/reverse read the expected samples; length freezes on the first tap; recording stops at the 60 s ceiling.
- **Allocation** — the existing `audio_alloc_invariant_tests` harness proves record / overdub / undo / clear / playback perform zero allocations on the audio thread.
- **Isolation** — two chains, one looping, the other untouched: the second chain's output is bit-identical to the no-looper baseline (law + invariant #4).
- **Rebuild** — a live block edit rebuilds the runtime while a loop plays; the loop keeps playing from the same position (`adopt_from`).
- **Dispatcher** — each command's happy path and rejections (unknown chain, unknown looper, layer budget exhausted).
- **Persistence** — save/reload round-trip of parameters and of the wav mixdown.
- **GUI** — a headless interaction test (`i-slint-backend-testing`) that actually clicks rec / play / undo / clear on the panel and asserts the callbacks fire, plus a `tools/slint-render` PNG check of the panel layout (renders prove layout only; the interaction test proves the clicks).
- **Real hardware** — the `OPENRIG_HW_TESTS=1` battery re-run to prove no xruns with a looper armed.

## 9. Out of scope for v1

- **Quantize to BPM** — there is no BPM source in the app yet; it waits for the metronome (#14).
- Sync between loopers, import of arbitrary audio into a looper, per-layer export, and a toolbar shortcut. Each is a follow-up if asked for.

## 10. Documentation

`docs/screens.md` (the panel), `docs/audio-config.md` (the injection point), `docs/midi-command-coverage.md` (the transport mapping) and `docs/testing.md` (the new suites) are updated in the same PR as the code that changes them.
