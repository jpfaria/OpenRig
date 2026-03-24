#!/usr/bin/env bash
# import-thumbnails.sh
# Scans plugins/*/modgui/thumbnail*.png, resizes them to max 128px height,
# and copies them to assets/blocks/thumbnails/{effect_type}/{mapped_name}.png
#
# Idempotent: can be run multiple times safely.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
PLUGINS_DIR="$REPO_ROOT/plugins"
THUMBNAILS_DIR="$REPO_ROOT/assets/blocks/thumbnails"

# ---------------------------------------------------------------------------
# Returns the effect_type for a given plugin key, or empty string if unknown.
# ---------------------------------------------------------------------------
get_effect_type() {
    local key="$1"
    case "$key" in
        # preamp
        gx_jcm800pre|gx_jcm800pre_st|gx_studiopre_st)
            echo "preamp" ;;
        # amp
        gx_amp|gx_amp_stereo|mod-caps-AmpVTS)
            echo "amp" ;;
        # cab
        gx_cabinet|mod-caps-CabinetIV|MOD-CabinetLoader)
            echo "cab" ;;
        # ir
        MOD-ConvolutionLoader|zeroconvo|lsp-plugins-impulsantworten)
            echo "ir" ;;
        # nam
        neural_amp_modeler|rt-neural-generic)
            echo "nam" ;;
        # gain
        gx_rangem|gx_cstb|gx_fumaster|gx_fuzz|gx_fuzzface|gx_fuzzfacefm|\
        gx_hfb|gx_hogsfoot|gx_hornet|gx_KnightFuzz|gx_mole|gx_muff|\
        gx_redeye|gx_scream|gx_susta|gx_vfm|gx_voxtb|gxbooster|gxts9|\
        mod-bigmuff|mod-ds1|ChowCentaur|OJD|wolf-shaper|\
        ZamTube|ZamAutoSat|mod-caps-Saturate|mod-caps-Spice|mod-caps-SpiceX2|\
        mod-caps-White|mod-mda-Overdrive|tap-tubewarmth|tap-sigmoid|\
        b_overdrive|CHOWTapeModel|mud)
            echo "gain" ;;
        # dynamics
        ZamComp|ZamCompX2|ZamGate|ZamGateX2|ZaMaximX2|ZaMultiComp|ZaMultiCompX2|\
        tap-dynamics|tap-dynamics-st|tap-limiter|tap-deesser|abGate|\
        mod-caps-Compress|mod-caps-CompressX2|master_me)
            echo "dynamics" ;;
        # delay
        gx_mbdelay|bolliedelay|ZamDelay|tap-echo|mod-mda-DubDelay)
            echo "delay" ;;
        # reverb
        DragonflyHallReverb|DragonflyPlateReverb|DragonflyRoomReverb|\
        DragonflyEarlyReflections|MVerb|tap-reverb|tap-reflector|b_reverb|\
        mod-caps-Plate|mod-caps-PlateX2|mod-caps-Scape|mod-tal-Reverb-2|\
        mod-mda-Ambience|Shiroverb|floaty)
            echo "reverb" ;;
        # modulation
        tap-tremolo|tap-chorusflanger|tap-vibrato|tap-autopan|tap-rotspeak|\
        mod-caps-ChorusI|mod-caps-PhaserII|mod-mda-Leslie|mod-mda-ThruZero|\
        mod-mda-RingMod|mod-mda-Shepard|Modulay|PingPongPan)
            echo "modulation" ;;
        # filter
        gx_quack|gxtilttone|tap-eq|tap-eqbw|ZamEQ2|ZamGEQ31|\
        mod-caps-CEO|mod-caps-AutoFilter|mod-caps-ToneStack|\
        mod-bpf|mod-hpf|mod-lpf|fil4|fomp)
            echo "filter" ;;
        # wah
        GxSwitchlessWah|gxwah)
            echo "wah" ;;
        # pitch
        gx_oc_2|gx_detune|tap-pitch|mod-mda-Detune|mod-drop|mod-supercapo|\
        mod-capo|mod-harmonizer|mod-harmonizer2|Pitchotto|mod-superwhammy)
            echo "pitch" ;;
        # utility
        gx_slowgear|sooperlooper|sooperlooper-2x2|tap-doubler|tap-pinknoise|\
        mod-caps-Narrower|mod-caps-Wider|mod-caps-Fractal|mod-mda-RoundPan|\
        mod-mda-SubSynth|mod-gain|mod-gain2x2|mod-volume|mod-volume-2x2|\
        tinygain|carla-audiogain|tuna|fat1|ZamHeadX2|mod-caps-Click)
            echo "utility" ;;
        *)
            echo "" ;;
    esac
}

# ---------------------------------------------------------------------------
# Ensure all type directories exist
# ---------------------------------------------------------------------------
EFFECT_TYPES="preamp amp cab body ir full_rig gain dynamics filter wah modulation delay reverb utility nam pitch"
for type in $EFFECT_TYPES; do
    mkdir -p "$THUMBNAILS_DIR/$type"
done

# ---------------------------------------------------------------------------
# Process each plugin thumbnail
# ---------------------------------------------------------------------------
imported=0
skipped=0

while IFS= read -r src_file; do
    plugin_dir="$(basename "$(dirname "$(dirname "$src_file")")")"

    # Strip .lv2 suffix to get the lookup key
    plugin_key="${plugin_dir%.lv2}"

    effect_type="$(get_effect_type "$plugin_key")"

    if [ -z "$effect_type" ]; then
        skipped=$((skipped + 1))
        continue
    fi

    # Destination filename: plugin_key.png
    dst_file="$THUMBNAILS_DIR/$effect_type/${plugin_key}.png"

    # Resize to max 128px height, keeping aspect ratio, using sips (macOS)
    sips --resampleHeight 128 "$src_file" --out "$dst_file" > /dev/null 2>&1

    echo "  [imported] $plugin_key → $effect_type"
    imported=$((imported + 1))

done < <(find "$PLUGINS_DIR" -path "*/modgui/thumbnail*.png" | sort)

echo ""
echo "Done. Imported: $imported  |  Skipped (no mapping): $skipped"
