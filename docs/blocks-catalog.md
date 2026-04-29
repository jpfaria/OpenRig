# Catálogo de blocos

## Tipos de bloco

| Tipo | O que faz | Total | Modelos (resumo) |
|------|-----------|-------|-----------------|
| **Preamp** | Pré-amp, gain, EQ | 39 | American Clean, Brit Crunch, Modern High Gain (native); JCM 800 2203, Thunder 50, '57 Champ/Deluxe, Frontman 15G, PA100, Bantamp Meteor, AVT50H, YJM100, Mark III, Micro Terror, Shaman, Classic 30, MIG-100, VX Kraken, MIG-50, 22 Caliber, Blues Baby 22, Fly, Multitone 50, L2, Lunchbox Jr, ADA MP-1, Mesa Triaxis, Marshall JMP-1, ENGL E530, Mesa Studio Preamp (NAM, #345) |
| **Amp** | Preamp + power amp + cab | 142 | Blackface Clean, Tweed Breakup, Chime (native); Bogner Ecstasy/Shiva/Uberschall/Helios/Goldfinger/Ecstasy 101B, Dumble ODS/Steel String Singer/ODS 100W, EVH 5150/5150 III/5150 III 50W Red, Friedman BE100/BE-50/BE 100/Dirty Shirley, Marshall JCM800/JVM/JMP-1/Plexi/Super Lead/JCM2000 TSL/DSL/JCM900/JTM45/1959HW/DSL40CR/Plexi 50W/6100 30th/1959 SLP, Mesa Mark V/Rectifier/Mark IV/Mark IIC/Mark VII/JP2C/Triple Rectifier/Dual Rectifier, Peavey 5150/6505/JSX, Ampeg SVT/SVT Classic, Fender Bassman/Deluxe Reverb/Super Reverb/Hot Rod Deluxe/Twin Reverb/Princeton Reverb/Princeton Reverb 1972/Blues Junior/Showman, Roland JC-120B, Vox AC30/Fawn/AC15, Diezel Herbert/Hagen, ENGL Ironball/Powerball/Fireball/Gigmaster 30/E530, Orange OR15/Rockerverb/Tiny Terror, PRS Archon/MT15, Tone King Imperial, Driftwood Purple Nightmare, Splawn Quickrod, Soldano SLO 100/SLO 30, Laney/VH100R/Ironheart, Bad Cat Lynx, Hughes & Kettner TubeMeister 18, Ceriatone OTS Mini 20, Matchless Clubman 35, Sunn Model T, Supro Black Magick, Jet City JCA22H, Randall RG100es (NAM); GxBlueAmp, GxSupersonic, MDA Combo (LV2) |
| **Cab** | Caixa/falante | 29 | American 2x12, Brit 4x12, Vintage 1x12 (native); Celestion Cream, Fender Deluxe Reverb Oxford/Twin Reverb 2x12/Super Reverb 4x10/Bassman 2x15, Greenback, G12T-75, Marshall 4x12 V30/1960AV/1960BV/1960TV Greenback, Mesa OS/Standard/Traditional 4x12/Recto V30, Roland JC-120, Vox AC30 Blue, Vox AC50, Orange 2x12 V30, EVH 5150III 4x12 G12-EVH, ENGL E412 Karnivore, Ampeg SVT 4x10/8x10 (IR); GxUltraCab (LV2) |
| **Gain** | Overdrive, distortion, fuzz, boost | 91 | TS9 (native); Boss DS-1/HM-2/FZ-1W/MT-2/BD-2, Klon, RAT/RAT2, OCD, OD808, TS808, Darkglass Alpha Omega/B7K, JHS Bonsai, Bluesbreaker, Vemuram Jan Ray + 34 outros (NAM); Guitarix ×40, CAPS, OJD, Wolf Shaper, MDA (LV2) |
| **Delay** | Eco | 14 | Analog Warm, Digital Clean, Slapback, Reverse, Modulated, Tape Vintage (native); MDA DubDelay, TAP Doubler/Echo/Reflector, Bollie, Avocado, Floaty, Modulay (LV2) |
| **Reverb** | Ambiência | 19 | Hall, Plate Foundation, Room, Spring (native); Dragonfly Hall/Room/Plate/Early, CAPS Plate/X2/Scape, TAP Reflector/Reverberator, MDA Ambience, MVerb, B Reverb, Roomy, Shiroverb, Floaty (LV2) |
| **Modulation** | Chorus, flanger, tremolo, vibrato | 16 | Classic/Stereo/Ensemble Chorus, Sine Tremolo, Vibrato (native); TAP Chorus/Flanger/Tremolo/Rotary, MDA Leslie/RingMod/ThruZero, FOMP, CAPS Phaser II, Harmless, Larynx (LV2) |
| **Dynamics** | Compressor e gate | 9 | Studio Clean Compressor, Noise Gate, Brick Wall Limiter (native); TAP DeEsser/Dynamics/Limiter, ZamComp, ZamGate, ZaMultiComp (LV2) |
| **Filter** | EQ, moldagem tonal | 13 | Three Band EQ, Guitar EQ, 8-Band Parametric EQ (native); TAP Equalizer/BW, ZamEQ2, ZamGEQ31, CAPS AutoFilter, FOMP Auto-Wah, MOD HPF/LPF, Filta, Mud (LV2) |
| **Wah** | Wah-wah | 2 | Cry Classic (native); GxQuack (LV2) |
| **Body** | Ressonância de corpo acústico | 114 | Martin (45), Taylor (30), Gibson (10), Yamaha (5), Guild (4), Takamine (4), Cort (4), Emerald (2), Rainsong (2), Lowden (2) + boutique (IR) |
| **Pitch** | Pitch shift e harmonização | 4 | Harmonizer, x42 Autotune, MDA Detune, MDA RePsycho (LV2) |
| **IR** / **NAM** | Loaders genéricos | 1+1 | generic_ir, generic_nam |
| **Input** / **Output** / **Insert** | I/O | — | standard, standard, external_loop |

