---
name: openrig-tone-builder
description: "Use when the user asks for a tone, timbre, or preset for a specific song or artist (\"timbre da Duality\", \"preset do Slipknot\", \"tom da [música]\", \"recreate the [song] sound\", \"build a [artist] preset\"). Researches the original signal chain, maps it to OpenRig blocks, and writes a YAML preset to the user's local preset directory."
---

# OpenRig Tone Builder

Build a faithful OpenRig preset for a real-world song/artist tone. Output is a single YAML file in the user's local preset directory.

## Iron rule -- the catalog source of truth

**The ONLY catalog source you may consult for `MODEL_ID`s and parameters is `docs/user-guide/blocks-reference.md`.** Specifically the **Model ID Quick Reference** section near the top of that file, and the per-section catalogs further down.

You MUST NOT:

- Open or grep any file under `crates/block-*/src/` to discover model IDs or parameters. Ever. Not for "double-checking", not for "the doc might be stale", not for "just one quick lookup".
- Read existing presets in `~/.openrig/presets/` or `presets/` to copy their `MODEL_ID` strings or parameter shapes. They drift from the registry; the doc does not.
- Guess or invent model IDs based on what "sounds right". Every ID is a string the runtime hard-matches.

If a model you need is not in `blocks-reference.md`, that is a doc bug -- stop, tell the user, suggest opening an issue against the doc. Do not work around it by reading source.

The doc is authoritative because issue #375 closed the gap that previously forced source reads. If you find yourself reaching for `find crates/`, `grep MODEL_ID`, or `Read` on an `.rs` file -- you are violating this skill.

## Mandatory inputs

- `<artist>` -- band/artist name
- `<song>` -- song title (optional but strongly preferred -- gear varies between eras)

If only `<artist>` is given, ask once for the song. Era-less presets drift toward generic and the user notices.

## Output path (CRITICAL)

Always write to:

```
~/.openrig/presets/<artist_snake>_-_<song_snake>.yaml
```

- `<artist_snake>` and `<song_snake>` are lowercase, ASCII, words separated by `_`. Drop punctuation. Examples: `slipknot_-_duality`, `green_day_-_basket_case`. Multiple parts (rhythm/lead): append `_-_rhythm` / `_-_solo`.
- **Never commit user presets to the repo.** The `presets/` folder in the repo is for project-shipped factory presets; user presets live only in `~/.openrig/presets/`. If the user asks to share, suggest a paste/gist.
- Confirm the directory exists with `mkdir -p ~/.openrig/presets` before writing.

## Workflow

### 1. Research the signal chain

Hit sources **in order**, stopping when you have a confident gear list (instrument → pedals → amp → cab → mic). Always cite which sources you used.

| Priority | Source | Why |
|---|---|---|
| 1 | `https://www.tonedb.co/` (search by song or artist) | Crowdsourced, song-specific, often has signal chain explicit. JS-heavy -- if WebFetch returns 404 or empty, ask the user to paste the page text. |
| 2 | `https://www.groundguitar.com/tone-breakdown/` (per-album breakdowns) | Detailed per-song gear listings with chain order. |
| 3 | `https://killerrig.com/` (e.g. `killerrig.com/<artist>-amp-settings-and-tone-guide/`) | Numeric knob settings per song. |
| 4 | `https://musicstrive.com/<artist>-amp-settings/` | Often splits settings per song and per guitarist (rhythm vs lead). |
| 5 | `https://www.guitarchalk.com/<player>-amp-settings/` | Player-focused (Jim Root, Synyster Gates, etc.). |
| 6 | `https://prosoundhq.com/how-to-sound-like-<artist>-amp-settings-guide/` | Generic recipes; useful for fallback EQ. |
| 7 | `https://blog.andertons.co.uk/sound-like/sound-like-<artist>` | Gear context (which amps/cabs/strings the player ran in that era). |
| 8 | Premier Guitar / Guitar World rig rundowns | Authoritative for era and recording context. |

When two sources disagree on knob values, prefer the one that names the song explicitly. If they all give general guidance, weight them equally and pick the median.

If WebFetch returns 404 on a guessed URL, fall back to WebSearch with the artist + song + "tone" / "amp settings" / "signal chain" -- don't keep guessing URL slugs.

