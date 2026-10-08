# shellcheck shell=bash
# VST3 build recipes — one cross-platform `.vst3` bundle per plugin.
#
# Sourced by scripts/build-lib-internal.sh (which defines the helpers used here:
# do_cmake, collect_bundle, and the DEPS_DIR / BUILD_WORK_DIR / CMAKE_EXTRA env).
# Each recipe builds ONLY the VST3 target (AU / Standalone / LV2 / CLAP skipped)
# and collects the resulting `<Name>.vst3` directory tree; the CI merge job then
# unions each platform's Contents/<arch>/ subfolder into the shipped bundle.

# Neutralise CGWindowListCreateImage (removed in the macOS 15 SDK) in the unused
# window-snapshot helper of old JUCE (< 7.0.6), so those plugins compile locally.
_patch_juce_macos15() {
    find "$1" -path '*juce_gui_basics/native/*.mm' 2>/dev/null | while read -r j; do
        grep -q CGWindowListCreateImage "$j" || continue;
        perl -0777 -i -pe 's/CGImageRef screenShot = CGWindowListCreateImage \([^;]*\);/CGImageRef screenShot = nullptr;/s' "$j"
    done
}

build_chowcentaur() {
    # ChowCentaur (jatinchowdhury18/KlonCentaur): a JUCE/CMake Klon Centaur
    # overdrive (WDF + RNN). This is the repo's first VST3 recipe, so it emits a
    # cross-platform VST3 *bundle* (a `Contents/<arch>/` directory tree), not a
    # single shared lib — hence collect_bundle instead of collect_libs. Only the
    # VST3 target is built (AU / Standalone / LV2 are skipped). The macOS slot is
    # a universal binary via CMAKE_OSX_ARCHITECTURES (set by the workflow).
    local src="$DEPS_DIR/KlonCentaur"
    do_cmake "$src" ChowCentaur_VST3
    collect_bundle "$LAST_BUILD_DIR" "ChowCentaur.vst3"
}

build_chowtape() {
    # ChowTapeModel (jatinchowdhury18/AnalogTapeModel): JUCE/CMake tape-saturation
    # emulation. The CMake project lives under Plugin/. CLAP is disabled (it needs
    # CMake 3.21 + the clap-juce-extensions submodule); only the VST3 target is
    # built. macOS is universal via CMAKE_OSX_ARCHITECTURES (set by the workflow).
    # RTNeural (a submodule) pins cmake_minimum_required below the floor CMake 4
    # accepts, so raise the policy baseline (same fix as build_artyfx).
    local src="$DEPS_DIR/AnalogTapeModel/Plugin"
    CMAKE_EXTRA="${CMAKE_EXTRA:-} -DCHOWTAPE_BUILD_CLAP=OFF -DCMAKE_POLICY_VERSION_MINIMUM=3.5" \
        do_cmake "$src" CHOWTapeModel_VST3
    collect_bundle "$LAST_BUILD_DIR" "CHOWTapeModel.vst3"
}

build_chowphaser() {
    # ChowPhaser (jatinchowdhury18/ChowPhaser): JUCE/CMake phaser. Upstream emits a
    # Mono and a Stereo plugin; OpenRig chains are always stereo, so ship only the
    # Stereo variant. The bundle name carries a space — the merge job handles it.
    local src="$DEPS_DIR/ChowPhaser"
    do_cmake "$src" ChowPhaserStereo_VST3
    # JUCE names the artefact after the target (ChowPhaserStereo), not ProductName.
    collect_bundle "$LAST_BUILD_DIR" "ChowPhaserStereo.vst3"
}

