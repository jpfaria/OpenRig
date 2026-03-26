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

# --- Plugin build recipes ---
# Each function builds one plugin group.
# Convention: build_<plugin_name>
# Must copy resulting .so/.dylib/.dll to $OUTPUT_DIR

# Use a separate build directory to avoid conflicts with host CMakeCache
BUILD_WORK_DIR="${BUILD_WORK_DIR:-/tmp/openrig-build}"
mkdir -p "$BUILD_WORK_DIR"

# --- Helpers ---

# Collect built libs from a directory
collect_libs() {
    local search_dir="$1"
    shift
    # Remaining args are name patterns (without extension)
    if [ $# -eq 0 ]; then
        # Collect all shared libs
        find "$search_dir" \( -name "*.so" -o -name "*.dylib" -o -name "*.dll" \) | while read -r f; do
            cp "$f" "$OUTPUT_DIR/"
        done
    else
        for pattern in "$@"; do
            find "$search_dir" \( -name "${pattern}.so" -o -name "${pattern}.dylib" -o -name "${pattern}.dll" -o -name "lib${pattern}.so" -o -name "lib${pattern}.dylib" -o -name "lib${pattern}.dll" \) | while read -r f; do
                cp "$f" "$OUTPUT_DIR/"
            done
        done
    fi
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

# --- Plugin build recipes ---

build_nam() {
    local src="$DEPS_DIR/neural-amp-modeler-lv2/deps/NeuralAudio"
    do_cmake "$src" NeuralAudioCAPI
    collect_libs "$LAST_BUILD_DIR" "NeuralAudioCAPI" "libNeuralAudioCAPI"
}

build_dragonfly_reverb() {
    local src="$DEPS_DIR/dragonfly-reverb"
    do_make "$src" BUILD_LV2=true NOOPT=true HAVE_OPENGL=false HAVE_CAIRO=false HAVE_VULKAN=false HAVE_STUB=true USE_FILE_BROWSER=false
    collect_libs "$src/bin" "*_dsp"
}

build_zam_plugins() {
    local src="$DEPS_DIR/zam-plugins"
    do_make "$src" BUILD_LV2=true NOOPT=true HAVE_OPENGL=false HAVE_CAIRO=false HAVE_VULKAN=false HAVE_STUB=true USE_FILE_BROWSER=false
    collect_libs "$src/bin" "Zam*_dsp"
}

build_mod_utilities() {
    local src="$DEPS_DIR/mod-utilities"
    do_make "$src"
    collect_libs "$src"
}

build_caps_lv2() {
    local src="$DEPS_DIR/caps-lv2"
    do_make "$src"
    collect_libs "$src"
}

build_tap_lv2() {
    local src="$DEPS_DIR/tap-lv2"
    do_make "$src"
    collect_libs "$src"
}

build_shiro_plugins() {
    local src="$DEPS_DIR/SHIRO-Plugins"
    do_make "$src" BUILD_LV2=true NOOPT=true HAVE_OPENGL=false HAVE_CAIRO=false HAVE_VULKAN=false HAVE_STUB=true USE_FILE_BROWSER=false
    collect_libs "$src/bin" "*_dsp"
}

build_dpf_plugins() {
    local src="$DEPS_DIR/DPF-Plugins"
    do_make "$src" BUILD_LV2=true NOOPT=true HAVE_OPENGL=false HAVE_CAIRO=false HAVE_VULKAN=false HAVE_STUB=true USE_FILE_BROWSER=false
    collect_libs "$src/bin" "*_dsp"
}

build_mverb() {
    local src="$DEPS_DIR/MVerb"
    do_make "$src" BUILD_LV2=true NOOPT=true HAVE_OPENGL=false HAVE_CAIRO=false HAVE_VULKAN=false HAVE_STUB=true USE_FILE_BROWSER=false
    collect_libs "$src/bin" "*_dsp"
}

build_mda_lv2() {
    local src="$DEPS_DIR/mda-lv2"
    do_meson "$src"
    collect_libs "$LAST_BUILD_DIR"
}

build_fomp() {
    local src="$DEPS_DIR/fomp"
    do_meson "$src"
    collect_libs "$LAST_BUILD_DIR"
}

build_invada_studio() {
    local src="$DEPS_DIR/invada-studio"
    do_make "$src"
    collect_libs "$src"
}

build_wolf_shaper() {
    local src="$DEPS_DIR/wolf-shaper"
    do_make "$src" BUILD_LV2=true NOOPT=true HAVE_OPENGL=false HAVE_CAIRO=false HAVE_VULKAN=false HAVE_STUB=true USE_FILE_BROWSER=false
    collect_libs "$src/bin" "*_dsp"
}

build_artyfx() {
    local src="$DEPS_DIR/openAV-ArtyFX"
    do_cmake "$src"
    collect_libs "$LAST_BUILD_DIR" "artyfx"
}

build_sooperlooper() {
    local src="$DEPS_DIR/sooperlooper"
    cd "$src"
    if [ ! -f configure ]; then
        autoreconf -fi
    fi
    # shellcheck disable=SC2046
    ./configure --prefix=/tmp/sl-install $([ -n "$CROSS_COMPILE" ] && echo "--host=$CROSS_COMPILE" || true)
    make -j "$JOBS"
    collect_libs "." "sooperlooper*"
}

build_setbfree() {
    local src="$DEPS_DIR/setBfree"
    do_make "$src"
    collect_libs "$src" "b_*"
}

build_gxplugins() {
    local src="$DEPS_DIR/GxPlugins.lv2"
    do_make "$src"
    collect_libs "$src"
}

build_chowcentaur() {
    local src="$DEPS_DIR/AnalogTapeModel"
    do_cmake "$src"
    collect_libs "$LAST_BUILD_DIR" "ChowCentaur"
}

build_ojd() {
    local src="$DEPS_DIR/Schrammel_OJD"
    do_cmake "$src"
    collect_libs "$LAST_BUILD_DIR" "OJD"
}

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
    ojd
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
        ojd)              build_ojd ;;
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
