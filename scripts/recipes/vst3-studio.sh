# shellcheck shell=bash
# Responsibility: VST3 build recipes for the studio (mixing/mastering) plugin set (#1104).
#
# Sourced by scripts/build-lib-internal.sh after recipes/vst3.sh, whose helpers
# (do_cmake, collect_bundle, collect_vst3, _patch_juce_macos15) and env
# (DEPS_DIR, BUILD_WORK_DIR, CMAKE_EXTRA, JOBS, OUTPUT_DIR) are used here.
# Each recipe builds ONLY VST3 targets; the CI merge job unions each platform's
# Contents/<arch>/ subfolder into the shipped bundle.

# Windows coverage: the windows-aarch64 runner has no C++ cross compiler
# (aarch64-w64-mingw32-g++ is missing from CLANG64), so no recipe here builds
# there; JUCE 7/8 refuses MinGW ("MinGW is not supported") and the Rust
# recipes need MSVC, so those skip windows-x86_64 too.
_skip_windows() { # skip every Windows runner
    if [ -n "${MINGW_TARGET:-}" ] || [ "${CROSS_COMPILE:-}" = "aarch64-w64-mingw32" ]; then
        echo "  skip: recipe is not built on Windows runners"
        return 0
    fi
    return 1
}

_skip_windows_arm() { # skip only the windows-aarch64 cross runner
    if [ "${CROSS_COMPILE:-}" = "aarch64-w64-mingw32" ]; then
        echo "  skip: windows-aarch64 runner has no C++ cross compiler"
        return 0
    fi
    return 1
}

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
    _skip_windows && return 0
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
    _skip_windows && return 0
    local src="$DEPS_DIR/valentine"
    CMAKE_EXTRA="${CMAKE_EXTRA:-} -DCMAKE_POLICY_VERSION_MINIMUM=3.5" \
        do_cmake "$src" Valentine_VST3
    collect_bundle "$LAST_BUILD_DIR" "Valentine.vst3"
}

build_ninestrip() {
    # NineStrip (blablack, AGPL-3): Airwindows-based channel strip. JUCE and
    # clap-juce-extensions are submodules under lib/. Its NineStripUI library
    # compiles juce_core without the plugin's JUCE_USE_CURL=0, so curl and the
    # web browser are switched off globally (no libcurl headers on the runner).
    _skip_windows && return 0
    local src="$DEPS_DIR/nine-strip"
    _juce_no_midi "$src/src/CMakeLists.txt"
    CXXFLAGS="${CXXFLAGS:-} -DJUCE_USE_CURL=0 -DJUCE_WEB_BROWSER=0" \
    CMAKE_EXTRA="${CMAKE_EXTRA:-} -DCMAKE_POLICY_VERSION_MINIMUM=3.5" \
        do_cmake "$src" NineStrip_VST3
    collect_bundle "$LAST_BUILD_DIR" "NineStrip.vst3"
}

build_kissofshame() {
    # The Kiss of Shame (hollance fork, GPL-3): tape deck emulation. JUCE 7.0.11
    # is fetched at configure time (FetchContent). The bundle name has spaces.
    _skip_windows_arm && return 0
    local src="$DEPS_DIR/TheKissOfShame"
    CMAKE_EXTRA="${CMAKE_EXTRA:-} -DCMAKE_POLICY_VERSION_MINIMUM=3.5" \
        do_cmake "$src" TheKissOfShame_VST3
    collect_bundle "$LAST_BUILD_DIR" "The Kiss Of Shame.vst3"
}

build_airwindows() {
    # Airwindows Consolidated (baconpaul/airwin2rack, MIT): every Airwindows
    # effect behind one selector. JUCE + clap-juce-extensions come via CPM at
    # configure time. The bundle name has a space.
    _skip_windows && return 0
    local src="$DEPS_DIR/airwin2rack"
    CMAKE_EXTRA="${CMAKE_EXTRA:-} -DBUILD_JUCE_PLUGIN=ON -DCMAKE_POLICY_VERSION_MINIMUM=3.5" \
        do_cmake "$src" airwin-consolidated_VST3
    collect_bundle "$LAST_BUILD_DIR" "Airwindows Consolidated.vst3"
}

# --- mimomusic-plugins (GPL-3, DPF/CMake) ---
# Five sibling CMake projects, each vendoring DPF as a submodule. Only the
# stereo studio plugins are built (mono twins, scopes and meters skipped); DPF
# emits <target>.vst3 under each project's bin/ with a matching binary stem.
# The compressor's curve (de)serialiser uses floating-point std::to_chars,
# which libc++ ships from macOS 13.3 (so these bundles target 13.3 there), and
# floating-point std::from_chars, which Apple's libc++ does not ship at all: on
# macOS it is swapped for a locale-independent strtod_l shim.
_mimo_from_chars_shim() { # rewrites every std::from_chars call; prints the -include flag
    local hdr="$BUILD_WORK_DIR/mimo_from_chars.hpp" f
    mkdir -p "$BUILD_WORK_DIR"
    cat > "$hdr" <<'HDR'
#pragma once
#if __cplusplus >= 201703L
#include <charconv>
#include <string>
#include <type_traits>
#include <xlocale.h>
template <typename T, typename... Rest>
static inline std::from_chars_result mimo_from_chars(const char* first, const char* last, T& value, Rest... rest)
{
    if constexpr (std::is_floating_point_v<T>)
    {
        static const locale_t cLocale = newlocale(LC_ALL_MASK, "C", nullptr);
        const std::string text(first, last);
        char* end = nullptr;
        const double parsed = strtod_l(text.c_str(), &end, cLocale);
        if (end == text.c_str()) return {first, std::errc::invalid_argument};
        value = static_cast<T>(parsed);
        return {first + (end - text.c_str()), std::errc()};
    }
    else
    {
        return std::from_chars(first, last, value, rest...);
    }
}
#endif
HDR
    grep -rl --include='*.hpp' --include='*.cpp' --include='*.h' 'std::from_chars(' "$1" | grep -v '/dpf/' |
        while read -r f; do sed -i.bak 's/std::from_chars(/mimo_from_chars(/g' "$f"; done
    echo "-include $hdr"
}
MIMO_STUDIO=(
    "compressor:compressor_stereo limiter contour"
    "statespace:parametric_eq multiband_compressor dynamic_eq dynamic_eq2"
    "reverb:reverb_dense reverb_plate"
)