build_chowmatrix() {
    # ChowMatrix (Chowdhury-DSP/ChowMatrix): JUCE/CMake growable multitap delay.
    # It pre-allocates maxNumNodes(50) x 13 = 650 host-automation "Blank" forwarding
    # params — a huge empty knob wall in the block editor. OpenRig drives nodes via
    # the native editor, not host automation, so cut the reserve to 2.
    local src="$DEPS_DIR/ChowMatrix"
    sed -i.bak -E 's/maxNumNodes = [0-9]+/maxNumNodes = 2/' "$src/src/dsp/Parameters/NodeParamControl.cpp"
    _patch_juce_macos15 "$src"
    do_cmake "$src" ChowMatrix_VST3
    collect_bundle "$LAST_BUILD_DIR" "ChowMatrix.vst3"
}

build_chowmultitool() {
    # ChowMultiTool (Chowdhury-DSP/ChowMultiTool): JUCE/CMake multi-effect (EQ,
    # waveshaper, signal gen…). CLAP disabled (CMake 3.21 + clap submodule); only
    # the VST3 target is built.
    local src="$DEPS_DIR/ChowMultiTool"
    CMAKE_EXTRA="${CMAKE_EXTRA:-} -DCHOWMULTITOOL_BUILD_CLAP=OFF" \
        do_cmake "$src" ChowMultiTool_VST3
    collect_bundle "$LAST_BUILD_DIR" "ChowMultiTool.vst3"
}

build_byod() {
    # BYOD (Chowdhury-DSP/BYOD): JUCE/CMake modular "build-your-own-drive". CLAP and
    # the preset server are disabled; only the VST3 target is built.
    local src="$DEPS_DIR/BYOD"
    # OpenRig routes no MIDI; disable MIDI I/O so the JUCE VST3 wrapper stops
    # exporting 2048 phantom MIDI-CC parameters (128 CC x 16 ch) that otherwise
    # flood the block editor and bury the real controls.
    sed -i.bak -E 's/(NEEDS_MIDI_(IN|OUT)PUT)[[:space:]]+(TRUE|True|true)/\1 FALSE/g' "$src/CMakeLists.txt"
    # Cut the 500 host-automation forwarding slots (a huge empty knob wall in the
    # block editor; OpenRig drives BYOD via its native editor) and patch old JUCE.
    sed -i.bak -E 's/ForwardingParametersManager<ParamForwardManager, [0-9]+>/ForwardingParametersManager<ParamForwardManager, 48>/g' "$src/src/state/ParamForwardManager.h" "$src/src/state/ParamForwardManager.cpp"
    _patch_juce_macos15 "$src"
    CMAKE_EXTRA="${CMAKE_EXTRA:-} -DBYOD_BUILD_CLAP=OFF -DBYOD_BUILD_PRESET_SERVER=OFF -DCMAKE_POLICY_VERSION_MINIMUM=3.5" \
        do_cmake "$src" BYOD_VST3
    collect_bundle "$LAST_BUILD_DIR" "BYOD.vst3"
}

# --- tiagolr family (GPL-3 / AGPL-3, JUCE/CMake) ---
# BUILD_VST3 is ON upstream; only the <Target>_VST3 target is built (AU /
# Standalone / LV2 skipped). The .vst3 bundle is named after JUCE PRODUCT_NAME
# with spaces stripped (REEV-R, GATE-12, TIME-12, FILT-R keep the hyphen;
# Sirial/QDelay equal their target). -DCMAKE_POLICY_VERSION_MINIMUM=3.5 guards
# against vendored deps pinning a pre-CMake-4 minimum on the macOS runner (same
# class as build_artyfx / chowtape).

build_reevr() {
    local src="$DEPS_DIR/reevr"
    # OpenRig routes no MIDI; disable MIDI I/O so the JUCE VST3 wrapper stops
    # exporting 2048 phantom MIDI-CC parameters (128 CC x 16 ch) that otherwise
    # flood the block editor and bury the real controls.
    sed -i.bak -E 's/(NEEDS_MIDI_(IN|OUT)PUT)[[:space:]]+(TRUE|True|true)/\1 FALSE/g' "$src/CMakeLists.txt"
    CMAKE_EXTRA="${CMAKE_EXTRA:-} -DCMAKE_POLICY_VERSION_MINIMUM=3.5" \
        do_cmake "$src" REEVR_VST3
    collect_bundle "$LAST_BUILD_DIR" "REEV-R.vst3"
}

