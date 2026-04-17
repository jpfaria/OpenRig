# Blocks Reference

OpenRig ships with **350 models** across **16 block types**, powered by four distinct audio backends. This document provides a complete reference for every block type and model available in the system.

## Audio Backends

| Backend    | Description                                                                                  |
|------------|----------------------------------------------------------------------------------------------|
| **Native** | Pure Rust DSP. Lowest latency, lowest CPU usage. Parameters are fully controllable in real time. |
| **NAM**    | Neural Amp Modeler. Capture-based modeling that reproduces realistic amp and pedal tones. Higher CPU usage than Native. |
| **IR**     | Impulse Response. Convolution-based speaker and body simulation. Produces a fixed frequency response shaped by the loaded impulse. |
| **LV2**    | Open-source audio plugins. The largest backend with 105 bundled plugins, extending the effects library with community-developed processors across all block types. |

---

## Preamp

A preamp block shapes the guitar signal before it reaches the power amp stage. It controls gain structure, EQ voicing, and tonal character. Preamps set the foundation for everything downstream.

### Models

| Model Name              | Brand    | Backend | Description                     |
|-------------------------|----------|---------|---------------------------------|
| American Clean          | --       | Native  | Clean American-style preamp     |
| Brit Crunch             | --       | Native  | British crunch preamp           |
| Modern High Gain        | --       | Native  | Modern high-gain preamp         |
| Marshall JCM 800 2203   | Marshall          | NAM     | Classic British crunch/gain                            |
| Diezel VH4              | Diezel            | NAM     | Modern high-gain German amp                            |
| Thunder 50              | ENGL              | NAM     | Tight German high-gain lead amp                        |
| '57 Custom Champ        | Fender            | NAM     | Small vintage tweed combo, clean to light breakup      |
| '57 Custom Deluxe       | Fender            | NAM     | Vintage tweed combo with warm, full breakup character  |
| Frontman 15G            | Fender            | NAM     | Solid-state practice amp, clean and gain channels      |
| PA100                   | Fender            | NAM     | Vintage PA head repurposed as a clean guitar amp       |
| Bantamp Meteor          | Joyo              | NAM     | Compact mini-head with a wide range of gain voicings   |
| AVT50H                  | Marshall          | NAM     | Hybrid head, modern high-gain focused                  |
| YJM100                  | Marshall          | NAM     | Yngwie signature, classic JCM800 character with boost  |
| Mark III                | Mesa/Boogie       | NAM     | Tight, percussive Mesa with multiple EQ modes          |
| Micro Terror            | Orange            | NAM     | Tiny all-valve head, warm Orange crunch and saturation |
| Shaman                  | Panama            | NAM     | Versatile amp spanning clean through high-gain         |
| Classic 30              | Peavey            | NAM     | EL84-based combo, clean and slightly pushed tones      |
| MIG-100 KT88            | Sovtek            | NAM     | Russian power-amp character, raw and punchy            |
| VX Kraken               | Victory           | NAM     | Aggressive high-gain head, shred-oriented voicing      |
| MIG-50                  | Electro-Harmonix  | NAM     | Boutique 50W head, clean through overdrive range       |
| 22 Caliber              | Electro-Harmonix  | NAM     | Low-wattage head with clean and crunch tones           |
| Blues Baby 22           | Award-Session     | NAM     | British-influenced 22W combo, clean to overdrive       |
| Fly                     | Blackstar         | NAM     | Ultra-compact amp, clean and crunch tones              |
| Multitone 50            | Koch              | NAM     | Dutch 50W amp with clean, crunch, and OD channels      |
| L2                      | Lab Series        | NAM     | Solid-state Lab Series, clean with unique filtering    |
| Lunchbox Jr             | ZT                | NAM     | Compact 35W solid-state, clean through overdrive       |

### Parameters -- Native Preamp

| Parameter | Range    | Default | Description              |
|-----------|----------|---------|--------------------------|
| input     | 0--100%  | --      | Input level              |
| gain      | 0--100%  | --      | Preamp gain              |
| bass      | 0--100%  | --      | Low-frequency EQ         |
| middle    | 0--100%  | --      | Mid-frequency EQ         |
| treble    | 0--100%  | --      | High-frequency EQ        |
| presence  | 0--100%  | --      | Upper-mid presence       |
| depth     | 0--100%  | --      | Low-end depth            |
| sag       | 0--100%  | --      | Power supply sag         |
| master    | 0--100%  | --      | Master output level      |
| bright    | on/off   | off     | Bright switch             |

### Parameters -- NAM Marshall JCM 800 2203

| Parameter | Range                      | Description              |
|-----------|----------------------------|--------------------------|
| volume    | 50--70%                    | Output volume            |
| gain      | 10--100% (10% steps)       | Gain level               |

### Parameters -- NAM Diezel VH4

| Parameter   | Description              |
|-------------|--------------------------|
| channel     | Amp channel selection    |
| voicing     | Voicing mode             |
| gain_level  | Gain level               |
| boost       | Boost switch             |

### Parameters -- NAM models (standard, issue #204)

All 21 new NAM preamp models added in issue #204 share the same two-parameter interface:

| Parameter | Range                | Description        |
|-----------|----------------------|--------------------|
| volume    | 50--70%              | Output volume      |
| gain      | 10--100% (10% steps) | Gain level         |

Each model selects a capture file based on the `gain` value (mapped to steps of 10). The available voicings per model are:

| Model ID | Available Voicings / Captures |
|----------|-------------------------------|
| `nam_engl_thunder_50` | lead |
| `nam_fender_57_champ` | clean, crunch, od × in1, in2 |
| `nam_fender_57_deluxe` | clean, crunch, od × in1, in2 |
| `nam_fender_frontman_15g` | clean, clean_boost, high_gain |
| `nam_joyo_bantamp_meteor` | clean, clean_gain, low_crunch, crunch, high_gain, high_gain_808, maxed |
| `nam_marshall_avt50h` | hg_bass_cut, hg_dimed, hg_mid_cut, hg_mid_forward, hg_treb_cut |
| `nam_marshall_yjm100` | standard, jcm800_mode, boost_bright, modern |
| `nam_mesa_mark_iii` | eq (lohi / lomed / medhi) × gain (cranked / pushed) |
| `nam_orange_micro_terror` | modified DI |
| `nam_panama_shaman` | clean, crunch, hg, od × gain g1--g9 |
| `nam_peavey_classic_30` | clean, clean_boost |
| `nam_sovtek_mig100` | v1, v2, v3 |
| `nam_victory_vx_kraken` | shred_di, shred_full |
| `nam_ehx_mig50` | clean, crunch, od |
| `nam_ehx_22_caliber` | clean, crunch |
| `nam_award_session_blues_baby_22` | clean, crunch, od |
| `nam_blackstar_fly` | clean, crunch, od |
| `nam_fender_pa100` | clean |
| `nam_koch_multitone_50` | clean, crunch, od × variant 1, 2, 3 |
| `nam_lab_series_l2` | clean, crunch, od |
| `nam_zt_lunchbox_jr` | clean, crunch, od |

---

## Amp

An amp block models a complete amplifier, including preamp and power amp stages together. This is the primary tone-shaping block in most signal chains.

### Models

| Model Name                | Brand     | Backend | Description                        |
|---------------------------|-----------|---------|------------------------------------|
| Blackface Clean           | --        | Native  | Clean American amp                 |
| Tweed Breakup             | --        | Native  | Warm breakup amp                   |
| Chime                     | --        | Native  | Chimey British-style amp           |
| Bogner Ecstasy            | Bogner    | NAM     | Versatile high-gain amp            |
| Bogner Shiva              | Bogner    | NAM     | Dynamic clean-to-gain amp          |
| Dumble ODS                | Dumble    | NAM     | Legendary smooth overdrive         |
| EVH 5150                  | EVH       | NAM     | Iconic high-gain metal amp         |
| Friedman BE100 Deluxe     | Friedman  | NAM     | EL34-powered, 5 channels, 3 mic positions |
| Marshall JCM 800          | Marshall  | NAM     | Classic British rock amp           |
| Marshall JMP-1 Head       | Marshall  | NAM     | JMP-1 OD2 head, no cab             |
| Marshall JVM              | Marshall  | NAM     | Modern versatile Marshall          |
| Mesa Mark V               | Mesa      | NAM     | Tight focused high-gain            |
| Mesa Rectifier            | Mesa      | NAM     | Aggressive modern high-gain        |
| Peavey 5150               | Peavey    | NAM     | Heavy metal workhorse              |
| Ampeg SVT Classic         | Ampeg     | NAM     | Classic bass amp + 6x10 cab        |
| Dover DA-50 + Mesa 4x12   | Dover     | NAM     | Boutique amp + Mesa OS 4x12        |
| Fender Bassman 1971       | Fender    | NAM     | 1971 Bassman, 9 tone presets       |
| Fender Deluxe Reverb '65  | Fender    | NAM     | Clean single-channel combo         |
| Fender Super Reverb 1977  | Fender    | NAM     | Clean multi-mic combo              |
| Marshall JMP-1 Full Rig   | Marshall  | NAM     | JMP-1 OD2 + V30 cab                |
| Marshall Super 100 1966   | Marshall  | NAM     | Vintage SA100 full stack           |
| Peavey 5150 + Mesa 4x12   | Peavey    | NAM     | High-gain with boost/mic options   |
| Roland JC-120B Jazz Chorus| Roland    | NAM     | All-in-one clean + built-in chorus |
| Synergy DRECT Mesa        | Synergy   | NAM     | Metal rig with boost options       |
| Vox AC30                  | Vox       | NAM     | Full rig with character variants   |
| Vox AC30 '61 Fawn EF86    | Vox       | NAM     | Vintage 1961 Vox combo             |
| GxBlueAmp                 | Guitarix  | LV2     | Guitarix blue amp simulation       |
| GxSupersonic              | Guitarix  | LV2     | Guitarix supersonic amp            |
| MDA Combo                 | MDA       | LV2     | Amp combo simulation               |

### Parameters -- Native Amps

| Parameter | Range   | Step | Description              |
|-----------|---------|------|--------------------------|
| gain      | 0--100% | 1.0  | Amp gain                 |
| bass      | 0--100% | 1.0  | Low-frequency EQ         |
| middle    | 0--100% | 1.0  | Mid-frequency EQ         |
| treble    | 0--100% | 1.0  | High-frequency EQ        |
| presence  | 0--100% | 1.0  | Upper-mid presence       |
| depth     | 0--100% | 1.0  | Low-end depth            |
| sag       | 0--100% | 1.0  | Power supply sag         |
| master    | 0--100% | 1.0  | Master output level      |
| room_mix  | 0--100% | 1.0  | Room ambience mix        |

### Parameters -- Friedman BE100 Deluxe

| Parameter | Options                                                                 | Default      |
|-----------|-------------------------------------------------------------------------|--------------|
| channel   | cln_tender (Clean Tender), cln_rock (Clean Rock), be (BE Eddie), hbe_tallica (HBE Tallica), hbe_mammoth (HBE Mammoth) | cln_tender |
| mic       | sm57, sm58, blend                                                       | sm57         |

### Parameters -- Marshall JMP-1 Head

No user-adjustable parameters. Single capture of the JMP-1 OD2 channel.

---

## Cab

A cab (cabinet) block simulates the speaker cabinet and microphone capture. It applies the frequency response of a physical speaker to the signal, which is essential for turning a raw amp tone into a realistic guitar sound.

### Models

