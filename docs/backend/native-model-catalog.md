# Native Model Catalog

This document is the working catalog for native OpenRig block targets that are safe to implement without guessing.

An item is approved only when all of these are true:
- the source product is well known
- there is enough public technical information to guide implementation
- there are local captures or IRs that can be used as validation oracles
- the current OpenRig block taxonomy has a clear place for it

This is an implementation catalog, not a user-facing product list.

## Rules

- `block_type` is the OpenRig public block type.
- `model_id` is the proposed native model id inside that block type.
- `oracle` is the local artifact used to validate the native implementation.
- `reference_basis` is the public technical basis that makes the implementation safe enough to start.
- `status` is one of:
  - `approved`
  - `approved_later`
  - `not_approved_yet`

## Approved Now

### Amp / Preamp

| block_type | model_id | manufacturer | product_name | class | oracle | reference_basis | status |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `amp_head` | `jcm800_2203` | Marshall | JCM800 2203 | amp head | NAM without cab | official product/manual/support material + large capture set | `approved` |
| `amp_head` | `jmp_1` | Marshall | JMP-1 | preamp | NAM head/full-rig pair | official/manual support material + dedicated capture set | `approved` |
| `amp_combo` | `ac30` | Vox | AC30 | combo | full rig + IR | official product/manual material + strong identity + curated IRs | `approved` |
| `amp_combo` | `deluxe_reverb` | Fender | Deluxe Reverb | combo | full rig + IR | official product/manual material + strong blackface reference base | `approved` |

#### `amp_head:jcm800_2203`

- Product:
  - manufacturer: `Marshall`
  - product: `JCM800 2203`
  - class: `single-channel amp head`
- Why this is safe:
  - iconic and heavily documented
  - strong public technical baseline
  - large local NAM set with repeated control positions
- Local validation files:
  - directory: `/Users/joao.faria/Library/Mobile Documents/com~apple~CloudDocs/Musica/Capturas/TONE3000/head/Marshall JCM 800 2203`
  - directory: `/Users/joao.faria/Library/Mobile Documents/com~apple~CloudDocs/Musica/Capturas/NAM CAPTURES /JCM800 MARSHALL`
  - representative files:
    - `/Users/joao.faria/Library/Mobile Documents/com~apple~CloudDocs/Musica/Capturas/TONE3000/head/Marshall JCM 800 2203/JCM800 2203 - P5 B5 M5 T5 MV7 G9 - AZG - 700.nam`
    - `/Users/joao.faria/Library/Mobile Documents/com~apple~CloudDocs/Musica/Capturas/NAM CAPTURES /JCM800 MARSHALL/CRUNCH /JCM 800 CRUNCH (NO CAB).nam`
- Native direction:
  - preamp/head native model
  - validate first against `NO CAB` captures

#### `amp_head:jmp_1`

- Product:
  - manufacturer: `Marshall`
  - product: `JMP-1`
  - class: `preamp`
- Why this is safe:
  - famous rack preamp
  - known control structure
  - dedicated local captures already split between head/full-rig semantics
- Local validation files:
  - directory: `/Users/joao.faria/Library/Mobile Documents/com~apple~CloudDocs/Musica/Capturas/CAPTURAS JMP-1/NAM`
  - representative files:
    - `/Users/joao.faria/Library/Mobile Documents/com~apple~CloudDocs/Musica/Capturas/CAPTURAS JMP-1/NAM/JMP-1 OD2 V-30 (HEAD).nam`
    - `/Users/joao.faria/Library/Mobile Documents/com~apple~CloudDocs/Musica/Capturas/CAPTURAS JMP-1/NAM/EP JMP-1 CLEAN.nam`
- Native direction:
  - treat as preamp-focused `block-amp-head`
  - keep combo/full-rig behavior out of the first implementation

#### `amp_combo:ac30`

- Product:
  - manufacturer: `Vox`
  - product: `AC30`
  - class: `combo`
- Why this is safe:
  - famous and very well understood
  - good local full-rig and cab references
  - strong public reference material
