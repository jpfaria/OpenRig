# 978 — Windows ASIO: no sound, constant xruns, stepped-input restart loop

Signature: `audio overload … 2 new xrun(s), 0 new underrun(s)` several times a
second, `input arrives stepped, restarting every stream on asio:<driver>` every
~30 s, each restart followed by `[ui-stall] … ~751ms`.

## Measured (2026-10-08, MSI 0.7.0-beta.1, Focusrite USB ASIO)

- Machine: Intel i5-1334U (laptop U-series, 10 cores / 12 threads), 16 GB.
- Chain: NAM Klon (gain) + split with two NAM amp paths, mono input at 44.1 kHz,
  ASIO buffer = the driver's own setting (`BufferSize::Default`).
- With the chain disabled: no xrun. Enabled: xruns from the first second.
- `openrig.exe` CPU: 113 % of one core.
- Output route 0 peak −20.7 dBFS (the app renders signal); route 1 −87.8 dBFS.
- Every stepped-input mark: `underruns 0`, `dropped_frames 0`, `streams: []`.

## Hypotheses

- One pipeline runs the three NAM models serially on one thread and cannot
  finish a buffer at this driver buffer size on this CPU: **OPEN** — matches
  the 113 % of one core; next measurement is a larger ASIO buffer and the chain
  with one NAM.
- The stepped-input detector trips on the xrun gaps (not on a broken device
  input) and its device restart, built for coreaudiod (#1081), only adds
  silence and a UI stall on ASIO: **OPEN**.