build_mimomusic() {
    _skip_windows_arm && return 0
    local entry proj plugins p extra=" -DCMAKE_POLICY_VERSION_MINIMUM=3.5"
    if [ "$(uname -s)" = "Darwin" ]; then
        extra="$extra -DCMAKE_OSX_DEPLOYMENT_TARGET=13.3"
        export CXXFLAGS="${CXXFLAGS:-} $(_mimo_from_chars_shim "$DEPS_DIR/mimomusic-plugins")"
    fi
    for entry in "${MIMO_STUDIO[@]}"; do
        proj="${entry%%:*}"
        plugins="${entry#*:}"
        CMAKE_EXTRA="${CMAKE_EXTRA:-}$extra" do_cmake "$DEPS_DIR/mimomusic-plugins/$proj"
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
# GNU make >= 4.4 (`gmake`) and LSP's x86 AVX2 inline assembly does not
# assemble under Apple clang ("expected relocatable expression"), so the macOS
# bundle is arm64 only, ad-hoc re-signed.
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
    _skip_windows_arm && return 0
    local src="$DEPS_DIR/lsp-plugins" work="$BUILD_WORK_DIR/lsp"
    rm -rf "$work" && mkdir -p "$work"
    cp -R "$src" "$work/src"
    if [ "$(uname -s)" = "Darwin" ]; then
        CC="clang -arch arm64" CXX="clang++ -arch arm64" \
            _lsp_build "$work/src" "$work/install" ARCHITECTURE=arm64
        local bundle
        bundle=$(find "$work/install" -type d -name "lsp-plugins.vst3" | head -1)
        codesign --remove-signature "$bundle" 2>/dev/null || true
        codesign --force --deep --sign - "$bundle"
    else
        _lsp_build "$work/src" "$work/install"
    fi
    collect_vst3 "$work/install" "lsp-plugins.vst3"
}

# --- Rust / nih-plug bundles (`cargo xtask bundle`) ---
# Both workspaces ship an `xtask` package (nih_plug_xtask / nice_plug_xtask);
# it is run by package name since bus_channel_strip has no `cargo xtask` alias.
# The xtask builds from the OUTERMOST directory holding a Cargo.toml, which
# under deps/ is OpenRig's own workspace, so each workspace is copied into
# $BUILD_WORK_DIR (no Cargo.toml above it) and built there.
# nih-plug's xtask writes target/bundled/<Name>.vst3 with the binary stem equal
# to the bundle name; on macOS `bundle-universal` lipo-merges both arches.

_xtask_bundle() { # $1=source workspace, $2=toolchain ("" = workspace default), rest=packages; sets XTASK_BUNDLED
    local src="$1" dir="$BUILD_WORK_DIR/rust-$(basename "$1")" tc="$2"
    shift 2
    rm -rf "$dir" && mkdir -p "$BUILD_WORK_DIR" && cp -R "$src" "$dir"
    XTASK_BUNDLED="$dir/target/bundled"
    local cargo=(cargo) pkgs=() p
    [ -n "$tc" ] && { rustup toolchain install "$tc" --profile minimal; cargo=(cargo "+$tc"); }
    for p in "$@"; do pkgs+=(-p "$p"); done
    if [ "$(uname -s)" = "Darwin" ]; then
        (cd "$dir" && rustup target add ${tc:+--toolchain "$tc"} x86_64-apple-darwin aarch64-apple-darwin)
        (cd "$dir" && MACOSX_DEPLOYMENT_TARGET=11.0 "${cargo[@]}" run --package xtask --release -- bundle-universal "${pkgs[@]}" --release)
    else
        (cd "$dir" && "${cargo[@]}" run --package xtask --release -- bundle "${pkgs[@]}" --release)
    fi
}

build_nihplug() {
    # nih-plug example plugins (GPL-3 as VST3): Soft Vacuum, Spectral Compressor,
    # Safety Limiter, Crossover. Crossover's default `simd` feature needs nightly,
    # so the whole set is built with nightly.
    _skip_windows && return 0
    local n
    _xtask_bundle "$DEPS_DIR/nih-plug" nightly soft_vacuum spectral_compressor safety_limiter crossover
    for n in "Soft Vacuum" "Spectral Compressor" "Safety Limiter" "Crossover"; do
        collect_bundle "$XTASK_BUNDLED" "$n.vst3"
    done
}

build_buschannelstrip() {
    # Bus Channel Strip (fsecada01, GPL-3 per Cargo.toml): API5500 EQ,
    # ButterComp2, Pultec, dynamic EQ, transformer, punch — built without its
    # `gui` feature (generic editor). Its rust-toolchain.toml pins nightly.
    _skip_windows && return 0
    _xtask_bundle "$DEPS_DIR/bus_channel_strip" "" bus_channel_strip
    collect_bundle "$XTASK_BUNDLED" "Bus-Channel-Strip.vst3"
}
