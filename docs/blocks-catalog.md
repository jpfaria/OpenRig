# Blocks catalog

## Block types

A block type is a slot in the chain; a model is the unit it runs. The built-in (native) models below are compiled into OpenRig. Everything else — NAM captures, impulse responses, LV2 and VST3 plugins — comes from [OpenRig-plugins](https://github.com/jpfaria/OpenRig-plugins), whose model list is in its `docs/blocks-reference.md`.

| Type | What it does | Built-in models |
|------|--------------|-----------------|
| **Preamp** | Preamp: gain and tone stack, no power amp or cab | Modern High Gain, Brit Crunch, American Clean |
| **Amp** | Preamp + power amp + cab | Tweed Breakup, Blackface Clean, Chime |
| **Cab** | Speaker cabinet | Brit 4x12, American 2x12, Vintage 1x12 |
| **Gain** | Overdrive, distortion, fuzz, boost, saturation, volume | TS9 Tube Screamer, Tube Saturation, Fuzz Face (Ge), Fuzz Face (Si), Wavefolder (Buchla), Octave-Up (Half-Wave), Sub-Octave (-1 oct), Transformer Saturation, Tape Saturation, Bitcrusher, Volume |
| **Delay** | Echo | Digital Clean, Analog Warm, Slapback, Reverse, Modulated, Tape Vintage, Tape Echo, BBD Analog, Multi-Tap, Rhythmic, Chorus Echo, Ping-Pong, Pitch, Granular — see [Native delay algorithms](#native-delay-algorithms) |
| **Reverb** | Ambience | Room, Hall, Cathedral, Plate Foundation, Plate (Dattorro), Spring Reverb, Spring (Parker 2010), Freeverb (canonical), FDN Reverb (Jot), Modulated/Lush, Shimmer, Gated, Reverse Reverb |
| **Modulation** | Chorus, flanger, phaser, tremolo, vibrato, rotary | Classic Chorus, Stereo Chorus, Ensemble Chorus, Classic Flanger, Subtle Flanger, Jet Flanger, Classic Phaser, 4-Stage Phaser, 8-Stage Phaser, Sine Tremolo, Vibrato, Rotary Leslie, Rotary Leslie (Vintage), Rotary Leslie (Studio), Ring Modulator, Frequency Shifter |
| **Dynamics** | Compressor, gate, limiter | Studio Clean Compressor, Noise Gate, Brick Wall Limiter |
| **Filter** | EQ and tone shaping | Three Band EQ, Guitar EQ, Guitar HPF/LPF, 8-Band Parametric EQ |
| **Wah** | Wah and auto-wah | Cry Classic, Auto-Wah (Classic), Auto-Wah (Funk), Auto-Wah (Dub), Pitch-Tracking Wah, Talk Box |
| **Pitch** | Pitch shift and harmony | Pitch Shifter |
| **Body** | Acoustic body resonance | — (IR captures from OpenRig-plugins) |
| **IR** / **NAM** | Load any `.wav` impulse response / `.nam` capture from disk | Impulse Response (`generic_ir`), Neural Amp Modeler (`neural_amp_modeler`) |
| **Input** / **Output** / **Insert** | I/O | standard input, standard output, external loop |
| **Split** | Divides the signal into N paths that end in a Mix or a Y — see [Chain split](#chain-split) | split |

**Utility** and **Full rig** have no models yet.

An LV2 package is offered only when it ships a binary for the platform OpenRig runs on; otherwise it is skipped at load.

## Common parameters

- **Native preamp/amp**: input, gain, bass, middle, treble, presence, depth, sag, master, output, bright.
- **Delay**: `time_ms` (1–2000), `feedback` (0–100 %), `mix` (0–100 %). Every native delay shares these three; some add character knobs — see [Native delay algorithms](#native-delay-algorithms).
- **Reverb (native)**: `room_size`, `damping`, `mix` (0–100 %); some models add more (Hall: `pre_delay_ms`).
- **Reverb (IR convolution, `backend: ir`)**: `mix` (0–100 %, default 30 %), `pre_delay_ms` (0–200), `level` (wet trim, −24..+24 dB). Convolves a reverb impulse response (same FFT engine as the IR cab) and blends it with the dry signal, unlike a cab block (100 % wet). True stereo for 2-channel IRs; a mono IR runs dual mono in a stereo chain.
- **Compressor** (`compressor_studio_clean`): `threshold`, `ratio`, `attack_ms`, `release_ms`, `makeup_gain`, `mix`.
- **Limiter** (`limiter_brickwall`): `threshold`, `ceiling`, `release_ms`, `lookahead_ms`, `knee_db`.
- **Gate** (`gate_basic`): `threshold` (−96..0 dB), `attack_ms` (0.1–100), `release_ms` (1–500), `hold_ms` (0–2000, default 150 — keeps the decay from being cut), `hysteresis_db` (0–20, default 6 — prevents chattering).
- **Three Band EQ** (`eq_three_band_basic`): `low_gain`/`mid_gain`/`high_gain` (−12..+12 dB), `low_freq`/`mid_freq`/`high_freq` (Hz), `mid_q` (0.1–6).
- **Guitar EQ** (`native_guitar_eq`): 4 boost/cut bands tuned for guitar, acoustic and bass — `low` (low shelf @150 Hz), `low_mid` (peak @500 Hz, Q 0.7), `high_mid` (peak @2.5 kHz, Q 0.7), `high` (high shelf @6 kHz). Each band −12..+12 dB, default 0.
- **Guitar HPF/LPF** (`native_guitar_hpf_lpf`): cleanup filter pair — `low_cut` 0–100 % sweeps the HPF 20→100 Hz, `high_cut` 0–100 % sweeps the LPF 20 kHz→7 kHz.
- **8-Band Parametric EQ** (`eq_eight_band_parametric`): per band — `band{N}_enabled`, `band{N}_type` (peak/low_shelf/high_shelf/low_pass/high_pass/notch), `band{N}_freq` (20–20000 Hz), `band{N}_gain` (−24..+24 dB), `band{N}_q` (0.1–10). Default frequencies: 62/125/250/500/1k/2k/4k/8k Hz.
- **Native gain pedals**: `drive`, `tone`, `level`.
- **Volume**: `volume` (0–100 %), `mute`.
- **Vibrato**: `rate_hz` (0.1–8), `depth` (0–100 %), 100 % wet.
- **Pitch Shifter** (`native_pitch_shifter`): `shift_semitones` (−24..+24), `shift_cents` (−100..+100), `mix` (0–100 %). A time-domain granular shifter (dual-tap overlap-add, the same family as the DigiTech Whammy/Drop): a delay line read by two taps half a grain apart, with Hann windows offset by π that sum to exactly 1 (flat amplitude, no warble). No FFT and no pitch detection, so it handles chords at very low CPU. The grain is 1024 samples: about 12 ms of latency at 48 kHz for −2 semitones, about 5 ms an octave up — comparable to a commercial pedal. A larger grain would mean less grain warble and more latency.
- **NAM gain pedals with a grid**: the pedal's real knobs (`tone`, `sustain`, `drive`, `volume`, `gain`, …) map to the nearest `.nam` capture on the grid. A `_feather`/`_lite`/`_nano` suffix becomes a `size` choice. Pedals with named settings (`chainsaw`, `medium`) or `preset_N` keep a dropdown.
  - **A grid knob always selects the nearest declared capture and keeps a real model loaded — including at the axis minimum.** A capture declared at `drive: 0` (or `level: 0`) is a real capture, not "off": the block makes sound there. On/off is only the block's enable toggle (`set_block_enabled` → `RuntimeProcessor::Bypass`), never a parameter value.
  - **A newly added grid pedal starts at the first declared capture's values**, never at the all-minimum combination. The manifest lists captures in order, so the first one is a real grid point (e.g. a TS9 at drive 0 / tone 6 / level 6) and the pedal is audible at once instead of landing on a 0/0/0 cell with no capture. Seeded in `application::block_factory::build_default_block` via `plugin_loader::dispatch::first_capture_axis_values`.
  - **Dead capture-selector axes are not shown.** `plugin_loader::grid_axes::effective_grid_axes` intersects each declared axis with the values that appear in `captures[].values`: an axis left with one capture-backed value or fewer is dropped (its single capture just loads), and a shown axis lists only capture-backed values (an over-declared `0..100` knob shows only its real captures). The manifest still records which capture/value a single-capture model is; it just stops being a user control.
  - **Three layers enforce this at load (`plugin_loader::discover`) and when building the editor:** (1) `unbacked_grid_values` logs a warning naming the plugin, axis and values for any declared value with no capture behind it — the package still loads; (2) `prune_unbacked_grid_values` strips those values from the in-memory `Backend`, so GUI, MCP and gRPC all see only capture-backed values; (3) `effective_grid_axes` drops an axis left with one value or fewer when building the GUI specs. The YAML on disk is never modified.
- **NAM block (`neural_amp_modeler` and every NAM plugin)**: `input_db`/`output_db`, `noise_gate.enabled`/`noise_gate.threshold_db`, `eq.enabled`/`eq.bass`/`eq.middle`/`eq.treble` (0–10, 5 = flat), `ir_path` (cab).
  - **All are applied** by the official NeuralAmpModelerCore through the `cpp/nam_wrapper` C++ shim (`dsp::noise_gate` + tone stack + `dsp::ImpulseResponse`), built from the vendored archive pinned by `deps/NeuralAmpModelerCore.lock`. The chain is input gain → model → gate → EQ → IR → output gain → peak-safety soft clip.
  - The soft clip is a memoryless saturator; run per sample at the base rate it would fold aliasing back as harsh hiss on hot captures, so it uses first-order antiderivative anti-aliasing (ADAA): zero added latency, and bit-exact below the knee, so a clean tone is untouched.
  - The wrapper folds the model's own calibration (`nam::DSP::GetLoudness()`/`GetInputLevel()`) into the gain staging, so a nonlinear capture is driven at its trained level (output normalized toward −18 dB) instead of raw unity, which sounds dull. That normalization is skipped when the catalog loudness audit already sets the output level (`audit_overrides_baked_output`), so the two never add up.
  - `noise_gate.enabled` **defaults off**: a gate on by default strangles decay and sustain. A plugin can ship the gate preset in the user-visible knobs through its manifest: `noise_gate: { enabled, threshold_db }` at manifest level applies to every capture, and `captures[].noise_gate` overrides it per capture (`plugin_loader::manifest::resolve_noise_gate`). These values are written into the block params when the block is created (`build_default_block`, like `output_gain_db` → `output_db`), so the knob shows them, the user can change them and they are saved; the engine never applies the gate as a hidden default. High-gain captures raise the input noise floor (about +32 dB) into audible hiss, so they ship the gate on; clean captures leave it off.
  - **A2 (`SlimmableContainer`) captures** — `.nam` version 0.7.0, a `config.submodels` array of WaveNet submodels at increasing `max_value` (e.g. 0.5 Lite / 1.0 Full) — are supported. They add a **`slim`** knob, 0–100 % (default 100 % = full), shown only for NAM/A2 packages and passed as a 0..1 ratio through the FFI (`slim_size`) to `nam::SlimmableModel::SetSlimmableSize`: it picks a smaller submodel, trading fidelity for CPU. It is set at load, off the audio thread; the staged submodel installs lock-free on the first `process()`. A1 models are not slimmable and never show the knob.
  - The offline diagnostics API (`nam::processor::{open_model_diag, nam_process, close_model_diag}`) wraps the same FFI and stays public for the OpenRig-plugins catalog audit.

### Native delay algorithms

All native delays are pure Rust, `brand: native`, `backend: Native`. They share `time_ms` (1–2000 ms), `feedback` (0–100 %) and `mix` (0–100 %); the extra knobs are what separate them. Feedback is divided by 100 and clamped to **0.95** internally, so a knob at 100 % is 95 % recirculation and never runs away (`block-delay::delay_math`). All are `DualMono` except Ping-Pong.

| Model id | Display name | Extra knobs | Character |
|---|---|---|---|
| `digital_clean` | Digital Clean Delay | — | Clean digital repeats |
| `slapback` | Slapback Delay | — | Short single slap |
| `reverse` | Reverse Delay | — | Repeats played backwards |
| `analog_warm` | Analog Warm Delay | `tone` | One-pole roll-off per repeat |
| `tape_vintage` | Tape Vintage Delay | `tone`, `flutter` | Tape-style roll-off and wobble |
| `modulated_delay` | Modulated Delay | `rate_hz`, `depth` | LFO-modulated delay time |
| `tape_echo` | Tape Echo | `flutter` 0–100 % (def 35) | Random-walk wow/flutter (not a sine LFO — the tape signature), hot magnetic saturation, 3 kHz tone roll-off per repeat |
| `bbd` | BBD Analog Delay | `tone` 0–100 % (def 40) | Two cascaded low-pass poles reproduce bucket-brigade reconstruction — far steeper HF loss per repeat than a one-pole "warm" delay; `tone` sweeps 1.2→4.5 kHz |
| `multitap` | Multi-Tap Delay | `taps` 2–6 (def 4) | Taps on even sub-divisions of the base time (time/n … time), gains falling as 1/i and normalized; only the longest tap recirculates. No filtering or saturation — clean taps |
| `rhythmic` | Rhythmic Delay | `subdivision` 0–2 (def 0) | `time_ms` is the **beat** (quarter note); the subdivision puts echoes on a syncopated grid — `0` dotted eighth (0.75), `1` triplet (2/3), `2` eighth (0.5). Fixed 6 kHz roll-off. Resolved once when the block is built, not modulated per sample |
| `chorus_echo` | Chorus Echo | `depth` 0–100 % (def 45) | A slow 1.6 Hz sine sweeps the delay time (up to 4.5 ms) so the repeats chorus; BBD-style 3.2 kHz roll-off + soft saturation. Periodic modulation is what tells it apart from Tape Echo's random walk |
| `ping_pong` | Ping-Pong Delay | — | **`TrueStereo`.** The input is summed to mid and enters one line only; each line recirculates into the other, so the echo bounces R→L→R even from a mono guitar. In a mono layout it is wrapped (broadcast in, sum out) |
| `pitch_delay` | Pitch Delay | `semitones` −24…+24 (def 12) | Pitch-shifts the repeat with a granular dual-tap overlap-add shifter (2048-sample window, no FFT); with feedback the shift cascades into a shimmer |
| `granular` | Granular Delay | `spread` 0–100 % (def 45) | Each grain respawns at the base delay plus a random offset (up to 6000 samples at full `spread`), scattering the signal into a cloud; two 2048-sample Hann grains overlap for continuous output |

Defaults for `time_ms` / `feedback` / `mix`: Tape Echo 280/40/32, BBD 320/35/30, Multi-Tap 400/25/35, Rhythmic 500/38/32, Chorus Echo 350/35/32, Ping-Pong 300/40/35, Pitch Delay 350/35/35, Granular 300/30/40.

⚠️ `pitch_delay` (Delay block, 2048-sample grain) is **not** `native_pitch_shifter` (Pitch block, 1024-sample grain) — different models, different blocks.

### Tempo sync

Every native delay has a `time_sync` select and every native modulation with a
`rate_hz` knob (choruses, flangers, phasers, tremolo, vibrato) has a `rate_sync`
select: `off`, `1/1`, `1/2`, `1/2.`, `1/2T`, `1/4`, `1/4.`, `1/4T`, `1/8`,
`1/8.`, `1/8T`, `1/16`, `1/16.`, `1/16T` (`.` dotted, `T` triplet). Default `off`.

- With a note value picked, `time_ms` / `rate_hz` follow the **global tempo** (the
  metronome's BPM): a delay repeats once per note value, a modulation completes one
  cycle per note value. The value is clamped to the model's own range (a whole note
  at 30 BPM cannot exceed a delay's 2000 ms).
- The dispatcher writes the derived number into the block on the control thread
  (`project::tempo_retime`, `block_core::tempo_sync`), exactly as a knob turn would;
  the DSP never sees the tempo. Picking a note value applies it immediately.
- Turning `time_ms` / `rate_hz` by hand sets the select back to `off`.

## Chain split

A `Split` block divides the signal into **N paths** (at least two, no upper bound), shown as A, B, C, …, AA, …
Blocks before it are shared by every path.

- **Split → Mix** (`end: mix`): every path → mixer → the rest of the list the split sits in. Main use: amp A hard
  left, amp B hard right.
- **Split → Y** (`end: y`): every path → its own outputs. No mixer.

A split may sit inside a path, at any depth: a Mix inside a Y path, a Y inside a Mix path, a Mix inside a Mix. The
app lays the graph out from the tree; no position is stored.

Knobs live in `SplitBlock.params` (keys in `project::block::split_params`) and are edited with the ordinary
`SetBlockParameter*` commands. The schema is generated from the path count (`split_param_specs(path_count)`):
`<i>` is the 0-based path index, so path A is `_0`, path B is `_1`. The split editor shows the `Split` group, the
mixer editor the `Mixer` group.

| Key | Range | Default | Meaning |
|---|---|---|---|
| `split_mode` | `same` \| `dual_mono` | `same` | Mode I / Mode II |
| `level_to_<i>` | 0–100 | 100 | Linear gain into path `i` (`x/100`) |
| `balance_<i>` | −50…+50 | 0 | Mode II only. −50 = L only, 0 = (L+R)/2, +50 = R only; the path gets dual mono `[s, s]` |
| `mix_level_<i>` | 0–100 | 100 | Mix only. Linear gain of path `i` into the mixer |
| `mix_pan_<i>` | −50…+50 | 0 | Mix only. Balance law: centre = unity on both sides; the opposite side falls linearly to 0 at ±50 |
| `mix_polarity_<i>` | `normal` \| `invert` | `normal` | Mix only. Multiplies path `i` by −1 |
| `mix_master` | 0–100 | 50 | Mix only. Output gain `x/100`, whatever the path count; at the default two identical paths sum to unity |
| `mix_master_sum` | bool | false | Mix only. Output becomes dual mono `L = R = (L+R)/2` |

Adding a path adds its keys at their defaults. Removing a path drops its keys and renumbers the keys above it; the
MIDI mappings and scene values that name a renumbered key move with it.

Rules (`project::block::split_block_methods`, enforced by `validate_structure` and `RigProject::validate`), judged
at every depth:

1. A split has at least two paths.
2. A Y ends the list it sits in. At the top level only the chain's own `Input`/`Output` ports may follow it; inside
   a path nothing may follow it.
3. A path holds no `Input`, `Output` or `Insert` (an insert's return is a separate input stream, and summing it with
   the other paths of a Mix would mix streams in our code). `Select` and `Split` are allowed in a path.

There is no limit on how many splits a chain holds, how many paths each has, or how deep they nest; the limit is
the machine.

**Y leaves and outputs.** A leaf is one path of a Y that holds no further Y. Every leaf ends in its own output
node, with the endpoint checklist (`disabled_endpoints.path_outputs`, one entry per leaf that has something
disabled). Several leaves may check the same output; that output sums them, time-aligned. Every output runs its
own copy of everything on the way to the leaves that feed it (one pipeline per output), so CPU grows with the
outputs. A Y nested in a Y gives one more leaf per extra path: a two-path Y inside path A of a two-path Y gives
three outputs.

In the GUI, clicking a split node of a chain graph (or a Split chip in touch and compact views) opens the
**split editor**: the Mix / Y switch (`SetSplitEnd`; a switch the chain refuses shows the error as a toast), the
paths row — one chip per path, a remove button on each chip while the split has more than two paths
(`RemoveSplitPath`; a path that holds blocks asks first), and "+ Path" (`AddSplitPath`) — then mode, the level
into each path and the balances. Clicking the mixer node opens the **mixer editor**: level, pan and polarity per
path, master, master sum. Both are a small root-level panel drawn from `split_param_specs()` by the block editor's
own grid, one row per path (`split_editor_grid`): the mode alone on top, then A, B, C… each on its own row,
master and master sum below; a grid taller than the window scrolls. In the compact view's split row a path's
knobs never wrap apart. Each knob is an ordinary `SetBlockParameter*` on the split block, so MIDI mapping,
scenes and MCP reach them like any knob. Mode II needs a stereo or dual-mono signal before any mono block; with a
mono source every balance gives the same signal. On a one-channel output, pan has no audible effect (the route
averages L and R).

### Split engine behaviour

A Split → Mix runs inside the chain's own segment: shared blocks → split →
every path → mixer → shared blocks. Nothing is summed across segments or
runtimes (stream isolation); every path beyond the first runs in its own
buffer, preallocated at build for a 1024-frame callback. A larger callback runs
through the split in 1024-frame chunks, so no buffer grows on the audio thread.

**Into the paths.** Mode I (`split_mode: same`): path `i` gets the bus ×
`level_to_<i>` (`x/100`). Mode II (`dual_mono`): path `i` gets the one channel
its balance picks — −50 = L, 0 = (L+R)/2, +50 = R, linear in between — as dual
mono, × its level. Mode II only means something when the bus is still stereo at
the split (a stereo or dual-mono source, before any mono block); with a mono
source every path gets the same signal.

**Mixer.** Per path: balance law (centre = unity on both sides; toward one side
the other side falls linearly to 0 at ±50) × `mix_level_<i>` (`x/100`), × −1
with `mix_polarity_<i>: invert`; the sum × `mix_master` (`x/100`); with
`mix_master_sum` on, both sides become `(L+R)/2`. At the defaults two identical
paths come out at unity (`mix_master` 50 halves the doubled sum); with more
paths the sum grows with the count, so lower `mix_master` or the path levels.
The split's output is always stereo; a 1-channel output averages L/R, so pan
does nothing there. A Y meets each leaf at unity: its mixer knobs do not apply.

**Alignment.** Every block reports the processing latency it adds. At build the
split sums it per path and delays every shorter path up to the longest one, in
a ring preallocated up to 16384 samples; above that it clamps and logs. The
longest path is never delayed, so the chain's latency does not change. A nested
Mix aligns inside its path first, and its path then counts its latency. The same
holds where several Y leaves meet on one output.
A knob move or a path edit reuses every path processor by block id (also when
a block is dragged between lanes, between depths or to the shared blocks) and
continues the delay lines where they were, so it is not heard as a gap.
A block switched on or off inside a path (a footswitch) fades like any block and
every split above it lines its paths up again on the same callback.

| Source | Reported latency |
|---|---|
| IR convolution (cab, body, `generic_ir`) | 64 samples (one partition) |
| 2× oversampler round trip (ring modulator) | 15 samples |
| Brick wall limiter | its look-ahead: `lookahead_ms` in samples (144 at the 3 ms default, 48 kHz) |
| VST3 | `IAudioProcessor::getLatencySamples()`, read at load |
| LV2 | the `lv2:reportsLatency` output port, read once at build |
| everything else | 0 |

Not aligned by design: delays and the pitch shifter (their delay is the
effect), the IR reverb's dry/wet blend and the ring modulator below 100 % mix
(their dry part is not delayed inside the block).

## Audio backends

- **Native** — DSP written in Rust and compiled into OpenRig; the lowest CPU cost.
- **NAM** — Neural Amp Modeler. Each NAM plugin manifest carries an
  `architecture: A1|A2` field: `A1` = NAM "v1" (WaveNet / LSTM / ConvNet),
  `A2` = NAM "v2" (`SlimmableContainer`, `.nam` version 0.7.0). Every NAM plugin
  is uniform (all captures share one architecture — mixed plugins are split into
  `<name>_a1` / `<name>_a2`), so the catalog shows a **NAM/A1** or **NAM/A2**
  badge straight from this field, without opening any `.nam`. `NAM/A2` uses an
  amber badge (text `#ffcc44`) to set it apart from the blue `NAM/A1`. The field
  is optional: IR plugins and older NAM manifests omit it and show a plain
  **NAM** badge (blue, same as A1). `NAM/A2` packages also expose the `slim`
  knob — see the NAM block parameters above; A1 never does.
- **IR** — Impulse responses (cabs, bodies). Uniformly partitioned FFT convolution (`crates/ir`); partition size 64, so the per-callback cost is even with no periodic FFT spike — safe at 64-frame device buffers, about 1.3 ms of added latency. For `type: cab`/`body` it is 100 % wet and has an **Output** knob (dB, −24..+24) like NAM: the absolute output level, defaulting to the selected capture's `output_gain_db` audit baseline and re-seeding to the new capture's baseline when the user switches mic or position. Presets saved before this knob existed keep their audit loudness (the audio path resolves the per-capture baseline from the raw params; volume invariant 10). For `type: reverb` the **same convolution engine** drives the wet path, but the block stays gain-passive and blends dry/wet with **mix** (+ **pre_delay_ms**, wet **level**) instead of normalising to a calibrated level — long diffuse reverb tails would be wrong under cab-style peak normalisation. Dispatch branches on the backend in `block_reverb::build_reverb_processor_for_layout`; the wet builder is shared via `ir::from_package::build_convolution_from_package`.
- **LV2** — Open-source plugins, loaded in-process.
- **VST3** — VST3 plugins hosted with the plugin's own editor window
  (`crates/vst3-host`). Discovered from the standard OS VST3 paths
  (`~/Library/Audio/Plug-Ins/VST3` and platform equivalents) **and** from the
  `vst3/` folder of each configured plugin root, so a VST3 shipped in
  OpenRig-plugins (`<plugins_root>/vst3/<id>/bundles/<Name>.vst3`, manifest
  `type: vst3` / `backend: vst3`) lands in the **same** VST3 block list as a
  user-installed one, with the same native editor and `Vst3Processor`.
  Parameters are read at runtime from the plugin's `IEditController`, not from
  the manifest. The manifest's optional `parameters:` overlay only groups: each
  entry names a `vst3_id` and the block-editor tab it belongs to; `min` / `max`
  / `default` are optional and unused for grouping, since the live controller
  owns the real ranges.
  The editor shows only the parameters the plugin marks as user-facing:
  anything flagged `kIsHidden`, `kIsReadOnly` or `kIsBypass` (the block
  footswitch already is the bypass) is skipped, and so are placeholder slots
  (`Reserved*`, `Unused*`, `Unnamed*`) and unmapped macro slots (`Assign*`),
  even when the plugin flags them automatable. A label that repeats its tab
  name drops it (`Node 1: Delay` in the `Node 1` tab reads `Delay`). Labels come
  from the plugin's `title` (falling back to `shortTitle`), split on `_` / `-` /
  camelCase, with ALL-CAPS titles turned into sentence case (`DELAY_MS` →
  `Delay ms`); words glued without any separator are only case-folded.
  Continuous knobs keep their 0–100 % storage, but the value box shows the
  plugin's own text (`getParamStringByValue` + `units`, e.g. `2.5 s`), sampled
  once per knob step when the controller is read (`ParameterSpec.value_labels`);
  a plugin that formats nothing falls back to the number.
  Changes made in the plugin's **native editor** are captured back into the
  block's params (`p{id}` percent) on save, via `CaptureRigEdits` — the
  controller's current non-default values are read through the block's
  registered `Vst3GuiContext` and saved, so tweaks survive save and reload. The
  VST3 GUI-context registry is keyed by **block instance** (`BlockId`), not by
  model, so two blocks using the same plugin open and save independently.

### NAM block editor tabs

A NAM block's controls have known origins, so the block editor puts them in tabs
with no per-plugin authoring. The tabs show when **adding** a block too, not only
when editing one — the add flow opens the same detached editor in add mode:

- **Capture** — the axes the manifest declares under `parameters:` (`channel`,
  `gain`, `mic`, …). They pick which `.nam` capture is loaded; the `captures:`
  list maps each combination to a file. A NAM whose axes are all dead (dropped by
  the dead-axis filter) or that declares none has no Capture tab. Tagged in
  `project::block::nam_schema`, the only layer that knows about the manifest.
- **Amp** — `input_db`, `output_db` and the A2-only `slim`.
- **Noise Gate** — `noise_gate.enabled` + `noise_gate.threshold_db`.
- **EQ** — `eq.enabled` + `eq.bass` / `eq.middle` / `eq.treble`.

The last three are the engine controls **every** NAM has (a plugin package or the
generic `neural_amp_modeler` loader), identical for A1 and A2 apart from `slim`,
so their tabs are declared once with the specs in `nam::params` — a manifest
never repeats them. The group of a `ParameterSpec` is the tab, and the generic
tab machinery renders one tab per group. IR and LV2 params stay ungrouped: one
flat grid.

### Native cab voicing

The native cabinets (`brit_4x12`, `vintage_1x12`, `american_2x12`) are a
per-model biquad cascade — body high-pass + a ~24 dB/oct resonant speaker
roll-off + low-end bump + mid scoop + presence peak — each tuned to the magnitude
response of a reference cabinet (a 4x12 with Celestion-style speakers, a warm
1x12, a bright scooped 2x12). Zero added latency (no convolution). It matches the
magnitude curve, not the comb filtering and phase of a measured IR — for that, use
an IR cab. The same engine voices the built-in cab of the native amps (`chime`,
`tweed_breakup`, `blackface_clean`). Knobs: Low/High Cut, Resonance (low bump),
Air (presence), Mic Position (on/off-axis brightness), Mic Distance + Room Mix
(room tap).

## Supported instruments

`electric_guitar`, `acoustic_guitar`, `bass`, `voice`, `keys`, `drums`, `generic`. Constants in `crates/block-core/src/constants.rs` (`INST_*`, `ALL_INSTRUMENTS`, `GUITAR_BASS`, `GUITAR_ACOUSTIC_BASS`).

Every model definition lists its `supported_instruments`, and the block picker offers only the models that support the chain's instrument. The chain's `instrument` is saved in the project YAML, defaults to `electric_guitar`, and is fixed after the chain is created.