- Local validation files:
  - directory: `/Users/joao.faria/Library/Mobile Documents/com~apple~CloudDocs/Musica/Capturas/TONE3000/full_rig/VOX AC30`
  - directory: `/Users/joao.faria/Library/Mobile Documents/com~apple~CloudDocs/Musica/Capturas/TONE3000/ir/Vox AC30 Mix Ready`
  - representative files:
    - `/Users/joao.faria/Library/Mobile Documents/com~apple~CloudDocs/Musica/Capturas/TONE3000/full_rig/VOX AC30/VOX AC30 + cab.nam`
    - `/Users/joao.faria/Library/Mobile Documents/com~apple~CloudDocs/Musica/Capturas/TONE3000/ir/Vox AC30 Mix Ready/VOX AC30 BLUE 1.wav`
- Native direction:
  - combo-native target
  - validate amp section and cab section separately where possible

#### `amp_combo:deluxe_reverb`

- Product:
  - manufacturer: `Fender`
  - product: `Deluxe Reverb`
  - class: `combo`
- Why this is safe:
  - famous clean combo
  - very strong public reference base
  - local full-rig and IR references are clear
- Local validation files:
  - directory: `/Users/joao.faria/Library/Mobile Documents/com~apple~CloudDocs/Musica/Capturas/TONE3000/full_rig/Fender Deluxe Reverb '65 Reissue _ Clean _ SM57 + Royer R-121 + Room`
  - directory: `/Users/joao.faria/Library/Mobile Documents/com~apple~CloudDocs/Musica/Capturas/TONE3000/ir/Fender Deluxe Reverb Mix Ready`
  - representative files:
    - `/Users/joao.faria/Library/Mobile Documents/com~apple~CloudDocs/Musica/Capturas/TONE3000/full_rig/Fender Deluxe Reverb '65 Reissue _ Clean _ SM57 + Royer R-121 + Room/Fender DRRI _ Clean _ DI Capture (No Cab).nam`
    - `/Users/joao.faria/Library/Mobile Documents/com~apple~CloudDocs/Musica/Capturas/TONE3000/ir/Fender Deluxe Reverb Mix Ready/DELUXE REVERB OXFORD - BIG.wav`
- Native direction:
  - combo-native target
  - split validation into amp/power feel and Oxford speaker voicing

### Cab

| block_type | model_id | manufacturer | product_name | class | oracle | reference_basis | status |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `cab` | `marshall_1960_v30` | Marshall | 1960-style 4x12 V30 | guitar cab | IR | strong local IR set + well-known cab archetype | `approved` |
| `cab` | `marshall_1960_t75` | Marshall | 1960-style 4x12 G12T-75 | guitar cab | IR | strong local IR set + well-known cab archetype | `approved` |
| `cab` | `marshall_1960_greenback` | Marshall | 1960-style 4x12 Greenback | guitar cab | IR | strong local IR set + well-known cab archetype | `approved` |
| `cab` | `mesa_os_4x12_v30` | Mesa/Boogie | Oversized 4x12 V30 | guitar cab | IR | curated IRs + iconic modern reference | `approved` |
| `cab` | `vox_ac30_blue` | Vox | AC30 Blue cab voice | combo speaker/cab voice | IR | curated IRs + iconic combo speaker target | `approved` |
| `cab` | `deluxe_reverb_oxford` | Fender | Deluxe Reverb Oxford voice | combo speaker/cab voice | IR | curated IRs + iconic combo speaker target | `approved` |

#### `cab:marshall_1960_v30`

- Product:
  - manufacturer: `Marshall`
  - product: `1960-style 4x12 with Vintage 30`
  - class: `guitar cab`
