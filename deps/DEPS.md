# Dependencies (Git Submodules)

Each dependency is a git submodule pinned to a specific commit.
Use `./scripts/add-dep.sh` to add new ones.

## Current

| Name | Repository | Pinned Hash | Build System | Plugins |
|------|-----------|-------------|--------------|---------|
| NeuralAmpModelerCore | https://github.com/sdatkinson/NeuralAmpModelerCore | vendored archive — see `NeuralAmpModelerCore.lock` | CMake (via `cpp/`) | nam_wrapper (NAM inference + tone stack) |
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
| AnalogTapeModel | https://github.com/jatinchowdhury18/AnalogTapeModel | `604372e` | CMake/JUCE | ChowTapeModel (tape saturation) — VST3 bundle, recipe `chowtape` |
| KlonCentaur | https://github.com/jatinchowdhury18/KlonCentaur | `f3bb633` | CMake/JUCE | ChowCentaur (Klon overdrive) — VST3 bundle, recipe `chowcentaur` |
| ChowPhaser | https://github.com/jatinchowdhury18/ChowPhaser | `31eac4d` | CMake/JUCE | ChowPhaser (WDF phaser, stereo) — VST3 bundle, recipe `chowphaser` |
| ChowMatrix | https://github.com/Chowdhury-DSP/ChowMatrix | `40d8e0e` | CMake/JUCE | ChowMatrix (multitap delay) — VST3 bundle, recipe `chowmatrix` |
| ChowMultiTool | https://github.com/Chowdhury-DSP/ChowMultiTool | `0b65e3a` | CMake/JUCE | ChowMultiTool (multi-effect) — VST3 bundle, recipe `chowmultitool` |
| BYOD | https://github.com/Chowdhury-DSP/BYOD | `1cf22b6` | CMake/JUCE | BYOD (modular distortion) — VST3 bundle, recipe `byod` |
| reevr | https://github.com/tiagolr/reevr | `e4d553a` | CMake/JUCE | REEV-R (convolution reverb) — VST3 bundle, recipe `reevr` |
| sirial | https://github.com/tiagolr/sirial | `ec31132` | CMake/JUCE | Sirial (rhythmic delay) — VST3 bundle, recipe `sirial` |
| qdelay | https://github.com/tiagolr/qdelay | `13ef451` | CMake/JUCE | QDelay (dual delay) — VST3 bundle, recipe `qdelay` |
| gate12 | https://github.com/tiagolr/gate12 | `df65245` | CMake/JUCE | GATE-12 (trance gate) — VST3 bundle, recipe `gate12` |
| time12 | https://github.com/tiagolr/time12 | `cb86fd6` | CMake/JUCE | TIME-12 (stutter/tape-stop) — VST3 bundle, recipe `time12` |
| filtr | https://github.com/tiagolr/filtr | `b42c4e0` | CMake/JUCE | FILT-R (envelope filter, AGPL-3) — VST3 bundle, recipe `filtr` |
| ZLEqualizer | https://github.com/ZL-Audio/ZLEqualizer | `903c0c9` | CMake/JUCE | ZLEqualizer (dynamic EQ, AGPL-3) — VST3 bundle, recipe `zl_equalizer` |
| ZLCompressor | https://github.com/ZL-Audio/ZLCompressor | `b2fe331` | CMake/JUCE | ZLCompressor (compressor, AGPL-3) — VST3 bundle, recipe `zl_compressor` |
| ZLSplitter | https://github.com/ZL-Audio/ZLSplitter | `dfaccc6` | CMake/JUCE | ZLSplitter (signal splitter, AGPL-3) — VST3 bundle, recipe `zl_splitter` |
| ZLSpectrumEqualizer | https://github.com/ZL-Audio/ZLSpectrumEqualizer | `21cc97d` | CMake/JUCE | ZLSpectrumEqualizer (spectrum EQ, AGPL-3) — VST3 bundle, recipe `zl_spectrum_equalizer` |
| ZLWarm | https://github.com/ZL-Audio/ZLWarm | `48093f3` | CMake/JUCE | ZLWarm (saturation, GPL-3) — VST3 bundle, recipe `zl_warm` |
| ZLInflator | https://github.com/ZL-Audio/ZLInflator | `b71bf48` | CMake/JUCE | ZLInflator (loudness, GPL-3) — VST3 bundle, recipe `zl_inflator` |
| CloudReverb | https://github.com/xunil-cloud/CloudReverb | `92804ed` | CMake/JUCE | CloudReverb (shimmer reverb) — VST3 bundle, recipe `cloudreverb` |
| RoomReverb | https://github.com/cvde/RoomReverb | `11f2de0` | CMake/JUCE | RoomReverb (algorithmic reverb) — VST3 bundle, recipe `roomreverb` |
| Frequalizer | https://github.com/ffAudio/Frequalizer | `c4b1b61` | CMake/JUCE | Frequalizer (parametric EQ, BSD-3) — VST3 bundle, recipe `frequalizer` |
| retuner | https://github.com/kushview/retuner | `4a8fb06` | CMake/JUCE | reTuner (pitch shift) — VST3 bundle, recipe `retuner` |
| setekh | https://github.com/fullfxmedia/setekh | `468a9bd` | CMake/JUCE | Setekh (saturation) — VST3 bundle, recipe `setekh` |
| vitOTTx | https://github.com/Sakhnovkrg/vitOTTx | `738ba9d` | CMake/JUCE | vitOTTx (multiband OTT) — VST3 bundle, recipe `vitottx` |
| AIDA-X | https://github.com/AidaDSP/AIDA-X | `41eb988` | CMake/DPF | AIDA-X (neural amp+cab) — VST3 bundle, recipe `aidax` |
| dfzitarev1 | https://github.com/SpotlightKid/dfzitarev1 | `5da5440` | DPF/Make | dfZitaRev1 (Zita reverb, MIT) — VST3 bundle, recipe `dfzitarev1` |
| master_me | https://github.com/trummerschlunk/master_me | `ea93d78` | DPF/Make | master_me (auto-mastering) — VST3 bundle, recipe `master_me` |
| vst3sdk | https://github.com/steinbergmedia/vst3sdk | `7d92338` | CMake | Steinberg VST3 SDK v3.7.11 (MIT) — static libs for the igorski/mda recipes |
| fogpad | https://github.com/igorski/fogpad | `2e86191` | CMake/VST3-SDK | fogpad (reverb, MIT) — VST3 bundle, recipe `fogpad` |
| regrader | https://github.com/igorski/regrader | `3fa916b` | CMake/VST3-SDK | regrader (delay, MIT) — VST3 bundle, recipe `regrader` |
| rechoir | https://github.com/igorski/rechoir | `583d9a6` | CMake/VST3-SDK | rechoir (delay, MIT) — VST3 bundle, recipe `rechoir` |
| transformant | https://github.com/igorski/transformant | `9415de2` | CMake/VST3-SDK | transformant (filter, MIT) — VST3 bundle, recipe `transformant` |
| darvaza | https://github.com/igorski/darvaza | `b935871` | CMake/VST3-SDK | darvaza (gate, MIT) — VST3 bundle, recipe `darvaza` |
| homecorrupter | https://github.com/igorski/homecorrupter | `7b594e6` | CMake/VST3-SDK | homecorrupter (lo-fi, MIT) — VST3 bundle, recipe `homecorrupter` |
| Schrammel_OJD | https://github.com/JanosGit/Schrammel_OJD | `03c0e84` | CMake/JUCE | OJD overdrive |
| slPlugins | https://github.com/FigBug/slPlugins | `43319db` | CMake/JUCE | Socalabs Compressor, Limiter, Gate, Expander, GraphicEQ (BSD-3) — VST3 bundles, recipe `slplugins` |
| valentine | https://github.com/tote-bag-labs/valentine | `87eba9b` | CMake/JUCE | Valentine compressor/saturator (GPL-3) — VST3 bundle, recipe `valentine` |
| nine-strip | https://github.com/blablack/nine-strip | `111dee6` | CMake/JUCE | NineStrip channel strip (AGPL-3) — VST3 bundle, recipe `ninestrip` |
| TheKissOfShame | https://github.com/hollance/TheKissOfShame | `2d43f68` | CMake/JUCE | The Kiss of Shame tape emulation (GPL-3) — VST3 bundle, recipe `kissofshame` |
| airwin2rack | https://github.com/baconpaul/airwin2rack | `9b87116` | CMake/JUCE | Airwindows Consolidated (MIT) — VST3 bundle, recipe `airwindows` |
| lsp-plugins | https://github.com/lsp-plugins/lsp-plugins | `2df6eea` (1.2.35) | GNU Make | LSP studio subset: EQ, compressor, multiband compressor, limiter, gate, expander, clipper, crossover, impulse reverb (LGPL-3) — one VST3 bundle, Linux only, recipe `lsp` |
| mimomusic-plugins | https://github.com/mimo-music/mimomusic-plugins | `3c202e8` | DPF/CMake | Compressor, Limiter, Contour, Parametric/Dynamic EQ, Multiband Compressor, Reverb Dense/Plate (GPL-3) — VST3 bundles, recipe `mimomusic` |
| nih-plug | https://github.com/robbert-vdh/nih-plug | `de42101` | Rust (cargo xtask) | Soft Vacuum, Spectral Compressor, Safety Limiter (GPL-3 as VST3) — VST3 bundles, recipe `nihplug` |
| bus_channel_strip | https://github.com/fsecada01/bus_channel_strip | `d663fe4` | Rust (cargo xtask) | Bus Channel Strip (GPL-3) — VST3 bundle, recipe `buschannelstrip` |

