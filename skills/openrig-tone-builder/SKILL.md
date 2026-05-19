---
name: openrig-tone-builder
description: "Use when the user asks for a tone, timbre, or preset for a specific song or artist (\"timbre da Duality\", \"preset do Slipknot\", \"tom da [música]\", \"recreate the [song] sound\", \"build a [artist] preset\"). Researches the original signal chain, maps it to OpenRig blocks, and builds it on the running rig through the OpenRig MCP server."
---

# OpenRig Tone Builder

Build a faithful tone for a real-world song/artist **on the running OpenRig
rig**, by driving the OpenRig MCP server. You do not write files. You call
tools on the live instance and the change is audible immediately.

## Precondition -- the MCP server must be connected

This skill drives OpenRig through its MCP server. Before doing anything:

1. Confirm the OpenRig MCP tools are available (e.g. `save_project`,
   `add_block`, `set_block_parameter_number`) and the resource
   `openrig://project` reads. The OpenRig plugin wires this automatically when
   OpenRig runs with `--mcp` (`openrig --mcp`, GUI or console).
2. If the tools/resource are **not** available, STOP. Tell the user to start
   OpenRig with `--mcp` and install the OpenRig plugin (`docs/mcp.md`). Do NOT
   fall back to writing a YAML file — that bypasses the running rig and is not
   what this skill does anymore.

The rig is shared: changes you make via MCP are reflected in the open GUI in
real time, and vice-versa.

## Iron rule -- the catalog source of truth

**The ONLY catalog source you may consult for `MODEL_ID`s and parameters is `docs/user-guide/blocks-reference.md`.** Specifically the **Model ID Quick Reference** section near the top of that file, and the per-section catalogs further down.

You MUST NOT:

- Open or grep any file under `crates/block-*/src/` to discover model IDs or parameters. Ever. Not for "double-checking", not for "the doc might be stale", not for "just one quick lookup".
- Read existing presets to copy their `MODEL_ID` strings or parameter shapes. They drift from the registry; the doc does not.
- Guess or invent model IDs based on what "sounds right". Every ID is a string the runtime hard-matches.

If a model you need is not in `blocks-reference.md`, that is a doc bug -- stop, tell the user, suggest opening an issue against the doc. Do not work around it by reading source.

The doc is authoritative because issue #375 closed the gap that previously forced source reads. If you find yourself reaching for `find crates/`, `grep MODEL_ID`, or `Read` on an `.rs` file -- you are violating this skill.

## Mandatory inputs

- `<artist>` -- band/artist name
- `<song>` -- song title (optional but strongly preferred -- gear varies between eras)

If only `<artist>` is given, ask once for the song. Era-less presets drift toward generic and the user notices.

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
2. **If no direct match**, scan the relevant section (Amp / Gain / etc.) for the closest **voicing** -- not the closest brand. Read the Description column.
3. **Document the substitution** in your final reply (Step 5).
4. **For NAM amps**, follow the parameter conventions documented in `blocks-reference.md` under `### Parameters -- NAM amps (catalog conventions)`.

Always prefer NAM amps over Native amps when the song has a real amp model -- Native preamps are generic.

### 3. Build the chain on the live rig (via MCP tools)

The chain below is the **plan**. You realize it by calling tools, not by
writing a file. Use the model IDs/params you resolved in Step 2 from
`blocks-reference.md`.

Steps:

1. **Read `openrig://project`.** Note the existing chain IDs. If there is no
   chain to target, create one with `add_chain` (give it the instrument the
   song uses, e.g. `electric_guitar`). Otherwise pick the chain the user means
   (ask if ambiguous).
2. **For each block in the plan, in order**, call `add_block` with
   `{ chain: <chain_id>, kind: <block type>, model_id: <MODEL_ID>, position: <index> }`.
   The tool result returns the emitted events including the **new block id**
   (`BlockAdded { chain, block }`). Capture that block id.
3. **Set its parameters** with the typed param tools, targeting that block id:
   - numeric → `set_block_parameter_number { chain, block, path, value }`
   - boolean → `set_block_parameter_bool { chain, block, path, value }`
   - text → `set_block_parameter_text { chain, block, path, value }`
   - enum/option → `select_block_parameter_option { chain, block, path, value }`
   `path` and value domains are exactly what that model's section in
   `blocks-reference.md` documents. If a section says "no user-adjustable
   parameters", set none.
4. **Disabled-by-default blocks** (e.g. the tuner): after `add_block`, call
   `toggle_block_enabled { chain, block }` so it lands disabled like the plan.
5. **Persist**: call `save_project` once the chain is complete.

Plan (high-gain rock/metal default — adjust per song style as in the notes
below):

```text
1.  utility / tuner_chromatic            enabled:false  params: mute_signal=true, reference_hz=440.0
2.  filter  / native_guitar_eq           enabled:true   params: low_cut=<70-100>, high_cut=<20-40>
3.  dynamics/ gate_basic                  enabled:true   params: attack_ms=0.1, release_ms=<60-120>, threshold=<30-65>
4.  gain    / <gain_model_id>             enabled:true   params: per blocks-reference.md (only if the song uses a boost/drive)
5.  amp     / <amp_model_id>              enabled:true   params: one of the 4 NAM amp patterns in blocks-reference.md
6.  filter  / eq_eight_band_parametric    enabled:true   params: mimic real bass/mid/treble/presence (see Step 4)
7.  delay   / analog_warm                 enabled:<bool> params: time_ms=<80-500>, feedback=<10-30>, mix=<5-15>
8.  reverb  / room                        enabled:true   params: room_size=<15-35>, damping=<65-85>, mix=<3-18>
9.  dynamics/ limiter_brickwall           enabled:true   params: threshold=-1.0, ceiling=-0.1, release_ms=50.0
10. gain    / volume                      enabled:true   params: volume=<70-90>, mute=false
11. utility / spectrum_analyzer           enabled:true   params: {}
```