### 2. Map gear to OpenRig models

Open `docs/user-guide/blocks-reference.md` and do the lookup yourself for **every** piece of gear in the chain. There is intentionally no precomputed mapping table in this skill -- those tables go stale silently and produce wrong `MODEL_ID`s. The Quick Reference does not.

Process per piece of gear:

1. **Look up the exact match first.** Search the Quick Reference for the brand and model name (e.g. "Big Muff", "Mesa Rectifier", "DS-2"). Many real-world pedals/amps have a direct entry. Use it.
2. **If no direct match**, scan the relevant section (Amp / Gain / etc.) for the closest **voicing** -- not the closest brand. A Marshall Major (200W non-master) is closer to `marshall_super_100_1966` (vintage non-master Plexi family) than to `nam_marshall_jcm_800` (master volume modern). Read the Description column.
3. **Document the substitution** in your final reply (see Step 5). The user needs to know when you used a fallback.
4. **For NAM amps**, follow the parameter conventions documented in `blocks-reference.md` under `### Parameters -- NAM amps (catalog conventions)` -- there are four patterns (single-capture / `character` / `cabinet` + `gain` grid / standard 21 NAM preamps). The doc tells you which pattern applies and what values to put.

Always prefer NAM amps over Native amps when the song has a real amp model -- Native preamps are generic.

### 3. Build the chain

Default chain template for high-gain rock/metal:

```yaml
id: <artist_snake>_-_<song_snake>
name: <Artist> - <Song>[ (<Tuning>)]   # e.g. "Slipknot - Duality (Drop B)"
blocks:
- type: utility            # 1. Tuner -- disabled by default, user enables when needed
  enabled: false
  model: tuner_chromatic
  params: { mute_signal: true, reference_hz: 440.0 }

- type: filter             # 2. EQ -- tame sub-rumble and ice-pick highs before the amp
  enabled: true
  model: native_guitar_eq
  params: { low_cut: <70-100>, high_cut: <20-40> }

- type: dynamics           # 3. Gate -- high-gain noise control
  enabled: true
  model: gate_basic
  params: { attack_ms: 0.1, release_ms: <60-120>, threshold: <30-65> }

- type: gain               # 4. Boost / drive (only if the song uses one)
  enabled: true
  model: <gain_model_id_from_quick_reference>
  params: { ... }          # exact params per the model's section in blocks-reference.md

- type: amp                # 5. Amp (the core of the tone)
  enabled: true
  model: <amp_model_id_from_quick_reference>
  params: { ... }          # one of the 4 NAM amp patterns documented in blocks-reference.md

- type: filter             # 6. Post-amp tone-stack emulation (when amp is single-capture NAM)
  enabled: true
  model: eq_eight_band_parametric
  params: { ... }          # used to mimic real-world bass/mid/treble/presence knob values

- type: delay              # 7. Optional -- only if the song has audible delay
  enabled: <true|false>
  model: analog_warm
  params: { time_ms: <80-500>, feedback: <10-30>, mix: <5-15> }

- type: reverb             # 8. Subtle space -- most metal/rock leaves studio reverb minimal
  enabled: true
  model: room
  params: { room_size: <15-35>, damping: <65-85>, mix: <3-18> }

- type: dynamics           # 9. Brickwall limiter -- protect output from clipping
  enabled: true
  model: limiter_brickwall
  params: { threshold: -1.0, ceiling: -0.1, release_ms: 50.0 }

- type: gain               # 10. Master volume
  enabled: true
  model: volume
  params: { volume: <70-90>, mute: false }

- type: utility            # 11. Spectrum analyzer -- visual feedback (no audio impact)
  enabled: true
  model: spectrum_analyzer
  params: {}
```

Adjust per song style:

- **Clean / acoustic**: drop the boost, drop the gate, switch to a clean amp from the Quick Reference, add a body IR for acoustic.
- **Funk / clean rhythm**: add `compressor_studio_clean` (paralleled with `mix: 30-50`) for picking dynamics. Lower gain on amp.
- **Lead solo**: bump `volume` to 85-90, raise delay mix to 12-25%, slightly larger reverb.
- **Doom / drone**: drop boost, raise reverb mix to 25%+, add `tape_vintage` delay.