**Total: 498+ modelos em 16 tipos (5 backends: Native 33, NAM 215, IR 139, LV2 105, VST3 6).**

`Utility` está vazio (Tuner e Spectrum viraram features de toolbar). `Full Rig` reservado para futuras capturas com cadeia completa.

## Parâmetros comuns

- **Preamp/Amp nativos**: input, gain, bass, middle, treble, presence, depth, sag, master, bright
- **NAM preamp**: volume (50–70%), gain (10–100%) em steps
- **Delay**: time_ms (1–2000), feedback (0–100%), mix (0–100%)
- **Reverb**: room_size, damping, mix (0–100%)
- **Compressor**: threshold, ratio, attack_ms, release_ms, makeup_gain, mix
- **Gate** (`gate_basic`): threshold (-96 a 0 dB), attack_ms (0.1–100), release_ms (1–500), **hold_ms** (0–2000, default 150 — evita cortar decay), **hysteresis_db** (0–20, default 6 — evita chattering)
- **EQ (Three Band / Guitar EQ)**: low, mid, high (0–100% → -24/+24 dB)
- **8-Band Parametric EQ** (`eq_eight_band_parametric`): por banda — `band{N}_enabled`, `band{N}_type` (peak/low_shelf/high_shelf/low_pass/high_pass/notch), `band{N}_freq` (20–20000 Hz), `band{N}_gain` (-24/+24 dB), `band{N}_q` (0.1–10). Freqs padrão: 62/125/250/500/1k/2k/4k/8kHz.
- **Gain pedals**: drive, tone, level
- **NAM gain pedals com grid**: knobs reais por modelo (`tone`, `sustain`, `drive`, `volume`, `gain`...) mapeiam para captura `.nam` mais próxima na grid. Sufixo `_feather`/`_lite`/`_nano` vira enum `size`. Pedais com nomes nominais (`chainsaw`, `medium`) ou `preset_N` mantêm enum dropdown. Codegen: `tools/gen_pedal_models.py`.
- **Volume**: volume (0–100%), mute
- **Vibrato**: rate_hz (0.1–8), depth (0–100%), 100% wet
- **Autotune Chromatic**: speed (0–100ms), mix, detune (±50 cents), sensitivity
- **Autotune Scale**: + key (C–B), scale (Major, Minor, Pent Maj/Min, Harmonic Minor, Melodic Minor, Blues, Dorian)

## Backends de áudio

- **Native** — DSP em Rust, mais rápido
- **NAM** — Neural Amp Modeler
- **IR** — Impulse Response (cabs, corpos)
- **LV2** — Plugins externos open-source

## Instrumentos suportados

`electric_guitar`, `acoustic_guitar`, `bass`, `voice`, `keys`, `drums`, `generic`. Constantes em `crates/block-core/src/lib.rs` (`INST_*`, `ALL_INSTRUMENTS`, `GUITAR_BASS`, `GUITAR_ACOUSTIC_BASS`).

Cada `MODEL_DEFINITION` tem `supported_instruments: &[&str]`. UI filtra a lista de blocos disponíveis. Campo `instrument` salvo no YAML da chain, default `electric_guitar`, fixo após criação.
