#!/usr/bin/env bash
set -euo pipefail

# This script runs INSIDE the Docker container (or natively on macOS).
# It builds a specific plugin and copies the output to /output.
#
# Usage: build-lib <plugin-name>
#        build-lib --list

DEPS_DIR="${DEPS_DIR:-/build/deps}"
OUTPUT_DIR="${OUTPUT_DIR:-/output}"
JOBS=$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 4)

# Cross-compilation support (set by build-lib.sh for Windows)
CROSS_COMPILE="${CROSS_COMPILE:-}"

# Library extension for current target
lib_ext() {
    if [ -n "$CROSS_COMPILE" ] && echo "$CROSS_COMPILE" | grep -q mingw; then
        echo "dll"
    elif [ -n "${MINGW_TARGET:-}" ]; then
        # MSYS2/MINGW64 builds natively on Windows runners (no cross prefix)
        # but the target IS Windows so libraries are .dll, not .so.
        echo "dll"
    elif [ "$(uname -s)" = "Darwin" ]; then
        echo "dylib"
    else
        echo "so"
    fi
}

# Make flags for cross-compilation
cross_make_flags() {
    if [ -n "$CROSS_COMPILE" ]; then
        echo "CC=${CROSS_COMPILE}-gcc CXX=${CROSS_COMPILE}-g++ AR=${CROSS_COMPILE}-ar"
    fi
}

LIB_EXT=$(lib_ext)

# Use a separate build directory to avoid conflicts with host CMakeCache
BUILD_WORK_DIR="${BUILD_WORK_DIR:-/tmp/openrig-build}"
mkdir -p "$BUILD_WORK_DIR"

# --- Helpers ---

# Returns 0 if $1 is a binary that matches the current target architecture.
# Some upstream LV2 source trees ship prebuilt .so files (typically x86_64)
# alongside their Makefiles. Without this filter those prebuilts leak into
# libs/lv2/linux-aarch64/ on the ARM runner — appimagetool then refuses
# the AppDir because it sees mixed architectures.
binary_matches_target() {
    local f="$1"
    local desc
    desc="$(file -b "$f" 2>/dev/null)" || return 1

    # Cross-compile to mingw → expect PE32+ DLL
    if [ -n "$CROSS_COMPILE" ] && echo "$CROSS_COMPILE" | grep -q mingw; then
        echo "$desc" | grep -qE "PE32\+? executable.*Windows"
        return $?
    fi
    # Native MinGW (MSYS2) → also PE
    if [ -n "${MINGW_TARGET:-}" ]; then
        echo "$desc" | grep -qE "PE32\+? executable.*Windows"
        return $?
    fi
    # macOS → Mach-O (any arch counts; we ship universal binaries)
    if [ "$(uname -s)" = "Darwin" ]; then
        echo "$desc" | grep -q "Mach-O"
        return $?
    fi
    # Linux native → must match the host's machine arch
    local host_arch
    host_arch="$(uname -m)"
    case "$host_arch" in
        aarch64|arm64)  echo "$desc" | grep -qE "ELF.*ARM aarch64" ;;
        x86_64|amd64)   echo "$desc" | grep -qE "ELF.*x86-64" ;;
        *)              return 0 ;; # unknown host arch — accept and let CI catch it
    esac
}

