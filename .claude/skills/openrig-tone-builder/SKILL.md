---
name: openrig-tone-builder
description: "Use when the user asks for a tone, timbre, or preset for a specific song or artist (\"timbre da Duality\", \"preset do Slipknot\", \"tom da [música]\", \"recreate the [song] sound\", \"build a [artist] preset\"). Researches the original signal chain, maps it to OpenRig blocks, and writes a YAML preset to the user's local preset directory."
---

# OpenRig Tone Builder

Build a faithful OpenRig preset for a real-world song/artist tone. Output is a single YAML file in the user's local preset directory.

## Mandatory inputs

- `<artist>` — band/artist name
- `<song>` — song title (optional but strongly preferred — gear varies between eras)

If only `<artist>` is given, ask once for the song. Era-less presets drift toward generic and the user notices.

## Output path (CRITICAL)

Always write to:

```
~/.openrig/presets/<artist_snake>_-_<song_snake>.yaml
```

- `<artist_snake>` and `<song_snake>` are lowercase, ASCII, words separated by `_`. Drop punctuation. Examples: `slipknot_-_duality`, `green_day_-_basket_case`.
- **Never commit user presets to the repo.** The `presets/` folder in the repo is for project-shipped factory presets; user presets live only in `~/.openrig/presets/`. If the user asks to share, suggest a paste/gist.
- Confirm the directory exists with `mkdir -p ~/.openrig/presets` before writing.

## Workflow

### 1. Research the signal chain

Hit sources **in order**, stopping when you have a confident gear list (instrument → pedals → amp → cab → mic). Always cite which sources you used.

| Priority | Source | Why |
|---|---|---|
| 1 | `https://www.tonedb.co/` (search by song or artist) | Crowdsourced, song-specific, often has signal chain explicit. JS-heavy — if WebFetch returns 404 or empty, ask the user to paste the page text (they did it for Psychosocial). |
| 2 | `https://killerrig.com/` (e.g. `killerrig.com/<artist>-amp-settings-and-tone-guide/`) | Numeric knob settings per song. |
| 3 | `https://musicstrive.com/<artist>-guitar-tone/` | Often splits settings per song and per guitarist (rhythm vs lead). |
| 4 | `https://www.guitarchalk.com/<player>-amp-settings/` | Player-focused (Jim Root, Synyster Gates, etc.). |
| 5 | `https://prosoundhq.com/how-to-sound-like-<artist>-amp-settings-guide/` | Generic recipes; useful for fallback EQ. |
| 6 | `https://blog.andertons.co.uk/sound-like/sound-like-<artist>` | Gear context (which amps/cabs/strings the player ran in that era). |
| 7 | `https://www.riffhard.com/how-to-get-a-<artist>-guitar-tone/` | Pedal/cab specifics. |
| 8 | Premier Guitar / Guitar World rig rundowns | Authoritative for era and recording context. |

When two sources disagree on knob values, prefer the one that names the song explicitly. If they all give general guidance, weight them equally and pick the median.

### 2. Map gear to OpenRig models

Use `docs/user-guide/blocks-reference.md` as the source of truth for available models — read it before building the preset, do not work from memory. The catalog has 350 models across 16 block types and changes frequently.

Mapping rules:

| Real-world gear | OpenRig model (preferred → fallback) |
|---|---|
| Mesa Boogie Triple/Dual Rectifier | `nam_mesa_rectifier` (gain: `drive_red`) |
| Mesa Mark III/IV/V | `nam_mesa_mark_iii` or `nam_mesa_mark_v` |
| Marshall JCM 800 | `nam_marshall_jcm_800_2203` (or `marshall_jcm_800_2203`) |
| Marshall Plexi / 1959 | `nam_marshall_super_100_1966` |
| EVH 5150 III | `nam_evh_5150` |
| Peavey 5150 / 6505 | `nam_peavey_5150` |
| Friedman BE-100 | `nam_friedman_be100_deluxe` |
| Bogner Ecstasy / Shiva | `nam_bogner_ecstasy` / `nam_bogner_shiva` |
| Diezel VH4 | NAM preamp model `diezel_vh4` (preamp-only) |
| Vox AC30 | `nam_vox_ac30` |
| Fender Twin / Deluxe | `nam_fender_deluxe_reverb_65` |
| Orange Rockerverb | **no direct match** — fallback `nam_mesa_rectifier` (modern channel) or `nam_evh_5150` |
| Rivera KR-7 / Knucklehead | **no direct match** — fallback `nam_mesa_rectifier` (tight modern) |
| Ibanez TS9 / TS808 / Maxon OD808 | `nam_ibanez_ts9` (drive 2-4, tone 6-8, level 7-9 for tighten-only boost) |
| ProCo RAT | `nam_procoaudio_rat` (or `nam_rat2`) |
| Klon Centaur | `nam_klon_centaur` |
| Boss DS-1 / SD-1 / BD-2 / OD-3 | `nam_boss_ds1` / `nam_boss_sd1` / `nam_boss_bd2` |
| Fulltone OCD | `nam_ocd` |
| Cab 4×12 V30 (Mesa, Marshall, Orange, etc.) | `cabinet: 4x12_v30` (NAM amps), or IR cab `cab_v30_4x12` for standalone |
| Cab Greenback 4×12 | `cab_g12m_greenback_4x12` |
| Cab AC30 Blue | `cab_vox_ac30_blue` |
| Acoustic body resonance | `body_*` IR (114 models — pick by guitar brand: martin/taylor/gibson/yamaha) |

For anything not in the table: read `docs/user-guide/blocks-reference.md` and pick the closest by **voicing description** (not just brand). Always prefer NAM amps over Native amps when the song has a real amp model — Native preamps are generic.

### 3. Build the chain

Default chain template for high-gain rock/metal:

```yaml
id: <artist_snake>_-_<song_snake>
name: <Artist> - <Song>[ (<Tuning>)]   # e.g. "Slipknot - Duality (Drop B)"
blocks:
- type: utility            # 1. Tuner — disabled by default, user enables when needed
  enabled: false
  model: tuner_chromatic
  params: { mute_signal: true, reference_hz: 440.0 }

- type: filter             # 2. EQ — tame sub-rumble and ice-pick highs before the amp
  enabled: true
  model: native_guitar_eq
  params: { low_cut: <80-100>, high_cut: <85-100> }

- type: dynamics           # 3. Gate — high-gain noise control
  enabled: true
  model: gate_basic
  params: { attack_ms: 0.1, release_ms: <60-120>, threshold: <50-65> }

- type: gain               # 4. Boost (if the song uses one — most metal does)
  enabled: true
  model: nam_ibanez_ts9
  params: { drive: <2-4>, tone: <6-8>, level: <7-9> }

- type: amp                # 5. Amp (the core of the tone)
  enabled: true
  model: nam_<amp_id>
  params: { cabinet: <cab_id>, gain: <channel_id> }

- type: delay              # 6. Optional — only if the song has audible delay
  enabled: <true|false>
  model: analog_warm
  params: { time_ms: <250-500>, feedback: <15-30>, mix: <5-12> }

- type: reverb             # 7. Subtle space — most metal/rock leaves studio reverb minimal
  enabled: true
  model: room
  params: { room_size: <15-30>, damping: <70-85>, mix: <3-15> }

- type: dynamics           # 8. Brickwall limiter — protect output from clipping
  enabled: true
  model: limiter_brickwall
  params: { threshold: -1.0, ceiling: -0.1, release_ms: 50.0 }

- type: gain               # 9. Master volume
  enabled: true
  model: volume
  params: { volume: 80.0, mute: false }

- type: utility            # 10. Spectrum analyzer — visual feedback (no audio impact)
  enabled: true
  model: spectrum_analyzer
  params: {}
```

