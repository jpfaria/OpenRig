# Dependencies (Git Submodules)

Each dependency is a git submodule pinned to a specific commit.
Use `./scripts/add-dep.sh` to add new ones.

## Current

| Name | Repository | Pinned Hash | Build System | Plugins |
|------|-----------|-------------|--------------|---------|
| neural-amp-modeler-lv2 | https://github.com/mikeoliphant/neural-amp-modeler-lv2 | `5a5865a` | CMake | NeuralAudioCAPI (NAM inference) |
| dragonfly-reverb | https://github.com/michaelwillis/dragonfly-reverb | `b3c15af` | DPF/Make | Hall, Plate, Room, EarlyReflections reverbs |
| zam-plugins | https://github.com/zamaudio/zam-plugins | `6a7fd03` | DPF/Make | ZamComp, ZamDelay, ZamEQ2, ZamTube, ZamGate |
| mod-utilities | https://github.com/mod-audio/mod-utilities | `b8a9d45` | Make | MOD gain, mixers, CV, switchboxes |
| caps-lv2 | https://github.com/mod-audio/caps-lv2 | `5d52a0c` | Make | AmpVTS, CabinetIV, Plate, Chorus, Phaser, Compress |
| tap-lv2 | https://github.com/moddevices/tap-lv2 | `cab6e0d` | Make | Echo, Reverb, Tremolo, EQ, Chorus, TubeWarmth |
| SHIRO-Plugins | https://github.com/ninodewit/SHIRO-Plugins | `3e0a1d3` | DPF/Make | Shiroverb, Modulay, Harmless, Larynx |
| DPF-Plugins | https://github.com/DISTRHO/DPF-Plugins | `df5cb65` | DPF/Make | Kars, Nekobi, PingPongPan |
| MVerb | https://github.com/DISTRHO/MVerb | `5ae9f57` | DPF/Make | MVerb reverb |
| mda-lv2 | https://gitlab.com/drobilla/mda-lv2 | `8218120` | Meson | DubDelay, Leslie, Overdrive, EPiano, Piano |
| fomp | https://gitlab.com/drobilla/fomp | `9ed4d2e` | Meson | VCO, VCF, Phaser, Flanger |
| invada-studio | https://github.com/BlokasLabs/invada-studio | `9525be9` | Make | Compressor, Delay, Reverb, Filter, Tube |
| wolf-shaper | https://github.com/wolf-plugins/wolf-shaper | `d38cc33` | DPF/Make | Waveshaper |
| openAV-ArtyFX | https://github.com/openAVproductions/openAV-ArtyFX | `284eab7` | CMake | Bitta, Filta, Kuiza, Satma |
| sooperlooper | https://github.com/essej/sooperlooper | `c5e22ce` | Autotools | Looper |
| setBfree | https://github.com/pantherb/setBfree | `25274ac` | Make | Hammond organ, whirl speaker |
| GxPlugins.lv2 | https://github.com/brummer10/GxPlugins.lv2 | `3a32527` | Make | Guitarix amp sims, effects |
| AnalogTapeModel | https://github.com/jatinchowdhury18/AnalogTapeModel | `604372e` | CMake/JUCE | ChowCentaur, CHOWTapeModel |
| Schrammel_OJD | https://github.com/JanosGit/Schrammel_OJD | `03c0e84` | CMake/JUCE | OJD overdrive |

## Visual Asset Dependencies

| Name | Repository | Pinned Hash | License | Content |
|------|-----------|-------------|---------|---------|
| mod-resources | https://github.com/moddevices/mod-resources | `7249dcb` | GPL-3.0 | Pedal templates, knobs, switches, backgrounds |
| libxputty | https://github.com/brummer10/libxputty | `2dcd730` | 0BSD | Widget framework with knobs/pedals (reference) |
| svg-pedals-boss | https://github.com/SVG-Effects-Pedals/Boss-SVG-Tribute-Pack | `5c7fa5a` | CC BY-NC-SA | Boss pedal SVGs (DS-1, SD-1, BD-2, DD-7, etc.) |
| svg-pedals-ehx | https://github.com/SVG-Effects-Pedals/EHX-SVG-Tribute-Pack | `5883f6b` | CC BY-NC-SA | EHX pedal SVGs (Memory Man, POG2, etc.) |
| svg-pedals-ibanez | https://github.com/SVG-Effects-Pedals/Maxon-Ibanez-SVG-Tribute-Pack | `1c293b6` | CC BY-NC-SA | Ibanez/Maxon SVGs (TS-9, CS-9, FL-9, AD-9) |
| svg-pedals-moogerfooger | https://github.com/SVG-Effects-Pedals/Moogerfooger-SVG-Tribute-Pack | `061310b` | CC BY-NC-SA | Moogerfooger SVGs (MF-101 to MF-108) |

## Updating a dependency

Submodules are pinned to the exact commit above. To update:

```bash
cd deps/<name>
git fetch origin
git checkout <new-commit-hash>
cd ../..
git add deps/<name>
git commit -m "Update <name> to <hash>"
```

Then update the hash in this table.

## Building

```bash
# Build one plugin (macOS native):
./scripts/build-lib.sh nam

# Build for Linux via Docker:
./scripts/build-lib.sh nam --platform linux-x86_64

# Build multiple:
./scripts/build-lib.sh dragonfly-reverb zam-plugins

# Build all:
./scripts/build-lib.sh all

# List available plugins:
./scripts/build-lib.sh --list
```

Output goes to:
- `libs/nam/{platform}/` — NAM libraries
- `libs/lv2/{platform}/` — LV2 plugin libraries
