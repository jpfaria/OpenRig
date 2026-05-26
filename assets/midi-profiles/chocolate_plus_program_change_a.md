# Chocolate Plus — Program Change A (factory)

Default profile for the **M-Vave Chocolate Plus** when CubeSuite is set
to `Mode selection → Program change A` (the out-of-the-box mode).

| Field | Value |
|---|---|
| Source (MIDI port substring) | `FootCtrlPlus` |
| Channel | 1 |
| Message type | `Program Change` |
| Bank N → PC range | A=`4(n-1)`, B=`+1`, C=`+2`, D=`+3` |

## Bindings

| Bank | Switch | PC | Action |
|---|---|---|---|
| **1 — Preset / Scene nav** | A | 0 | prev_preset |
| | B | 1 | next_preset |
| | C | 2 | prev_scene |
| | D | 3 | next_scene |
| **2 — Chain controls** | A | 4 | prev_chain |
| | B | 5 | next_chain |
| | C | 6 | toggle_active_chain_enabled |
| | D | 7 | toggle_compact_view |
| **3 — Block nav** | A | 8 | prev_block_1 |
| | B | 9 | next_block_1 |
| | C | 10 | prev_block_2 |
| | D | 11 | next_block_2 |
| **4 — Block + global toggles** | A | 12 | toggle_active_block_enabled |
| | B | 13 | toggle_tuner |
| | C | 14 | toggle_output_mute |
| | D | 15 | toggle_spectrum |

Banks **5–32** (PCs 16–127) are unbound by the factory. Clone this
profile in **Settings → MIDI → [Customize]** to add your own bindings
(presets jumps, block param knobs, etc.) without touching the factory
file.

## Pairing the pedal

1. Put the Chocolate Plus in pairing mode (per its manual).
2. macOS: *Audio MIDI Setup → Window → Show MIDI Studio → Bluetooth →
   Connect "Chocolate".* On Windows/Linux follow the platform's
   Bluetooth manager; the device appears as a regular MIDI port
   (typically named "FootCtrlPlus …" — the factory profile filters on
   the `FootCtrlPlus` substring).
3. In CubeSuite, confirm `Mode selection → Program change A` is
   selected. If your pedal was previously customized, re-enter that
   mode and `Export` to the pedal before running OpenRig.
4. Open OpenRig, go to **Settings → MIDI**, activate the profile.

The same Bluetooth radio can only talk to one host at a time: close
OpenRig (or unpair the pedal) before editing in CubeSuite, then reopen
OpenRig.