| Model Name                      | Brand    | Backend | Description                      |
|---------------------------------|----------|---------|----------------------------------|
| American 2x12                   | --       | Native  | Open-back American cab           |
| Brit 4x12                       | --       | Native  | Closed-back British cab          |
| Vintage 1x12                    | --       | Native  | Small vintage combo cab          |
| Marshall 4x12 V30               | Marshall | IR      | Classic Marshall with Vintage 30s|
| G12M Greenback 2x12             | --       | IR      | Warm vintage speakers            |
| G12T-75 4x12                    | --       | IR      | Bright articulate speakers       |
| V30 4x12                        | --       | IR      | Modern rock/metal standard       |
| Fender Deluxe Reverb Oxford     | Fender   | IR      | Classic American clean           |
| Celestion Cream 4x12            | --       | IR      | Smooth alnico speakers           |
| Mesa Oversized 4x12 V30         | Mesa     | IR      | Deep tight low-end, 9 mic/position options |
| Mesa Standard 4x12 V30          | Mesa     | IR      | Standard OS 4x12, SM57 and SM58  |
| Vox AC30 Blue                   | Vox      | IR      | Chimey British jangle            |
| Vox AC50 2x12 Goodmans          | Vox      | IR      | Vintage AC50 with Goodmans 241   |
| Evil Chug (Blackstar + PRS)     | Blackstar| IR      | High-gain Blackbird 50 + PRS cab |
| G12M Greenback Multi-Mic        | --       | IR      | 6 mic options: SM57/LCT441/MC834/M160/OC818/CC8 |
| Roland JC-120 Cab               | Roland   | IR      | JC-120 cab, SM57+MD421 mix       |
| GxUltraCab                      | Guitarix | LV2     | Guitarix ultra cab simulation    |

### Parameters

| Parameter    | Range          | Description                          |
|--------------|----------------|--------------------------------------|
| low_cut_hz   | 20--500 Hz     | High-pass filter cutoff frequency    |
| high_cut_hz  | 2000--20000 Hz | Low-pass filter cutoff frequency     |
| resonance    | 0--100%        | Cabinet resonance amount             |
| air          | 0--100%        | High-frequency air/openness          |
| mic_position | 0--100%        | Microphone position (center to edge) |
| mic_distance | 0--100%        | Microphone distance from speaker     |
| room_mix     | 0--100%        | Room ambience mix                    |

---

## Gain

A gain block covers overdrive, distortion, fuzz, and volume control pedals. These blocks add harmonic saturation or shape the signal level before or after the amp.

NAM-based gain models capture real hardware with specific parameter snapshots (tone, drive, and boost settings fixed at capture time). They reproduce the character of a particular pedal setting rather than offering continuously variable parameters.

### Models

| Model Name                    | Brand         | Backend | Description                                                    |
|-------------------------------|---------------|---------|----------------------------------------------------------------|
| Volume                        | --            | Native  | Simple volume/mute control                                     |
| Ibanez TS9                    | --            | Native  | Classic tube screamer overdrive                                |
| Blues Overdrive BD-2          | Ibanez        | NAM     | Smooth blues overdrive                                         |
| Ibanez TS9                    | Ibanez        | NAM     | NAM-captured tube screamer                                     |
| JHS Andy Timmons              | JHS           | NAM     | Signature artist overdrive                                     |
| Ampeg SCR-DI                  | Ampeg         | NAM     | Bass DI/preamp with tone and scrambler variants                |
| Behringer SF300 Super Fuzz    | Behringer     | NAM     | Fuzz pedal with fuzz1/fuzz2 variants                           |
| BluesBreaker                  | Marshall      | NAM     | Marshall BluesBreaker clone                                    |
| Boss DS-1 Distortion          | Boss          | NAM     | Classic distortion, tone x dist grid                           |
| Boss DS-1 Wampler JCM Mod     | Boss          | NAM     | JCM-modded DS-1, tone x dist grid                              |
| Boss FZ-1W Fuzz               | Boss          | NAM     | Modern/vintage fuzz modes                                      |
| Boss HM-2 Heavy Metal '86     | Boss          | NAM     | 1986 HM-2, chainsaw and variants                               |
| Boss HM-2 Heavy Metal MiJ     | Boss          | NAM     | Made-in-Japan HM-2, SWEDE/Godflesh/ATG tones                   |
| CC Boost                      | Custom        | NAM     | Clean boost                                                    |
| Darkglass Alpha Omega Ultra   | Darkglass     | NAM     | Bass overdrive, alpha/omega channels                           |
| Darkglass B7K Ultra           | Darkglass     | NAM     | Bass preamp/drive, 5 tones                                     |
| Demonfx BE-OD Clone           | Demonfx       | NAM     | Friedman BE-OD clone, gain variants                            |
| Fulltone OCD v1.2             | Fulltone      | NAM     | Overdrive, LP/HP modes                                         |
| Fulltone OCD v1.5             | Fulltone      | NAM     | Anti-aliased overdrive, LP/HP modes                            |
| Grind                         | TC Electronic | NAM     | Distortion                                                     |
| HM-2                          | Boss          | NAM     | HM-2 single capture                                            |
| Ibanez TS808                  | Ibanez        | NAM     | Tube Screamer 808, standard/driven                             |
| JHS Bonsai                    | JHS           | NAM     | 9 Tube Screamer modes + boost                                  |
| Klon Centaur Silver           | Klon          | NAM     | Legendary overdrive, 6 settings                                |
| Klone                         | Custom        | NAM     | Klon clone, single capture                                     |
| Lokajaudio Der Blend          | Lokajaudio    | NAM     | Fuzz/sustain, 5 character settings                             |
| Lokajaudio Doom Machine V3    | Lokajaudio    | NAM     | Fuzz/octave                                                    |
| Maxon OD808                   | Maxon         | NAM     | OD808 overdrive, drive 0--100%                                 |
| Metal Zone MT-2               | Boss          | NAM     | Metal distortion                                               |
| MXR GT-OD (Zakk Wylde)        | MXR           | NAM     | Overdrive with hq/v2 versions                                  |
| PoT Boost                     | PoT           | NAM     | Clean boost                                                    |
| PoT OD                        | PoT           | NAM     | Overdrive                                                      |
| ProCo RAT                     | ProCo         | NAM     | Classic RAT distortion                                         |
| ProCo RAT 2                   | ProCo         | NAM     | RAT 2, dist/filter variants                                    |
| ROD-10 DS1                    | Custom        | NAM     | ROD-10 into DS-1                                               |
| ROD-10 SD1                    | Custom        | NAM     | ROD-10 into SD-1                                               |
| RR Golden Clone               | RR            | NAM     | Klon-style overdrive, 3 settings                               |
| SansAmp DI-2112               | Tech21        | NAM     | Bass preamp, 9 artist presets (Geddy Lee, Jack Bruce, etc.)    |
| Slammin Clean Booster         | Slammin       | NAM     | 10 clean boost voicings                                        |
| Tascam 424 Preamp             | Tascam        | NAM     | Cassette preamp pedal, gain 7--max                             |
| TC Spark                      | TC Electronic | NAM     | Clean boost, clean/mid                                         |
| TCIP                          | TC Electronic | NAM     | Boost                                                          |
| Tech21 Steve Harris SH-1      | Tech21        | NAM     | Iron Maiden bass preamp                                        |
| Velvet Katana                 | Velvet        | NAM     | Dumble-like tones, 6 characters                                |
| Vemuram Jan Ray               | Vemuram       | NAM     | Mateus Asato signature overdrive                               |
| Bitta                         | --            | LV2     | Bitcrusher distortion                                          |
| MDA Degrade                   | MDA           | LV2     | Lo-fi degradation effect                                       |
| MDA Overdrive                 | MDA           | LV2     | Soft-clip overdrive                                            |
| OJD                           | --            | LV2     | OCD-style overdrive                                            |
| Paranoia                      | --            | LV2     | Fuzz/distortion                                                |
| TAP Sigmoid                   | TAP           | LV2     | Waveshaper distortion                                          |
| Wolf Shaper                   | --            | LV2     | Waveshaper with visual editor                                  |
| CAPS Spice                    | CAPS          | LV2     | Overdrive/distortion                                           |
| CAPS Spice X2                 | CAPS          | LV2     | Overdrive/distortion (stereo)                                  |
| Driva                         | Artyfx        | LV2     | Drive/distortion                                               |
| Satma                         | Artyfx        | LV2     | Saturation effect                                              |
| Invada Tube                   | Invada        | LV2     | Tube saturation/warmth                                         |
| TAP Tubewarmth                | TAP           | LV2     | Tube warmth simulator                                          |