- Local validation files:
  - `/Users/joao.faria/Library/Mobile Documents/com~apple~CloudDocs/Musica/Capturas/TONE3000/ir/Marshall 4x12 V30 IR/EV MIX B.wav`
  - `/Users/joao.faria/Library/Mobile Documents/com~apple~CloudDocs/Musica/Capturas/TONE3000/ir/Marshall 4x12 V30 IR/EV MIX D.wav`
  - `/Users/joao.faria/Library/Mobile Documents/com~apple~CloudDocs/Musica/Capturas/IRCELESTION/V30 412 C SM57 Balanced Celestion.wav`
- Native direction:
  - first native Marshall V30 cab target

#### `cab:marshall_1960_t75`

- Product:
  - manufacturer: `Marshall`
  - product: `1960-style 4x12 with G12T-75`
  - class: `guitar cab`
- Local validation files:
  - `/Users/joao.faria/Library/Mobile Documents/com~apple~CloudDocs/Musica/Capturas/IRCELESTION/G12T-75 412 C SM57 Dark Celestion.wav`
  - `/Users/joao.faria/Library/Mobile Documents/com~apple~CloudDocs/Musica/Capturas/IRCELESTION/G12T-75 412 C SM57 Fat Celestion.wav`
- Native direction:
  - second native Marshall 4x12 voice

#### `cab:marshall_1960_greenback`

- Product:
  - manufacturer: `Marshall`
  - product: `1960-style 4x12 with Greenback`
  - class: `guitar cab`
- Local validation files:
  - `/Users/joao.faria/Library/Mobile Documents/com~apple~CloudDocs/Musica/Capturas/IRCELESTION/G12M Greenbk 212 C SM57 Dark Celestion.wav`
  - `/Users/joao.faria/Library/Mobile Documents/com~apple~CloudDocs/Musica/Capturas/IRCELESTION/G12M Greenbk 212 C SM57 Fat Celestion.wav`
  - `/Users/joao.faria/Library/Mobile Documents/com~apple~CloudDocs/Musica/Capturas/SUHRIRS/Suhr G12M Greenbk 412 C SM57 Fat Celestion.wav`
- Native direction:
  - greenback voicing target

#### `cab:mesa_os_4x12_v30`

- Product:
  - manufacturer: `Mesa/Boogie`
  - product: `Oversized 4x12 V30`
  - class: `guitar cab`
- Local validation files:
  - `/Users/joao.faria/Library/Mobile Documents/com~apple~CloudDocs/Musica/Capturas/TONE3000/ir/Mesa OS 4x12 IR/Mesa_OS_4x12_57_m160.wav`
  - `/Users/joao.faria/Library/Mobile Documents/com~apple~CloudDocs/Musica/Capturas/TONE3000/ir/Mesa OS 4x12 V30 - SM57-SM58-AT2020-Stereo Room/Mesa Oversized V30 SM57 1 - jp_is_out_of_tune.wav`
  - `/Users/joao.faria/Library/Mobile Documents/com~apple~CloudDocs/Musica/Capturas/TONE3000/ir/Mesa OS 4x12 V30 - SM57-SM58-AT2020-Stereo Room/Room Left Mesa Oversized AT2020 - jp_is_out_of_tune.wav`
- Native direction:
  - core modern high-gain cab target

#### `cab:vox_ac30_blue`

- Product:
  - manufacturer: `Vox`
  - product: `AC30 Blue voice`
  - class: `combo speaker/cab voice`
- Local validation files:
  - `/Users/joao.faria/Library/Mobile Documents/com~apple~CloudDocs/Musica/Capturas/TONE3000/ir/Vox AC30 Mix Ready/VOX AC30 BLUE 1.wav`
  - `/Users/joao.faria/Library/Mobile Documents/com~apple~CloudDocs/Musica/Capturas/TONE3000/ir/Vox AC30 Mix Ready/VOX AC30 BLUE 2.wav`
- Native direction:
  - combo-speaker voicing, not just a generic 2x12

#### `cab:deluxe_reverb_oxford`

- Product:
  - manufacturer: `Fender`
  - product: `Deluxe Reverb Oxford voice`
  - class: `combo speaker/cab voice`
