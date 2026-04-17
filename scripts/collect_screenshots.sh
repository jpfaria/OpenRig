#!/usr/bin/env bash
# Collect LV2 plugin screenshots from modgui into assets/blocks/screenshots/
# Usage: bash scripts/collect_screenshots.sh
set -euo pipefail

PLUGINS_DIR=".plugins/lv2"
OUT_DIR="assets/blocks/screenshots"

# Mapping: "model_id|effect_type|bundle_glob|screenshot_glob"
declare -a MAPPINGS=(
  # TAP plugins
  "lv2_tap_equalizer|filter|tap-eq.lv2|screenshot-tap-equalizer*"
  "lv2_tap_equalizer_bw|filter|tap-eqbw.lv2|screenshot-tap-equalizerbw*"
  "lv2_tap_chorus_flanger|modulation|tap-chorusflanger.lv2|screenshot-tap-chorusflanger*"
  "lv2_tap_tremolo|modulation|tap-tremolo.lv2|screenshot-tap-tremolo*"
  "lv2_tap_rotspeak|modulation|tap-rotspeak.lv2|screenshot-tap-rotspeak*"
  "lv2_tap_reverb|reverb|tap-reverb.lv2|screenshot-tap-reverberator*"
  "lv2_tap_reflector|reverb|tap-reflector.lv2|screenshot-tap-reflector*"
  "lv2_tap_deesser|dynamics|tap-deesser.lv2|screenshot-tap-deesser*"
  "lv2_tap_dynamics|dynamics|tap-dynamics.lv2|screenshot-tap-dynamics*"
  "lv2_tap_limiter|dynamics|tap-limiter.lv2|screenshot-tap-limiter*"
  "lv2_tap_sigmoid|gain|tap-sigmoid.lv2|screenshot-tap-sigmoid*"
  "lv2_tap_tubewarmth|gain|tap-tubewarmth.lv2|screenshot-tap-tubewarmth*"
  "lv2_tap_doubler|delay|tap-doubler.lv2|screenshot-tap-doubler*"
  "lv2_tap_echo|delay|tap-echo.lv2|screenshot-tap-stereoecho*"
  # ZAM plugins
  "lv2_zamcomp|dynamics|ZamComp.lv2|screenshot-zamcomp*"
  "lv2_zamgate|dynamics|ZamGate.lv2|screenshot-zamgate*"
  "lv2_zamulticomp|dynamics|ZamCompX2.lv2|screenshot-zamulticomp*"
  "lv2_zameq2|filter|ZamEQ2.lv2|screenshot-zameq2*"
  "lv2_zamgeq31|filter|ZamGEQ31.lv2|screenshot-zamgeq31*"
  # Dragonfly reverbs (screenshot.png, not named)
  "lv2_dragonfly_hall|reverb|DragonflyHallReverb.lv2|screenshot.png"
  "lv2_dragonfly_room|reverb|DragonflyRoomReverb.lv2|screenshot.png"
  "lv2_dragonfly_plate|reverb|DragonflyPlateReverb.lv2|screenshot.png"
  "lv2_dragonfly_early|reverb|DragonflyEarlyReflections.lv2|screenshot.png"
  # MVerb / B-Reverb / Shiroverb / Roomy
  "lv2_mverb|reverb|MVerb.lv2|screenshot-mverb*"
  "lv2_b_reverb|reverb|b_reverb|screenshot-setbfree-organ-reverb*"
  "lv2_shiroverb|reverb|Shiroverb.lv2|screenshot-shiroverb*"
  "lv2_roomy|reverb|artyfx.lv2|screenshot-roomy*"
  # CAPS (separate bundle per plugin)
  "lv2_caps_plate|reverb|mod-caps-Plate.lv2|screenshot-c-plate*"
  "lv2_caps_platex2|reverb|mod-caps-PlateX2.lv2|screenshot-c-platex2*"
  "lv2_caps_scape|reverb|mod-caps-Scape.lv2|screenshot-c-scape*"
  "lv2_caps_autofilter|filter|mod-caps-AutoFilter.lv2|screenshot-c-autofilter*"
  "lv2_caps_phaser2|modulation|mod-caps-PhaserII.lv2|screenshot-c-phaser*"
  "lv2_caps_spice|gain|mod-caps-Spice.lv2|screenshot-c-spice*"
  "lv2_caps_spicex2|gain|mod-caps-SpiceX2.lv2|screenshot-c-spicex2*"
  # OJD / Wolf Shaper
  "lv2_ojd|gain|OJD.lv2|screenshot-ojd*"
  "lv2_wolf_shaper|gain|wolf-shaper.lv2|screenshot.png"
  # MDA plugins (separate bundle per plugin)
  "lv2_mda_overdrive|gain|mod-mda-Overdrive.lv2|screenshot-mda-overdrive*"
  "lv2_mda_degrade|gain|mod-mda-Degrade.lv2|screenshot-mda-degrade*"
  "lv2_mda_ambience|reverb|mod-mda-Ambience.lv2|screenshot-mda-ambience*"
  "lv2_mda_leslie|modulation|mod-mda-Leslie.lv2|screenshot-mda-leslie*"
  "lv2_mda_ringmod|modulation|mod-mda-RingMod.lv2|screenshot-mda-ringmod*"
  "lv2_mda_thruzero|modulation|mod-mda-ThruZero.lv2|screenshot-mda-thruzero*"
  "lv2_mda_dubdelay|delay|mod-mda-DubDelay.lv2|screenshot-mda-dubdelay*"
  "lv2_mda_detune|pitch|mod-mda-Detune.lv2|screenshot-mda-detune*"
  "lv2_mda_repsycho|pitch|mod-mda-RePsycho.lv2|screenshot-mda-repsycho*"
  # Autotune / Harmonizer
  "lv2_fat1_autotune|pitch|fat1.lv2|screenshot-x42-autotune*"
  # FOMP
  "lv2_fomp_cs_chorus|modulation|fomp.lv2|screenshot-cs-chorus-1*"
  "lv2_fomp_cs_phaser|modulation|fomp.lv2|screenshot-cs-phaser-1*"
  "lv2_fomp_autowah|filter|fomp.lv2|screenshot-auto-wah*"
  # ArtyFX
  "lv2_bitta|gain|artyfx-bitta.lv2|screenshot-bitta*"
  "lv2_driva|gain|artyfx-bad.lv2|screenshot-driva*"
  "lv2_satma|gain|artyfx-bad.lv2|screenshot-satma*"
  "lv2_artyfx_filta|filter|artyfx.lv2|screenshot-filta*"
  # Invada
  "lv2_invada_tube|gain|invada.lv2|screenshot-invada-tube-distortion-mono*"
  # Delay
  "lv2_avocado|delay|avocado.lv2|screenshot-avocado*"
  "lv2_floaty|delay|floaty.lv2|screenshot-floaty*"
  "lv2_bolliedelay|delay|bolliedelay.lv2|screenshot-bollie-delay*"
  "lv2_modulay|delay|Modulay.lv2|screenshot-modulay*"
  # Modulation
  "lv2_harmless|modulation|Harmless.lv2|screenshot-harmless*"
  "lv2_larynx|modulation|Larynx.lv2|screenshot-larynx*"
  # Filter
  "lv2_mod_hpf|filter|mod-utilities.lv2|screenshot-mod-hpf*"
  "lv2_mod_lpf|filter|mod-utilities.lv2|screenshot-mod-lpf*"
  # GX plugins
  "lv2_gx_ultracab|cab|gx_ultra_cab.lv2|screenshot-gx-ultracab*"
  "lv2_gx_blueamp|amp|gx_blueamp.lv2|screenshot-gx-blueamp*"
  "lv2_gx_supersonic|amp|gx_supersonic.lv2|screenshot-gx-supersonic*"
  "lv2_gx_quack|wah|gx_quack.lv2|screenshot-gxquack*"
)

copied=0
missing=0

for entry in "${MAPPINGS[@]}"; do
  IFS='|' read -r model_id effect_type bundle_glob screenshot_glob <<< "$entry"
  dest_dir="$OUT_DIR/$effect_type"
  dest_file="$dest_dir/$model_id.png"

  mkdir -p "$dest_dir"

  # Find the bundle directory
  bundle=$(find "$PLUGINS_DIR" -maxdepth 1 -name "$bundle_glob" -type d 2>/dev/null | head -1)
  if [ -z "$bundle" ]; then
    echo "SKIP  $model_id — bundle not found: $bundle_glob"
    ((missing++)) || true
    continue
  fi

  # Find the screenshot in modgui
  src=$(find "$bundle/modgui" -name "$screenshot_glob" -type f 2>/dev/null | head -1)
  if [ -z "$src" ]; then
    echo "SKIP  $model_id — screenshot not found in $bundle/modgui"
    ((missing++)) || true
    continue
  fi

  cp "$src" "$dest_file"
  echo "OK    $model_id -> $dest_file"
  ((copied++)) || true
done

echo ""
echo "Done: $copied copied, $missing skipped (no modgui screenshot)"