#### Guitarix LV2 Gain Plugins (40 models)

The following 40 overdrive, distortion, and fuzz plugins are provided by the Guitarix project via LV2:

| Model Name            | Brand    | Backend | Description                     |
|-----------------------|----------|---------|---------------------------------|
| Axis Face             | Guitarix | LV2     | Fuzz                            |
| BaJa Tube Driver      | Guitarix | LV2     | Tube driver                     |
| Boob Tube             | Guitarix | LV2     | Tube overdrive                  |
| Bottle Rocket         | Guitarix | LV2     | Overdrive                       |
| Club Drive            | Guitarix | LV2     | Drive pedal                     |
| Cream Machine         | Guitarix | LV2     | Overdrive/distortion            |
| DOP 250               | Guitarix | LV2     | DOD 250 clone                   |
| Epic                  | Guitarix | LV2     | High-gain distortion            |
| Eternity              | Guitarix | LV2     | Eternity overdrive clone        |
| Maestro FZ-1B         | Guitarix | LV2     | Maestro Fuzz-Tone clone (bass)  |
| Maestro FZ-1S         | Guitarix | LV2     | Maestro Fuzz-Tone clone         |
| Guvnor                | Guitarix | LV2     | Marshall Guvnor clone           |
| Hot Box               | Guitarix | LV2     | Overdrive                       |
| Hyperion              | Guitarix | LV2     | Distortion                      |
| Knight Fuzz           | Guitarix | LV2     | Fuzz                            |
| Liquid Drive          | Guitarix | LV2     | Smooth overdrive                |
| Luna                  | Guitarix | LV2     | Overdrive                       |
| Micro Amp             | Guitarix | LV2     | Clean boost                     |
| Saturator             | Guitarix | LV2     | Saturation/clipping             |
| SD-1                  | Guitarix | LV2     | Boss SD-1 clone                 |
| SD-2 Lead             | Guitarix | LV2     | Boss SD-2 lead channel clone    |
| Shaka Tube            | Guitarix | LV2     | Tube overdrive                  |
| Sloopy Blue           | Guitarix | LV2     | Blues overdrive                 |
| Sun Face              | Guitarix | LV2     | Fuzz Face clone                 |
| Super Fuzz            | Guitarix | LV2     | Uni-Vibe era fuzz               |
| Suppa Tone Bender     | Guitarix | LV2     | Tone Bender clone               |
| Tim Ray               | Guitarix | LV2     | Overdrive                       |
| Tone Machine          | Guitarix | LV2     | Octave fuzz                     |
| Tube Distortion       | Guitarix | LV2     | Tube-style distortion           |
| Valve Caster          | Guitarix | LV2     | Tube valve overdrive            |
| Vintage Fuzz Master   | Guitarix | LV2     | Vintage fuzz                    |
| Vmk2                  | Guitarix | LV2     | Distortion                      |
| Voodo Fuzz            | Guitarix | LV2     | Voodoo fuzz                     |

### Parameters -- Native TS9

| Parameter | Range   | Description              |
|-----------|---------|--------------------------|
| drive     | 0--100% | Overdrive amount         |
| tone      | 0--100% | Tone control             |
| level     | 0--100% | Output level             |

### Parameters -- Volume

| Parameter | Range   | Description              |
|-----------|---------|--------------------------|
| volume    | 0--100% | Volume level             |
| mute      | on/off  | Mute switch              |

### Parameters -- NAM Gain Models

NAM gain models expose discrete capture variants rather than continuous knobs. Each model section below lists the selectable options per parameter.

#### Ampeg SCR-DI

| Parameter | Options                                                             | Default  |
|-----------|---------------------------------------------------------------------|----------|
| tone      | standard, ultra_lo, ultra_hi, ultra_lo_hi, scrambler_med, scrambler_max | standard |

#### Behringer SF300 Super Fuzz

