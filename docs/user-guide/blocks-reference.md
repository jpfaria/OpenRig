# Blocks Reference

OpenRig ships with **229 models** across **15 block types**, powered by four distinct audio backends. This document provides a complete reference for every block type and model available in the system.

## Audio Backends

| Backend    | Description                                                                                  |
|------------|----------------------------------------------------------------------------------------------|
| **Native** | Pure Rust DSP. Lowest latency, lowest CPU usage. Parameters are fully controllable in real time. |
| **NAM**    | Neural Amp Modeler. Capture-based modeling that reproduces realistic amp and pedal tones. Higher CPU usage than Native. |
| **IR**     | Impulse Response. Convolution-based speaker and body simulation. Produces a fixed frequency response shaped by the loaded impulse. |
| **LV2**    | Open-source audio plugins. Extends the effects library with community-developed processors.  |

---

## Preamp

A preamp block shapes the guitar signal before it reaches the power amp stage. It controls gain structure, EQ voicing, and tonal character. Preamps set the foundation for everything downstream.

### Models

| Model Name              | Brand    | Backend | Description                     |
|-------------------------|----------|---------|---------------------------------|
| American Clean          | --       | Native  | Clean American-style preamp     |
| Brit Crunch             | --       | Native  | British crunch preamp           |
| Modern High Gain        | --       | Native  | Modern high-gain preamp         |
| Marshall JCM 800 2203   | Marshall | NAM     | Classic British crunch/gain     |
| Diezel VH4              | Diezel   | NAM     | Modern high-gain German amp     |

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

| Model Name                      | Brand   | Backend | Description                      |
|---------------------------------|---------|---------|----------------------------------|
| American 2x12                   | --      | Native  | Open-back American cab           |
| Brit 4x12                       | --      | Native  | Closed-back British cab          |
| Vintage 1x12                    | --      | Native  | Small vintage combo cab          |
| Marshall 4x12 V30               | Marshall| IR      | Classic Marshall with Vintage 30s|
| G12M Greenback 2x12             | --      | IR      | Warm vintage speakers            |
| G12T-75 4x12                    | --      | IR      | Bright articulate speakers       |
| V30 4x12                        | --      | IR      | Modern rock/metal standard       |
| Fender Deluxe Reverb Oxford     | Fender  | IR      | Classic American clean           |
| Celestion Cream 4x12            | --      | IR      | Smooth alnico speakers           |
| Mesa Oversized 4x12 V30         | Mesa    | IR      | Deep tight low-end               |
| Vox AC30 Blue                   | Vox     | IR      | Chimey British jangle            |

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
| Chow Centaur                  | --            | LV2     | Klon Centaur clone                                             |
| MDA Degrade                   | --            | LV2     | Lo-fi degradation effect                                       |
| MDA Overdrive                 | --            | LV2     | Soft-clip overdrive                                            |
| OJD                           | --            | LV2     | OCD-style overdrive                                            |
| Paranoia                      | --            | LV2     | Fuzz/distortion                                                |
| TAP Sigmoid                   | --            | LV2     | Waveshaper distortion                                          |
| Wolf Shaper                   | --            | LV2     | Waveshaper with visual editor                                  |

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

All models use the **Native** backend.

| Model Name       | Description                                    |
|------------------|------------------------------------------------|
| Digital Clean    | Clean digital delay                            |
| Analog Warm      | Warm analog-style delay with filtering         |
| Slapback         | Short slapback echo                            |
| Reverse          | Reversed delay tails                           |
| Modulated Delay  | Delay with modulation                          |
| Tape Vintage     | Vintage tape echo simulation                   |

### Parameters

| Parameter | Range       | Description                      |
|-----------|-------------|----------------------------------|
| time_ms   | 1--2000 ms  | Delay time in milliseconds       |
| feedback  | 0--100%     | Amount of signal fed back        |
| mix       | 0--100%     | Dry/wet mix                      |

---

## Reverb

A reverb block simulates the natural reflections of an acoustic space or mechanical reverb device.

### Models

| Model Name        | Brand | Backend | Description           |
|-------------------|-------|---------|-----------------------|
| Plate Foundation  | --    | Native  | Studio plate reverb   |

### Parameters

| Parameter | Range   | Description                           |
|-----------|---------|---------------------------------------|
| room_size | 0--100% | Size of the simulated space           |
| damping   | 0--100% | High-frequency absorption amount      |
| mix       | 0--100% | Dry/wet mix                           |