Adjust per song style:

- **Clean / acoustic**: drop the boost, drop the gate, switch amp to a clean NAM (`nam_fender_deluxe_reverb_65`, `nam_vox_ac30`), add `body_*` IR for acoustic.
- **Funk / clean rhythm**: add `compressor_studio_clean` after the boost slot (or replacing the boost). Lower gain on amp.
- **Lead solo**: bump `volume` to 85-90, raise delay mix to 15-25%.
- **Doom / drone**: drop boost, raise reverb mix to 25%+, add `delay_tape_vintage`.

### 4. Knob translation rule

NAM amp captures have **knobs baked into the capture** — `nam_mesa_rectifier` only exposes `cabinet` and `gain` (channel selector), not bass/mid/treble/presence/master. So when sources give numeric knob values for a NAM-captured amp, you cannot apply them directly. Instead:

- Use the EQ block (`native_guitar_eq` or `eq_eight_band_parametric`) **before** the amp to approximate the EQ shape: high `bass` knob → less `low_cut`; scooped `mid` → notch around 500-1kHz in the parametric; bright `treble`/`presence` → less `high_cut`.
- Use the TS9 `tone` knob to bias mid presence into the amp.
- Use `volume` block to compensate master.

For Native preamps (`american_clean`, `brit_crunch`, `modern_high_gain`) you DO get all knobs (gain, bass, middle, treble, presence, depth, sag, master, bright) — apply numeric values directly.

### 5. Provenance comment in the chat reply

After writing the file, summarize to the user:

1. Mapping table you used (which real gear → which OpenRig model).
2. Cite sources you actually fetched, not the full priority list.
3. Note any uncertainty (e.g. "Orange Rockerverb has no direct OpenRig match — fell back to Mesa Rectifier modern channel because Killer Rig describes the song's voicing as 'tight modern recto-style'").
4. Drop B / Drop A / Drop C tunings: append to the display name in parentheses, e.g. `Slipknot - Duality (Drop B)`. Tuning isn't applied in software — it's a hint to the user.

## Validation before declaring done

- File exists at `~/.openrig/presets/<id>.yaml`.
- `id:` is unique (grep `~/.openrig/presets/`).
- Every `model:` referenced is listed in `docs/user-guide/blocks-reference.md` — if not, you invented a model. Read the doc and pick a real one.
- Every `params:` key is documented for that model in `blocks-reference.md`. If unsure, read the existing `presets/*.yaml` in the repo for canonical key spellings (`drive` not `gain` for TS9, `mix` not `wet` for reverb, etc.).
- YAML parses (no tab indentation, no trailing commas).

## Anti-patterns

- ❌ Inventing a model name that "sounds right" — every model name has a Rust struct backing it. Wrong name = preset fails to load silently or with cryptic error.
- ❌ Using `preamp` block for a full amp song — `preamp` is preamp-only (no power amp / cab). Songs almost always want `amp` (full amp + cab).
- ❌ Writing to `presets/` in the repo. That's the factory preset folder, gets shipped with the binary, and contaminates the project tree. User presets go in `~/.openrig/presets/`.
- ❌ Skipping the source citation. The user has caught this before and asked for sources — always show your work.
- ❌ Assuming knob values transfer to NAM amps. They don't (see § 4).

## Reference example (Slipknot - Duality, Drop B)

Built using tonedb.co (Psychosocial entry implied same era), Killer Rig, MusicStrive, Pro Sound HQ. Mesa Rectifier `drive_red` channel as fallback for Orange Rockerverb (which OpenRig doesn't ship), TS9 `drive: 2` for tighten-only (low gain — both rhythm guitarists drive amp gain hard, not pedal), gate threshold 60 against high-gain noise on active humbuckers, 4×12 V30 cab. See `~/.openrig/presets/slipknot_-_duality.yaml` for the canonical YAML.