| Parameter | Options                            | Default    |
|-----------|------------------------------------|------------|
| tone      | fuzz1, fuzz2_low, fuzz2_high, fuzz2_max | fuzz2_high |

#### BluesBreaker

No user-adjustable parameters. Single capture.

#### Boss DS-1 Distortion

| Parameter | Options                  | Default |
|-----------|--------------------------|---------|
| tone      | 4 (Dark), 7 (Neutral), 10 (Bright) | 7 |
| dist      | 5 (Low), 8 (Medium), 10 (High)     | 8 |

#### Boss DS-1 Wampler JCM Mod

| Parameter | Options                              | Default |
|-----------|--------------------------------------|---------|
| tone      | 2 (Dark), 6 (Neutral), 8 (Bright)   | 6       |
| dist      | 0 (Clean), 5 (Medium), 10 (High)    | 5       |

#### Boss FZ-1W Fuzz

| Parameter | Options        | Default |
|-----------|----------------|---------|
| mode      | modern, vintage | modern  |
| fuzz      | 2, 5, 7, 11    | 5       |

#### Boss HM-2 '86

| Parameter | Options                                              | Default  |
|-----------|------------------------------------------------------|----------|
| tone      | chainsaw_0gain, chainsaw, medium, warm, bright, high_gain, full | chainsaw |

#### Boss HM-2 MiJ

| Parameter | Options                                        | Default |
|-----------|------------------------------------------------|---------|
| tone      | swede, godflesh, atg, boost_sharp, boost_blunt, boost_che | swede |

#### CC Boost

No user-adjustable parameters. Single capture.

#### Darkglass Alpha Omega Ultra

| Parameter | Options         | Default |
|-----------|-----------------|---------|
| channel   | alpha, omega    | omega   |
| gain      | 2, 5, 8, 10     | 5       |

#### Darkglass B7K Ultra

| Parameter | Options                                        | Default |
|-----------|------------------------------------------------|---------|
| tone      | clean, hard_rock, heavy, djent, distortion     | heavy   |

#### Demonfx BE-OD Clone

| Parameter | Options                                            | Default |
|-----------|----------------------------------------------------|---------|
| gain      | 50 (Low), 75 (Medium), 100 (High), 100_tight (High Tight) | 75 |

#### Fulltone OCD v1.2

| Parameter | Options                    | Default |
|-----------|----------------------------|---------|
| mode      | lp (LP), hp (HP)           | lp      |
| drive     | 0 (Low), 4 (Medium), 7 (High) | 4    |

#### Fulltone OCD v1.5

| Parameter | Options                         | Default |
|-----------|---------------------------------|---------|
| mode      | lp (LP), hp (HP)                | lp      |
| drive     | 3 (Low), 9 (Medium), 12 (High)  | 9       |

#### Grind

No user-adjustable parameters. Single capture.

#### HM-2

No user-adjustable parameters. Single capture.

#### Ibanez TS808

| Parameter | Options                | Default  |
|-----------|------------------------|----------|
| character | standard, driven       | standard |

#### JHS Bonsai (9 TS)

| Parameter | Options                          | Default |
|-----------|----------------------------------|---------|
| mode      | 808, ts9, od1, jhs, keeley       | 808     |
| boost     | on, off                          | off     |

#### Klon Centaur Silver

| Parameter | Options                                    | Default |
|-----------|--------------------------------------------|---------|
| setting   | 255, 277, 468, 555, 668, john_mayer        | 555     |

#### Klone

No user-adjustable parameters. Single capture.

#### Lokajaudio Der Blend

| Parameter | Options                                | Default |
|-----------|----------------------------------------|---------|
| character | off, mid, high, high_boost, max        | high    |

#### Lokajaudio Doom Machine V3

No user-adjustable parameters. Single capture.

#### Maxon OD808

| Parameter     | Options                      | Default |
|---------------|------------------------------|---------|
| drive_percent | 0, 25, 50, 75, 100%          | 50%     |

#### Metal Zone MT-2

No user-adjustable parameters. Single capture.

#### MXR GT-OD (Zakk Wylde)

| Parameter | Options    | Default |
|-----------|------------|---------|
| version   | hq, v2     | hq      |

#### PoT Boost

No user-adjustable parameters. Single capture.

#### PoT OD

No user-adjustable parameters. Single capture.

#### ProCo RAT

No user-adjustable parameters. Single capture.

#### ProCo RAT 2

| Parameter | Options                          | Default |
|-----------|----------------------------------|---------|
| tone      | light, medium, heavy, max        | medium  |

#### ROD-10 DS1

No user-adjustable parameters. Single capture.

#### ROD-10 SD1

No user-adjustable parameters. Single capture.

#### RR Golden Clone

| Parameter | Options                          | Default |
|-----------|----------------------------------|---------|
| setting   | 5_4 (5/4), 6_6 (6/6), 2_7 (2/7) | 6_6     |

#### SansAmp DI-2112

| Parameter | Options                                                                              | Default        |
|-----------|--------------------------------------------------------------------------------------|----------------|
| preset    | geddy_standard, geddy_roundabout, yyz, jack_bruce, jpj, les_claypool, entwistle, radiohead, deep_sat | geddy_standard |

#### Slammin Clean Booster

| Parameter | Options                                                                                                               | Default   |
|-----------|-----------------------------------------------------------------------------------------------------------------------|-----------|
| character | od808_t5 (OD808 T5), od808_t7 (OD808 T7), ocd_lp_t5 (OCD LP), ocd_hp_t5 (OCD HP), sd1_t5 (SD1 T5), sd1_t7 (SD1 T7), goldenpearl_t5 (Golden Pearl), echopre_bright (EchoPre Bright), echopre_mid (EchoPre Mid), echopre_dark (EchoPre Dark) | od808_t5 |

#### Tascam 424 Preamp

| Parameter | Options                          | Default |
|-----------|----------------------------------|---------|
| gain      | 7 (Low), 8 (Medium), 9 (High), max (Max) | 8 |

#### TC Spark

