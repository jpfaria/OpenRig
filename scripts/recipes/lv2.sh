# shellcheck shell=bash
# LV2 (and the native NAM inference lib) build recipes.
#
# Sourced by scripts/build-lib-internal.sh, which defines the helpers used here
# (do_make / do_cmake / do_meson / collect_libs) and the DEPS_DIR / OUTPUT_DIR /
# BUILD_WORK_DIR / LIB_EXT / CROSS_COMPILE / MINGW_TARGET env.

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
    # ArtyFX's CMakeLists pins cmake_minimum_required(VERSION 2.6); CMake
    # >= 4 dropped support for < 3.5 so configure fails on any runner whose
    # CMake is newer than Ubuntu 22.04's apt build. Promote the policy
    # baseline so the configure step accepts the upstream file as-is.
    CMAKE_EXTRA="${CMAKE_EXTRA:-} -DCMAKE_POLICY_VERSION_MINIMUM=3.5" \
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
    local os=$(uname -s)

    # GxPlugins use __attribute__((section(".rt.text"))) which is Linux/ELF-only.
    # On macOS (Mach-O) and Windows (PE/MinGW) we strip it via sed and compile
    # each plugin individually. Linux works straight via the upstream Makefile.
    if [ "$os" != "Darwin" ] && [ -z "${MINGW_TARGET:-}" ] \
       && ! echo "${CROSS_COMPILE:-}" | grep -q mingw; then
        do_make "$src"
        collect_libs "$src"
        return
    fi

    # Per-plugin compile fallback (Darwin + MinGW).
    local target compiler arch_flags target_flags output_ext
    if [ "$os" = "Darwin" ]; then
        target="darwin"
        compiler="c++"
        arch_flags="-arch arm64 -arch x86_64 -mmacosx-version-min=11.0"
        target_flags="-bundle"
        output_ext="dylib"
    else
        target="mingw"
        compiler="${CXX:-g++}"
        # MinGW64 libc/STL does not transitively expose <cstdint>/<cstddef>,
        # while GxPlugins sources rely on int32_t et al. being visible without
        # an explicit include. Force-include the header instead of patching
        # every .cpp upstream. _USE_MATH_DEFINES unlocks M_PI in <cmath> for
        # the bundled zita-resampler-1.1.0 several plugins ship inline.
        arch_flags="-include cstdint -include cstddef -D_USE_MATH_DEFINES"
        target_flags="-shared"
        output_ext="dll"
    fi

    local lv2_cflags
    lv2_cflags=$(pkg-config --cflags lv2 2>/dev/null || echo "")

    for plugin_dir in "$src"/Gx*.lv2; do
        [ -d "$plugin_dir" ] || continue
        local name
        name=$(grep "^	NAME" "$plugin_dir/Makefile" 2>/dev/null | head -1 | sed 's/.*= *//')
        [ -n "$name" ] || continue

        local cpp_file
        cpp_file=$(ls "$plugin_dir/plugin/"*.cpp 2>/dev/null | head -1)
        [ -n "$cpp_file" ] && [ -f "$cpp_file" ] || continue

        local patched="$BUILD_WORK_DIR/gxplugins_${name}_patched.cpp"
        sed 's/__attribute__((section("[^"]*")))//g' "$cpp_file" > "$patched"

        # Some plugins include zita-resampler from a subdirectory
        local extra_include=""
        local zita_dir
        zita_dir=$(find "$plugin_dir/dsp" -name "resampler.cc" -exec dirname {} \; 2>/dev/null | head -1)
        if [ -n "$zita_dir" ]; then
            extra_include="-I$zita_dir"
        fi

        local errlog="$BUILD_WORK_DIR/gxplugins_${name}.err"
        # shellcheck disable=SC2086
        if "$compiler" -std=c++11 \
            $arch_flags \
            -I"$plugin_dir" -I"$plugin_dir/dsp" -I"$plugin_dir/plugin" \
            $extra_include $lv2_cflags \
            -fPIC -DPIC -O2 \
            -Wno-duplicate-decl-specifier -Wno-macro-redefined \
            $target_flags -o "$OUTPUT_DIR/${name}.${output_ext}" \
            "$patched" -lm 2>"$errlog"; then
            echo "  OK ($target): $name"
            rm -f "$errlog"
        else
            echo "  FAIL ($target): $name"
            # First few error lines help diagnose missing header / undefined symbol
            sed -n '1,8p' "$errlog" | sed "s/^/    | /"
        fi

        rm -f "$patched"
    done
}

build_ojd() {
    local src="$DEPS_DIR/Schrammel_OJD"
    do_cmake "$src"
    collect_libs "$LAST_BUILD_DIR" "OJD"
}

