#!/usr/bin/env bash
# Collect LV2 plugin screenshots from modgui into assets/blocks/screenshots/
# Usage: bash scripts/collect_screenshots.sh
set -euo pipefail

PLUGINS_DIR=".plugins/lv2"
OUT_DIR="assets/blocks/screenshots"

# Mapping: "model_id|effect_type|bundle_glob|screenshot_glob"
declare -a MAPPINGS=(
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
  "lv2_zamcomp|dynamics|ZaMultiComp.lv2|screenshot-zacomp*"
  "lv2_zamgate|dynamics|ZamGate.lv2|screenshot-zamgate*"
  "lv2_zamulticomp|dynamics|ZaMultiComp.lv2|screenshot-zamulticomp*"
  "lv2_zameq2|filter|ZamEQ2.lv2|screenshot-zameq2*"
  "lv2_zamgeq31|filter|ZamGEQ31.lv2|screenshot-zamgeq31*"
  "lv2_dragonfly_hall|reverb|DragonflyHallReverb.lv2|screenshot-dragonfly-hall*"
  "lv2_dragonfly_room|reverb|DragonflyRoomReverb.lv2|screenshot-dragonfly-room*"
  "lv2_dragonfly_plate|reverb|DragonflyPlateReverb.lv2|screenshot-dragonfly-plate*"
  "lv2_dragonfly_early|reverb|DragonflyEarlyReflections.lv2|screenshot-dragonfly-early*"
  "lv2_mverb|reverb|MVerb.lv2|screenshot-mverb*"
  "lv2_b_reverb|reverb|b_reverb|screenshot-setbfree-dsp*"
  "lv2_caps_plate|reverb|caps.lv2|screenshot-caps-plate*"
  "lv2_caps_platex2|reverb|caps.lv2|screenshot-caps-platex2*"
  "lv2_caps_scape|reverb|caps.lv2|screenshot-caps-scape*"
  "lv2_caps_autofilter|filter|caps.lv2|screenshot-caps-autofilter*"
  "lv2_caps_phaser2|modulation|caps.lv2|screenshot-caps-phaser2*"
  "lv2_caps_spice|gain|caps.lv2|screenshot-caps-spice*"
  "lv2_caps_spicex2|gain|caps.lv2|screenshot-caps-spicex2*"
  "lv2_ojd|gain|OJD.lv2|screenshot-ojd*"
  "lv2_wolf_shaper|gain|wolf-shaper.lv2|screenshot-wolf-shaper*"
  "lv2_mda_overdrive|gain|mda.lv2|screenshot-mda-overdrive*"
  "lv2_mda_degrade|gain|mda.lv2|screenshot-mda-degrade*"
  "lv2_mda_ambience|reverb|mda.lv2|screenshot-mda-ambience*"
  "lv2_mda_leslie|modulation|mda.lv2|screenshot-mda-leslie*"
  "lv2_mda_ringmod|modulation|mda.lv2|screenshot-mda-ringmod*"
  "lv2_mda_thruzero|modulation|mda.lv2|screenshot-mda-thruzero*"
  "lv2_mda_dubdelay|delay|mda.lv2|screenshot-mda-dubdelay*"
  "lv2_mda_combo|amp|mda.lv2|screenshot-mda-combo*"
  "lv2_mda_detune|pitch|mda.lv2|screenshot-mda-detune*"
  "lv2_mda_repsycho|pitch|mda.lv2|screenshot-mda-repsycho*"
  "lv2_ewham_harmonizer|pitch|infamousPlugins.lv2|screenshot-ewham-harmonizer*"
  "lv2_fat1_autotune|pitch|fat1.lv2|screenshot-fat1*"
  "lv2_fomp_cs_chorus|modulation|fomp.lv2|screenshot-fomp-cs-chorus*"
  "lv2_fomp_cs_phaser|modulation|fomp.lv2|screenshot-fomp-cs-phaser*"
  "lv2_fomp_autowah|filter|fomp.lv2|screenshot-fomp-autowah*"
  "lv2_bitta|gain|artyfx.lv2|screenshot-bitta*"
  "lv2_driva|gain|artyfx.lv2|screenshot-driva*"
  "lv2_satma|gain|artyfx.lv2|screenshot-satma*"
  "lv2_artyfx_filta|filter|artyfx.lv2|screenshot-filta*"
  "lv2_invada_tube|gain|invada_studio_plugins.lv2|screenshot-invada-tube*"
  "lv2_paranoia|gain|remaincalm.lv2|screenshot-paranoia*"
  "lv2_mud|filter|remaincalm.lv2|screenshot-mud*"
  "lv2_avocado|delay|avocado.lv2|screenshot-avocado*"
  "lv2_floaty|delay|floaty.lv2|screenshot-floaty*"
  "lv2_bolliedelay|delay|bolliedelay.lv2|screenshot-bolliedelay*"
  "lv2_modulay|delay|modulay.lv2|screenshot-modulay*"
  "lv2_harmless|modulation|harmless.lv2|screenshot-harmless*"
  "lv2_larynx|modulation|larynx.lv2|screenshot-larynx*"
  "lv2_shiroverb|reverb|shiroverb.lv2|screenshot-shiroverb*"
  "lv2_roomy|reverb|artyfx.lv2|screenshot-roomy*"
  "lv2_mod_hpf|filter|mod-utilities.lv2|screenshot-mod-hpf*"
  "lv2_mod_lpf|filter|mod-utilities.lv2|screenshot-mod-lpf*"
  "lv2_gx_ultracab|cab|gx_ultra_cab.lv2|screenshot-gx-ultracab*"
  "lv2_gx_blueamp|amp|gx_blueamp.lv2|screenshot-gx-blueamp*"
  "lv2_gx_supersonic|amp|gx_supersonic.lv2|screenshot-gx-supersonic*"
  "lv2_gx_quack|wah|gx_quack.lv2|screenshot-gx-quack*"
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