| Parameter | Options       | Default |
|-----------|---------------|---------|
| character | clean, mid    | clean   |

#### TCIP

No user-adjustable parameters. Single capture.

#### Tech21 Steve Harris SH-1

| Parameter | Options                    | Default  |
|-----------|----------------------------|----------|
| character | standard, less_highs       | standard |

#### Velvet Katana

| Parameter | Options                                                                                       | Default |
|-----------|-----------------------------------------------------------------------------------------------|---------|
| character | country (Country), blues_bright (Blues Bright), larry (Larry Carlton), brad (Brad), drive (Drive), drive_plus (Drive ++) | larry |

#### Vemuram Jan Ray

| Parameter | Options                    | Default  |
|-----------|----------------------------|----------|
| character | mid_gain, high_gain        | mid_gain |

---

## Delay

A delay block produces echo and repetition effects by playing back a copy of the signal after a configurable time interval. Different delay models apply distinct filtering and modulation characteristics to the repeats.

### Models

| Model Name       | Brand        | Backend | Description                              |
|------------------|--------------|---------|------------------------------------------|
| Digital Clean    | --           | Native  | Clean digital delay                      |
| Analog Warm      | --           | Native  | Warm analog-style delay with filtering   |
| Slapback         | --           | Native  | Short slapback echo                      |
| Reverse          | --           | Native  | Reversed delay tails                     |
| Modulated Delay  | --           | Native  | Delay with modulation                    |
| Tape Vintage     | --           | Native  | Vintage tape echo simulation             |
| Bollie Delay     | Bollie       | LV2     | Delay effect                             |
| Avocado          | Remaincalm   | LV2     | Delay effect                             |
| Floaty           | Remaincalm   | LV2     | Delay effect                             |
| Modulay          | Shiro        | LV2     | Modulated delay                          |
| MDA DubDelay     | MDA          | LV2     | Dub-style delay                          |
| TAP Doubler      | TAP          | LV2     | Stereo doubler delay                     |
| TAP Stereo Echo  | TAP          | LV2     | Stereo echo                              |
| TAP Reflector    | TAP          | LV2     | Reflective delay                         |

### Parameters -- Native Delays

| Parameter | Range       | Description                      |
|-----------|-------------|----------------------------------|
| time_ms   | 1--2000 ms  | Delay time in milliseconds       |
| feedback  | 0--100%     | Amount of signal fed back        |
| mix       | 0--100%     | Dry/wet mix                      |

---

## Reverb

A reverb block simulates the natural reflections of an acoustic space or mechanical reverb device.

### Models

| Model Name                    | Brand     | Backend | Description                            |
|-------------------------------|-----------|---------|----------------------------------------|
| Plate Foundation              | --        | Native  | Studio plate reverb                    |
| Hall                          | --        | Native  | Large hall reverb                      |
| Room                          | --        | Native  | Small room reverb                      |
| Spring                        | --        | Native  | Spring reverb simulation               |
| Dragonfly Early Reflections   | Dragonfly | LV2     | Early reflections simulator            |
| Dragonfly Hall Reverb         | Dragonfly | LV2     | Algorithmic hall reverb                |
| Dragonfly Plate Reverb        | Dragonfly | LV2     | Algorithmic plate reverb               |
| Dragonfly Room Reverb         | Dragonfly | LV2     | Algorithmic room reverb                |
| CAPS Plate                    | CAPS      | LV2     | Plate reverb                           |
| CAPS Plate X2                 | CAPS      | LV2     | Stereo plate reverb                    |
| CAPS Scape                    | CAPS      | LV2     | Ambient reverb/soundscape              |
| TAP Reflector                 | TAP       | LV2     | Reflective reverb                      |
| TAP Reverberator              | TAP       | LV2     | General-purpose reverberator           |
| MDA Ambience                  | MDA       | LV2     | Ambience reverb                        |
| MVerb                         | Distrho   | LV2     | High-quality algorithmic reverb        |
| B Reverb                      | SetBfree  | LV2     | Reverb effect                          |
| Roomy                         | OpenAV    | LV2     | Room reverb                            |
| Shiroverb                     | Shiro     | LV2     | Reverb effect                          |
| Floaty                        | Remaincalm| LV2     | Ambient reverb                         |

### Parameters -- Native Reverbs

| Parameter | Range   | Description                           |
|-----------|---------|---------------------------------------|
| room_size | 0--100% | Size of the simulated space           |
| damping   | 0--100% | High-frequency absorption amount      |
| mix       | 0--100% | Dry/wet mix                           |

---

## Modulation

Modulation blocks alter the signal with periodic variation in amplitude, pitch, or time, producing effects like tremolo, vibrato, chorus, phaser, and rotary speaker.

### Models

| Model Name          | Brand | Backend | Description                              |
|---------------------|-------|---------|------------------------------------------|
| Sine Tremolo        | --    | Native  | Classic sine-wave tremolo                |
| Vibrato             | --    | Native  | Pitch vibrato (100% wet, no dry signal)  |
| Classic Chorus      | --    | Native  | Traditional chorus effect                |
| Ensemble Chorus     | --    | Native  | Rich ensemble-style chorus               |
| Stereo Chorus       | --    | Native  | Wide stereo chorus                       |
| TAP Chorus/Flanger  | TAP   | LV2     | Combined chorus and flanger              |
| TAP Tremolo         | TAP   | LV2     | Tremolo effect                           |
| TAP Rotary Speaker  | TAP   | LV2     | Rotary speaker (Leslie) simulation       |
| MDA Leslie          | MDA   | LV2     | Leslie cabinet simulator                 |
| MDA RingMod         | MDA   | LV2     | Ring modulator                           |
| MDA ThruZero        | MDA   | LV2     | Through-zero flanger                     |
| FOMP CS Chorus      | FOMP  | LV2     | CS-style chorus                          |
| FOMP CS Phaser      | FOMP  | LV2     | CS-style phaser                          |
| CAPS Phaser II      | CAPS  | LV2     | Phaser effect                            |
| Harmless            | Shiro | LV2     | Harmonic modulation                      |
| Larynx              | Shiro | LV2     | Vocal-style modulation                   |