build_aether() {
    local src="$DEPS_DIR/Aether"
    # GUI off: OpenRig provides the UI, and this drops the X11/OpenGL/nanovg/
    # pugl dependency tree so the DSP-only build cross-compiles cleanly.
    CMAKE_EXTRA="${CMAKE_EXTRA:-} -DBUILD_GUI=OFF -DBUILD_TESTS=OFF -DBUILD_BENCHMARKS=OFF" \
        do_cmake "$src"
    # Aether's CMake emits the DSP as a MODULE named aether_dsp.so on every
    # platform (the macOS build is already a universal Mach-O, just .so-suffixed).
    # Normalise to the slot's native extension so collect_libs (which filters by
    # LIB_EXT) and the OpenRig slot convention (.dylib/.dll/.so) line up.
    local mod
    mod=$(find "$LAST_BUILD_DIR" -path '*aether.lv2/aether_dsp.so' -type f | head -1)
    if [ -n "$mod" ] && [ "$LIB_EXT" != "so" ]; then
        cp "$mod" "$(dirname "$mod")/aether_dsp.${LIB_EXT}"
    fi
    collect_libs "$LAST_BUILD_DIR" "aether_dsp"
}

build_x42() {
    # x42-plugins (Robin Gareus): robtk-based GNU Make. Build DSP-only
    # (BUILDOPENGL/BUILDJACKAPP/INLINEDISPLAY=no) to skip the Cairo/Pango/X11
    # GUI stack. The Makefile only emits a Windows .dll when XWIN is set (its
    # else branch — which also catches MSYS2 — would wrongly build a .so with
    # X11), so force XWIN on the MinGW target. macOS builds host-arch only, so
    # inject both arches for a real universal binary.
    local mk_args="BUILDOPENGL=no BUILDJACKAPP=no INLINEDISPLAY=no"
    if [ "$(uname -s)" = "Darwin" ]; then
        export CFLAGS="-arch x86_64 -arch arm64 -mmacosx-version-min=11.0"
        export LDFLAGS="-arch x86_64 -arch arm64"
    fi
    if [ -n "${MINGW_TARGET:-}" ] || echo "${CROSS_COMPILE:-}" | grep -q mingw; then
        # XWIN switches the Makefile to Windows semantics (.dll, static libgcc)
        # but also points CC/STRIP at the x86_64-w64-mingw32-* triplet. On a
        # native MSYS2/MINGW64 runner the gcc triplet exists but strip does not
        # (it is just `strip`), so override STRIP back to the plain name.
        mk_args="$mk_args XWIN=x86_64-w64-mingw32 STRIP=strip"
    fi
    local p
    for p in darc dpl fil4; do
        local src="$DEPS_DIR/$p.lv2"
        # shellcheck disable=SC2086
        do_make "$src" $mk_args
        collect_libs "$src/build" "$p"
    done
    unset CFLAGS LDFLAGS
}

build_distrho() {
    # DISTRHO-Ports: Meson monorepo of JUCE-based ports. Build only the five
    # pro effects we ship, LV2-only, JUCE5 tree only. macOS gets a universal
    # binary; Linux builds headless (no X11 editor). The generated per-plugin
    # <Name>.ttl are staged under $OUTPUT_DIR/ttl so they can be captured from
    # the CI artifact (JUCE emits the TTL at build time; there is no other way
    # to obtain a faithful one).
    local src="$DEPS_DIR/DISTRHO-Ports"
    local build_dir="$BUILD_WORK_DIR/DISTRHO-Ports"
    local opts="-Dbuild-vst2=false -Dbuild-vst3=false -Dbuild-juce5-only=true"
    opts="$opts -Dplugins=tal-reverb-2,tal-dub-3,pitchedDelay,tal-filter-2,luftikus"
    if [ "$(uname -s)" = "Darwin" ]; then
        opts="$opts -Dbuild-universal=true"
    elif [ "$(uname -s)" = "Linux" ]; then
        opts="$opts -Dlinux-headless=true"
    fi
    local cross_args=""
    if [ -n "$CROSS_COMPILE" ] && [ -f "/build/meson-cross-$CROSS_COMPILE.ini" ]; then
        cross_args="--cross-file /build/meson-cross-$CROSS_COMPILE.ini"
    fi
    # shellcheck disable=SC2086
    meson setup "$build_dir" "$src" --buildtype=release $opts $cross_args
    ninja -C "$build_dir" -j "$JOBS"
    LAST_BUILD_DIR="$build_dir"
    collect_libs "$build_dir"
    # Stage each .lv2 bundle whole (per-plugin subdir) so the per-plugin
    # manifest.ttl / presets.ttl do not collide by basename in the artifact.
    mkdir -p "$OUTPUT_DIR/ttl"
    find "$build_dir" -name "*.lv2" -type d 2>/dev/null | while read -r b; do
        cp -R "$b" "$OUTPUT_DIR/ttl/$(basename "$b")" 2>/dev/null || true
    done
}
