# shellcheck shell=bash
# Responsibility: VST3 build recipes for the studio (mixing/mastering) plugin set (#1104).
#
# Sourced by scripts/build-lib-internal.sh after recipes/vst3.sh, whose helpers
# (do_cmake, collect_bundle, collect_vst3, _patch_juce_macos15) and env
# (DEPS_DIR, BUILD_WORK_DIR, CMAKE_EXTRA, JOBS, OUTPUT_DIR) are used here.
# Each recipe builds ONLY VST3 targets; the CI merge job unions each platform's
# Contents/<arch>/ subfolder into the shipped bundle.

# OpenRig routes no MIDI: switch JUCE MIDI I/O off so the VST3 wrapper stops
# exporting 2048 phantom MIDI-CC parameters (128 CC x 16 ch) into the editor.
_juce_no_midi() {
    sed -i.bak -E 's/(NEEDS_MIDI_(IN|OUT)PUT)[[:space:]]+(TRUE|True|true|ON|On|on)/\1 FALSE/g' "$@"
}

# --- Socalabs slPlugins (BSD-3, JUCE/CMake) ---
# One top-level CMake project configures every Socalabs plugin; only the six
# studio ones are built. Their PRODUCT_NAMEs are generic ("Compressor", "Gate"),
# so they are prefixed with "Socalabs" before configuring: the bundle folder and
# the binary inside it keep the same stem (the Windows host requires that), and
# the names cannot collide with another package's bundle in the merge job.
SLPLUGINS_STUDIO=(Compressor Limiter Gate Expander GraphicEQ StereoProcessor)

build_slplugins() {
    local src="$DEPS_DIR/slPlugins" p
    for p in "${SLPLUGINS_STUDIO[@]}"; do
        _juce_no_midi "$src/plugins/$p/CMakeLists.txt"
        sed -i.bak -E "s/PRODUCT_NAME \"$p\"/PRODUCT_NAME \"Socalabs$p\"/" "$src/plugins/$p/CMakeLists.txt"
    done
    CMAKE_EXTRA="${CMAKE_EXTRA:-} -DCMAKE_POLICY_VERSION_MINIMUM=3.5" \
        do_cmake "$src" "${SLPLUGINS_STUDIO[0]}_VST3"
    for p in "${SLPLUGINS_STUDIO[@]:1}"; do
        cmake --build "$LAST_BUILD_DIR" --config Release --target "${p}_VST3" -j "$JOBS"
    done
    for p in "${SLPLUGINS_STUDIO[@]}"; do
        collect_bundle "$LAST_BUILD_DIR/plugins/$p" "Socalabs$p.vst3"
    done
}

# --- Single-plugin JUCE/CMake projects ---

build_valentine() {
    # Valentine (Tote Bag Labs, GPL-3): compressor/saturator. JUCE vendored in libs/.
    local src="$DEPS_DIR/valentine"
    CMAKE_EXTRA="${CMAKE_EXTRA:-} -DCMAKE_POLICY_VERSION_MINIMUM=3.5" \
        do_cmake "$src" Valentine_VST3
    collect_bundle "$LAST_BUILD_DIR" "Valentine.vst3"
}

build_ninestrip() {
    # NineStrip (blablack, AGPL-3): Airwindows-based channel strip. JUCE and
    # clap-juce-extensions are submodules under lib/.
    local src="$DEPS_DIR/nine-strip"
    _juce_no_midi "$src/src/CMakeLists.txt"
    CMAKE_EXTRA="${CMAKE_EXTRA:-} -DCMAKE_POLICY_VERSION_MINIMUM=3.5" \
        do_cmake "$src" NineStrip_VST3
    collect_bundle "$LAST_BUILD_DIR" "NineStrip.vst3"
}

build_kissofshame() {
    # The Kiss of Shame (hollance fork, GPL-3): tape deck emulation. JUCE 7.0.11
    # is fetched at configure time (FetchContent). The bundle name has spaces.
    local src="$DEPS_DIR/TheKissOfShame"
    CMAKE_EXTRA="${CMAKE_EXTRA:-} -DCMAKE_POLICY_VERSION_MINIMUM=3.5" \
        do_cmake "$src" TheKissOfShame_VST3
    collect_bundle "$LAST_BUILD_DIR" "The Kiss Of Shame.vst3"
}

build_airwindows() {
    # Airwindows Consolidated (baconpaul/airwin2rack, MIT): every Airwindows
    # effect behind one selector. JUCE + clap-juce-extensions come via CPM at
    # configure time. The bundle name has a space.
    local src="$DEPS_DIR/airwin2rack"
    CMAKE_EXTRA="${CMAKE_EXTRA:-} -DBUILD_JUCE_PLUGIN=ON -DCMAKE_POLICY_VERSION_MINIMUM=3.5" \
        do_cmake "$src" airwin-consolidated_VST3
    collect_bundle "$LAST_BUILD_DIR" "Airwindows Consolidated.vst3"
}