---

## Modulation

Modulation blocks alter the signal with periodic variation in amplitude, pitch, or time, producing effects like tremolo, vibrato, and chorus.

### Models

All models use the **Native** backend.

| Model Name     | Description                              |
|----------------|------------------------------------------|
| Sine Tremolo   | Classic sine-wave tremolo                |
| Vibrato        | Pitch vibrato (100% wet, no dry signal)  |
| Classic Chorus | Traditional chorus effect                |
| Ensemble Chorus| Rich ensemble-style chorus               |
| Stereo Chorus  | Wide stereo chorus                       |

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

Dynamics blocks control the dynamic range of the signal, either compressing loud peaks or gating unwanted noise.

### Models

All models use the **Native** backend.

| Model Name                | Description                      |
|---------------------------|----------------------------------|
| Studio Clean Compressor   | Transparent studio compressor    |
| Noise Gate                | Simple noise gate                |

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

Filter blocks shape the frequency spectrum of the signal using equalization.

### Models

| Model Name    | Brand | Backend | Description          |
|---------------|-------|---------|-----------------------|
| Three Band EQ | --    | Native  | 3-band parametric EQ |

### Parameters

| Parameter | Range   | Mapped Range       | Description        |
|-----------|---------|--------------------|--------------------|
| low       | 0--100% | -24 dB to +24 dB  | Low-band gain      |
| mid       | 0--100% | -24 dB to +24 dB  | Mid-band gain      |
| high      | 0--100% | -24 dB to +24 dB  | High-band gain     |

---

## Wah

A wah block produces a resonant bandpass filter sweep, controlled by a position parameter that simulates a rocker pedal.

### Models

| Model Name   | Brand | Backend | Description            |
|--------------|-------|---------|-----------------------|
| Cry Classic  | --    | Native  | Classic wah-wah pedal |

### Parameters

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

| Model Name       | Brand | Backend | Description      |
|------------------|-------|---------|--------------------|
| Chromatic Tuner  | --    | Native  | Reference tuner  |

### Parameters

| Parameter    | Range        | Default | Description                       |
|--------------|--------------|---------|-----------------------------------|
| reference_hz | 400--480 Hz  | 440 Hz  | Reference pitch for A4 tuning     |

---

## Pitch

Pitch blocks provide real-time pitch correction (autotune) for monophonic audio sources such as vocals and solo instruments.

### Models

| Model Name         | Brand | Backend | Description                                      |
|--------------------|-------|---------|--------------------------------------------------|
| Chromatic Autotune | --    | Native  | Corrects pitch to nearest chromatic note         |
| Scale Autotune     | --    | Native  | Corrects pitch to nearest note in selected key/scale |

### Parameters -- Chromatic Autotune

| Parameter   | Range          | Default | Description                                         |
|-------------|----------------|---------|-----------------------------------------------------|
| speed       | 0--100 ms      | 20 ms   | Correction speed. 0 = instant/robotic, 100 = natural/smooth |
| mix         | 0--100%        | 100%    | Dry/wet blend                                       |
| detune      | -50--+50 cents | 0       | Fine offset from target note                        |
| sensitivity | 0--100%        | 50%     | Minimum signal level to activate correction         |

### Parameters -- Scale Autotune

Scale Autotune shares all Chromatic Autotune parameters plus two additional controls:

| Parameter   | Range          | Default | Description                                         |
|-------------|----------------|---------|-----------------------------------------------------|
| speed       | 0--100 ms      | 20 ms   | Correction speed. 0 = instant/robotic, 100 = natural/smooth |
| mix         | 0--100%        | 100%    | Dry/wet blend                                       |
| detune      | -50--+50 cents | 0       | Fine offset from target note                        |
| sensitivity | 0--100%        | 50%     | Minimum signal level to activate correction         |
| key         | C through B    | C       | Root note of the scale                              |
| scale       | 8 options      | Major   | Scale type (see table below)                        |

### Available Scales

Major, Natural Minor, Pentatonic Major, Pentatonic Minor, Harmonic Minor, Melodic Minor, Blues, Dorian

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

A full rig block combines the entire signal chain -- preamp, power amp, cabinet, and built-in effects -- into a single unit. This is useful for recalling a complete amp tone with one block.

### Models

