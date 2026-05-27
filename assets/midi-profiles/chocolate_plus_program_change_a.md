# Chocolate Plus — Program Change A (factory)

Default profile for the **M-Vave Chocolate Plus** when CubeSuite is set
to `Mode selection → Program change A` (the out-of-the-box mode).

| Field | Value |
|---|---|
| Source (MIDI port substring) | `FootCtrlPlus` |
| Channel | 1 |
| Message type | `Program Change` |
| Bank N → PC range | A=`4(n-1)`, B=`+1`, C=`+2`, D=`+3` |

## Hierarchy

Banks 1-4 are bound. Banks 5-32 (PCs 16-127) are left for your custom
mappings (clone the profile via **Settings → MIDI → [Customize]** —
Phase 7 — or drop a YAML in `~/Library/Application Support/openrig/midi-profiles/`
on macOS).

| Bank | Theme | A | B | C | D |
|---|---|---|---|---|---|
| **1** | Chains | prev_chain | toggle_active_chain_enabled | toggle_compact_view | next_chain |
| **2** | Preset / Scene | prev_preset | next_preset | prev_scene | next_scene |
| **3** | Block pair | prev_block_2 | toggle_active_block_enabled | toggle_active_block_neighbor_enabled | next_block_2 |
| **4** | Global toggles | toggle_tuner | toggle_output_mute | toggle_spectrum | *(unbound)* |

## How bank 3 works with compact view

Compact view shows the active block + the next one side by side.
With this profile, your foot has the natural mapping:

- **A** — step the visible pair 2 blocks back.
- **B** — bypass / engage the **left** block of the pair (the active one).
- **C** — bypass / engage the **right** block of the pair (the neighbor).
- **D** — step the visible pair 2 blocks forward.

## Pairing the pedal

1. Put the Chocolate Plus in pairing mode (per its manual).
2. macOS: *Audio MIDI Setup → Window → Show MIDI Studio → Bluetooth →
   Connect "Chocolate".* On Windows/Linux follow the platform's
   Bluetooth manager; the device appears as a MIDI port named
   `FootCtrlPlus …` (factory profile filters on that substring).
3. In CubeSuite, confirm `Mode selection → Program change A`. If the
   pedal was customized, switch back to "Program change A" and Export
   to the pedal before running OpenRig.
4. Open OpenRig with `--midi` (or wire the auto-start), open a
   project, and pisar.

The same Bluetooth radio can only talk to one host at a time: close
OpenRig (or unpair the pedal) before editing in CubeSuite, then reopen
OpenRig.
