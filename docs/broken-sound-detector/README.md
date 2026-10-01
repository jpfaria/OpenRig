# Broken-sound detector (idea, 2026-09-30)

The owner: "we need something that tells us the sound IS broken". Counters
(`underruns`, `latency_trims`) did not move while the sound stayed broken
(#979, 2026-09-30), so they cannot be the alarm.

## The measure that worked

On a broken stream the audio has a **step at the edge of each device buffer**.
Per buffer of N frames: the second difference across the edge
(`|x[n+1] - 2x[n] + x[n-1]|` at the boundary) against the median of the same
measure inside the buffer. A buffer whose edge is > 5x its interior is a
"seam".

| Capture | Seam buffers | Edge / median phase |
|---|---|---|
| Main loopback, broken (2026-09-30) | 73 % | 8.6x |
| Raw HD 8 In 1, broken, recorded outside OpenRig | 95 % | 124x |
| Owner's clean DI takes (`~/.openrig/evaluations/gravity-john-mayer/di/jpfaria-*.wav`) | — | 1.05-1.19x |

## Proposal

- Compute the seam fraction per stream, on the input it receives and on each
  route it writes, outside the audio thread (from the existing taps).
- Expose it in `openrig://routes` (`input_seam_pct`, `output_seam_pct`) and
  light an alarm when it stays above a threshold for a second while the
  input carries signal.
- Input seams high → the device stream arrives broken; input clean and
  output seams high → our processing or hand-off breaks it.