# Collect built libs from a directory
collect_libs() {
    local search_dir="$1"
    shift
    # Filter both by extension (.so/.dylib/.dll) AND by binary architecture.
    # Without the architecture filter, prebuilt artifacts shipped in upstream
    # source trees (e.g. setBfree b_*.so x86_64 binaries committed alongside
    # the Makefile) leak into libs/lv2/<platform>/ for the wrong arch.
    if [ $# -eq 0 ]; then
        find "$search_dir" -name "*.${LIB_EXT}" | while read -r f; do
            if binary_matches_target "$f"; then
                cp "$f" "$OUTPUT_DIR/"
            fi
        done
    else
        for pattern in "$@"; do
            find "$search_dir" \( -name "${pattern}.${LIB_EXT}" -o -name "lib${pattern}.${LIB_EXT}" \) | while read -r f; do
                if binary_matches_target "$f"; then
                    cp "$f" "$OUTPUT_DIR/"
                fi
            done
        done
    fi
}

# Collect a built plugin BUNDLE DIRECTORY (e.g. a VST3 `.vst3/`) into
# $OUTPUT_DIR, preserving its internal `Contents/<arch>/` tree. Unlike
# collect_libs (single shared objects), a VST3 bundle is a directory: each
# platform runner only populates its own `Contents/<arch>/` subfolder, and the
# commit-libs merge step unions them into the shipped cross-platform bundle.
collect_bundle() {
    local search_dir="$1"
    local bundle_name="$2"
    local found
    found=$(find "$search_dir" -type d -name "$bundle_name" | head -1)
    if [ -z "$found" ]; then
        echo "  collect_bundle: no $bundle_name found under $search_dir" >&2
        return 1
    fi
    rm -rf "${OUTPUT_DIR:?}/$bundle_name"
    cp -R "$found" "$OUTPUT_DIR/$bundle_name"
    echo "  collected bundle: $bundle_name ($(find "$OUTPUT_DIR/$bundle_name" -type f | wc -l | tr -d ' ') files)"
}

# Collect the SINGLE VST3 bundle built under $search_dir, normalising its folder
# name to $dest_name. JUCE names the .vst3 after PRODUCT_NAME (often unequal to
# the CMake target — e.g. "REEV-R.vst3"), which the manifest `bundle:` must
# match exactly; rather than predict that name per plugin, each recipe builds
# exactly one VST3 target and we rename whatever it produced to a stable name.
# Fails loudly if zero or more than one .vst3 is found.
collect_vst3() {
    local search_dir="$1" dest_name="$2"
    local found
    found=()
    while IFS= read -r d; do found+=("$d"); done \
        < <(find "$search_dir" -type d -name "*.vst3")
    if [ "${#found[@]}" -ne 1 ]; then
        echo "  collect_vst3: expected exactly one .vst3 under $search_dir, found ${#found[@]}" >&2
        return 1
    fi
    rm -rf "${OUTPUT_DIR:?}/$dest_name"
    cp -R "${found[0]}" "$OUTPUT_DIR/$dest_name"
    echo "  collected bundle: ${found[0]##*/} -> $dest_name ($(find "$OUTPUT_DIR/$dest_name" -type f | wc -l | tr -d ' ') files)"
}

# Build with Make (supports cross-compilation)
do_make() {
    local src="$1"
    shift
    # shellcheck disable=SC2046,SC2086
    make -C "$src" -j "$JOBS" $(cross_make_flags) "$@"
}

# Build with CMake (supports cross-compilation via CMAKE_EXTRA env)
# Uses $BUILD_WORK_DIR to avoid conflicts with host CMakeCache
do_cmake() {
    local src="$1"
    local target="${2:-}"
    local build_dir="$BUILD_WORK_DIR/$(basename "$src")"
    # shellcheck disable=SC2086
    cmake -S "$src" -B "$build_dir" \
        -DCMAKE_BUILD_TYPE=Release \
        ${CMAKE_EXTRA:-}
    if [ -n "$target" ]; then
        cmake --build "$build_dir" --config Release --target "$target" -j "$JOBS"
    else
        cmake --build "$build_dir" --config Release -j "$JOBS"
    fi
    # Store last build dir for collect_libs
    LAST_BUILD_DIR="$build_dir"
}

# Build with Meson (supports cross-compilation via meson cross file)
do_meson() {
    local src="$1"
    local build_dir="$BUILD_WORK_DIR/$(basename "$src")"
    local cross_args=""
    if [ -n "$CROSS_COMPILE" ] && [ -f "/build/meson-cross-$CROSS_COMPILE.ini" ]; then
        cross_args="--cross-file /build/meson-cross-$CROSS_COMPILE.ini"
    fi
    # shellcheck disable=SC2086
    meson setup "$build_dir" "$src" --buildtype=release $cross_args
    ninja -C "$build_dir" -j "$JOBS"
    LAST_BUILD_DIR="$build_dir"
}

# --- Plugin build recipes (sourced modules) ---
# Each backend's build_<name> functions live in scripts/recipes/*.sh so this
# dispatcher stays small and the recipes are grouped by backend. Sourced
# relative to THIS script (BASH_SOURCE), so it resolves both from the repo
# checkout (CI runs `bash scripts/build-lib-internal.sh`) and from the Docker
# image (Dockerfile.build-libs copies recipes/ next to the baked build-lib).
RECIPES_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/recipes"
# shellcheck source=/dev/null
source "$RECIPES_DIR/lv2.sh"
# shellcheck source=/dev/null
source "$RECIPES_DIR/vst3.sh"

# --- Registry ---

PLUGINS=(
    nam
    dragonfly-reverb
    zam-plugins
    mod-utilities
    caps-lv2
    tap-lv2
    shiro-plugins
    dpf-plugins
    mverb
    mda-lv2
    fomp
    invada-studio
    wolf-shaper
    artyfx
    sooperlooper
    setbfree
    bolliedelay
    gxplugins
    chowcentaur
    chowtape
    chowphaser
    chowmatrix
    chowmultitool
    byod
    reevr
    sirial
    qdelay
    gate12
    time12
    filtr
    zl_equalizer
    zl_compressor
    zl_splitter
    zl_spectrum_equalizer
    zl_warm
    zl_inflator
    cloudreverb
    roomreverb
    frequalizer
    retuner
    setekh
    vitottx
    aidax
    dfzitarev1
    master_me
    fogpad
    regrader
    rechoir
    transformant
    darvaza
    homecorrupter
    ojd
    aether
    x42
    distrho
)

# Map plugin name to build function
dispatch() {
    case "$1" in
        nam)              build_nam ;;
        dragonfly-reverb) build_dragonfly_reverb ;;
        zam-plugins)      build_zam_plugins ;;
        mod-utilities)    build_mod_utilities ;;
        caps-lv2)         build_caps_lv2 ;;
        tap-lv2)          build_tap_lv2 ;;
        shiro-plugins)    build_shiro_plugins ;;
        dpf-plugins)      build_dpf_plugins ;;
        mverb)            build_mverb ;;
        mda-lv2)          build_mda_lv2 ;;
        fomp)             build_fomp ;;
        invada-studio)    build_invada_studio ;;
        wolf-shaper)      build_wolf_shaper ;;
        artyfx)           build_artyfx ;;
        sooperlooper)     build_sooperlooper ;;
        setbfree)         build_setbfree ;;
        bolliedelay)      build_bolliedelay ;;
        gxplugins)        build_gxplugins ;;
        chowcentaur)      build_chowcentaur ;;
        chowtape)         build_chowtape ;;
        chowphaser)       build_chowphaser ;;
        chowmatrix)       build_chowmatrix ;;
        chowmultitool)    build_chowmultitool ;;
        byod)             build_byod ;;
        reevr)            build_reevr ;;
        sirial)           build_sirial ;;
        qdelay)           build_qdelay ;;
        gate12)           build_gate12 ;;
        time12)           build_time12 ;;
        filtr)            build_filtr ;;
        zl_equalizer)         build_zl_equalizer ;;
        zl_compressor)        build_zl_compressor ;;
        zl_splitter)          build_zl_splitter ;;
        zl_spectrum_equalizer) build_zl_spectrum_equalizer ;;
        zl_warm)              build_zl_warm ;;
        zl_inflator)          build_zl_inflator ;;
        cloudreverb)      build_cloudreverb ;;
        roomreverb)       build_roomreverb ;;
        frequalizer)      build_frequalizer ;;
        retuner)          build_retuner ;;
        setekh)           build_setekh ;;
        vitottx)          build_vitottx ;;
        aidax)            build_aidax ;;
        dfzitarev1)       build_dfzitarev1 ;;
        master_me)        build_master_me ;;
        fogpad)          build_fogpad ;;
        regrader)        build_regrader ;;
        rechoir)         build_rechoir ;;
        transformant)    build_transformant ;;
        darvaza)         build_darvaza ;;
        homecorrupter)   build_homecorrupter ;;
        ojd)              build_ojd ;;
        aether)           build_aether ;;
        x42)              build_x42 ;;
        distrho)          build_distrho ;;
        *) echo "Unknown plugin: $1"; exit 1 ;;
    esac
}

# --- Main ---

if [ $# -eq 0 ] || [ "$1" = "--help" ]; then
    echo "Usage: build-lib <plugin|all> [--list]"
    echo ""
    echo "Builds a plugin and copies output to $OUTPUT_DIR"
    echo ""
    echo "Available plugins:"
    printf '  %s\n' "${PLUGINS[@]}"
    exit 0
fi

if [ "$1" = "--list" ]; then
    printf '%s\n' "${PLUGINS[@]}"
    exit 0
fi

mkdir -p "$OUTPUT_DIR"

if [ "$1" = "all" ]; then
    for plugin in "${PLUGINS[@]}"; do
        echo ""
        echo "========================================="
        echo "  Building: $plugin"
        echo "========================================="
        dispatch "$plugin" || echo "FAILED: $plugin (continuing...)"
    done
else
    for plugin in "$@"; do
        echo ""
        echo "========================================="
        echo "  Building: $plugin"
        echo "========================================="
        dispatch "$plugin"
    done
fi

echo ""
echo "Done. Output in $OUTPUT_DIR:"
ls -la "$OUTPUT_DIR/"
