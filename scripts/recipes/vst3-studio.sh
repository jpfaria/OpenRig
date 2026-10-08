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

# --- mimomusic-plugins (GPL-3, DPF/CMake) ---
# Five sibling CMake projects, each vendoring DPF as a submodule. Only the
# stereo studio plugins are built (mono twins, scopes and meters skipped); DPF
# emits <target>.vst3 under each project's bin/ with a matching binary stem.
MIMO_STUDIO=(
    "compressor:compressor_stereo limiter contour"
    "statespace:parametric_eq multiband_compressor dynamic_eq dynamic_eq2"
    "reverb:reverb_dense reverb_plate"
)

build_mimomusic() {
    local entry proj plugins p
    for entry in "${MIMO_STUDIO[@]}"; do
        proj="${entry%%:*}"
        plugins="${entry#*:}"
        CMAKE_EXTRA="${CMAKE_EXTRA:-} -DCMAKE_POLICY_VERSION_MINIMUM=3.5" \
            do_cmake "$DEPS_DIR/mimomusic-plugins/$proj"
        for p in $plugins; do
            collect_bundle "$LAST_BUILD_DIR" "$p.vst3"
        done
    done
}

# --- LSP Plugins (LGPL-3, GNU Make) ---
# One lsp-plugins.vst3 bundle exposes every LSP plugin as a factory class (with
# mono/stereo/LR/MS variants); LSP writes Contents/Resources/moduleinfo.json, so
# the catalog lists each class without loading the binary. plugins.mk is cut to
# the studio modules, and only the `vst3` feature is built: no `ui` means no
# cairo/X11 runtime dependency on the user's machine (the editor is generic).
# `make fetch` pulls the LSP module repos pinned by dependencies.mk. macOS needs
# GNU make >= 4.4 (`gmake`) and has no universal mode, so each arch is built in
# its own copy and the two binaries are lipo-merged, then ad-hoc re-signed.
LSP_STUDIO_MODULES="CLIPPER COMPRESSOR CROSSOVER EXPANDER GATE GRAPH_EQUALIZER IMPULSE_REVERB LIMITER MB_COMPRESSOR PARA_EQUALIZER"

_lsp_build() { # $1=source copy, $2=install root, rest=extra make config args
    local src="$1" dest="$2" mk=make
    shift 2
    [ "$(uname -s)" = "Darwin" ] && mk=gmake
    local deps="" m
    for m in $LSP_STUDIO_MODULES; do deps="$deps  LSP_PLUGINS_$m \\\\\n"; done
    deps="${deps% \\\\\\n}"
    perl -0777 -i -pe "s/PLUGIN_DEPENDENCIES\s*=\s*\\\\\n(?:[ \t]+LSP_PLUGINS_\w+[ \t]*\\\\?\n)+/PLUGIN_DEPENDENCIES     = \\\\\n$deps\n/" "$src/plugins.mk"
    "$mk" -C "$src" config FEATURES='vst3' PREFIX=/usr "$@"
    "$mk" -C "$src" fetch
    "$mk" -C "$src" -j "$JOBS"
    "$mk" -C "$src" install DESTDIR="$dest"
}

build_lsp() {
    local src="$DEPS_DIR/lsp-plugins" work="$BUILD_WORK_DIR/lsp"
    rm -rf "$work" && mkdir -p "$work"
    if [ "$(uname -s)" = "Darwin" ]; then
        local arch
        for arch in arm64 x86_64; do
            cp -R "$src" "$work/src-$arch"
            CC="clang -arch $arch" CXX="clang++ -arch $arch" \
                _lsp_build "$work/src-$arch" "$work/install-$arch" \
                ARCHITECTURE="$arch" $([ "$arch" = x86_64 ] && echo ADD_FEATURES=crosscompile)
        done
        local arm x86
        arm=$(find "$work/install-arm64" -type d -name "lsp-plugins.vst3" | head -1)
        x86=$(find "$work/install-x86_64" -type d -name "lsp-plugins.vst3" | head -1)
        lipo -create "$arm/Contents/MacOS/lsp-plugins" "$x86/Contents/MacOS/lsp-plugins" \
            -output "$arm/Contents/MacOS/lsp-plugins.universal"
        mv "$arm/Contents/MacOS/lsp-plugins.universal" "$arm/Contents/MacOS/lsp-plugins"
        codesign --remove-signature "$arm" 2>/dev/null || true
        codesign --force --deep --sign - "$arm"
        collect_vst3 "$work/install-arm64" "lsp-plugins.vst3"
    else
        cp -R "$src" "$work/src"
        _lsp_build "$work/src" "$work/install"
        collect_vst3 "$work/install" "lsp-plugins.vst3"
    fi
}

# --- Rust / nih-plug bundles (`cargo xtask bundle`) ---
# nih-plug's xtask writes target/bundled/<Name>.vst3 with the binary stem equal
# to the bundle name; on macOS `bundle-universal` lipo-merges both arches. No
# shipped VST3 bundle carries a Windows slot yet, so these recipes build Linux
# and macOS only and skip the Windows runners (rustc there is MSVC, outside the
# MSYS2 toolchain the other recipes use).
_rust_skip_windows() {
    if [ -n "${MINGW_TARGET:-}" ] || [ "${CROSS_COMPILE:-}" = "aarch64-w64-mingw32" ]; then
        echo "  skip: Rust VST3 recipe is not built on Windows runners"
        return 0
    fi
    return 1
}

_xtask_bundle() { # $1=workspace dir, $2=toolchain ("" = workspace default), rest=packages
    local dir="$1" tc="$2"
    shift 2
    local cargo=(cargo)
    [ -n "$tc" ] && { rustup toolchain install "$tc" --profile minimal; cargo=(cargo "+$tc"); }
    if [ "$(uname -s)" = "Darwin" ]; then
        (cd "$dir" && rustup target add ${tc:+--toolchain "$tc"} x86_64-apple-darwin aarch64-apple-darwin)
        (cd "$dir" && MACOSX_DEPLOYMENT_TARGET=11.0 "${cargo[@]}" xtask bundle-universal "$@" --release)
    else
        (cd "$dir" && "${cargo[@]}" xtask bundle "$@" --release)
    fi
}

build_nihplug() {
    # nih-plug example plugins (GPL-3 as VST3): Soft Vacuum, Spectral Compressor,
    # Safety Limiter, Crossover. Crossover's default `simd` feature needs nightly,
    # so the whole set is built with nightly.
    _rust_skip_windows && return 0
    local src="$DEPS_DIR/nih-plug" n
    _xtask_bundle "$src" nightly soft_vacuum spectral_compressor safety_limiter crossover
    for n in "Soft Vacuum" "Spectral Compressor" "Safety Limiter" "Crossover"; do
        collect_bundle "$src/target/bundled" "$n.vst3"
    done
}

build_buschannelstrip() {
    # Bus Channel Strip (fsecada01, GPL-3 per Cargo.toml): API5500 EQ,
    # ButterComp2, Pultec, dynamic EQ, transformer, punch — built without its
    # `gui` feature (generic editor). Its rust-toolchain.toml pins nightly.
    _rust_skip_windows && return 0
    local src="$DEPS_DIR/bus_channel_strip"
    _xtask_bundle "$src" "" bus_channel_strip
    collect_bundle "$src/target/bundled" "Bus-Channel-Strip.vst3"
}