build_sirial() {
    local src="$DEPS_DIR/sirial"
    CMAKE_EXTRA="${CMAKE_EXTRA:-} -DCMAKE_POLICY_VERSION_MINIMUM=3.5" \
        do_cmake "$src" Sirial_VST3
    collect_bundle "$LAST_BUILD_DIR" "Sirial.vst3"
}

build_qdelay() {
    local src="$DEPS_DIR/qdelay"
    CMAKE_EXTRA="${CMAKE_EXTRA:-} -DCMAKE_POLICY_VERSION_MINIMUM=3.5" \
        do_cmake "$src" QDelay_VST3
    collect_bundle "$LAST_BUILD_DIR" "QDelay.vst3"
}

build_gate12() {
    local src="$DEPS_DIR/gate12"
    # OpenRig routes no MIDI; disable MIDI I/O so the JUCE VST3 wrapper stops
    # exporting 2048 phantom MIDI-CC parameters (128 CC x 16 ch) that otherwise
    # flood the block editor and bury the real controls.
    sed -i.bak -E 's/(NEEDS_MIDI_(IN|OUT)PUT)[[:space:]]+(TRUE|True|true)/\1 FALSE/g' "$src/CMakeLists.txt"
    CMAKE_EXTRA="${CMAKE_EXTRA:-} -DCMAKE_POLICY_VERSION_MINIMUM=3.5" \
        do_cmake "$src" GATE12_VST3
    collect_bundle "$LAST_BUILD_DIR" "GATE-12.vst3"
}

build_time12() {
    local src="$DEPS_DIR/time12"
    # OpenRig routes no MIDI; disable MIDI I/O so the JUCE VST3 wrapper stops
    # exporting 2048 phantom MIDI-CC parameters (128 CC x 16 ch) that otherwise
    # flood the block editor and bury the real controls.
    sed -i.bak -E 's/(NEEDS_MIDI_(IN|OUT)PUT)[[:space:]]+(TRUE|True|true)/\1 FALSE/g' "$src/CMakeLists.txt"
    CMAKE_EXTRA="${CMAKE_EXTRA:-} -DCMAKE_POLICY_VERSION_MINIMUM=3.5" \
        do_cmake "$src" TIME12_VST3
    collect_bundle "$LAST_BUILD_DIR" "TIME-12.vst3"
}

build_filtr() {
    local src="$DEPS_DIR/filtr"
    # OpenRig routes no MIDI; disable MIDI I/O so the JUCE VST3 wrapper stops
    # exporting 2048 phantom MIDI-CC parameters (128 CC x 16 ch) that otherwise
    # flood the block editor and bury the real controls.
    sed -i.bak -E 's/(NEEDS_MIDI_(IN|OUT)PUT)[[:space:]]+(TRUE|True|true)/\1 FALSE/g' "$src/CMakeLists.txt"
    CMAKE_EXTRA="${CMAKE_EXTRA:-} -DCMAKE_POLICY_VERSION_MINIMUM=3.5" \
        do_cmake "$src" FILTR_VST3
    collect_bundle "$LAST_BUILD_DIR" "FILT-R.vst3"
}

# --- ZL-Audio family (AGPL-3 / GPL-3, JUCE/CMake) — DEFERRED ---
# These recipes are NOT wired into the catalogue yet (no manifests). Upstream
# CMake hard-requires ZL_HWY_STATIC_TARGET (Google Highway static SIMD dispatch:
# SSE2/SSE42/AVX2/NEON), which is incompatible with the single-pass universal
# macOS build (one -march cannot cover arm64+x86_64). Finishing them needs a
# per-arch build + lipo, or a dynamic-dispatch patch — tracked as a follow-up.
# Kept here (with submodules) so that work can resume. collect_vst3 normalises
# the PRODUCT_NAME artefact; -DCMAKE_POLICY_VERSION_MINIMUM=3.5 for CMake-4 macOS.

