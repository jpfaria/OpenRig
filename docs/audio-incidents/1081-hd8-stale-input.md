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
- A voice-processing client explains every episode: **REFUTED** — the owner
  reports the break with and without WhatsApp running, and the marks agree:
  no mark lists an aggregate on the HD 8, and in 4 of the 10 marks
  (2026-10-05 23:01, 2026-10-06 18:11:36, 18:12:06, 18:12:36 UTC) OpenRig was
  the only process with the HD 8 open. What starts those episodes is unknown.
- OpenRig's own periodic device scan starts the episodes OpenRig is alone in:
  **OPEN, strongest candidate** — OpenRig re-reads the device list every
  10 s (`device_cache`, `DEVICE_CACHE_TTL`) while chains play. Measured
  2026-10-07 13:07:41 -03:00 in the system log, one scan of 6 output
  devices: 31 `setPlayState Stopped` from OpenRig's own process, 15 of them on
  the HD 8 (12 Output, 3 Input), i.e. an IOProc started and stopped on the
  running HD 8 inside OpenRig's own IO context 15 times per scan, forever.
  Source: cpal 0.17.3 `Device::description()` calls
  `supported_input_configs()` and `supported_output_configs()` uncached, and
  each builds an AUHAL, which attaches to the default output (the HD 8)
  before it is pointed at the device asked about. Fixed in the scan (see
  Shipped). Not measured against an onset: the marks' 3 s of input start
  already stale, so the onset is earlier than the mark; the next weeks
  without a mark are the evidence.
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
- The stepped-input restart as #979 shipped it cures it: **REFUTED** —
  coreaudiod keeps one IO
  context per process per device, alive while any of the process's IOProcs on
  that device runs (2026-10-07: two IOProcs in one process on the HD 8 gave one
  `IOWorkLoopInit` and one `IOWorkLoopDeinit`, only at the last stop). The
  restart opens the new streams before closing the old ones, so OpenRig's
  context, the one that broke at 15:07, is never torn down. The owner's
  restarts on 2026-10-06 18:10–18:12 UTC left the input stepped.
- Stopping every OpenRig stream on the HD 8 and starting them again (a fresh
  context, a fresh `Register_IOThread`) clears it when OpenRig's context is
  the broken one: **OPEN** — not measured; the test was cut by the coreaudiod
  crash. The stepped-input restart now does exactly this (Shipped), so its
  next marks on the rig answer it.

## Shipped

- `7d80a3c3f` — new streams crossfade in over the old ones and a fresh VST3 is
  built ahead: the IR swap, the VST3 enable and the stepped-input restart no
  longer click or gap. It does not touch the stale input.
- Device restart (`infra-cpal/src/controller_device_restart.rs`) — when an
  input trips the stepped-input detector, every OpenRig stream on the device
  it reads is closed (each chain that reads or plays there, the metronome,
  the player, the drums, the isolated DI and looper outputs), and after
  100 ms opened again, so coreaudiod tears OpenRig's IO context down and
  builds a new one. Self-recovery, not a fix of the cause. Not measured yet:
  whether the new context comes back clean, the length of the cut, whether
  the 100 ms pause is enough, and whether starting IO again on a broken
  engine can crash coreaudiod as it did once (Hypotheses).

- Every mark keeps the system's audio log of the 5 minutes before it
  (`system-audio-log.txt`, `adapter-gui/src/stepped_input_system_log.rs`), so
  the next episode shows what the system and OpenRig did at the onset.

- Device names are read once per device list (`infra-cpal/src/device_name_cache.rs`):
  the 10 s scan no longer calls cpal's `description()`, so it no longer
  starts and stops IOProcs on the HD 8. A hot-plug or a settings save still
  re-reads them once. Measured on the owner's app after it (13:36 -03:00): a
  scan still built one AUHAL per output device, 7 IOProc starts and stops on
  the HD 8 — cpal's `output_devices()` / `input_devices()` filter each
  device through `supported_*_configs()`, which builds an AudioUnit, and the
  hot-plug counter called both.
- The device lists come from `infra-cpal/src/device_list.rs`: the host's
  whole list, kept by direction from the cached configs, so a scan of an
  unchanged device list builds no AudioUnit. The hot-plug counter counts the
  unfiltered list. `device_list_tests` fails if any other file of the crate
  calls cpal's filtered lists or `description()`. Measured on the owner's
  app with it (pid started 13:43 -03:00): the 39 AudioUnits are all built at
  13:46:09, the first enumeration at startup, before any chain plays; the
  six scans of 13:52:15–13:53:15 built none.

## Open

- Why the HD 8 driver stops answering the HAL's eventlink after the IO
  contexts restart.
- Whether the coreaudiod crash comes from the same broken engine state (one
  occurrence).
- What OpenRig can do on its own while another app's context is the broken
  one.
- What starts the episodes with OpenRig alone on the HD 8 — the next mark's
  `system-audio-log.txt` is the first evidence of it.

## Related

The stepped-input detector (`docs/audio-config.md`, "Stepped-input detector")
trips on this signature and writes the marks used above.
