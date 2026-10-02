# Drum machine

A built-in play-along drum machine: pick a groove, press play, ask for a fill.
It plays on its own output stream, apart from every chain, the same way the
metronome does.

## Engine (`crates/feature-dsp/src/drums/`)

| Piece | What it is |
|---|---|
| `DrumRole` | The kit pieces a groove can address (kick, snare, hats, toms, cymbals...). Grooves never use a kit's note numbers; each kit maps its instruments to roles. A role the kit lacks falls back to the nearest piece it has (`fallback()`), so any groove plays on any kit. `from_general_midi` maps GM percussion notes. |
| `DrumKit` / `DrumPiece` / `DrumLayer` | Mono samples per role, in velocity layers; each layer can hold alternate samples that play round-robin. A piece has gain, constant-power pan and an optional choke group (closed hat cuts the open hat with a 10 ms fade; a piece re-hit by itself keeps its previous hit ringing, so a crash played twice does not cut itself). Samples play one to one, so a kit is prepared at the stream's sample rate before it reaches the engine. |
| `DrumPattern` / `Groove` | Hits positioned in beats. A groove has a main loop (one or more bars) and fills used in turn. A fill is one bar, plus an optional landing (the crash on the next downbeat) past the bar line. |
| `DrumMachine` | Renders the groove on the kit into a stereo buffer. |

### Timing

Time is counted in whole frames since play. A hit maps to the first frame at or
after its beat, so it lands on the same sample whatever the buffer size; a
tempo change keeps the position in the bar.

### Fills

Asked for mid-bar, the fill takes over from the current position to the end of
the bar. Asked for in the last beat of a bar, it plays the whole next bar. The
groove resumes after the fill bar; a fill's landing hits play on top of it. A groove with no fills ignores the request.

### Real-time safety

`render` never allocates, locks or blocks: 64 preallocated voices (the oldest
is stolen when all are busy), voices refer to kit samples by index, and a kit
or groove swap hands the previous one back to the caller to drop off the audio
thread. Stop ends new hits; hits already sounding ring out.

## Kits and grooves (`crates/application/src/drums/`)

A drum folder holds `kits/<id>/` and `grooves/<genre>.yaml`. The app ships
one in `assets/drums/` (licenses in `assets/drums/README.md`).

- **Kits** are Hydrogen kit folders: `drumkit.xml` plus its WAVs. Layers
  sharing one velocity range play round-robin; `muteGroup` becomes the choke
  group; `pan_L`/`pan_R`, `volume` and the gains set level and pan.
- **Roles** come from the instrument names (`Pearl-22-Kick` is the kick,
  `HatSemi` the open hat, the second crash is `crash2`); articulations no groove
  plays (chokes, swishes, tom rims, ride shanks) are left out, and the General
  MIDI note fills any role still empty. A `roles.yaml` in the kit folder
  (`role_key: Instrument Name`) overrides both.
- **Loading** decodes every sample, folds it to mono and resamples it to the
  stream rate before the kit reaches the engine. A missing sample fails the
  whole kit.
- **Grooves** are one YAML file per genre; each hit is
  `[beat, role, velocity 0-127]`. The bundled ones are converted from the
  Groove MIDI Dataset by `tools/drums/gmd_to_grooves.py`: a two-bar loop from
  each performance, and fills taken from bars that end on a crash.
- `scan_drum_library` lists kits (reading only `drumkit.xml`) and grooves;
  a bad file is logged and skipped.
- The library scanned is the bundled folder plus the user's own
  `drums/` folder under the app data root (`user_drum_dir()`), in that order.

## Playback

- **Commands** (`DrumsCommand`, so the GUI, MCP and MIDI share them):
  `SetDrumsEnabled`, `PlayDrums`, `StopDrums`, `ToggleDrums`,
  `TriggerDrumFill`, `SetDrumsBpm`, `SetDrumsVolume`, `SelectDrumKit`,
  `SelectDrumGroove`, `SetDrumsOutput`. Play opens the output when it is
  closed; Stop only stops the transport; `SetDrumsEnabled { false }` closes
  the output. A fill while stopped is an error.
- **Read**: `QueryKind::DrumsState`, served on MCP as `openrig://drums`
  (settings, kit and groove lists, live bar and beat).
- **MIDI** slots: `toggle_drums`, `drum_fill`.
- **Persistence**: BPM, volume, kit, groove and output go in the machine's
  `config.yaml` under `drums:`.
- **Output**: the drums open their own cpal stream on the chosen output
  endpoint (the first the drums offer when none is saved), like the metronome.
  Only Play may create the audio runtime, so the drums sound with no chain
  enabled. A kit is decoded at the stream's rate on a worker thread and
  handed to the callback lock-free; a newer load overtakes an older one, and
  a kit at the wrong rate is never played. On the JACK backend (Linux) the
  drums are a JACK client of their own, `openrig_drums`, on the endpoint's
  server, with one port per channel of the pair connected to that channel's
  playback port (`drums_jack_stream.rs`); JACK sums it with the chains.

## Panel (`crates/adapter-gui`)

- **Where**: the drum icon on the top bar opens `DrumsWindow` (windowed
  desktop) or the inline panel (fullscreen / touch); the Compact Chain View
  has a DRUMS section with the same panel, without its header. All of them
  read the `DrumsBridge` global and drive the one global drum machine.
- **What**: POWER, an LCD with a lamp per beat of the groove's bar, the tempo,
  the bar and FILL while a fill plays; KIT and GROOVE pickers (grooves under a
  header per genre, in library order); PLAY/STOP and FILL footswitches (FILL
  only while playing); BPM (the engine's range) and VOLUME knobs; OUTPUT.
  Every picker list opens with a search box that narrows it as you type
  (`drums_picker_filter.rs`; a genre header stays while a groove under it
  matches, and typing the genre keeps the whole genre).
- **Outputs**: only the endpoints of output-only bindings (no inputs), since
  an in+out binding repeats an output another binding already names; with no
  output-only binding, every output (`drums_outputs.rs`).
- **Wiring**: `drums_intents.rs` maps each control to its `DrumsCommand`;
  `drums_view.rs` builds what the panel shows from the snapshot, the library
  and the project's output endpoints; `drums_wiring.rs` (window and inline)
  and `compact_drums_wiring.rs` (each compact view) redraw when the
  dispatcher's state changes and move the lamps from `LiveSource::drums`
  every frame.