### Parameters -- Tremolo

| Parameter | Range       | Description              |
|-----------|-------------|--------------------------|
| rate_hz   | 0.1--20 Hz  | Modulation rate          |
| depth     | 0--100%     | Modulation depth         |

### Parameters -- Vibrato

| Parameter | Range      | Description              |
|-----------|------------|--------------------------|
| rate_hz   | 0.1--8 Hz  | Modulation rate          |
| depth     | 0--100%    | Modulation depth         |

### Parameters -- Chorus

| Parameter | Range   | Description              |
|-----------|---------|--------------------------|
| rate_hz   | --      | Modulation rate          |
| depth     | --      | Modulation depth         |
| mix       | --      | Dry/wet mix              |

---

## Dynamics

Dynamics blocks control the dynamic range of the signal, either compressing loud peaks, gating unwanted noise, or hard-limiting output.

### Models

| Model Name                | Brand | Backend | Description                          |
|---------------------------|-------|---------|--------------------------------------|
| Studio Clean Compressor   | --    | Native  | Transparent studio compressor        |
| Noise Gate                | --    | Native  | Simple noise gate                    |
| Brick Wall Limiter        | --    | Native  | Hard limiter                         |
| TAP DeEsser               | TAP   | LV2     | De-esser                             |
| TAP Dynamics              | TAP   | LV2     | Dynamic processor                    |
| TAP Scaling Limiter       | TAP   | LV2     | Limiter                              |
| ZamComp                   | ZAM   | LV2     | Compressor                           |
| ZamGate                   | ZAM   | LV2     | Gate                                 |
| ZaMultiComp               | ZAM   | LV2     | Multiband compressor                 |

### Parameters -- Studio Clean Compressor

| Parameter   | Range        | Description                       |
|-------------|--------------|-----------------------------------|
| threshold   | 0--100%      | Compression threshold             |
| ratio       | 0--100%      | Compression ratio                 |
| attack_ms   | 0.1--200 ms  | Attack time in milliseconds       |
| release_ms  | 1--500 ms    | Release time in milliseconds      |
| makeup_gain | 0--100%      | Makeup gain after compression     |
| mix         | 0--100%      | Dry/wet mix (parallel compression)|

### Parameters -- Noise Gate

| Parameter  | Range   | Description                       |
|------------|---------|-----------------------------------|
| threshold  | 0--100% | Gate threshold                    |
| attack_ms  | --      | Attack time in milliseconds       |
| release_ms | --      | Release time in milliseconds      |

---

## Filter

Filter blocks shape the frequency spectrum of the signal using equalization and dynamic filtering.

### Models

| Model Name        | Brand | Backend | Description                    |
|-------------------|-------|---------|--------------------------------|
| Three Band EQ          | --    | Native  | 3-band parametric EQ                       |
| Guitar EQ              | --    | Native  | Low-cut + high-cut EQ for guitar           |
| 8-Band Parametric EQ   | --    | Native  | 8 independent bands, full parametric control |
| TAP Equalizer     | TAP   | LV2     | Parametric EQ                  |
| TAP Equalizer/BW  | TAP   | LV2     | Butterworth EQ                 |
| ZamEQ2            | ZAM   | LV2     | 2-band parametric EQ           |
| ZamGEQ31          | ZAM   | LV2     | 31-band graphic EQ             |
| CAPS AutoFilter   | CAPS  | LV2     | Auto filter                    |
| FOMP Auto-Wah     | FOMP  | LV2     | Auto-wah filter                |
| MOD High Pass     | MOD   | LV2     | High-pass filter               |
| MOD Low Pass      | MOD   | LV2     | Low-pass filter                |
| Filta             | OpenAV| LV2     | Filter effect                  |
| Mud               | Remaincalm | LV2 | Mud filter                    |

### Parameters -- Three Band EQ

| Parameter | Range   | Mapped Range       | Description        |
|-----------|---------|--------------------|--------------------|
| low       | 0--100% | -24 dB to +24 dB  | Low-band gain      |
| mid       | 0--100% | -24 dB to +24 dB  | Mid-band gain      |
| high      | 0--100% | -24 dB to +24 dB  | High-band gain     |

### Parameters -- Guitar EQ

Cuts the two frequency ranges known to cause noise and mud in guitar signals. Each cut uses a gentle Butterworth shelf (Q=0.707) so the rolloff is musical rather than surgical.

| Parameter | Range   | Mapped Range  | Description                                          |
|-----------|---------|---------------|------------------------------------------------------|
| low_cut   | 0--100% | 0 to -12 dB   | Low-shelf cut below 80 Hz (rumble, stage noise, mud) |
| high_cut  | 0--100% | 0 to -12 dB   | High-shelf cut above 8 kHz (hiss, pick fizz)         |

### Parameters -- 8-Band Parametric EQ

Eight independent biquad filter stages in cascade. Each band is separately configurable. Default frequencies: 62, 125, 250, 500, 1k, 2k, 4k, 8kHz (all Peak, 0 dB).

Each band `{N}` (1–8) exposes five parameters:

| Parameter        | Type   | Range          | Default | Description                                        |
|------------------|--------|----------------|---------|----------------------------------------------------|
| `band{N}_enabled`| bool   | on/off         | on      | Bypass this band when off                          |
| `band{N}_type`   | enum   | see below      | peak    | Filter shape                                       |
| `band{N}_freq`   | float  | 20–20000 Hz    | *       | Center / corner frequency                          |
| `band{N}_gain`   | float  | -24 to +24 dB  | 0       | Boost or cut (ignored by LP/HP/Notch)              |
| `band{N}_q`      | float  | 0.1–10         | 1.0     | Bandwidth / resonance (higher = narrower)          |

Band types:

| Value        | Name        | Description                               |
|--------------|-------------|-------------------------------------------|
| `peak`       | Peak        | Bell-shaped boost or cut                  |
| `low_shelf`  | Low Shelf   | Boost/cut all frequencies below `freq`    |
| `high_shelf` | High Shelf  | Boost/cut all frequencies above `freq`    |
| `low_pass`   | Low Pass    | Attenuates frequencies above `freq`       |
| `high_pass`  | High Pass   | Attenuates frequencies below `freq`       |
| `notch`      | Notch       | Narrow cut at `freq` (gain ignored)       |

Example YAML:

```yaml
- type: filter
  model: eq_eight_band_parametric
  enabled: true
  params:
    band1_enabled: true
    band1_type: high_pass
    band1_freq: 80.0
    band1_gain: 0.0
    band1_q: 0.707
    band2_enabled: true
    band2_type: low_shelf
    band2_freq: 200.0
    band2_gain: 3.0
    band2_q: 0.707
    band3_enabled: true
    band3_type: peak
    band3_freq: 500.0
    band3_gain: -2.0
    band3_q: 2.0
    # bands 4–8 follow the same pattern
```

---

## Wah

A wah block produces a resonant bandpass filter sweep, controlled by a position parameter that simulates a rocker pedal.

### Models

| Model Name   | Brand    | Backend | Description            |
|--------------|----------|---------|-----------------------|
| Cry Classic  | --       | Native  | Classic wah-wah pedal |
| GxQuack      | Guitarix | LV2     | Wah effect            |

### Parameters -- Cry Classic

| Parameter | Description                        |
|-----------|------------------------------------|
| position  | Pedal position (heel to toe)       |
| Q         | Filter resonance width             |
| mix       | Dry/wet mix                        |
| output    | Output level                       |

---

## Utility

Utility blocks provide non-audio-processing tools that support the signal chain workflow.

### Models

| Model Name         | Brand | Backend | Description                                    |
|--------------------|-------|---------|------------------------------------------------|
| Chromatic Tuner    | --    | Native  | Reference tuner                                |
| Spectrum Analyzer  | --    | Native  | Real-time frequency spectrum display           |

### Parameters -- Chromatic Tuner

| Parameter    | Range        | Default | Description                       |
|--------------|--------------|---------|-----------------------------------|
| reference_hz | 400--480 Hz  | 440 Hz  | Reference pitch for A4 tuning     |

Spectrum Analyzer is a display-only block with no user-adjustable parameters.

---

## Pitch

Pitch blocks provide real-time pitch shifting, correction, and harmonization for monophonic audio sources.

### Models

| Model Name      | Brand   | Backend | Description                          |
|-----------------|---------|---------|--------------------------------------|
| Harmonizer      | Infamous| LV2     | Pitch harmonizer                     |
| x42 Autotune    | x42     | LV2     | Chromatic pitch correction           |
| MDA Detune      | MDA     | LV2     | Subtle pitch detune/doubler          |
| MDA RePsycho!   | MDA     | LV2     | Pitch shifting effect                |

---

## Body

Body blocks simulate the acoustic resonance of a guitar body using impulse responses. They are designed for use with piezo or magnetic pickups to produce a convincing acoustic tone. OpenRig includes **114 body models** spanning a wide range of acoustic guitar brands and body types.

All models use the **IR** backend.

### Models by Brand

| Brand       | Count | Examples                                              |
|-------------|-------|-------------------------------------------------------|
| Martin      | 37    | Dreadnought, OM, 000 series and variants              |
| Taylor      | 31    | Various guitar body types and tonewoods                |
| Gibson      | 9     | J-45, Hummingbird, and other iconic models             |
| Takamine    | 4     | Acoustic-electric models                               |
| Yamaha      | 4     | Concert and dreadnought models                         |
| Guild       | 3     | Jumbo and orchestra models                             |
| Others      | 26    | Ibanez, Ovation, Rainsong, Lowden, classical, vintage  |

---

## Full Rig

A full rig block is reserved for NAM captures that include the complete signal chain — preamp, power amp, cabinet, **and** effects pedals baked in. Currently no models are bundled.

> **Note:** Models previously listed here (Ampeg SVT, Fender Bassman, Vox AC30, etc.) were reclassified as **Amp** blocks (issue #208), since they are amp+cab captures without pedals.

---

## IR Loader

The IR Loader block is a generic impulse response loader that allows users to load their own IR files from disk. Unlike the fixed cab and body IR models bundled with OpenRig, this block accepts any standard WAV-format IR file.

### Models

| Model Name  | Brand | Backend | Description               |
|-------------|-------|---------|---------------------------|
| generic_ir  | --    | Native  | User-supplied IR file     |

---

## NAM Loader

The NAM Loader block is a generic Neural Amp Modeler capture loader that allows users to load their own `.nam` capture files from disk. Unlike the fixed NAM amp and pedal models bundled with OpenRig, this block accepts any compatible NAM capture.

### Models

| Model Name   | Brand | Backend | Description                   |
|--------------|-------|---------|-------------------------------|
| generic_nam  | --    | NAM     | User-supplied NAM capture     |

---

## Summary

| Block Type  | Models  | Backends Available       |
|-------------|---------|--------------------------|
| Preamp      | 5       | Native, NAM              |
| Amp         | 17      | Native, NAM, LV2         |
| Cab         | 12      | Native, IR, LV2          |
| Gain        | 91      | Native, NAM, LV2         |
| Delay       | 14      | Native, LV2              |
| Reverb      | 19      | Native, LV2              |
| Modulation  | 16      | Native, LV2              |
| Dynamics    | 9       | Native, LV2              |
| Filter      | 13      | Native, LV2              |
| Wah         | 2       | Native, LV2              |
| Utility     | 2       | Native                   |
| Pitch       | 4       | LV2                      |
| Body        | 114     | IR                       |
| Full Rig    | 12      | NAM                      |
| IR Loader   | 1       | Native                   |
| NAM Loader  | 1       | NAM                      |
| **Total**   | **331** |                          |

> Gain includes 2 Native models, 43 NAM captures, and 46 LV2 plugins (including 33 Guitarix models). NAM captures reproduce specific hardware settings at capture time; parameters are fixed per capture variant rather than continuously variable.