build_zl_equalizer() {
    local src="$DEPS_DIR/ZLEqualizer"
    CMAKE_EXTRA="${CMAKE_EXTRA:-} -DCMAKE_POLICY_VERSION_MINIMUM=3.5" \
        do_cmake "$src" ZLEqualizer_VST3
    collect_vst3 "$LAST_BUILD_DIR" "ZLEqualizer.vst3"
}

build_zl_compressor() {
    local src="$DEPS_DIR/ZLCompressor"
    CMAKE_EXTRA="${CMAKE_EXTRA:-} -DCMAKE_POLICY_VERSION_MINIMUM=3.5" \
        do_cmake "$src" ZLCompressor_VST3
    collect_vst3 "$LAST_BUILD_DIR" "ZLCompressor.vst3"
}

build_zl_splitter() {
    local src="$DEPS_DIR/ZLSplitter"
    CMAKE_EXTRA="${CMAKE_EXTRA:-} -DCMAKE_POLICY_VERSION_MINIMUM=3.5" \
        do_cmake "$src" ZLSplitter_VST3
    collect_vst3 "$LAST_BUILD_DIR" "ZLSplitter.vst3"
}

build_zl_spectrum_equalizer() {
    local src="$DEPS_DIR/ZLSpectrumEqualizer"
    CMAKE_EXTRA="${CMAKE_EXTRA:-} -DCMAKE_POLICY_VERSION_MINIMUM=3.5" \
        do_cmake "$src" ZLSpectrumEqualizer_VST3
    collect_vst3 "$LAST_BUILD_DIR" "ZLSpectrumEqualizer.vst3"
}

build_zl_warm() {
    local src="$DEPS_DIR/ZLWarm"
    CMAKE_EXTRA="${CMAKE_EXTRA:-} -DCMAKE_POLICY_VERSION_MINIMUM=3.5" \
        do_cmake "$src" ZLWarm_VST3
    collect_vst3 "$LAST_BUILD_DIR" "ZLWarm.vst3"
}

build_zl_inflator() {
    local src="$DEPS_DIR/ZLInflator"
    CMAKE_EXTRA="${CMAKE_EXTRA:-} -DCMAKE_POLICY_VERSION_MINIMUM=3.5" \
        do_cmake "$src" ZLInflator_VST3
    collect_vst3 "$LAST_BUILD_DIR" "ZLInflator.vst3"
}

# --- Individual JUCE/CMake effects (mixed licenses) ---
# Each builds only its <Target>_VST3; collect_vst3 normalises the artefact name.
# -DCMAKE_POLICY_VERSION_MINIMUM=3.5 for the CMake-4 macOS runner.

build_cloudreverb() {
    local src="$DEPS_DIR/CloudReverb"
    CMAKE_EXTRA="${CMAKE_EXTRA:-} -DCMAKE_POLICY_VERSION_MINIMUM=3.5" \
        do_cmake "$src" CloudReverb_VST3
    collect_vst3 "$LAST_BUILD_DIR" "CloudReverb.vst3"
}

build_roomreverb() {
    local src="$DEPS_DIR/RoomReverb"
    CMAKE_EXTRA="${CMAKE_EXTRA:-} -DCMAKE_POLICY_VERSION_MINIMUM=3.5" \
        do_cmake "$src" RoomReverb_VST3
    collect_vst3 "$LAST_BUILD_DIR" "RoomReverb.vst3"
}