| Model Name                      | Brand    | Backend | Description                                      |
|---------------------------------|----------|---------|--------------------------------------------------|
| Roland JC-120B Jazz Chorus      | Roland   | NAM     | All-in-one clean amp with built-in chorus        |
| Ampeg SVT Classic               | Ampeg    | NAM     | Classic bass amp with 6x10 cab                   |
| Dover DA-50 + Mesa 4x12         | Dover    | NAM     | Full rig with Mesa OS 4x12 cab                   |
| Fender Bassman 1971             | Fender   | NAM     | 1971 Bassman full rig, 9 tone presets            |
| Fender Deluxe Reverb '65        | Fender   | NAM     | Clean single-channel with mic variants           |
| Fender Super Reverb 1977        | Fender   | NAM     | Clean amp with mic variants                      |
| Marshall JMP-1 Full Rig         | Marshall | NAM     | JMP-1 OD2 + V30 full rig                         |
| Marshall Super 100 1966         | Marshall | NAM     | Vintage Marshall SA100 full rig                  |
| Peavey 5150 + Mesa 4x12         | Peavey   | NAM     | High-gain full rig with boost and mic options    |
| Synergy DRECT Mesa              | Synergy  | NAM     | Metal full rig with boost options                |
| Vox AC30                        | Vox      | NAM     | Full rig with character variants                 |
| Vox AC30 '61 Fawn EF86          | Vox      | NAM     | Vintage 1961 Vox full rig                        |

### Parameters

#### Roland JC-120B Jazz Chorus

No user-adjustable parameters. Single capture.

#### Ampeg SVT Classic

| Parameter | Options                    | Default  |
|-----------|----------------------------|----------|
| tone      | standard, ultra_hi, ultra_lo | standard |
| mic       | md421, sm57                | md421    |

#### Dover DA-50 + Mesa 4x12

| Parameter | Options             | Default |
|-----------|---------------------|---------|
| boost     | clean, boosted      | clean   |

#### Fender Bassman 1971

| Parameter | Options                                                                                    | Default     |
|-----------|--------------------------------------------------------------------------------------------|-------------|
| tone      | clean, bright_clean, warm_clean, sweet_spot, warm_sweet_spot, cranked, 80s_clean, big_clean, warm_fuzz | sweet_spot |

#### Fender Deluxe Reverb '65

| Parameter | Options                              | Default     |
|-----------|--------------------------------------|-------------|
| mic       | sm57_royer, sm57_royer_room, room    | sm57_royer  |

#### Fender Super Reverb 1977

| Parameter | Options                     | Default |
|-----------|-----------------------------|---------|
| mic       | sm57, akg414, sm57_akg414   | sm57    |

#### Marshall JMP-1 Full Rig

No user-adjustable parameters. Single capture of the JMP-1 OD2 channel with V30 cab.

#### Marshall Super 100 1966

No user-adjustable parameters. Single capture.

#### Peavey 5150 + Mesa 4x12

| Parameter | Options                    | Default  |
|-----------|----------------------------|----------|
| boost     | no_boost, maxon, mxr       | no_boost |
| mic       | sm57, sm58                 | sm57     |

#### Synergy DRECT Mesa

| Parameter | Options                    | Default   |
|-----------|----------------------------|-----------|
| boost     | unboosted, od808, sd1      | unboosted |

#### Vox AC30

| Parameter | Options                          | Default  |
|-----------|----------------------------------|----------|
| character | standard, clean_65prince         | standard |

#### Vox AC30 '61 Fawn EF86

No user-adjustable parameters. Single capture.

---

## Summary

| Block Type  | Models | Backends Available       |
|-------------|--------|--------------------------|
| Preamp      | 5      | Native, NAM              |
| Amp         | 14     | Native, NAM              |
| Cab         | 11     | Native, IR               |
| Gain        | 53     | Native, NAM, LV2         |
| Delay       | 6      | Native                   |
| Reverb      | 1      | Native                   |
| Modulation  | 5      | Native                   |
| Dynamics    | 2      | Native                   |
| Filter      | 1      | Native                   |
| Wah         | 1      | Native                   |
| Utility     | 1      | Native                   |
| Pitch       | 2      | Native                   |
| Body        | 114    | IR                       |
| Full Rig    | 12     | NAM                      |
| **Total**   | **229**|                          |

> Gain includes 2 Native models, 43 NAM captures, and 8 LV2 plugins. NAM captures reproduce specific hardware settings at capture time; parameters are fixed per capture variant rather than continuously variable.
