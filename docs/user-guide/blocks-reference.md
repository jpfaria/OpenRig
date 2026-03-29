# Blocks Reference

OpenRig ships with **174 models** across **14 block types**, powered by four distinct audio backends. This document provides a complete reference for every block type and model available in the system.

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

| Model Name        | Brand   | Backend | Description                        |
|-------------------|---------|---------|------------------------------------|
| Blackface Clean   | --      | Native  | Clean American amp                 |
| Tweed Breakup     | --      | Native  | Warm breakup amp                   |
| Chime             | --      | Native  | Chimey British-style amp           |
| Bogner Ecstasy    | Bogner  | NAM     | Versatile high-gain amp            |
| Bogner Shiva      | Bogner  | NAM     | Dynamic clean-to-gain amp          |
| Dumble ODS        | Dumble  | NAM     | Legendary smooth overdrive         |
| EVH 5150          | EVH     | NAM     | Iconic high-gain metal amp         |
| Marshall JCM 800  | Marshall| NAM     | Classic British rock amp           |
| Marshall JVM      | Marshall| NAM     | Modern versatile Marshall          |
| Mesa Mark V       | Mesa    | NAM     | Tight focused high-gain            |
| Mesa Rectifier    | Mesa    | NAM     | Aggressive modern high-gain        |
| Peavey 5150       | Peavey  | NAM     | Heavy metal workhorse              |

### Parameters

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

### Models

| Model Name           | Brand   | Backend | Description                          |
|----------------------|---------|---------|--------------------------------------|
| Volume               | --      | Native  | Simple volume/mute control           |
| Ibanez TS9           | --      | Native  | Classic tube screamer overdrive      |
| Blues Overdrive BD-2  | Ibanez  | NAM     | Smooth blues overdrive               |
| Ibanez TS9           | Ibanez  | NAM     | NAM-captured tube screamer           |
| JHS Andy Timmons     | JHS     | NAM     | Signature artist overdrive           |
| Bitta                | --      | LV2     | Bitcrusher distortion                |
| Chow Centaur         | --      | LV2     | Klon Centaur clone                   |
| MDA Degrade          | --      | LV2     | Lo-fi degradation effect             |
| MDA Overdrive        | --      | LV2     | Soft-clip overdrive                  |
| OJD                  | --      | LV2     | OCD-style overdrive                  |
| Paranoia             | --      | LV2     | Fuzz/distortion                      |
| TAP Sigmoid          | --      | LV2     | Waveshaper distortion                |
| Wolf Shaper          | --      | LV2     | Waveshaper with visual editor        |

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

| Model Name                      | Brand  | Backend | Description                                    |
|---------------------------------|--------|---------|-------------------------------------------------|
| Roland JC-120B Jazz Chorus      | Roland | NAM     | All-in-one clean amp with built-in chorus       |

---

## Summary

| Block Type  | Models | Backends Available       |
|-------------|--------|--------------------------|
| Preamp      | 5      | Native, NAM              |
| Amp         | 12     | Native, NAM              |
| Cab         | 11     | Native, IR               |
| Gain        | 13     | Native, NAM, LV2         |
| Delay       | 6      | Native                   |
| Reverb      | 1      | Native                   |
| Modulation  | 5      | Native                   |
| Dynamics    | 2      | Native                   |
| Filter      | 1      | Native                   |
| Wah         | 1      | Native                   |
| Utility     | 1      | Native                   |
| Body        | 114    | IR                       |
| Full Rig    | 1      | NAM                      |
| **Total**   | **174**|                          |