Every row above except NeuralAmpModelerCore (vendored, see below) is a real git submodule (a committed gitlink under `deps/`). Each `build_*` recipe — grouped by backend in `scripts/recipes/lv2.sh`, `scripts/recipes/vst3.sh` and `scripts/recipes/vst3-studio.sh`, sourced by `scripts/build-lib-internal.sh` — expects its upstream checked out under `deps/<name>/` (CI checks them out with `submodules: recursive`). Register a new one with `./scripts/add-dep.sh <name> <url> <commit>` when activating the recipe.

## Visual Asset Dependencies

| Name | Repository | Pinned Hash | License | Content |
|------|-----------|-------------|---------|---------|
| mod-resources | https://github.com/moddevices/mod-resources | `7249dcb` | GPL-3.0 | Pedal templates, knobs, switches, backgrounds |
| libxputty | https://github.com/brummer10/libxputty | `2dcd730` | 0BSD | Widget framework with knobs/pedals (reference) |
| svg-pedals-boss | https://github.com/SVG-Effects-Pedals/Boss-SVG-Tribute-Pack | `5c7fa5a` | CC BY-NC-SA | Boss pedal SVGs (DS-1, SD-1, BD-2, DD-7, etc.) |
| svg-pedals-ehx | https://github.com/SVG-Effects-Pedals/EHX-SVG-Tribute-Pack | `5883f6b` | CC BY-NC-SA | EHX pedal SVGs (Memory Man, POG2, etc.) |
| svg-pedals-ibanez | https://github.com/SVG-Effects-Pedals/Maxon-Ibanez-SVG-Tribute-Pack | `1c293b6` | CC BY-NC-SA | Ibanez/Maxon SVGs (TS-9, CS-9, FL-9, AD-9) |
| svg-pedals-moogerfooger | https://github.com/SVG-Effects-Pedals/Moogerfooger-SVG-Tribute-Pack | `061310b` | CC BY-NC-SA | Moogerfooger SVGs (MF-101 to MF-108) |