build_frequalizer() {
    # Vendors an old JUCE that pulls <curl/curl.h> (JUCE_USE_CURL) — not present
    # on the linux runners; disable it (also drops the runtime libcurl dep).
    # POSITION_INDEPENDENT_CODE=ON: its BinaryData static lib is otherwise built
    # without -fPIC, which fails to link into the .so on linux.
    # COPY_FOLDER: its POST_BUILD `copy_directory ${ARTEFACTS_DIR} ${COPY_FOLDER}`
    # only sets COPY_FOLDER under APPLE/WIN32, so on linux it expands empty →
    # malformed `cmake -E copy_directory`. Seed it as a cache var to a scratch dir.
    local src="$DEPS_DIR/Frequalizer"
    CMAKE_EXTRA="${CMAKE_EXTRA:-} -DCMAKE_POLICY_VERSION_MINIMUM=3.5 -DCMAKE_CXX_FLAGS=-DJUCE_USE_CURL=0 -DCMAKE_POSITION_INDEPENDENT_CODE=ON -DCOPY_FOLDER=${TMPDIR:-/tmp}/frequalizer_copy" \
        do_cmake "$src" frequalizer_VST3
    collect_vst3 "$LAST_BUILD_DIR" "Frequalizer.vst3"
}

build_retuner() {
    # JUCE pulls <curl/curl.h> (JUCE_USE_CURL) — absent on the linux runners.
    local src="$DEPS_DIR/retuner"
    CMAKE_EXTRA="${CMAKE_EXTRA:-} -DCMAKE_POLICY_VERSION_MINIMUM=3.5 -DCMAKE_CXX_FLAGS=-DJUCE_USE_CURL=0" \
        do_cmake "$src" reTuner_VST3
    collect_vst3 "$LAST_BUILD_DIR" "reTuner.vst3"
}

build_setekh() {
    # Setekh (fullfxmedia): JUCE/CMake saturation; plugin CMake under plugin/.
    local src="$DEPS_DIR/setekh"
    CMAKE_EXTRA="${CMAKE_EXTRA:-} -DCMAKE_POLICY_VERSION_MINIMUM=3.5" \
        do_cmake "$src" Setekh_VST3
    collect_vst3 "$LAST_BUILD_DIR" "Setekh.vst3"
}

build_vitottx() {
    # vitOTTx (Sakhnovkrg): JUCE/CMake multiband upward/downward compressor (OSS
    # "OTT"). PLUGIN_NAME drives target/product; collect_vst3 normalises the name.
    local src="$DEPS_DIR/vitOTTx"
    CMAKE_EXTRA="${CMAKE_EXTRA:-} -DCMAKE_POLICY_VERSION_MINIMUM=3.5 -DPLUGIN_NAME=vitOTTx" \
        do_cmake "$src" vitOTTx_VST3
    collect_vst3 "$LAST_BUILD_DIR" "vitOTTx.vst3"
}

build_aidax() {
    # AIDA-X (AidaDSP): DPF/CMake neural amp modeler + cab (RTNeural). Build only
    # the DPF vst3 target; the artefact lands in <build>/bin/AIDA-X.vst3.
    local src="$DEPS_DIR/AIDA-X"
    CMAKE_EXTRA="${CMAKE_EXTRA:-} -DCMAKE_POLICY_VERSION_MINIMUM=3.5" \
        do_cmake "$src" AIDA-X-vst3
    collect_vst3 "$LAST_BUILD_DIR" "AIDA-X.vst3"
}

build_dfzitarev1() {
    # dfzitarev1 (SpotlightKid): DPF/Make Zita-Rev1 FDN reverb. Build via the DPF
    # Makefile (emits bin/dfzitarev1.vst3); universal on macOS.
    local src="$DEPS_DIR/dfzitarev1"
    if [ "$(uname -s)" = "Darwin" ]; then
        # This DPF fork ignores MACOS_UNIVERSAL; inject both arches directly.
        export CFLAGS="-arch arm64 -arch x86_64 -mmacosx-version-min=11.0"
        export CXXFLAGS="$CFLAGS"
        export LDFLAGS="-arch arm64 -arch x86_64"
    fi
    do_make "$src"
    unset CFLAGS CXXFLAGS LDFLAGS
    collect_vst3 "$src/bin" "dfzitarev1.vst3"
}