- Local validation files:
  - `/Users/joao.faria/Library/Mobile Documents/com~apple~CloudDocs/Musica/Capturas/TONE3000/ir/Fender Deluxe Reverb Mix Ready/DELUXE REVERB OXFORD - BIG.wav`
  - `/Users/joao.faria/Library/Mobile Documents/com~apple~CloudDocs/Musica/Capturas/TONE3000/ir/Fender Deluxe Reverb Mix Ready/DELUXE REVERB OXFORD - LEAN.wav`
- Native direction:
  - clean combo speaker target with two curated variants already visible in source material

### Pedal

| block_type | model_id | manufacturer | product_name | class | oracle | reference_basis | status |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `gain` | `ibanez_ts9` | Ibanez | TS9 Tube Screamer | overdrive | NAM | iconic circuit + dedicated local captures | `approved` |
| `gain` | `boss_bd_2` | Boss | Blues Driver BD-2 | overdrive | NAM | iconic circuit + dedicated local captures | `approved` |
| `gain` | `proco_rat` | ProCo | RAT | distortion | NAM | iconic circuit + dedicated local captures | `approved` |
| `gain` | `klon` | Klon / Klone | Centaur-style Klone | boost/od | NAM | iconic product family + clear local capture name | `approved` |
| `gain` | `marshall_bluesbreaker` | Marshall | Bluesbreaker pedal | low-gain overdrive | NAM | iconic product family + clear local capture name | `approved` |

#### `gain:ibanez_ts9`

- Product:
  - manufacturer: `Ibanez`
  - product: `TS9 Tube Screamer`
  - class: `overdrive`
- Local validation files:
  - directory: `/Users/joao.faria/Library/Mobile Documents/com~apple~CloudDocs/Musica/Capturas/TONE3000/pedals/Ibanez TS9 Tube Screamer`
  - representative files:
    - `/Users/joao.faria/Library/Mobile Documents/com~apple~CloudDocs/Musica/Capturas/TONE3000/pedals/Ibanez TS9 Tube Screamer/Ibanez TS9 Tube Screamer Drive 0 Tone 7 Level 7.nam`
    - `/Users/joao.faria/Library/Mobile Documents/com~apple~CloudDocs/Musica/Capturas/TONE3000/pedals/Ibanez TS9 Tube Screamer/Ibanez TS9 Tube Screamer Drive 8 Tone 8 Level 8.nam`
- Native direction:
  - strong first native pedal target

#### `gain:boss_bd_2`

- Product:
  - manufacturer: `Boss`
  - product: `Blues Driver BD-2`
  - class: `overdrive`
- Local validation files:
  - directory: `/Users/joao.faria/Library/Mobile Documents/com~apple~CloudDocs/Musica/Capturas/TONE3000/pedals/Boss Blues Driver BD-2`
  - representative files:
    - `/Users/joao.faria/Library/Mobile Documents/com~apple~CloudDocs/Musica/Capturas/TONE3000/pedals/Boss Blues Driver BD-2/Boss Blues Driver BD-2 Gain 25percent.nam`
    - `/Users/joao.faria/Library/Mobile Documents/com~apple~CloudDocs/Musica/Capturas/TONE3000/pedals/Boss Blues Driver BD-2/Boss Blues Driver BD-2 Gain 75percent.nam`
- Native direction:
  - second native pedal target

#### `gain:proco_rat`

- Product:
  - manufacturer: `ProCo`
  - product: `RAT`
  - class: `distortion`
- Local validation files:
  - `/Users/joao.faria/Library/Mobile Documents/com~apple~CloudDocs/Musica/Capturas/TONE3000/pedals/ProCo Rat/ProCo RAT.nam`
- Native direction:
  - strong distortion target

#### `gain:klon`

- Product:
  - manufacturer: `Klon / Klone`
  - product: `Centaur-style Klone`
  - class: `boost/overdrive`
- Local validation files:
  - `/Users/joao.faria/Library/Mobile Documents/com~apple~CloudDocs/Musica/Capturas/TONE3000/pedals/Boost Pedal Library/Klone.nam`