## NeuralAmpModelerCore (vendored, #974)

NeuralAmpModelerCore is not a submodule. Its own submodules (Eigen on GitLab,
AudioDSPTools) made every CI checkout depend on GitLab being up, so the whole
tree — NAM, AudioDSPTools, both Eigen copies, nlohmann — ships as one archive:

- `NeuralAmpModelerCore.tar.gz` — the tree, reproducibly packed, in Git LFS.
- `NeuralAmpModelerCore.lock` — the upstream tag, its commit, every submodule
  commit and the archive's sha256.
- `NeuralAmpModelerCore/` — extracted by `crates/nam/build.rs`; never versioned.
  The build re-extracts it when it is missing or came from another lock, and
  never needs the network.

Only CI moves it to a newer upstream release, and only on `develop`:

- **CI:** after develop's Test Suite passes, the `nam-refresh` job runs
  `scripts/nam_vendor.py update`. When upstream's newest `vX.Y.Z` tag is newer
  than the lock, it clones the tag with every submodule, repacks the archive,
  rewrites the lock and commits both. The job then runs the whole suite on it
  and pushes (`nam_vendor.py push`) only if it passes — a release that breaks
  the build or the tests never lands, and the job goes red naming it. Release
  branches take the new version when they are cut from develop.
- **Local build:** the first `cargo build` after a `git fetch` / `git pull`
  runs `nam_vendor.py check` (once per fetch) and, if upstream has a newer
  release, shows it as a cargo warning. It never writes or commits anything.
  Other builds never touch the network: rerunning the `nam` build script
  recompiles every crate above it (~3.5 min), so it is tied to the fetch. It
  is skipped in CI, with `CARGO_NET_OFFLINE`, and in cargo dependency
  checkouts.

An unreachable upstream, a timeout (`NAM_VENDOR_TIMEOUT`, 20 s per network step
in a local build, 300 s otherwise), a failed commit (both files are put back) or
a refused push is a warning and exit 0 — the vendored copy keeps building.

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

Output goes to `output/` (or `$OUTPUT_DIR`). The `Build Plugin Libraries`
workflow (`.github/workflows/build-libs.yml`) places each binary under
`plugins/source/lv2/<plugin>/platform/<slot>/` and merges each VST3 bundle into
`plugins/source/vst3/<plugin>/bundles/` — see `docs/development/plugin-binaries.md`.