build_master_me() {
    # master_me (trummerschlunk): DPF/Make automatic mastering chain. Universal on
    # macOS via explicit -arch (DPF Makefile); emits bin/master_me.vst3.
    local src="$DEPS_DIR/master_me"
    if [ "$(uname -s)" = "Darwin" ]; then
        export CFLAGS="-arch arm64 -arch x86_64 -mmacosx-version-min=11.0"
        export CXXFLAGS="$CFLAGS"
        export LDFLAGS="-arch arm64 -arch x86_64"
    fi
    do_make "$src"
    unset CFLAGS CXXFLAGS LDFLAGS
    collect_vst3 "$src/bin" "master_me.vst3"
}

# --- igorski family (MIT / GPL-3, raw Steinberg VST3 SDK) ---
# These link the Steinberg VST3 SDK static libs. _build_vst3sdk builds them once
# into deps/vst3sdk/build (patching vstgui's -Werror, which newer clang trips).
VST3SDK_DIR="${VST3SDK_DIR:-$DEPS_DIR/vst3sdk}"

_build_vst3sdk() {
    [ -f "$VST3SDK_DIR/build/lib/Release/libvstgui_uidescription.a" ] && return 0
    sed -i.bak 's/-Wall -Werror/-Wall/' "$VST3SDK_DIR/vstgui4/vstgui/lib/CMakeLists.txt" 2>/dev/null || true
    local osx=""
    [ "$(uname -s)" = "Darwin" ] && osx="-DCMAKE_OSX_ARCHITECTURES=arm64;x86_64"
    # shellcheck disable=SC2086
    cmake -S "$VST3SDK_DIR" -B "$VST3SDK_DIR/build" -DCMAKE_BUILD_TYPE=Release \
        -DCMAKE_POLICY_VERSION_MINIMUM=3.5 -DSMTG_ENABLE_VSTGUI_SUPPORT=ON \
        -DSMTG_ENABLE_VST3_PLUGIN_EXAMPLES=OFF -DSMTG_ENABLE_VST3_HOSTING_EXAMPLES=OFF \
        -DSMTG_CREATE_PLUGIN_LINK=OFF -DCMAKE_CXX_FLAGS="-Wno-error" $osx
    cmake --build "$VST3SDK_DIR/build" --config Release -j "$JOBS" \
        --target base sdk pluginterfaces vstgui vstgui_support vstgui_uidescription
}

_build_igorski() { # $1=dep dir, $2=bundle name
    _build_vst3sdk
    local src="$DEPS_DIR/$1"
    local osx=""
    [ "$(uname -s)" = "Darwin" ] && osx="-DCMAKE_OSX_ARCHITECTURES=arm64;x86_64"
    CMAKE_EXTRA="${CMAKE_EXTRA:-} -DVST3_SDK_ROOT=$VST3SDK_DIR -DSMTG_CREATE_PLUGIN_LINK=OFF -DSMTG_RUN_VST_VALIDATOR=OFF -DCMAKE_CXX_FLAGS=-Wno-error -DCMAKE_POLICY_VERSION_MINIMUM=3.5 $osx" \
        do_cmake "$src"
    collect_vst3 "$LAST_BUILD_DIR" "$2"
}

build_fogpad()       { _build_igorski fogpad       "fogpad.vst3"; }
build_regrader()     { _build_igorski regrader     "regrader.vst3"; }
build_rechoir()      { _build_igorski rechoir      "rechoir.vst3"; }
build_transformant() { _build_igorski transformant "transformant.vst3"; }
build_darvaza()      { _build_igorski darvaza      "darvaza.vst3"; }
build_homecorrupter(){ _build_igorski homecorrupter "homecorrupter.vst3"; }
