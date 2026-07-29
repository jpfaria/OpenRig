# Looper as an independent stream — redesign (#323)

**Status:** design approved (owner, 2026-07-28); spec under review.

## Problem

The looper is unreliable in a way that no point-fix has cured: pressing REC
sometimes records nothing, pressing stop/clear/× sometimes does not stop the
sound, and reopening a project or toggling the chain leaves it in a broken
state. Every failure traces to one root cause.

**Root cause — the looper's STATE and CONTROL live inside the chain's audio
runtime, which is volatile.** The recorded material and the transport state (the
`LooperBank`) sit inside `ChainRuntimeState`. That runtime is created, destroyed
and rebuilt asynchronously and often — cold-start activation, every enable
toggle, every block edit, navigating away and back. Transport ops (record, stop,
clear) are queued to that runtime and only applied inside its audio callback, so
when the callback is not running the ops never land; a rebuild throws the bank
away, losing slots; the published status the UI and the reconciler read goes
stale. Playback, by contrast, already runs on an independent worker
(`IsolatedSource::Looper`) — so it keeps sounding while control is dead. That
split is the whole bug family.

## The model (owner's framing)

Everything is a **stream = SOURCE → PRESET (block graph) → OUTPUT**. Only the
source differs:

| Stream  | Source                                             |
|---------|----------------------------------------------------|
| Guitar  | the interface's hardware input                     |
| DI      | an audio file                                      |
| Looper  | a buffer recorded **clean (dry)** from a source    |

The looper is not a special subsystem bolted onto the chain runtime — it is just
another stream whose source is a recorded buffer. It must live where the DI
already lives: as an independent stream owned by the controller, decoupled from
the chain runtime.

### Tone independence via a preset reference

The loop records **dry** (pre-effects). Each loop carries a **preset reference** —
which preset it plays through:

- On record, the loop captures the preset active at that moment as its reference.
- Playback is `dry buffer → the referenced preset's block graph → chosen output`.
- Editing that preset's blocks re-renders the loop in the new tone (it is a
  reference, not a frozen snapshot).
- The looper UI can **reassign** which preset a loop plays through.
- Consequence: record on preset A (the loop stays on A), switch live to preset B
  to solo — the loop keeps A's tone while you play B. Different tones at once,
  because the loop points at its OWN preset, not the currently-active one.

## Design

### Where looper state lives

Move looper state OUT of `ChainRuntimeState` into the controller, keyed by
`(ChainId, uid)` — exactly where DI stream state already lives. The controller
owns, per loop:

- the recorded **dry** PCM (the layers), the transport state (Empty / Recording
  / Playing / Overdubbing / Stopped), undo/redo history, and the parameters
  (level, decay, speed, reverse);
- the **preset reference**, the chosen **input** (record-from) and **output**
  (play-to) endpoints.

Nothing looper-related remains inside `ChainRuntimeState`. The guitar hot path is
therefore **untouched** — no change to per-segment block processing, so the
audio invariants (latency, isolation, zero-alloc on the audio thread) cannot
regress from this move.

### Recording — a tap, off the audio thread

Recording captures the chosen source's **dry** signal through a lock-free **tap
ring**, the same mechanism the meters already use (`build_streams_from_taps` /
the per-stream input taps). A dedicated looper worker thread drains the ring into
the loop's PCM buffer — never on the audio thread (invariant #8: no alloc/lock/
syscall there). This is the only piece that needs the source stream running:
there must be live input to capture. Stop/clear/undo/redo/play do NOT — they
mutate controller-owned state directly and are deterministic.

Because recording rides a tap rather than the chain's block-processing callback,
it survives whatever the chain runtime does: the tap ring is drained regardless
of rebuilds, and the buffer lives in the controller.

### Playback — the existing isolated stream

Playback is already an isolated stream (`IsolatedSource::Looper(uid)`) and stays
so. Its source becomes the controller-owned dry buffer; its graph is built from
the **referenced preset** (not the live chain); its output is the loop's chosen
endpoint. Arm/disarm are driven by the loop's transport state, which now lives in
the controller — no dependency on a stale published status.

### Control — deterministic

`record` / `stop` / `clear` / `undo` / `redo` / `play` operate on the
controller's looper state and immediately reconcile the isolated stream (arm on
play, disarm on stop/clear/remove). No queue to a volatile runtime, no
suppression hacks, no re-arm races — those existed only to paper over the stale
status and are removed.

### Parity (Command / Query — the architecture law)

Every operation stays a `Command`, so MCP/gRPC/MIDI inherit it (the existing
auto-derived tool-per-variant parity guard must stay green). The read side stays
the `openrig://chains/<id>/loopers` resource, extended with the preset reference.
The whole looper must be drivable and readable over MCP end-to-end — this is the
acceptance harness (below).

## Phasing

The redesign is delivered in two phases, each independently testable and
shippable.

**Phase 1 — stability (kills the bug family).** Move looper state to the
controller; record via a tap drained by a looper worker; control ops mutate
controller state and reconcile the stream directly; playback sources the
controller buffer. Loops survive activation, enable toggle and reopen. Tone for
now = the chain's active preset (Phase 2 makes it a per-loop reference). This
phase removes `LooperBank` from `ChainRuntimeState` and the suppression/slot-sync
band-aids.

**Phase 2 — tone independence.** Add the per-loop **preset reference**:
`LooperConfig.preset_ref`, captured on record, reassignable in the looper UI;
playback builds its graph from the referenced preset; a preset edit re-renders
the loop. The looper row's parameter drawer gains a **preset picker**.

## Testing / acceptance

- **Unit (engine/controller):** loop state transitions, undo/redo, decay/level/
  reverse on the exported mixdown, and — the regression that matters — stop/clear/
  remove take effect with NO chain runtime running (the exact failure the current
  design cannot pass).
- **Integration (adapter-gui):** record → close → play → stop → clear → remove
  through the real `apply_looper_event` path, asserting the isolated stream arms/
  disarms on intent, and that a chain rebuild/toggle between record and playback
  does not lose the loop.
- **End-to-end via MCP (owner's requirement):** drive `AddChainLooper`,
  `SetChainLooperTransport{Record/Stop/Clear}`, and read
  `openrig://chains/<id>/loopers` against the running app; the reported state must
  match what was driven. This is how the redesign is proven on the real rig, not
  just in unit tests.
- **Invariants:** the guitar path is untouched; golden/volume/isolation tests
  must stay green, and the real-hardware battery (`OPENRIG_HW_TESTS=1`) records +
  overdubs + undo/redo/clear with zero xruns.

## Out of scope

- Saving a loop as a reusable DI source (#827) — the controller-owned dry buffer
  makes it trivial later, but it is not part of this redesign.
- Waveform trim/crop editing (#826).
- Speed (½×/2×) applied to the isolated playback — tracked follow-up.
