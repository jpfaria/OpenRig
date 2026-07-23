# Command coverage audit — every user action should be a Command (#22)

**Principle (from the command-dispatch architecture):** the GUI carries
no business logic — *every* user-meaningful action is a `Command` the
dispatcher applies, so MIDI / MCP / GUI all do the same thing the same
way. MIDI is just another producer; it can only reach what is a Command.

**Reality today:** the `Command` enum has **54** variants. The desktop
GUI still exposes more user-action callbacks than that — the remainder
listed below is the live gap. That is the audit; it is an architectural
program (done red-first, incrementally), not a single change.

**Recent additions:**
- #323: `AddChainLooper`, `RemoveChainLooper`, `SetChainLooperTransport`,
  `SetChainLooperParam`, `SetChainLooperAudioFile` (5). The transport backs the
  MIDI slots `looper_record`, `looper_play_stop`, `looper_undo`,
  `looper_clear` — a footswitch sends looper uid `0`, the sentinel the
  dispatcher resolves to the chain's first looper. `LooperAction::PlayStop` is
  the one-button toggle: the adapter resolves it against the runtime, so pedal
  and screen behave identically.
- #513 / #493: `SaveMidiDevices`, `SaveMidiMapping`, `StartMidiLearn`,
  `StopMidiLearn`, `PublishMidiEvent` (5).
- #548 Phase 3a: `SelectActiveChainRelative`, `SelectActiveBlockRelative`,
  `SetCompactViewEnabled` (3). These back the MIDI slots `prev/next_chain`,
  `prev/next_block_1`, `prev/next_block_2`, `toggle_compact_view` — see
  [`docs/midi-profiles.md`](midi-profiles.md).

## The 34 that ARE commands (work via MIDI today)

Block params (6), block lifecycle/edit (7), chain CRUD/order/save (10),
chain I/O + preset (4), project/audio (5), rig nav + block selection +
capture (4: `ApplyRigNav`, `SelectChainBlock`, `RenameRigPreset`,
`CaptureRigEdits`). Full list with args + the MIDI message to set:
[`docs/midi.md`](midi.md).

## NOT commands yet — must become commands

Grouped by area (representative GUI callbacks; the user explicitly
called out block-click, compact view, latency test, opening configs):

- **Selection / click:** `select_chain_block`, `open_block_detail`,
  `toggle_block_drawer_enabled`, `close_block_drawer`,
  `delete_block_drawer` — clicking a block, opening its drawer.
- **View toggles & windows:** `open_compact_chain_view`,
  `close_compact_view` ✅ (#548 Phase 3a: `SetCompactViewEnabled`),
  `open_spectrum_window`, `close_spectrum`,
  `open_tuner_window`, `close_tuner`, `toggle_spectrum_enabled`
  ✅ (existing `SetSpectrumEnabled`, exposed via MIDI slot
  `toggle_spectrum` since #548 Phase 3d),
  `toggle_tuner_enabled` ✅ (`SetTunerEnabled`, MIDI slot
  `toggle_tuner`),
  `toggle_tuner_mute`, `show_plugin_info`,
  `open_plugin`, `open_vst`, `close_plugin_info`.
- **Latency / probe:** the per-chain latency probe trigger (latency
  badge) — running a measurement.
- **Screen navigation:** `back_to_launcher`, `open_homepage`,
  `open_recent_project`, `open_project_file`, `filter_recent_projects`,
  `configure_project`, `close_project_settings`.
- **Language / app:** `change_language`.
- **Audio & device config (Settings screen):**
  `update_input_sample_rate`, `update_output_sample_rate`,
  `update_input_buffer_size`, `update_output_buffer_size`,
  `update_project_sample_rate`, `update_project_buffer_size`,
  `update_project_bit_depth`, `toggle_input_device`,
  `toggle_output_device`, `toggle_project_device`, `select_*_device`,
  `select_*_mode`, `toggle_*_channel`, `toggle_mute`.
- **Project setup wizard:** `go_to_input_step`, `go_to_output_step`,
  `create_project_file`, `confirm_new_project`, the I/O group editors
  (`chain_io_groups_*`, `configure_chain_input/output`, `edit_*`).
- **Per-chain rig (already partly commands):** `switch_chain_preset`,
  `switch_chain_scene` dispatch `ApplyRigNav` ✅; `clear_chain_block`,
  `start_block_insert` do not yet.

> Pure dialog plumbing (`cancel`, `confirm_delete_block`,
> `cancel_*`, `virtual_key_pressed`, window `close_requested`) is
> arguably UI-internal and may legitimately stay non-command — the rule
> is *user-meaningful state change or navigation* = a Command.

## How this gets fixed

Each becomes a `Command` the dispatcher owns, the GUI just dispatches
it, MIDI/MCP get it for free — exactly how `SelectChainBlock` /
`RenameRigPreset` / `ApplyRigNav` were done. TDD red-first per
command; doc (`docs/midi.md` table + this audit) updated in the same
commit; `COMMAND_VARIANT_COUNT` bumped each time.

Order of attack (highest live-performance value first): selection &
view toggles (compact view, tuner/spectrum, block drawer) → screen
navigation → latency probe → audio/device config → wizard. This is the
remaining body of work after the standard map; it is large and tracked
here so nothing is hidden.
