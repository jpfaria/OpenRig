# 1081 — HD 8 input goes stale after a voice-processing client starts

## Symptom (reported)

2026-10-06, while playing the VIOLAO chain (`rig:input-6`: IR → VST3
CloudReverb → VST3 ValhallaSupermassive): the sound broke into a buzz ("caixa
de abelha", a beehive) and stayed broken ("o som ta completamente quebrado").
The owner listens to Spotify on the same HD 8 and never hears it there.

## Symptom (measured)

- **Stale frames.** Inside every input buffer the HD 8 delivers, a run of
  frames is bit-identical to the input 11025 frames earlier — the driver's IO
  ring (zero-timestamp period 11025, 250 ms at 44.1 kHz). Every channel of the
  device carries it, so every chain on the HD 8 breaks at once. The step it
  leaves at the same position of every buffer is the buzz the stepped-input
  detector trips on.
- **Geometry.** OpenRig at 64 frames: a run of 8–17 frames starting at
  position 17, where the previous IO cycle woke (marks of 2026-10-05 13:56:23
  and 23:01:16 UTC, 2026-10-06 18:10:20–18:12:36 UTC). A 64-frame and a
  512-frame reader together: 36–38 frames once per 512 frames, the same frames
  in both, ending exactly at the 512-frame reader's window end. Two 512-frame
  readers alone: 121 of every 512 frames on average (24 %; min 6, max 358), in
  79–87 of 86 buffers per second.
- **Onset.** Each episode starts right after coreaudiod logs
  `HALS_IOUAEngine: Timed out signalling event link` (HALS_IOUAEngine.cpp:281
  or :373) for the HD 8's AudioDriverKit engine (`com.fender.tusbaudiodriver`
  1.17.0). Real incident: 2026-10-06 15:07:04.154 and 15:07:10.575 (-03:00),
  with OpenRig (64), OBS (512) and WhatsApp's `CADefaultDeviceAggregate` on
  the HD 8; OpenRig's next IO cycle broke its safety offset and was
  re-anchored.
- **Duration.** It stays while the device's IO runs. The underrun, trim and
  xrun counters do not move.

## Rig

PreSonus Quantum HD 8 (Fender TUSBAudio AudioDriverKit dext 1.17.0),
CoreAudio, 44100 Hz / 64 frames, macOS 26.1 (25B78). Other HD 8 clients at the
time: OBS (input, 512 frames) and WhatsApp (a voice-processing aggregate of the
default devices: the HD 8 output and the MacBook mic).

## Reproduce

OpenRig closed, three small CoreAudio clients (scratch C programs, not in the
repo; run details in #1081):

- a stale detector: an input reader on the HD 8 at 64 or 512 frames that
  compares every sample with the sample 11025 frames earlier;
- a `VoiceProcessingIO` unit started on the default devices, which is what
  WhatsApp starts for a call;
- `log stream --predicate 'process == "coreaudiod" AND eventMessage CONTAINS "event link"'`.

Start the voice-processing unit while the detector runs. Its start stops and
restarts every HD 8 IO context (`_StopIO`, `IOWorkLoopInit`, `_StartIO`);
usually within 1–15 s coreaudiod logs the eventlink timeout and the detector's
buffers go stale. **Warning:** after one such run coreaudiod crashed when the
next client started IO on the HD 8 (see Hypotheses), which drops every app's
audio for about a second.

## Hypotheses

- OpenRig's DSP or chain (IR, VST3) causes it: **REFUTED** — reproduced with
  OpenRig closed, 5 runs.
- OpenRig's 11 s device scan is needed: **REFUTED** — reproduced without it.
- A 64-frame client is needed, or OpenRig's 64-frame buffer makes it worse:
  **REFUTED** — two 512-frame readers and nothing else: 3 timeouts at the first
  call start, then 24 % of every buffer stale in both.
- Client-side load breaks it (8–11 realtime workers, stalled callbacks, memory
  pressure, `skipping cycle due to overload`): **REFUTED** — 0 timeouts, 0
  stale frames.
- The frames are missing in the driver's ring, not misread by one client:
  **CONFIRMED** — a 64-frame and a 512-frame reader got the same holes at the
  same device sample times.
- A voice-processing client on the default devices triggers it:
  **CONFIRMED** for `VoiceProcessingIO` (4 runs). For WhatsApp the evidence is
  strong but correlational: CoreAudio's analytics for the 15:07:04 overload
  list `net.whatsapp.WhatsApp` at 512 frames, and its aggregate ran
  15:06:18.765–15:07:31.074.
- Output-only clients (Spotify) are hit: **OPEN** — they never read the input,
  so they cannot show it; the output path was not measured.
- The engine is clean again once every client stops the HD 8: **OPEN** — a
  new client was clean right after one broken run; but 85 s after another,
  with every client stopped, the second client to start IO on the HD 8 crashed
  coreaudiod (2026-10-06 22:41:52.730 -03:00): `EXC_BAD_ACCESS`, possible
  pointer authentication failure, in `HALS_IOUAEngine::Register_IOThread`
  called from `HALS_IOContext_Legacy_Impl::IOWorkLoopInit`, on the
  `audio IO: TUSBAudio:Fender:Quantum HD 8` thread. launchd respawned it 40 ms
  later, the kernel aborted the HD 8's USB pipes and every app's audio dropped
  for about 1 s.
- The stepped-input restart cures it: **OPEN** — its new streams come up while
  the old ones still run, so the HD 8 never stops. Whether a fresh OpenRig IO
  context escapes the holes when OpenRig's own context is the broken one was
  not measured: the test was cut by the coreaudiod crash.

## Shipped

- `7d80a3c3f` — new streams crossfade in over the old ones and a fresh VST3 is
  built ahead: the IR swap, the VST3 enable and the stepped-input restart no
  longer click or gap. It does not touch the stale input.

## Open

- Why the HD 8 driver stops answering the HAL's eventlink after the IO
  contexts restart.
- Whether the coreaudiod crash comes from the same broken engine state (one
  occurrence).
- What OpenRig can do on its own while another app's context is the broken
  one.

## Related

The stepped-input detector (`docs/audio-config.md`, "Stepped-input detector")
trips on this signature and writes the marks used above.