- Native direction:
  - clean/transparent drive target

#### `gain:marshall_bluesbreaker`

- Product:
  - manufacturer: `Marshall`
  - product: `Bluesbreaker pedal`
  - class: `low-gain overdrive`
- Local validation files:
  - `/Users/joao.faria/Library/Mobile Documents/com~apple~CloudDocs/Musica/Capturas/TONE3000/pedals/Boost Pedal Library/Bluesbreaker.nam`
- Native direction:
  - low-gain texture target

## Approved Later

These are safe enough to keep in the catalog, but they are not the first implementation wave.

### Amp / Preamp

| block_type | model_id | manufacturer | product_name | class | why_later |
| --- | --- | --- | --- | --- | --- |
| `amp_head` | `peavey_5150` | Peavey | 5150 | amp head | high-value target, but harder to tune than the first wave |
| `amp_head` | `dual_rectifier` | Mesa/Boogie | Dual Rectifier | amp head | strong candidate, but more complex to get right quickly |
| `amp_head` | `mark_v` | Mesa/Boogie | Mark V | amp head | strong candidate, but parameter and voicing complexity is higher |
| `amp_combo` | `jc_120` | Roland | Jazz Chorus JC-120 | combo | safe candidate, but not the highest priority before core gain/amp work lands |

### Pedal

| block_type | model_id | manufacturer | product_name | class | why_later |
| --- | --- | --- | --- | --- | --- |
| `gain` | `boss_sd_1` | Boss | SD-1 | overdrive | good candidate, but second wave after TS9/BD-2 |
| `gain` | `boss_ds_1` | Boss | DS-1 | distortion | good candidate, but second wave after RAT |
| `gain` | `tc_spark_clean` | TC Electronic | Spark Clean Boost | clean boost | useful, but less foundational than the first wave |
| `gain` | `tc_spark_mid` | TC Electronic | Spark Mid Boost | colored boost | useful, but less foundational than the first wave |
| `gain` | `boss_hm_2` | Boss | HM-2 | distortion | iconic, but voicing is extreme and should come later |

## Not Approved Yet

These are present in the local capture library, but they should not be treated as safe native targets yet.

### Reason: not enough confidence for first-pass high-fidelity native work

- `Bogner Ecstasy`
- `Bogner Shiva`
- `Diezel VH4`
- `Dumble`

### Reason: source exists, but format is not directly useful for current OpenRig runtime

- `.am3Data` under:
  - `/Users/joao.faria/Library/Mobile Documents/com~apple~CloudDocs/Musica/Capturas/AM3 MVAVE  CAPTURES`
  - `/Users/joao.faria/Library/Mobile Documents/com~apple~CloudDocs/Musica/Capturas/CAPTURASMVAVW2.0`
  - `/Users/joao.faria/Library/Mobile Documents/com~apple~CloudDocs/Musica/Capturas/TANKGPREMIUMCAPTURES`
- proprietary IR/profile/container formats:
  - `.kipr`
  - `.kpa`
  - `.irs`
  - `.ir`
  - `.sd2`
  - `.syx`
  - `.fxp`
  - `.gpk`

## Suggested Implementation Order

1. `cab:mesa_os_4x12_v30`
2. `cab:marshall_1960_v30`
3. `gain:ibanez_ts9`
4. `gain:boss_bd_2`
5. `gain:proco_rat`
6. `amp_head:jcm800_2203`
7. `amp_head:jmp_1`
8. `amp_combo:ac30`
9. `amp_combo:deluxe_reverb`
10. `cab:vox_ac30_blue`
11. `cab:deluxe_reverb_oxford`
12. `gain:klon`
13. `gain:marshall_bluesbreaker`

## Notes

- For `cab`, the native model should be fitted against the IR set. The product runtime should remain native DSP, not convolution.
- For `amp_head`, `amp_combo`, and `gain`, the NAM files are validation oracles, not shipping implementations.
- The first wave should prefer targets where the local oracle set is both small and clear. That reduces fitting noise and speeds up native iteration.