Adjust per song style:

- **Clean / acoustic**: drop the boost, drop the gate, switch to a clean amp from the Quick Reference, add a body IR for acoustic.
- **Funk / clean rhythm**: add `compressor_studio_clean` (paralleled with `mix: 30-50`). Lower amp gain.
- **Lead solo**: bump `volume` to 85-90, raise delay mix to 12-25%, slightly larger reverb.
- **Doom / drone**: drop boost, raise reverb mix to 25%+, add `tape_vintage` delay.

### 4. Knob translation rule

NAM amp captures have **knobs baked into the capture**. Most NAM amps expose only structural switches (`character` / `cabinet` + `gain`) -- not continuous bass/mid/treble/master controls. So when a source gives "bass 10, mid 5, treble 7" for a Marshall, you cannot set those on the amp block.

Approximate the EQ shape with the parametric EQ block **after** the amp (block 6 in the plan), via `set_block_parameter_number` on its bands:

- High bass knob → low-shelf boost around 150--250 Hz, +2 to +5 dB.
- Scooped mids → peak cut around 500--1000 Hz, -1 to -3 dB, Q ≈ 1.0--1.5.
- High treble → high-shelf boost around 3--5 kHz, +2 to +4 dB.
- Cut fizz → low-pass around 8--10 kHz, Q ≈ 0.7.
- Use the `volume` block to compensate master.

For Native preamps (`american_clean`, `brit_crunch`, `modern_high_gain`) you DO get all knobs -- set numeric values directly on the amp block instead of via parametric EQ.

### 5. Provenance comment in the chat reply

After `save_project` succeeds, summarize to the user:

1. **Mapping table**: real gear → OpenRig model, one row per block. Mark fallbacks/approximations explicitly.
2. **Cite sources** you actually fetched, not the full priority list.
3. **Note uncertainty** (e.g. "Orange Rockerverb has no direct OpenRig match -- fell back to `nam_mesa_rectifier` because <source> describes the voicing as 'tight modern recto-style'").
4. **Tunings** (Drop B / Drop A / Drop C): mention in the reply and, if the user wants it in the chain name, set it via the project/chain name tool. Tuning isn't applied in software -- it's a hint to the user.

## Validation before declaring done

Read `openrig://project` back and confirm:

- [ ] The target chain contains the blocks from the plan, in order.
- [ ] Every `model:` referenced appears in `docs/user-guide/blocks-reference.md` Quick Reference. If not, you invented or guessed a model -- go back to the Quick Reference and pick a real one.
- [ ] Every parameter you set is a documented `path` for that model in `blocks-reference.md`.
- [ ] Disabled-by-default blocks (tuner) read back disabled.
- [ ] `save_project` returned without error.
- [ ] No knowledge of `MODEL_ID`s came from anywhere other than `blocks-reference.md`.

## Red flags -- STOP

If you catch yourself doing any of the following, you have left the skill. Stop, restart from the Quick Reference / the MCP tools:

- Running `find crates/` or `grep MODEL_ID` or `Read` on any `.rs` file.
- Writing a YAML preset file to disk (`~/.openrig/presets/`, `presets/`, anywhere). This skill drives the live rig via MCP; it does not write files. If MCP is unavailable, stop and tell the user (see Precondition).
- Reading another preset to copy a `MODEL_ID` or `params:` shape.
- Saying "I think the model id is X" without having seen X in `blocks-reference.md` in the current session.
- Telling the user "the doc seems incomplete, let me check the source". The doc is the source.

## Anti-patterns

- ❌ Inventing a model name that "sounds right" -- every model name is a hard-matched string in the registry. Wrong name = the `add_block`/`replace_block_model` tool errors.
- ❌ Using a `preamp` block for a full amp song -- `preamp` is preamp-only (no power amp / cab). Songs almost always want `amp`.
- ❌ Writing YAML to disk as a "shortcut". The whole point of this skill now is the live rig via MCP. No files.
- ❌ Skipping the source citation. Always show your work.
- ❌ Pattern-matching another preset's structure instead of the spec in `blocks-reference.md`.
- ❌ Reading `.rs` "just to confirm". The Quick Reference is the contract.

## Common rationalizations -- forbidden

| Rationalization | Reality |
|---|---|
| "The doc might be out of date" | Then file an issue. Don't read source. |
| "Just one quick grep to verify" | One grep is one violation. |
| "MCP isn't connected, I'll just write the YAML" | No. Stop and tell the user to run `openrig --mcp` + install the plugin. Writing files is not this skill. |
| "I know this MODEL_ID from training" | Verify against the Quick Reference before using. |
| "This is faster than reading the doc" | Yes -- and that's the point. The doc is the contract. |