### 4. Knob translation rule

NAM amp captures have **knobs baked into the capture**. Most NAM amps expose only structural switches (`character` / `cabinet` + `gain`) -- not continuous bass/mid/treble/master controls. So when a source gives "bass 10, mid 5, treble 7" for a Marshall, you cannot type those into the amp block.

Approximate the EQ shape with a parametric EQ block **after** the amp:

- High bass knob → low-shelf boost around 150--250 Hz, +2 to +5 dB.
- Scooped mids → peak cut around 500--1000 Hz, -1 to -3 dB, Q ≈ 1.0--1.5.
- High treble → high-shelf boost around 3--5 kHz, +2 to +4 dB.
- Cut fizz → low-pass around 8--10 kHz, Q ≈ 0.7.
- Use a `volume` block to compensate master.

For Native preamps (`american_clean`, `brit_crunch`, `modern_high_gain`) you DO get all knobs -- apply numeric values directly to the amp block instead of via parametric EQ.

### 5. Provenance comment in the chat reply

After writing the file, summarize to the user:

1. **Mapping table** you used: real gear → OpenRig model. One row per piece in the chain. Mark fallbacks/approximations explicitly.
2. **Cite sources** you actually fetched, not the full priority list.
3. **Note uncertainty** (e.g. "Orange Rockerverb has no direct OpenRig match -- fell back to `nam_mesa_rectifier` because <source> describes the song's voicing as 'tight modern recto-style'").
4. **Tunings** (Drop B / Drop A / Drop C): append to the display name in parentheses, e.g. `Slipknot - Duality (Drop B)`. Tuning isn't applied in software -- it's a hint to the user.

## Validation before declaring done

- [ ] File exists at `~/.openrig/presets/<id>.yaml`.
- [ ] `id:` is unique (grep `~/.openrig/presets/`).
- [ ] Every `model:` referenced appears in `docs/user-guide/blocks-reference.md` Quick Reference. If not, you invented or guessed a model -- go back to the Quick Reference and pick a real one.
- [ ] Every `params:` key for a model is documented in that model's section of `blocks-reference.md`. If a section says "no user-adjustable parameters", `params: {}`.
- [ ] No knowledge of `MODEL_ID`s came from anywhere other than `blocks-reference.md`.
- [ ] YAML parses (no tab indentation, no trailing commas).

## Red flags -- STOP

If you catch yourself doing any of the following, you have left the skill. Stop, restart from the Quick Reference:

- Running `find crates/` or `grep MODEL_ID` or `Read` on any `.rs` file.
- Reading another preset YAML (factory or user) to copy a `MODEL_ID` or `params:` shape.
- Saying "I think the model id is X" without having seen X in `blocks-reference.md` in the current session.
- Telling the user "the doc seems incomplete, let me check the source". The doc is the source. If it's actually incomplete, that is the user's call to make and a separate issue.

## Anti-patterns

- ❌ Inventing a model name that "sounds right" -- every model name has a hard-matched string in the registry. Wrong name = preset fails to load.
- ❌ Using `preamp` block for a full amp song -- `preamp` is preamp-only (no power amp / cab). Songs almost always want `amp` (full amp + cab + mic).
- ❌ Writing to `presets/` in the repo. That's the factory preset folder, gets shipped with the binary, and contaminates the project tree. User presets go in `~/.openrig/presets/`.
- ❌ Skipping the source citation. Always show your work.
- ❌ Pattern-matching another preset's structure instead of the spec in `blocks-reference.md`.
- ❌ Reading `.rs` "just to confirm". The Quick Reference is the contract. If it's wrong, that is a doc bug, not a skill workaround.

## Common rationalizations -- forbidden

| Rationalization | Reality |
|---|---|
| "The doc might be out of date" | Then file an issue. Don't read source. |
| "Just one quick grep to verify" | One grep is one violation. |
| "I'll cross-reference with an existing preset" | Existing presets drift from the registry. The doc doesn't. |
| "I know this MODEL_ID from training" | Verify against the Quick Reference before using. |
| "This is faster than reading the doc" | Yes -- and that's the point. The doc is the contract. |
EOF
