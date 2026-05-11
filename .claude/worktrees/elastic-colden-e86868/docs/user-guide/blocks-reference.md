# Blocks Reference

OpenRig ships with hundreds of models across **16 block types**, powered by four distinct audio backends. This document is the reference for every block type and model available.

**Looking for the canonical `MODEL_ID` to use in a preset YAML?** See the [Model ID Quick Reference](#model-id-quick-reference) section below -- it lists every registered model ID grouped by block type. The per-section catalogs further down add descriptions, voicing variants, and parameter details.

> The Summary table at the end of this document still reflects historical counts and is being kept in sync separately (issue #202). The Quick Reference is the current source of truth for what is registered in the codebase right now.

## Audio Backends

| Backend    | Description                                                                                  |
|------------|----------------------------------------------------------------------------------------------|
| **Native** | Pure Rust DSP. Lowest latency, lowest CPU usage. Parameters are fully controllable in real time. |
| **NAM**    | Neural Amp Modeler. Capture-based modeling that reproduces realistic amp and pedal tones. Higher CPU usage than Native. |
| **IR**     | Impulse Response. Convolution-based speaker and body simulation. Produces a fixed frequency response shaped by the loaded impulse. |
| **LV2**    | Open-source audio plugins. The largest backend with 105 bundled plugins, extending the effects library with community-developed processors across all block types. |

---

<!-- AUTO_GENERATED FROM SOURCE: see Quick Reference section in this doc -->
## Model ID Quick Reference

Every model in the catalog is registered with a canonical `MODEL_ID` string. **This is the value you put in the `model:` field of a preset YAML.** Tables below list every registered ID, grouped by block type. The full per-section catalogs (with descriptions, params, voicing variants) follow further down.

If you find a model in the codebase that is **not** listed here, that is a doc bug -- open an issue. These lists are extracted from `pub const MODEL_ID:` declarations in each `block-*` crate.

### Preamp

| Model ID | Display Name | Brand |
|---|---|---|
| `american_clean` | American Clean | -- |
| `brit_crunch` | Brit Crunch | -- |
| `marshall_jcm_800_2203` | JCM 800 2203 | marshall |
| `modern_high_gain` | Modern High Gain | -- |
| `nam_ada_mp_1` | MP-1 | ada |
| `nam_award_session_blues_baby_22` | Blues Baby 22 | award-session |
| `nam_blackstar_fly` | Fly | blackstar |
| `nam_diezel_vh4` | VH4 | diezel |
| `nam_diezel_zerrer` | Zerrer | diezel |
| `nam_ehx_22_caliber` | 22 Caliber | electro-harmonix |
| `nam_ehx_mig50` | MIG-50 | electro-harmonix |
| `nam_engl_e530` | E530 | engl |
| `nam_engl_thunder_50` | Thunder 50 | engl |
| `nam_fender_57_champ` | '57 Custom Champ | fender |
| `nam_fender_57_deluxe` | '57 Custom Deluxe | fender |
| `nam_fender_frontman_15g` | Frontman 15G | fender |
| `nam_fender_pa100` | PA100 | fender |
| `nam_fortin_meshuggah_preamp` | Meshuggah Preamp | fortin |
| `nam_joyo_bantamp_meteor` | Bantamp Meteor | joyo |
| `nam_koch_multitone_50` | Multitone 50 | koch |
| `nam_lab_series_l2` | L2 | lab-series |
| `nam_marshall_avt50h` | AVT50H | marshall |
| `nam_marshall_jmp1` | JMP-1 | marshall |
| `nam_marshall_yjm100` | YJM100 | marshall |
| `nam_mesa_mark_iii` | Mark III | mesa |
| `nam_mesa_studio_preamp` | Studio Preamp | mesa |
| `nam_mesa_triaxis` | Triaxis | mesa |
| `nam_orange_micro_terror` | Micro Terror | orange |
| `nam_panama_shaman` | Shaman | panama |
| `nam_peavey_classic_30` | Classic 30 | peavey |
| `nam_sovtek_mig100` | MIG-100 KT88 | sovtek |
| `nam_synergy_bogner_ecstasy` | Bogner Ecstasy Module | synergy |
| `nam_synergy_dumble_os` | Dumble OS Module | synergy |
| `nam_synergy_friedman_ds` | Friedman DS Module | synergy |
| `nam_synergy_syn1` | SYN-1 Chassis | synergy |
| `nam_synergy_tone_king_imperial_mkii` | Tone King Imperial MKII Module | synergy |
| `nam_tone_king_imperial_preamp` | Imperial Preamp | tone_king |
| `nam_victory_vx_kraken` | VX Kraken | victory |
| `nam_zt_lunchbox_jr` | Lunchbox Jr | zt |

### Amp

| Model ID | Display Name | Brand |
|---|---|---|
| `ampeg_svt_classic` | SVT Classic | ampeg |
| `blackface_clean` | Blackface Clean | -- |
| `bogner_ecstasy` | Ecstasy | bogner |
| `chime` | Chime | -- |
| `dover_da50_mesa` | DA-50 + Mesa 4×12 | dover |
| `fender_bassman_1971` | Bassman 1971 | fender |
| `fender_deluxe_reverb_65` | Deluxe Reverb '65 | fender |
| `fender_super_reverb_1977` | Super Reverb 1977 | fender |
| `friedman_be100_deluxe` | BE100 Deluxe | friedman |
| `lv2_gx_blueamp` | GxBlueAmp | guitarix |
| `lv2_gx_supersonic` | GxSupersonic | guitarix |
| `lv2_mda_combo` | MDA Combo | mda |
| `marshall_jmp_1_full_rig` | JMP-1 Full Rig | marshall |
| `marshall_jmp_1_head` | JMP-1 Head | marshall |
| `marshall_super_100_1966` | Super 100 1966 | marshall |
| `nam_ampeg_svt` | SVT | ampeg |
| `nam_bad_cat_lynx` | bad cat lynx | bad_cat |
| `nam_badcat_wildcat_40r` | WildCat 40R | bad_cat |
| `nam_bogner_ecstasy_101b` | Ecstasy 101B | bogner |
| `nam_bogner_goldfinger` | Goldfinger | bogner |
| `nam_bogner_helios` | Helios | bogner |
| `nam_bogner_shiva` | Shiva | bogner |
| `nam_bogner_uberschall` | Uberschall | bogner |
| `nam_bugera_1960` | Bugera 1960 | bugera |
| `nam_ceriatone_king_kong` | Kong | ceriatone |
| `nam_ceriatone_ots_mini_20` | OTS Mini 20 | ceriatone |
| `nam_diezel_hagen` | Hagen | diezel |
| `nam_diezel_herbert` | Herbert | diezel |
| `nam_driftwood_purple_nightmare` | Purple Nightmare | driftwood |
| `nam_dumble` | Dumble ODS | dumble |
| `nam_dumble_ods_100w` | ODS 100W | dumble |
| `nam_dumble_steel_string_singer` | Steel String Singer | dumble |
| `nam_eden_e300t` | E300T | eden |
| `nam_engl_e305_gigmaster_30_head` | E305 Gigmaster 30 Head | engl |
| `nam_engl_e530_preamp` | E530 Preamp | engl |
| `nam_engl_fireball` | Fireball | engl |
| `nam_engl_ironball` | Ironball | engl |
| `nam_engl_powerball` | Powerball | engl |
| `nam_engl_powerball_ii` | Powerball II | engl |
| `nam_evh_5150` | 5150 | evh |
| `nam_evh_5150_iii` | 5150 III | evh |
| `nam_evh_5150_iii_50w_red_channel` | 5150 III 50w Red Channel | evh |
| `nam_evh_5150iiis_el34_100w` | 5150IIIS EL34 100W | evh |
| `nam_fender_57_custom_deluxe` | 57 Custom Deluxe | fender |
| `nam_fender_bassman` | Bassman | fender |
| `nam_fender_blues_junior` | Blues Junior | fender |
| `nam_fender_deluxe_reverb` | Deluxe Reverb | fender |
| `nam_fender_hot_rod_deluxe` | Hot Rod Deluxe | fender |
| `nam_fender_princeton_reverb` | Princeton Reverb | fender |
| `nam_fender_princeton_reverb_1972` | Princeton Reverb 1972 | fender |
| `nam_fender_showman` | Showman | fender |
| `nam_fender_twin_reverb` | Twin Reverb | fender |
| `nam_fortin_33` | 33 | fortin |
| `nam_friedman_be100` | BE 100 | friedman |
| `nam_friedman_be_100` | BE 100 | friedman |
| `nam_friedman_be_50` | BE-50 | friedman |
| `nam_friedman_dirty_shirley` | Dirty Shirley | friedman |
| `nam_friedman_jose_arredondo` | Jose Arredondo | friedman |
| `nam_friedman_ss_100_v1` | SS 100 V1 | friedman |
| `nam_hiwatt_dr103` | HiWatt DR103 | hiwatt |
| `nam_hiwatt_super_hi_50` | Super-Hi 50 | hiwatt |
| `nam_hotone_heart_attack` | Hotone Heart Attack | hotone |
| `nam_hughes_and_kettner_tubemeister` | and TubeMeister | hughes_kettner |
| `nam_hughes_kettner_tubemeister_18` | Hughes Kettner TubeMeister 18 | hughes |
| `nam_jet_city_jca22h` | Jet City - JCA22H | jet |
| `nam_lab_series_l5` | L5 | lab_series |
| `nam_laney` | Laney | laney |
| `nam_laney_ironheart_irt120` | Ironheart IRT120 | laney |
| `nam_laney_ironheart_irt60h` | Ironheart IRT60H | laney |
| `nam_laney_vh100r` | VH100R | laney |
| `nam_marshall_1959_slp` | 1959 SLP | marshall |
| `nam_marshall_1959hw` | 1959HW | marshall |
| `nam_marshall_1959hw_handwired` | 1959HW Handwired | marshall |
| `nam_marshall_1987` | 1987 | marshall |
| `nam_marshall_6100_30th_anniversary` | 6100 - 30th anniversary | marshall |
| `nam_marshall_dsl40cr` | DSL40CR | marshall |
| `nam_marshall_dsl_20_hr` | DSL 20 HR | marshall |
| `nam_marshall_jcm2000_dsl` | JCM2000 DSL | marshall |
| `nam_marshall_jcm2000_tsl` | JCM2000 TSL | marshall |
| `nam_marshall_jcm900` | JCM900 | marshall |
| `nam_marshall_jcm_800` | JCM 800 | marshall |
| `nam_marshall_jmp_2203` | JMP 2203 | marshall |
| `nam_marshall_jtm45` | JTM45 | marshall |
| `nam_marshall_jtm50_hw_plexi` | JTM50 HW (Plexi) | marshall |
| `nam_marshall_jvm` | JVM | marshall |
| `nam_marshall_lead_12` | Lead 12 | marshall |
| `nam_marshall_plexi` | Plexi | marshall |
| `nam_marshall_plexi_50w` | Plexi 50W | marshall |
| `nam_marshall_super_lead` | Super Lead | marshall |
| `nam_matchless_clubman_35_head` | Matchless Clubman 35 head | matchless |
| `nam_mesa_badlander` | Badlander | mesa |
| `nam_mesa_boogie_badlander_50` | Badlander 50 | mesa |
| `nam_mesa_boogie_dc_5` | DC-5 | mesa |
| `nam_mesa_boogie_dual_rectifier` | Dual Rectifier | mesa |
| `nam_mesa_boogie_jp2c` | JP2C | mesa |
| `nam_mesa_boogie_mark_iic` | Mark IIC | mesa |
| `nam_mesa_boogie_mark_iv` | Mark IV | mesa |
| `nam_mesa_boogie_mark_vii` | Mark VII | mesa |
| `nam_mesa_dual_rectifier_multiwatt` | Dual Rectifier Multi-Watt | mesa |
| `nam_mesa_dual_rectifier_rev_f` | Dual Rectifier Rev F | mesa |
| `nam_mesa_mark_iv` | Mark IV | mesa |
| `nam_mesa_mark_v` | Mark V | mesa |
| `nam_mesa_rectifier` | Rectifier | mesa |
| `nam_mesa_triple_rectifier` | Triple Rectifier | mesa |
| `nam_mesaboogie_290_simul_class` | MesaBoogie 290 Simul-Class | mesaboogie |
| `nam_omega_ampworks_granophyre` | Omega Ampworks Granophyre | omega |
| `nam_orange_dark_terror` | Dark Terror | orange |
| `nam_orange_or15` | OR15 | orange |
| `nam_orange_rockerverb` | Rockerverb | orange |
| `nam_orange_rockerverb_mk3` | Rockerverb MK3 | orange |
| `nam_orange_tiny_terror` | Tiny Terror | orange |
| `nam_peavey_5150` | 5150 | peavey |
| `nam_peavey_6505` | 6505 | peavey |
| `nam_peavey_6505_plus` | 6505 Plus | peavey |
| `nam_peavey_classic_50` | Classic 50 | peavey |
| `nam_peavey_jsx` | JSX | peavey |
| `nam_peavey_xxx` | XXX | peavey |
| `nam_prs_archon` | Archon | prs |
| `nam_prs_mt100_tremonti` | MT-100 Tremonti | prs |
| `nam_prs_mt15` | MT15 | prs |
| `nam_randall_rg100es` | Randall RG100es | randall |
| `nam_randall_warhead` | Warhead | randall |
| `nam_revv_generator_120_mkiii` | Revv Generator 120 MKIII | revv |
| `nam_rocktron` | Rocktron | rocktron |
| `nam_soldano_slo_100` | SLO 100 | soldano |
| `nam_soldano_slo_30` | slo 30 | soldano |
| `nam_splawn_quickrod` | Quickrod | splawn |
| `nam_sunn_model_t` | Sunn Model T | sunn |
| `nam_supro_black_magick` | Supro Black Magick | supro |
| `nam_synergy_soldano` | Synergy Soldano | synergy |
| `nam_tone_king_imperial` | Imperial | tone_king |
| `nam_two_rock_studio_signature` | Two Rock Studio Signature | two |
| `nam_vox_ac15` | AC15 | vox |
| `nam_vox_night_train` | Night Train | vox |
| `nam_wizard_hellrazor` | Wizard HellRazor | wizard |
| `peavey_5150_mesa_4x12` | 5150 + Mesa 4×12 | peavey |
| `roland_jc_120b_jazz_chorus` | JC-120B Jazz Chorus | roland |
| `synergy_drect_mesa` | DRECT Mesa | synergy |
| `tweed_breakup` | Tweed Breakup | -- |
| `vox_ac30` | AC30 | vox |
| `vox_ac30_1961_fawn_ef86` | AC30 '61 Fawn EF86 | vox |

### Cab

| Model ID | Display Name | Brand |
|---|---|---|
| `american_2x12` | American 2x12 | -- |
| `ampeg_svt_8x10` | SVT 4x10/8x10 | ampeg |
| `brit_4x12` | Brit 4x12 | -- |
| `cel_cream_4x12` | Celestion Cream 4x12 | celestion |
| `engl_e412` | E412 Karnivore | engl |
| `evh_5150iii_4x12` | 5150III 4x12 G12-EVH | evh |
| `evil_chug_blackstar_prs` | Evil Chug (Blackstar + PRS) | blackstar |
| `fender_bassman_2x15` | Bassman 2x15 CTS | fender |
| `fender_deluxe_reverb_oxford` | Deluxe Reverb Oxford | fender |
| `fender_super_reverb_4x10` | Super Reverb 4x10 | fender |
| `fender_twin_reverb_2x12` | Twin Reverb 2x12 | fender |
| `g12m_greenback_2x12` | G12M Greenback 2x12 | celestion |
| `g12m_greenback_multi_mic` | G12M Greenback Multi-Mic | celestion |
| `g12t_75_4x12` | G12T-75 4x12 | celestion |
| `lv2_gx_ultracab` | GxUltraCab | guitarix |
| `marshall_1960av_4x12` | 1960AV 4x12 | marshall |
| `marshall_1960bv_4x12` | 1960BV 4x12 | marshall |
| `marshall_1960tv_greenback` | 1960TV Greenback | marshall |
| `marshall_4x12_v30` | 4x12 V30 | marshall |
| `mesa_os_4x12_v30` | Oversized 4x12 V30 | mesa |
| `mesa_recto_v30` | Recto V30 | mesa |
| `mesa_standard_4x12_v30` | Standard 4x12 V30 | mesa |
| `mesa_traditional_4x12_v30` | Traditional 4x12 V30 | mesa |
| `orange_2x12_v30` | Orange 2x12 V30 | orange |
| `roland_jc_120_cab` | JC-120 Cab | roland |
| `v30_4x12` | V30 4x12 | celestion |
| `vintage_1x12` | Vintage 1x12 | -- |
| `vox_ac30_blue` | AC30 Blue | vox |
| `vox_ac50_2x12` | AC50 2x12 Goodmans | vox |

### Gain

| Model ID | Display Name | Brand |
|---|---|---|
| `ampeg_scr_di` | SCR-DI | ampeg |
| `behringer_sf300` | SF300 Super Fuzz | behringer |
| `blues_overdrive_bd_2` | Blues Driver BD-2 | boss |
| `bluesbreaker` | BluesBreaker | marshall |
| `boss_ds1` | DS-1 Distortion | boss |
| `boss_ds1_wampler` | DS-1 Wampler JCM Mod | boss |
| `boss_fz1w` | FZ-1W Fuzz | boss |
| `boss_hm2_1986` | Heavy Metal HM-2 '86 | boss |
| `boss_hm2_mij` | Heavy Metal HM-2 MiJ | boss |
| `cc_boost` | CC Boost | custom |
| `darkglass_alpha_omega` | Alpha Omega Ultra | darkglass |
| `darkglass_b7k` | B7K Ultra | darkglass |
| `demonfx_be_od` | BE-OD Clone | demonfx |
| `fulltone_ocd` | OCD Overdrive | fulltone |
| `fulltone_ocd_v15` | OCD v1.5 | fulltone |
| `grind` | Grind | tc-electronic |
| `hm2` | Heavy Metal HM-2 | boss |
| `ibanez_ts808` | Tube Screamer TS808 | ibanez |
| `ibanez_ts9` | TS9 Tube Screamer | ibanez |
| `jhs_bonsai` | Bonsai (9 TS) | jhs |
| `klon_centaur` | Klon Centaur Silver | klon |
| `klone` | Klone | electro-harmonix |
| `lokajaudio_der_blend` | Der Blend | lokajaudio |
| `lokajaudio_doom_machine` | Doom Machine V3 | lokajaudio |
| `lv2_bitta` | Bitta | artyfx |
| `lv2_caps_spice` | Spice | caps |
| `lv2_caps_spicex2` | Spice X2 | caps |
| `lv2_driva` | Driva | artyfx |
| `lv2_gx_axisface` | Axis Face | guitarix |
| `lv2_gx_bajatubedriver` | BaJa Tube Driver | guitarix |
| `lv2_gx_boobtube` | Boob Tube | guitarix |
| `lv2_gx_bottlerocket` | Bottle Rocket | guitarix |
| `lv2_gx_clubdrive` | Club Drive | guitarix |
| `lv2_gx_creammachine` | Cream Machine | guitarix |
| `lv2_gx_dop250` | DOP 250 | guitarix |
| `lv2_gx_epic` | Epic | guitarix |
| `lv2_gx_eternity` | Eternity | guitarix |
| `lv2_gx_fz1b` | Maestro FZ-1B | guitarix |
| `lv2_gx_fz1s` | Maestro FZ-1S | guitarix |
| `lv2_gx_guvnor` | Guvnor | guitarix |
| `lv2_gx_hotbox` | Hot Box | guitarix |
| `lv2_gx_hyperion` | Hyperion | guitarix |
| `lv2_gx_knightfuzz` | Knight Fuzz | guitarix |
| `lv2_gx_liquiddrive` | Liquid Drive | guitarix |
| `lv2_gx_luna` | Luna | guitarix |
| `lv2_gx_microamp` | Micro Amp | guitarix |
| `lv2_gx_saturator` | Saturator | guitarix |
| `lv2_gx_sd1` | SD-1 | guitarix |
| `lv2_gx_sd2lead` | SD-2 Lead | guitarix |
| `lv2_gx_shakatube` | Shaka Tube | guitarix |
| `lv2_gx_sloopyblue` | Sloopy Blue | guitarix |
| `lv2_gx_sunface` | Sun Face | guitarix |
| `lv2_gx_superfuzz` | Super Fuzz | guitarix |
| `lv2_gx_suppatonebender` | Suppa Tone Bender | guitarix |
| `lv2_gx_timray` | Tim Ray | guitarix |
| `lv2_gx_tonemachine` | Tone Machine | guitarix |
| `lv2_gx_tubedistortion` | Tube Distortion | guitarix |
| `lv2_gx_valvecaster` | Valve Caster | guitarix |
| `lv2_gx_vintagefuzzmaster` | Vintage Fuzz Master | guitarix |
| `lv2_gx_vmk2` | Vmk2 | guitarix |
| `lv2_gx_voodofuzz` | Voodo Fuzz | guitarix |
| `lv2_invada_tube` | Invada Tube | invada |
| `lv2_mda_degrade` | MDA Degrade | mda |
| `lv2_mda_overdrive` | MDA Overdrive | mda |
| `lv2_ojd` | OJD | schrammel |
| `lv2_paranoia` | Paranoia | remaincalm |
| `lv2_satma` | Satma | artyfx |
| `lv2_tap_sigmoid` | TAP Sigmoid Booster | tap |
| `lv2_tap_tubewarmth` | TAP Tubewarmth | tap |
| `lv2_wolf_shaper` | Wolf Shaper | wolf |
| `maxon_od808` | OD808 Overdrive | maxon |
| `metal_zone` | Metal Zone MT-2 | boss |
| `mxr_gt_od` | GT-OD (Zakk Wylde) | mxr |
| `nam_analog_man_sun_face` | Analog Man Sun Face | analogman |
| `nam_benson_preamp` | Benson Preamp | benson |
| `nam_big_muff` | Big Muff | ehx |
| `nam_boss_ds_2` | Boss DS-2 | boss |
| `nam_boss_fz_2_hyper_fuzz` | Boss FZ-2 Hyper Fuzz | boss |
| `nam_boss_hm_3` | Boss HM-3 | boss |
| `nam_boss_mt_2_metal_zone` | Boss MT-2 Metal Zone | boss |
| `nam_boss_od_3` | Boss OD-3 | boss |
| `nam_boss_odb3_bass` | Boss ODB3 Bass | boss |
| `nam_browne_protein` | Browne Protein | browne |
| `nam_ceriatone_centura` | Ceriatone Centura | ceriatone |
| `nam_chase_bliss_automatone` | Chase Bliss Automatone | chase_bliss |
| `nam_dallas_rangemaster` | Dallas Rangemaster | dallas |
| `nam_death_by_audio_fuzz_war` | Death By Audio Fuzz War | death_by_audio |
| `nam_digitech_bad_monkey` | DigiTech Bad Monkey | digitech |
| `nam_digitech_grunge` | DigiTech Grunge | digitech |
| `nam_dod_250_overdrive` | DOD 250 Overdrive | dod |
| `nam_earthquaker_acapulco_gold` | EarthQuaker Acapulco Gold | earthquaker_devices |
| `nam_earthquaker_hizumitas` | EarthQuaker Hizumitas | earthquaker |
| `nam_earthquaker_plumes` | EarthQuaker Plumes | earthquaker |
| `nam_earthquaker_talons` | EarthQuaker Talons | earthquaker_devices |
| `nam_ehx_metal_muff` | EHX Metal Muff | ehx |
| `nam_ehx_soul_food` | EHX Soul Food | ehx |
| `nam_foxx_tone_machine` | Foxx Tone Machine | foxx |
| `nam_fulltone_69` | Fulltone 69 | fulltone |
| `nam_fulltone_full_drive_2` | Fulltone Full-Drive 2 | fulltone |
| `nam_fuzz_face` | Fuzz Face | dunlop |
| `nam_hermida_zendrive` | Hermida Zendrive | hermida |
| `nam_horizon_precision_drive` | Horizon Precision Drive | horizon_devices |
| `nam_hudson_broadcast` | Hudson Broadcast | hudson |
| `nam_ibanez_ts9` | TS9 Tube Screamer (NAM) | ibanez |
| `nam_ibanez_tube_screamer` | Ibanez Tube Screamer | fortin |
| `nam_jhs_andy_timmons` | Andy Timmons | jhs |
| `nam_jhs_angry_charlie` | JHS Angry Charlie | jhs |
| `nam_jhs_morning_glory` | JHS Morning Glory | jhs |
| `nam_ksr_ceres` | KSR Ceres | ksr |
| `nam_maestro_fuzz_tone_fz_1` | Maestro Fuzz-Tone FZ-1 | maestro |
| `nam_marshall_guvnor` | Marshall Guvnor | marshall |
| `nam_maxon_od_9` | Maxon OD-9 | maxon |
| `nam_mesa_throttle_box` | Mesa Throttle Box | mesa |
| `nam_mxr_classic_108_fuzz` | MXR Classic 108 Fuzz | mxr |
| `nam_mxr_distortion` | MXR Distortion+ | mxr |
| `nam_mxr_duke_of_tone` | MXR Duke of Tone | mxr |
| `nam_mxr_evh_5150_overdrive` | MXR EVH 5150 Overdrive | mxr |
| `nam_mxr_micro_amp` | MXR Micro Amp | mxr |
| `nam_nobels_odr_1` | Nobels ODR-1 | nobels |
| `nam_paul_cochrane_timmy` | Paul Cochrane Timmy | mxr |
| `nam_pete_cornish_g_2` | Pete Cornish G-2 | pete_cornish |
| `nam_pete_cornish_ss_3` | Pete Cornish SS-3 | pete_cornish |
| `nam_sadowsky_sbp_2_bass` | Sadowsky SBP-2 Bass | sadowsky |
| `nam_strymon_riverside` | Strymon Riverside | strymon |
| `nam_strymon_sunset` | Strymon Sunset | strymon |
| `nam_suhr_riot` | Suhr Riot | suhr |
| `nam_tech_21_sansamp_bddi` | Tech 21 SansAmp BDDI | sansamp |
| `nam_thorpyfx_peacekeeper` | ThorpyFX Peacekeeper | thorpy_fx |
| `nam_tone_bender` | Tone Bender | boss |
| `nam_tube_driver` | Tube Driver | tube |
| `nam_wampler_tumnus` | Wampler Tumnus | wampler |
| `nam_warm_audio_centavo` | Warm Audio Centavo | warm_audio |
| `nam_way_huge_pork_pickle` | Way Huge Pork Pickle | way_huge |
| `nam_way_huge_swollen_pickle` | Way Huge Swollen Pickle | boss |
| `nam_xotic_sl_drive` | Xotic SL Drive | xotic |
| `nam_zvex_box_of_rock` | ZVEX Box of Rock | zvex |
| `nam_zvex_fuzz_factory` | ZVEX Fuzz Factory | zvex |
| `pot_boost` | PoT Boost | pot |
| `pot_od` | PoT OD | pot |
| `proco_rat` | RAT | proco |
| `proco_rat2` | RAT 2 | proco |
| `rod10_ds1` | ROD-10 DS1 | boss |
| `rod10_sd1` | ROD-10 SD1 | boss |
| `rr_golden_clone` | Golden Clone | klon |
| `sansamp_di_2112` | SansAmp DI-2112 | tech21 |
| `slammin_booster` | Slammin Clean Booster | jhs |
| `tascam_424` | Tascam 424 Preamp | tascam |
| `tc_spark` | Spark | tc-electronic |
| `tcip` | TCIP | tc-electronic |
| `tech21_steve_harris` | Steve Harris SH-1 | tech21 |
| `velvet_katana` | Velvet Katana | velvet |
| `vemuram_jan_ray` | Jan Ray | vemuram |
| `volume` | Volume | -- |
| `vst3_chow_tape` | CHOW Tape | chow-dsp |

### Delay

| Model ID | Display Name | Brand |
|---|---|---|
| `analog_warm` | Analog Warm Delay | -- |
| `digital_clean` | Digital Clean Delay | -- |
| `lv2_avocado` | Avocado | remaincalm |
| `lv2_bolliedelay` | Bollie Delay | bollie |
| `lv2_floaty` | Floaty | remaincalm |
| `lv2_mda_dubdelay` | MDA DubDelay | mda |
| `lv2_modulay` | Modulay | shiro |
| `lv2_tap_doubler` | TAP Doubler | tap |
| `lv2_tap_echo` | TAP Stereo Echo | tap |
| `lv2_tap_reflector` | TAP Reflector | tap |
| `modulated_delay` | Modulated Delay | -- |
| `reverse` | Reverse Delay | -- |
| `slapback` | Slapback Delay | -- |
| `tape_vintage` | Tape Vintage Delay | -- |
| `vst3_cocoa_delay` | Cocoa Delay | klevgrand |

### Reverb

| Model ID | Display Name | Brand |
|---|---|---|
| `hall` | Hall Reverb | -- |
| `lv2_b_reverb` | B Reverb | setbfree |
| `lv2_caps_plate` | CAPS Plate | caps |
| `lv2_caps_platex2` | CAPS Plate X2 | caps |
| `lv2_caps_scape` | CAPS Scape | caps |
| `lv2_dragonfly_early` | Dragonfly Early Reflections | dragonfly |
| `lv2_dragonfly_hall` | Dragonfly Hall Reverb | dragonfly |
| `lv2_dragonfly_plate` | Dragonfly Plate Reverb | dragonfly |
| `lv2_dragonfly_room` | Dragonfly Room Reverb | dragonfly |
| `lv2_floaty` | Floaty | remaincalm |
| `lv2_mda_ambience` | MDA Ambience | mda |
| `lv2_mverb` | MVerb | distrho |
| `lv2_roomy` | Roomy | openav |
| `lv2_shiroverb` | Shiroverb | shiro |
| `lv2_tap_reflector` | TAP Reflector | tap |
| `lv2_tap_reverb` | TAP Reverberator | tap |
| `plate_foundation` | Plate Foundation Reverb | -- |
| `room` | Room Reverb | -- |
| `spring` | Spring Reverb | -- |
| `vst3_cloud_seed` | Cloud Seed | cloud-seed |

### Modulation

| Model ID | Display Name | Brand |
|---|---|---|
| `classic_chorus` | Classic Chorus | -- |
| `ensemble_chorus` | Ensemble Chorus | -- |
| `lv2_caps_phaser2` | CAPS Phaser II | caps |
| `lv2_fomp_cs_chorus` | CS Chorus | fomp |
| `lv2_fomp_cs_phaser` | CS Phaser | fomp |
| `lv2_harmless` | Harmless | shiro |
| `lv2_larynx` | Larynx | shiro |
| `lv2_mda_leslie` | MDA Leslie | mda |
| `lv2_mda_ringmod` | MDA RingMod | mda |
| `lv2_mda_thruzero` | MDA ThruZero | mda |
| `lv2_tap_chorus_flanger` | TAP Chorus/Flanger | tap |
| `lv2_tap_rotspeak` | TAP Rotary Speaker | tap |
| `lv2_tap_tremolo` | TAP Tremolo | tap |
| `stereo_chorus` | Stereo Chorus | -- |
| `tremolo_sine` | Sine Tremolo | -- |
| `vibrato` | Vibrato | -- |
| `vst3_stone_mistress` | Stone Mistress | stone-mistress |

### Dynamics

| Model ID | Display Name | Brand |
|---|---|---|
| `compressor_studio_clean` | Studio Clean Compressor | -- |
| `gate_basic` | Noise Gate | -- |
| `limiter_brickwall` | Brick Wall Limiter | -- |
| `lv2_tap_deesser` | TAP DeEsser | tap |
| `lv2_tap_dynamics` | TAP Dynamics | tap |
| `lv2_tap_limiter` | TAP Scaling Limiter | tap |
| `lv2_zamcomp` | ZamComp | zam |
| `lv2_zamgate` | ZamGate | zam |
| `lv2_zamulticomp` | ZaMultiComp | zam |
| `vst3_squeezer` | Squeezer | squeezer |

### Filter

| Model ID | Display Name | Brand |
|---|---|---|
| `eq_eight_band_parametric` | 8-Band Parametric EQ | -- |
| `eq_three_band_basic` | Three Band EQ | -- |
| `lv2_artyfx_filta` | Filta | openav |
| `lv2_caps_autofilter` | AutoFilter | caps |
| `lv2_fomp_autowah` | Auto-Wah | fomp |
| `lv2_mod_hpf` | MOD High Pass | mod |
| `lv2_mod_lpf` | MOD Low Pass | mod |
| `lv2_mud` | Mud | remaincalm |
| `lv2_tap_equalizer` | TAP Equalizer | tap |
| `lv2_tap_equalizer_bw` | TAP Equalizer/BW | tap |
| `lv2_zameq2` | ZamEQ2 | zam |
| `lv2_zamgeq31` | ZamGEQ31 | zam |
| `native_guitar_eq` | Guitar EQ | -- |
| `vst3_modeq` | modEQ | modeq |

### Wah

| Model ID | Display Name | Brand |
|---|---|---|
| `cry_classic` | Cry Classic | -- |
| `lv2_gx_quack` | GxQuack | guitarix |

### Pitch

| Model ID | Display Name | Brand |
|---|---|---|
| `lv2_ewham_harmonizer` | Harmonizer | infamous |
| `lv2_fat1_autotune` | x42 Autotune | x42 |
| `lv2_mda_detune` | MDA Detune | mda |
| `lv2_mda_repsycho` | MDA RePsycho! | mda |

### Body, IR Loader, NAM Loader

- **Body**: 114 brand-organized acoustic body IRs (Martin, Taylor, Gibson, Yamaha, Takamine, Guild, Ibanez, Ovation, Rainsong, Lowden, classical, vintage). Each canonical `MODEL_ID` is the body capture's snake_case identifier -- see `crates/block-body/src/` for the full list.
- **IR Loader**: `generic_ir` (single model, user-supplied IR file via `path:` parameter).
- **NAM Loader**: `generic_nam` (single model, user-supplied NAM capture via `path:` parameter).

---


## Preamp

A preamp block shapes the guitar signal before it reaches the power amp stage. It controls gain structure, EQ voicing, and tonal character. Preamps set the foundation for everything downstream.

### Models

| Model Name              | Brand    | Backend | Description                     |
|-------------------------|----------|---------|---------------------------------|
| American Clean          | --       | Native  | Clean American-style preamp     |
| Brit Crunch             | --       | Native  | British crunch preamp           |
| Modern High Gain        | --       | Native  | Modern high-gain preamp         |
| Marshall JCM 800 2203   | Marshall          | NAM     | Classic British crunch/gain                            |
| Diezel VH4              | Diezel            | NAM     | Modern high-gain German amp                            |
| Thunder 50              | ENGL              | NAM     | Tight German high-gain lead amp                        |
| '57 Custom Champ        | Fender            | NAM     | Small vintage tweed combo, clean to light breakup      |
| '57 Custom Deluxe       | Fender            | NAM     | Vintage tweed combo with warm, full breakup character  |
| Frontman 15G            | Fender            | NAM     | Solid-state practice amp, clean and gain channels      |
| PA100                   | Fender            | NAM     | Vintage PA head repurposed as a clean guitar amp       |
| Bantamp Meteor          | Joyo              | NAM     | Compact mini-head with a wide range of gain voicings   |
| AVT50H                  | Marshall          | NAM     | Hybrid head, modern high-gain focused                  |
| YJM100                  | Marshall          | NAM     | Yngwie signature, classic JCM800 character with boost  |
| Mark III                | Mesa/Boogie       | NAM     | Tight, percussive Mesa with multiple EQ modes          |
| Micro Terror            | Orange            | NAM     | Tiny all-valve head, warm Orange crunch and saturation |
| Shaman                  | Panama            | NAM     | Versatile amp spanning clean through high-gain         |
| Classic 30              | Peavey            | NAM     | EL84-based combo, clean and slightly pushed tones      |
| MIG-100 KT88            | Sovtek            | NAM     | Russian power-amp character, raw and punchy            |
| VX Kraken               | Victory           | NAM     | Aggressive high-gain head, shred-oriented voicing      |
| MIG-50                  | Electro-Harmonix  | NAM     | Boutique 50W head, clean through overdrive range       |
| 22 Caliber              | Electro-Harmonix  | NAM     | Low-wattage head with clean and crunch tones           |
| Blues Baby 22           | Award-Session     | NAM     | British-influenced 22W combo, clean to overdrive       |
| Fly                     | Blackstar         | NAM     | Ultra-compact amp, clean and crunch tones              |
| Multitone 50            | Koch              | NAM     | Dutch 50W amp with clean, crunch, and OD channels      |
| L2                      | Lab Series        | NAM     | Solid-state Lab Series, clean with unique filtering    |
| Lunchbox Jr             | ZT                | NAM     | Compact 35W solid-state, clean through overdrive       |

### Parameters -- Native Preamp

| Parameter | Range    | Default | Description              |
|-----------|----------|---------|--------------------------|
| input     | 0--100%  | --      | Input level              |
| gain      | 0--100%  | --      | Preamp gain              |
| bass      | 0--100%  | --      | Low-frequency EQ         |
| middle    | 0--100%  | --      | Mid-frequency EQ         |
| treble    | 0--100%  | --      | High-frequency EQ        |
| presence  | 0--100%  | --      | Upper-mid presence       |
| depth     | 0--100%  | --      | Low-end depth            |
| sag       | 0--100%  | --      | Power supply sag         |
| master    | 0--100%  | --      | Master output level      |
| bright    | on/off   | off     | Bright switch             |

### Parameters -- NAM Marshall JCM 800 2203

| Parameter | Range                      | Description              |
|-----------|----------------------------|--------------------------|
| volume    | 50--70%                    | Output volume            |
| gain      | 10--100% (10% steps)       | Gain level               |

### Parameters -- NAM Diezel VH4

| Parameter   | Description              |
|-------------|--------------------------|
| channel     | Amp channel selection    |
| voicing     | Voicing mode             |
| gain_level  | Gain level               |
| boost       | Boost switch             |

### Parameters -- NAM models (standard, issue #204)

All 21 new NAM preamp models added in issue #204 share the same two-parameter interface:

| Parameter | Range                | Description        |
|-----------|----------------------|--------------------|
| volume    | 50--70%              | Output volume      |
| gain      | 10--100% (10% steps) | Gain level         |

Each model selects a capture file based on the `gain` value (mapped to steps of 10). The available voicings per model are:

| Model ID | Available Voicings / Captures |
|----------|-------------------------------|
| `nam_engl_thunder_50` | lead |
| `nam_fender_57_champ` | clean, crunch, od × in1, in2 |
| `nam_fender_57_deluxe` | clean, crunch, od × in1, in2 |
| `nam_fender_frontman_15g` | clean, clean_boost, high_gain |
| `nam_joyo_bantamp_meteor` | clean, clean_gain, low_crunch, crunch, high_gain, high_gain_808, maxed |
| `nam_marshall_avt50h` | hg_bass_cut, hg_dimed, hg_mid_cut, hg_mid_forward, hg_treb_cut |
| `nam_marshall_yjm100` | standard, jcm800_mode, boost_bright, modern |
| `nam_mesa_mark_iii` | eq (lohi / lomed / medhi) × gain (cranked / pushed) |
| `nam_orange_micro_terror` | modified DI |
| `nam_panama_shaman` | clean, crunch, hg, od × gain g1--g9 |
| `nam_peavey_classic_30` | clean, clean_boost |
| `nam_sovtek_mig100` | v1, v2, v3 |
| `nam_victory_vx_kraken` | shred_di, shred_full |
| `nam_ehx_mig50` | clean, crunch, od |
| `nam_ehx_22_caliber` | clean, crunch |
| `nam_award_session_blues_baby_22` | clean, crunch, od |
| `nam_blackstar_fly` | clean, crunch, od |
| `nam_fender_pa100` | clean |
| `nam_koch_multitone_50` | clean, crunch, od × variant 1, 2, 3 |
| `nam_lab_series_l2` | clean, crunch, od |
| `nam_zt_lunchbox_jr` | clean, crunch, od |

---

## Amp

An amp block models a complete amplifier, including preamp and power amp stages together. This is the primary tone-shaping block in most signal chains.

### Models

| Model Name                | Brand     | Backend | Description                        |
|---------------------------|-----------|---------|------------------------------------|
| Blackface Clean           | --        | Native  | Clean American amp                 |
| Tweed Breakup             | --        | Native  | Warm breakup amp                   |
| Chime                     | --        | Native  | Chimey British-style amp           |
| Bogner Ecstasy            | Bogner    | NAM     | Versatile high-gain amp            |
| Bogner Shiva              | Bogner    | NAM     | Dynamic clean-to-gain amp          |
| Dumble ODS                | Dumble    | NAM     | Legendary smooth overdrive         |
| EVH 5150                  | EVH       | NAM     | Iconic high-gain metal amp         |
| Friedman BE100 Deluxe     | Friedman  | NAM     | EL34-powered, 5 channels, 3 mic positions |
| Marshall JCM 800          | Marshall  | NAM     | Classic British rock amp           |
| Marshall JMP-1 Head       | Marshall  | NAM     | JMP-1 OD2 head, no cab             |
| Marshall JVM              | Marshall  | NAM     | Modern versatile Marshall          |
| Mesa Mark V               | Mesa      | NAM     | Tight focused high-gain            |
| Mesa Rectifier            | Mesa      | NAM     | Aggressive modern high-gain        |
| Peavey 5150               | Peavey    | NAM     | Heavy metal workhorse              |
| Ampeg SVT Classic         | Ampeg     | NAM     | Classic bass amp + 6x10 cab        |
| Dover DA-50 + Mesa 4x12   | Dover     | NAM     | Boutique amp + Mesa OS 4x12        |
| Fender Bassman 1971       | Fender    | NAM     | 1971 Bassman, 9 tone presets       |
| Fender Deluxe Reverb '65  | Fender    | NAM     | Clean single-channel combo         |
| Fender Super Reverb 1977  | Fender    | NAM     | Clean multi-mic combo              |
| Marshall JMP-1 Full Rig   | Marshall  | NAM     | JMP-1 OD2 + V30 cab                |
| Marshall Super 100 1966   | Marshall  | NAM     | Vintage SA100 full stack           |
| Peavey 5150 + Mesa 4x12   | Peavey    | NAM     | High-gain with boost/mic options   |
| Roland JC-120B Jazz Chorus| Roland    | NAM     | All-in-one clean + built-in chorus |
| Synergy DRECT Mesa        | Synergy   | NAM     | Metal rig with boost options       |
| Vox AC30                  | Vox       | NAM     | Full rig with character variants   |
| Vox AC30 '61 Fawn EF86    | Vox       | NAM     | Vintage 1961 Vox combo             |
| GxBlueAmp                 | Guitarix  | LV2     | Guitarix blue amp simulation       |
| GxSupersonic              | Guitarix  | LV2     | Guitarix supersonic amp            |
| MDA Combo                 | MDA       | LV2     | Amp combo simulation               |

### Parameters -- Native Amps

| Parameter | Range   | Step | Description              |
|-----------|---------|------|--------------------------|
| gain      | 0--100% | 1.0  | Amp gain                 |
| bass      | 0--100% | 1.0  | Low-frequency EQ         |
| middle    | 0--100% | 1.0  | Mid-frequency EQ         |
| treble    | 0--100% | 1.0  | High-frequency EQ        |
| presence  | 0--100% | 1.0  | Upper-mid presence       |
| depth     | 0--100% | 1.0  | Low-end depth            |
| sag       | 0--100% | 1.0  | Power supply sag         |
| master    | 0--100% | 1.0  | Master output level      |
| room_mix  | 0--100% | 1.0  | Room ambience mix        |

### Parameters -- Friedman BE100 Deluxe

| Parameter | Options                                                                 | Default      |
|-----------|-------------------------------------------------------------------------|--------------|
| channel   | cln_tender (Clean Tender), cln_rock (Clean Rock), be (BE Eddie), hbe_tallica (HBE Tallica), hbe_mammoth (HBE Mammoth) | cln_tender |
| mic       | sm57, sm58, blend                                                       | sm57         |

### Parameters -- Marshall JMP-1 Head

No user-adjustable parameters. Single capture of the JMP-1 OD2 channel.

### Parameters -- NAM amps (catalog conventions)

NAM amps are captures with knob settings baked in at capture time. The amp itself does **not** expose `bass`, `mid`, `treble`, or `master` -- to mimic a different EQ shape, place an `eq_eight_band_parametric` (or `eq_three_band_basic`) block before or after the amp.

What an individual NAM amp model exposes follows one of three patterns:

**Pattern A -- single capture, no params.** A full rig (preamp + power amp + cab + mic) baked into one capture. `params: {}`. Examples: `marshall_super_100_1966`, `marshall_jmp_1_full_rig`, `nam_marshall_plexi`, most vintage Marshalls.

```yaml
- type: amp
  model: marshall_super_100_1966
  params: {}
```

**Pattern B -- `character` selector.** One field that switches between voicings/captures. Examples: `vox_ac30`, `nam_velvet_katana`, `nam_vemuram_jan_ray`. The valid `character` values per model are listed in the model's source file in `crates/block-amp/src/`.

```yaml
- type: amp
  model: vox_ac30
  params:
    character: clean_65prince
```

**Pattern C -- `cabinet` + `gain` grid.** Two fields whose Cartesian product selects a capture: `gain` is the channel/voicing (`clean`, `crunch`, `crunch_orange`, `drive_red`, etc.) and `cabinet` is the speaker cab the amp was captured into (`4x12_v30`, `4x12_greenback`, `4x12_g12t`, etc.). Examples: `nam_mesa_rectifier`, `nam_evh_5150`, `nam_peavey_5150`, `nam_marshall_jcm_800`.

```yaml
- type: amp
  model: nam_mesa_rectifier
  params:
    cabinet: 4x12_v30
    gain: drive_red
```

The exact `gain × cabinet` combinations supported per model are defined by the `CAPTURES` array in the model's `.rs` file. Mismatched combinations fail at load time with the message `amp-combo model 'X' does not support gain='Y' cabinet='Z'`.

**Pattern D -- standard 21 NAM preamps (issue #204).** `volume` (50--70%) + `gain` (10--100%, 10% steps). The gain value selects a discrete capture variant per model. See [Parameters -- NAM models (standard, issue #204)](#parameters----nam-models-standard-issue-204) above for the full table of available voicings per `MODEL_ID`.

> When in doubt, the source of truth is the `validate_params` function and `CAPTURES` array in `crates/block-amp/src/<model_id>.rs`. The same conventions apply to NAM **gain** models (overdrive, distortion, fuzz pedals) -- they typically expose a single `character`/`tone`/`gain`/`mode` selector, with a per-model `Parameters` subsection in the Gain section below.

---

## Cab

A cab (cabinet) block simulates the speaker cabinet and microphone capture. It applies the frequency response of a physical speaker to the signal, which is essential for turning a raw amp tone into a realistic guitar sound.

### Models

| Model Name                      | Brand    | Backend | Description                      |
|---------------------------------|----------|---------|----------------------------------|
| American 2x12                   | --       | Native  | Open-back American cab           |
| Brit 4x12                       | --       | Native  | Closed-back British cab          |
| Vintage 1x12                    | --       | Native  | Small vintage combo cab          |
| Marshall 4x12 V30               | Marshall | IR      | Classic Marshall with Vintage 30s|
| G12M Greenback 2x12             | --       | IR      | Warm vintage speakers            |
| G12T-75 4x12                    | --       | IR      | Bright articulate speakers       |
| V30 4x12                        | --       | IR      | Modern rock/metal standard       |
| Fender Deluxe Reverb Oxford     | Fender   | IR      | Classic American clean           |
| Celestion Cream 4x12            | --       | IR      | Smooth alnico speakers           |
| Mesa Oversized 4x12 V30         | Mesa     | IR      | Deep tight low-end, 9 mic/position options |
| Mesa Standard 4x12 V30          | Mesa     | IR      | Standard OS 4x12, SM57 and SM58  |
| Vox AC30 Blue                   | Vox      | IR      | Chimey British jangle            |
| Vox AC50 2x12 Goodmans          | Vox      | IR      | Vintage AC50 with Goodmans 241   |
| Evil Chug (Blackstar + PRS)     | Blackstar| IR      | High-gain Blackbird 50 + PRS cab |
| G12M Greenback Multi-Mic        | --       | IR      | 6 mic options: SM57/LCT441/MC834/M160/OC818/CC8 |
| Roland JC-120 Cab               | Roland   | IR      | JC-120 cab, SM57+MD421 mix       |
| GxUltraCab                      | Guitarix | LV2     | Guitarix ultra cab simulation    |

### Parameters

| Parameter    | Range          | Description                          |
|--------------|----------------|--------------------------------------|
| low_cut_hz   | 20--500 Hz     | High-pass filter cutoff frequency    |
| high_cut_hz  | 2000--20000 Hz | Low-pass filter cutoff frequency     |
| resonance    | 0--100%        | Cabinet resonance amount             |
| air          | 0--100%        | High-frequency air/openness          |
| mic_position | 0--100%        | Microphone position (center to edge) |
| mic_distance | 0--100%        | Microphone distance from speaker     |
| room_mix     | 0--100%        | Room ambience mix                    |

---

## Gain

A gain block covers overdrive, distortion, fuzz, and volume control pedals. These blocks add harmonic saturation or shape the signal level before or after the amp.

NAM-based gain models capture real hardware with specific parameter snapshots (tone, drive, and boost settings fixed at capture time). They reproduce the character of a particular pedal setting rather than offering continuously variable parameters.

### Models

| Model Name                    | Brand         | Backend | Description                                                    |
|-------------------------------|---------------|---------|----------------------------------------------------------------|
| Volume                        | --            | Native  | Simple volume/mute control                                     |
| Ibanez TS9                    | --            | Native  | Classic tube screamer overdrive                                |
| Blues Overdrive BD-2          | Ibanez        | NAM     | Smooth blues overdrive                                         |
| Ibanez TS9                    | Ibanez        | NAM     | NAM-captured tube screamer                                     |
| JHS Andy Timmons              | JHS           | NAM     | Signature artist overdrive                                     |
| Ampeg SCR-DI                  | Ampeg         | NAM     | Bass DI/preamp with tone and scrambler variants                |
| Behringer SF300 Super Fuzz    | Behringer     | NAM     | Fuzz pedal with fuzz1/fuzz2 variants                           |
| BluesBreaker                  | Marshall      | NAM     | Marshall BluesBreaker clone                                    |
| Boss DS-1 Distortion          | Boss          | NAM     | Classic distortion, tone x dist grid                           |
| Boss DS-1 Wampler JCM Mod     | Boss          | NAM     | JCM-modded DS-1, tone x dist grid                              |
| Boss FZ-1W Fuzz               | Boss          | NAM     | Modern/vintage fuzz modes                                      |
| Boss HM-2 Heavy Metal '86     | Boss          | NAM     | 1986 HM-2, chainsaw and variants                               |
| Boss HM-2 Heavy Metal MiJ     | Boss          | NAM     | Made-in-Japan HM-2, SWEDE/Godflesh/ATG tones                   |
| CC Boost                      | Custom        | NAM     | Clean boost                                                    |
| Darkglass Alpha Omega Ultra   | Darkglass     | NAM     | Bass overdrive, alpha/omega channels                           |
| Darkglass B7K Ultra           | Darkglass     | NAM     | Bass preamp/drive, 5 tones                                     |
| Demonfx BE-OD Clone           | Demonfx       | NAM     | Friedman BE-OD clone, gain variants                            |
| Fulltone OCD v1.2             | Fulltone      | NAM     | Overdrive, LP/HP modes                                         |
| Fulltone OCD v1.5             | Fulltone      | NAM     | Anti-aliased overdrive, LP/HP modes                            |
| Grind                         | TC Electronic | NAM     | Distortion                                                     |
| HM-2                          | Boss          | NAM     | HM-2 single capture                                            |
| Ibanez TS808                  | Ibanez        | NAM     | Tube Screamer 808, standard/driven                             |
| JHS Bonsai                    | JHS           | NAM     | 9 Tube Screamer modes + boost                                  |
| Klon Centaur Silver           | Klon          | NAM     | Legendary overdrive, 6 settings                                |
| Klone                         | Custom        | NAM     | Klon clone, single capture                                     |
| Lokajaudio Der Blend          | Lokajaudio    | NAM     | Fuzz/sustain, 5 character settings                             |
| Lokajaudio Doom Machine V3    | Lokajaudio    | NAM     | Fuzz/octave                                                    |
| Maxon OD808                   | Maxon         | NAM     | OD808 overdrive, drive 0--100%                                 |
| Metal Zone MT-2               | Boss          | NAM     | Metal distortion                                               |
| MXR GT-OD (Zakk Wylde)        | MXR           | NAM     | Overdrive with hq/v2 versions                                  |
| PoT Boost                     | PoT           | NAM     | Clean boost                                                    |
| PoT OD                        | PoT           | NAM     | Overdrive                                                      |
| ProCo RAT                     | ProCo         | NAM     | Classic RAT distortion                                         |
| ProCo RAT 2                   | ProCo         | NAM     | RAT 2, dist/filter variants                                    |
| ROD-10 DS1                    | Custom        | NAM     | ROD-10 into DS-1                                               |
| ROD-10 SD1                    | Custom        | NAM     | ROD-10 into SD-1                                               |
| RR Golden Clone               | RR            | NAM     | Klon-style overdrive, 3 settings                               |
| SansAmp DI-2112               | Tech21        | NAM     | Bass preamp, 9 artist presets (Geddy Lee, Jack Bruce, etc.)    |
| Slammin Clean Booster         | Slammin       | NAM     | 10 clean boost voicings                                        |
| Tascam 424 Preamp             | Tascam        | NAM     | Cassette preamp pedal, gain 7--max                             |
| TC Spark                      | TC Electronic | NAM     | Clean boost, clean/mid                                         |
| TCIP                          | TC Electronic | NAM     | Boost                                                          |
| Tech21 Steve Harris SH-1      | Tech21        | NAM     | Iron Maiden bass preamp                                        |
| Velvet Katana                 | Velvet        | NAM     | Dumble-like tones, 6 characters                                |
| Vemuram Jan Ray               | Vemuram       | NAM     | Mateus Asato signature overdrive                               |
| Bitta                         | --            | LV2     | Bitcrusher distortion                                          |
| MDA Degrade                   | MDA           | LV2     | Lo-fi degradation effect                                       |
| MDA Overdrive                 | MDA           | LV2     | Soft-clip overdrive                                            |
| OJD                           | --            | LV2     | OCD-style overdrive                                            |
| Paranoia                      | --            | LV2     | Fuzz/distortion                                                |
| TAP Sigmoid                   | TAP           | LV2     | Waveshaper distortion                                          |
| Wolf Shaper                   | --            | LV2     | Waveshaper with visual editor                                  |
| CAPS Spice                    | CAPS          | LV2     | Overdrive/distortion                                           |
| CAPS Spice X2                 | CAPS          | LV2     | Overdrive/distortion (stereo)                                  |
| Driva                         | Artyfx        | LV2     | Drive/distortion                                               |
| Satma                         | Artyfx        | LV2     | Saturation effect                                              |
| Invada Tube                   | Invada        | LV2     | Tube saturation/warmth                                         |
| TAP Tubewarmth                | TAP           | LV2     | Tube warmth simulator                                          |

#### Guitarix LV2 Gain Plugins (40 models)

The following 40 overdrive, distortion, and fuzz plugins are provided by the Guitarix project via LV2:

| Model Name            | Brand    | Backend | Description                     |
|-----------------------|----------|---------|---------------------------------|
| Axis Face             | Guitarix | LV2     | Fuzz                            |
| BaJa Tube Driver      | Guitarix | LV2     | Tube driver                     |
| Boob Tube             | Guitarix | LV2     | Tube overdrive                  |
| Bottle Rocket         | Guitarix | LV2     | Overdrive                       |
| Club Drive            | Guitarix | LV2     | Drive pedal                     |
| Cream Machine         | Guitarix | LV2     | Overdrive/distortion            |
| DOP 250               | Guitarix | LV2     | DOD 250 clone                   |
| Epic                  | Guitarix | LV2     | High-gain distortion            |
| Eternity              | Guitarix | LV2     | Eternity overdrive clone        |
| Maestro FZ-1B         | Guitarix | LV2     | Maestro Fuzz-Tone clone (bass)  |
| Maestro FZ-1S         | Guitarix | LV2     | Maestro Fuzz-Tone clone         |
| Guvnor                | Guitarix | LV2     | Marshall Guvnor clone           |
| Hot Box               | Guitarix | LV2     | Overdrive                       |
| Hyperion              | Guitarix | LV2     | Distortion                      |
| Knight Fuzz           | Guitarix | LV2     | Fuzz                            |
| Liquid Drive          | Guitarix | LV2     | Smooth overdrive                |
| Luna                  | Guitarix | LV2     | Overdrive                       |
| Micro Amp             | Guitarix | LV2     | Clean boost                     |
| Saturator             | Guitarix | LV2     | Saturation/clipping             |
| SD-1                  | Guitarix | LV2     | Boss SD-1 clone                 |
| SD-2 Lead             | Guitarix | LV2     | Boss SD-2 lead channel clone    |
| Shaka Tube            | Guitarix | LV2     | Tube overdrive                  |
| Sloopy Blue           | Guitarix | LV2     | Blues overdrive                 |
| Sun Face              | Guitarix | LV2     | Fuzz Face clone                 |
| Super Fuzz            | Guitarix | LV2     | Uni-Vibe era fuzz               |
| Suppa Tone Bender     | Guitarix | LV2     | Tone Bender clone               |
| Tim Ray               | Guitarix | LV2     | Overdrive                       |
| Tone Machine          | Guitarix | LV2     | Octave fuzz                     |
| Tube Distortion       | Guitarix | LV2     | Tube-style distortion           |
| Valve Caster          | Guitarix | LV2     | Tube valve overdrive            |
| Vintage Fuzz Master   | Guitarix | LV2     | Vintage fuzz                    |
| Vmk2                  | Guitarix | LV2     | Distortion                      |
| Voodo Fuzz            | Guitarix | LV2     | Voodoo fuzz                     |

### Parameters -- Native TS9

| Parameter | Range   | Description              |
|-----------|---------|--------------------------|
| drive     | 0--100% | Overdrive amount         |
| tone      | 0--100% | Tone control             |
| level     | 0--100% | Output level             |

### Parameters -- Volume

| Parameter | Range   | Description              |
|-----------|---------|--------------------------|
| volume    | 0--100% | Volume level             |
| mute      | on/off  | Mute switch              |

### Parameters -- NAM Gain Models

NAM gain models expose discrete capture variants rather than continuous knobs. Each model section below lists the selectable options per parameter.

#### Ampeg SCR-DI

| Parameter | Options                                                             | Default  |
|-----------|---------------------------------------------------------------------|----------|
| tone      | standard, ultra_lo, ultra_hi, ultra_lo_hi, scrambler_med, scrambler_max | standard |

#### Behringer SF300 Super Fuzz

| Parameter | Options                            | Default    |
|-----------|------------------------------------|------------|
| tone      | fuzz1, fuzz2_low, fuzz2_high, fuzz2_max | fuzz2_high |

#### BluesBreaker

No user-adjustable parameters. Single capture.

#### Boss DS-1 Distortion

| Parameter | Options                  | Default |
|-----------|--------------------------|---------|
| tone      | 4 (Dark), 7 (Neutral), 10 (Bright) | 7 |
| dist      | 5 (Low), 8 (Medium), 10 (High)     | 8 |

#### Boss DS-1 Wampler JCM Mod

| Parameter | Options                              | Default |
|-----------|--------------------------------------|---------|
| tone      | 2 (Dark), 6 (Neutral), 8 (Bright)   | 6       |
| dist      | 0 (Clean), 5 (Medium), 10 (High)    | 5       |

#### Boss FZ-1W Fuzz

| Parameter | Options        | Default |
|-----------|----------------|---------|
| mode      | modern, vintage | modern  |
| fuzz      | 2, 5, 7, 11    | 5       |

#### Boss HM-2 '86

| Parameter | Options                                              | Default  |
|-----------|------------------------------------------------------|----------|
| tone      | chainsaw_0gain, chainsaw, medium, warm, bright, high_gain, full | chainsaw |

#### Boss HM-2 MiJ

| Parameter | Options                                        | Default |
|-----------|------------------------------------------------|---------|
| tone      | swede, godflesh, atg, boost_sharp, boost_blunt, boost_che | swede |

#### CC Boost

No user-adjustable parameters. Single capture.

#### Darkglass Alpha Omega Ultra

| Parameter | Options         | Default |
|-----------|-----------------|---------|
| channel   | alpha, omega    | omega   |
| gain      | 2, 5, 8, 10     | 5       |

#### Darkglass B7K Ultra

| Parameter | Options                                        | Default |
|-----------|------------------------------------------------|---------|
| tone      | clean, hard_rock, heavy, djent, distortion     | heavy   |

#### Demonfx BE-OD Clone

| Parameter | Options                                            | Default |
|-----------|----------------------------------------------------|---------|
| gain      | 50 (Low), 75 (Medium), 100 (High), 100_tight (High Tight) | 75 |

#### Fulltone OCD v1.2

| Parameter | Options                    | Default |
|-----------|----------------------------|---------|
| mode      | lp (LP), hp (HP)           | lp      |
| drive     | 0 (Low), 4 (Medium), 7 (High) | 4    |

#### Fulltone OCD v1.5

| Parameter | Options                         | Default |
|-----------|---------------------------------|---------|
| mode      | lp (LP), hp (HP)                | lp      |
| drive     | 3 (Low), 9 (Medium), 12 (High)  | 9       |

#### Grind

No user-adjustable parameters. Single capture.

#### HM-2

No user-adjustable parameters. Single capture.

#### Ibanez TS808

| Parameter | Options                | Default  |
|-----------|------------------------|----------|
| character | standard, driven       | standard |

#### JHS Bonsai (9 TS)

| Parameter | Options                          | Default |
|-----------|----------------------------------|---------|
| mode      | 808, ts9, od1, jhs, keeley       | 808     |
| boost     | on, off                          | off     |

#### Klon Centaur Silver

| Parameter | Options                                    | Default |
|-----------|--------------------------------------------|---------|
| setting   | 255, 277, 468, 555, 668, john_mayer        | 555     |

#### Klone

No user-adjustable parameters. Single capture.

#### Lokajaudio Der Blend

| Parameter | Options                                | Default |
|-----------|----------------------------------------|---------|
| character | off, mid, high, high_boost, max        | high    |

#### Lokajaudio Doom Machine V3

No user-adjustable parameters. Single capture.

#### Maxon OD808

| Parameter     | Options                      | Default |
|---------------|------------------------------|---------|
| drive_percent | 0, 25, 50, 75, 100%          | 50%     |

#### Metal Zone MT-2

No user-adjustable parameters. Single capture.

#### MXR GT-OD (Zakk Wylde)

| Parameter | Options    | Default |
|-----------|------------|---------|
| version   | hq, v2     | hq      |

#### PoT Boost

No user-adjustable parameters. Single capture.

#### PoT OD

No user-adjustable parameters. Single capture.

#### ProCo RAT

No user-adjustable parameters. Single capture.

#### ProCo RAT 2

| Parameter | Options                          | Default |
|-----------|----------------------------------|---------|
| tone      | light, medium, heavy, max        | medium  |

#### ROD-10 DS1

No user-adjustable parameters. Single capture.

#### ROD-10 SD1

No user-adjustable parameters. Single capture.

#### RR Golden Clone

| Parameter | Options                          | Default |
|-----------|----------------------------------|---------|
| setting   | 5_4 (5/4), 6_6 (6/6), 2_7 (2/7) | 6_6     |

#### SansAmp DI-2112

| Parameter | Options                                                                              | Default        |
|-----------|--------------------------------------------------------------------------------------|----------------|
| preset    | geddy_standard, geddy_roundabout, yyz, jack_bruce, jpj, les_claypool, entwistle, radiohead, deep_sat | geddy_standard |

#### Slammin Clean Booster

| Parameter | Options                                                                                                               | Default   |
|-----------|-----------------------------------------------------------------------------------------------------------------------|-----------|
| character | od808_t5 (OD808 T5), od808_t7 (OD808 T7), ocd_lp_t5 (OCD LP), ocd_hp_t5 (OCD HP), sd1_t5 (SD1 T5), sd1_t7 (SD1 T7), goldenpearl_t5 (Golden Pearl), echopre_bright (EchoPre Bright), echopre_mid (EchoPre Mid), echopre_dark (EchoPre Dark) | od808_t5 |

#### Tascam 424 Preamp

| Parameter | Options                          | Default |
|-----------|----------------------------------|---------|
| gain      | 7 (Low), 8 (Medium), 9 (High), max (Max) | 8 |

#### TC Spark

| Parameter | Options       | Default |
|-----------|---------------|---------|
| character | clean, mid    | clean   |

#### TCIP

No user-adjustable parameters. Single capture.

#### Tech21 Steve Harris SH-1

| Parameter | Options                    | Default  |
|-----------|----------------------------|----------|
| character | standard, less_highs       | standard |

#### Velvet Katana

| Parameter | Options                                                                                       | Default |
|-----------|-----------------------------------------------------------------------------------------------|---------|
| character | country (Country), blues_bright (Blues Bright), larry (Larry Carlton), brad (Brad), drive (Drive), drive_plus (Drive ++) | larry |

#### Vemuram Jan Ray

| Parameter | Options                    | Default  |
|-----------|----------------------------|----------|
| character | mid_gain, high_gain        | mid_gain |

---

## Delay

A delay block produces echo and repetition effects by playing back a copy of the signal after a configurable time interval. Different delay models apply distinct filtering and modulation characteristics to the repeats.

### Models

| Model Name       | Brand        | Backend | Description                              |
|------------------|--------------|---------|------------------------------------------|
| Digital Clean    | --           | Native  | Clean digital delay                      |
| Analog Warm      | --           | Native  | Warm analog-style delay with filtering   |
| Slapback         | --           | Native  | Short slapback echo                      |
| Reverse          | --           | Native  | Reversed delay tails                     |
| Modulated Delay  | --           | Native  | Delay with modulation                    |
| Tape Vintage     | --           | Native  | Vintage tape echo simulation             |
| Bollie Delay     | Bollie       | LV2     | Delay effect                             |
| Avocado          | Remaincalm   | LV2     | Delay effect                             |
| Floaty           | Remaincalm   | LV2     | Delay effect                             |
| Modulay          | Shiro        | LV2     | Modulated delay                          |
| MDA DubDelay     | MDA          | LV2     | Dub-style delay                          |
| TAP Doubler      | TAP          | LV2     | Stereo doubler delay                     |
| TAP Stereo Echo  | TAP          | LV2     | Stereo echo                              |
| TAP Reflector    | TAP          | LV2     | Reflective delay                         |

### Parameters -- Native Delays

| Parameter | Range       | Description                      |
|-----------|-------------|----------------------------------|
| time_ms   | 1--2000 ms  | Delay time in milliseconds       |
| feedback  | 0--100%     | Amount of signal fed back        |
| mix       | 0--100%     | Dry/wet mix                      |

---

## Reverb

A reverb block simulates the natural reflections of an acoustic space or mechanical reverb device.

### Models

| Model Name                    | Brand     | Backend | Description                            |
|-------------------------------|-----------|---------|----------------------------------------|
| Plate Foundation              | --        | Native  | Studio plate reverb                    |
| Hall                          | --        | Native  | Large hall reverb                      |
| Room                          | --        | Native  | Small room reverb                      |
| Spring                        | --        | Native  | Spring reverb simulation               |
| Dragonfly Early Reflections   | Dragonfly | LV2     | Early reflections simulator            |
| Dragonfly Hall Reverb         | Dragonfly | LV2     | Algorithmic hall reverb                |
| Dragonfly Plate Reverb        | Dragonfly | LV2     | Algorithmic plate reverb               |
| Dragonfly Room Reverb         | Dragonfly | LV2     | Algorithmic room reverb                |
| CAPS Plate                    | CAPS      | LV2     | Plate reverb                           |
| CAPS Plate X2                 | CAPS      | LV2     | Stereo plate reverb                    |
| CAPS Scape                    | CAPS      | LV2     | Ambient reverb/soundscape              |
| TAP Reflector                 | TAP       | LV2     | Reflective reverb                      |
| TAP Reverberator              | TAP       | LV2     | General-purpose reverberator           |
| MDA Ambience                  | MDA       | LV2     | Ambience reverb                        |
| MVerb                         | Distrho   | LV2     | High-quality algorithmic reverb        |
| B Reverb                      | SetBfree  | LV2     | Reverb effect                          |
| Roomy                         | OpenAV    | LV2     | Room reverb                            |
| Shiroverb                     | Shiro     | LV2     | Reverb effect                          |
| Floaty                        | Remaincalm| LV2     | Ambient reverb                         |

### Parameters -- Native Reverbs

| Parameter | Range   | Description                           |
|-----------|---------|---------------------------------------|
| room_size | 0--100% | Size of the simulated space           |
| damping   | 0--100% | High-frequency absorption amount      |
| mix       | 0--100% | Dry/wet mix                           |

---

## Modulation

Modulation blocks alter the signal with periodic variation in amplitude, pitch, or time, producing effects like tremolo, vibrato, chorus, phaser, and rotary speaker.

### Models

| Model Name          | Brand | Backend | Description                              |
|---------------------|-------|---------|------------------------------------------|
| Sine Tremolo        | --    | Native  | Classic sine-wave tremolo                |
| Vibrato             | --    | Native  | Pitch vibrato (100% wet, no dry signal)  |
| Classic Chorus      | --    | Native  | Traditional chorus effect                |
| Ensemble Chorus     | --    | Native  | Rich ensemble-style chorus               |
| Stereo Chorus       | --    | Native  | Wide stereo chorus                       |
| TAP Chorus/Flanger  | TAP   | LV2     | Combined chorus and flanger              |
| TAP Tremolo         | TAP   | LV2     | Tremolo effect                           |
| TAP Rotary Speaker  | TAP   | LV2     | Rotary speaker (Leslie) simulation       |
| MDA Leslie          | MDA   | LV2     | Leslie cabinet simulator                 |
| MDA RingMod         | MDA   | LV2     | Ring modulator                           |
| MDA ThruZero        | MDA   | LV2     | Through-zero flanger                     |
| FOMP CS Chorus      | FOMP  | LV2     | CS-style chorus                          |
| FOMP CS Phaser      | FOMP  | LV2     | CS-style phaser                          |
| CAPS Phaser II      | CAPS  | LV2     | Phaser effect                            |
| Harmless            | Shiro | LV2     | Harmonic modulation                      |
| Larynx              | Shiro | LV2     | Vocal-style modulation                   |

### Parameters -- Tremolo

| Parameter | Range       | Description              |
|-----------|-------------|--------------------------|
| rate_hz   | 0.1--20 Hz  | Modulation rate          |
| depth     | 0--100%     | Modulation depth         |

### Parameters -- Vibrato

| Parameter | Range      | Description              |
|-----------|------------|--------------------------|
| rate_hz   | 0.1--8 Hz  | Modulation rate          |
| depth     | 0--100%    | Modulation depth         |

### Parameters -- Chorus

| Parameter | Range   | Description              |
|-----------|---------|--------------------------|
| rate_hz   | --      | Modulation rate          |
| depth     | --      | Modulation depth         |
| mix       | --      | Dry/wet mix              |

---

## Dynamics

Dynamics blocks control the dynamic range of the signal, either compressing loud peaks, gating unwanted noise, or hard-limiting output.

### Models

| Model Name                | Brand | Backend | Description                          |
|---------------------------|-------|---------|--------------------------------------|
| Studio Clean Compressor   | --    | Native  | Transparent studio compressor        |
| Noise Gate                | --    | Native  | Simple noise gate                    |
| Brick Wall Limiter        | --    | Native  | Hard limiter                         |
| TAP DeEsser               | TAP   | LV2     | De-esser                             |
| TAP Dynamics              | TAP   | LV2     | Dynamic processor                    |
| TAP Scaling Limiter       | TAP   | LV2     | Limiter                              |
| ZamComp                   | ZAM   | LV2     | Compressor                           |
| ZamGate                   | ZAM   | LV2     | Gate                                 |
| ZaMultiComp               | ZAM   | LV2     | Multiband compressor                 |

### Parameters -- Studio Clean Compressor

| Parameter   | Range        | Description                       |
|-------------|--------------|-----------------------------------|
| threshold   | 0--100%      | Compression threshold             |
| ratio       | 0--100%      | Compression ratio                 |
| attack_ms   | 0.1--200 ms  | Attack time in milliseconds       |
| release_ms  | 1--500 ms    | Release time in milliseconds      |
| makeup_gain | 0--100%      | Makeup gain after compression     |
| mix         | 0--100%      | Dry/wet mix (parallel compression)|

### Parameters -- Noise Gate

| Parameter  | Range   | Description                       |
|------------|---------|-----------------------------------|
| threshold  | 0--100% | Gate threshold                    |
| attack_ms  | --      | Attack time in milliseconds       |
| release_ms | --      | Release time in milliseconds      |

---

## Filter

Filter blocks shape the frequency spectrum of the signal using equalization and dynamic filtering.

### Models

| Model Name        | Brand | Backend | Description                    |
|-------------------|-------|---------|--------------------------------|
| Three Band EQ          | --    | Native  | 3-band parametric EQ                       |
| Guitar EQ              | --    | Native  | Low-cut + high-cut EQ for guitar           |
| 8-Band Parametric EQ   | --    | Native  | 8 independent bands, full parametric control |
| TAP Equalizer     | TAP   | LV2     | Parametric EQ                  |
| TAP Equalizer/BW  | TAP   | LV2     | Butterworth EQ                 |
| ZamEQ2            | ZAM   | LV2     | 2-band parametric EQ           |
| ZamGEQ31          | ZAM   | LV2     | 31-band graphic EQ             |
| CAPS AutoFilter   | CAPS  | LV2     | Auto filter                    |
| FOMP Auto-Wah     | FOMP  | LV2     | Auto-wah filter                |
| MOD High Pass     | MOD   | LV2     | High-pass filter               |
| MOD Low Pass      | MOD   | LV2     | Low-pass filter                |
| Filta             | OpenAV| LV2     | Filter effect                  |
| Mud               | Remaincalm | LV2 | Mud filter                    |

### Parameters -- Three Band EQ

| Parameter | Range   | Mapped Range       | Description        |
|-----------|---------|--------------------|--------------------|
| low       | 0--100% | -24 dB to +24 dB  | Low-band gain      |
| mid       | 0--100% | -24 dB to +24 dB  | Mid-band gain      |
| high      | 0--100% | -24 dB to +24 dB  | High-band gain     |

### Parameters -- Guitar EQ

Cuts the two frequency ranges known to cause noise and mud in guitar signals. Each cut uses a gentle Butterworth shelf (Q=0.707) so the rolloff is musical rather than surgical.

| Parameter | Range   | Mapped Range  | Description                                          |
|-----------|---------|---------------|------------------------------------------------------|
| low_cut   | 0--100% | 0 to -12 dB   | Low-shelf cut below 80 Hz (rumble, stage noise, mud) |
| high_cut  | 0--100% | 0 to -12 dB   | High-shelf cut above 8 kHz (hiss, pick fizz)         |

### Parameters -- 8-Band Parametric EQ

Eight independent biquad filter stages in cascade. Each band is separately configurable. Default frequencies: 62, 125, 250, 500, 1k, 2k, 4k, 8kHz (all Peak, 0 dB).

Each band `{N}` (1–8) exposes five parameters:

| Parameter        | Type   | Range          | Default | Description                                        |
|------------------|--------|----------------|---------|----------------------------------------------------|
| `band{N}_enabled`| bool   | on/off         | on      | Bypass this band when off                          |
| `band{N}_type`   | enum   | see below      | peak    | Filter shape                                       |
| `band{N}_freq`   | float  | 20–20000 Hz    | *       | Center / corner frequency                          |
| `band{N}_gain`   | float  | -24 to +24 dB  | 0       | Boost or cut (ignored by LP/HP/Notch)              |
| `band{N}_q`      | float  | 0.1–10         | 1.0     | Bandwidth / resonance (higher = narrower)          |

Band types:

| Value        | Name        | Description                               |
|--------------|-------------|-------------------------------------------|
| `peak`       | Peak        | Bell-shaped boost or cut                  |
| `low_shelf`  | Low Shelf   | Boost/cut all frequencies below `freq`    |
| `high_shelf` | High Shelf  | Boost/cut all frequencies above `freq`    |
| `low_pass`   | Low Pass    | Attenuates frequencies above `freq`       |
| `high_pass`  | High Pass   | Attenuates frequencies below `freq`       |
| `notch`      | Notch       | Narrow cut at `freq` (gain ignored)       |

Example YAML:

```yaml
- type: filter
  model: eq_eight_band_parametric
  enabled: true
  params:
    band1_enabled: true
    band1_type: high_pass
    band1_freq: 80.0
    band1_gain: 0.0
    band1_q: 0.707
    band2_enabled: true
    band2_type: low_shelf
    band2_freq: 200.0
    band2_gain: 3.0
    band2_q: 0.707
    band3_enabled: true
    band3_type: peak
    band3_freq: 500.0
    band3_gain: -2.0
    band3_q: 2.0
    # bands 4–8 follow the same pattern
```

---

## Wah

A wah block produces a resonant bandpass filter sweep, controlled by a position parameter that simulates a rocker pedal.

### Models

| Model Name   | Brand    | Backend | Description            |
|--------------|----------|---------|-----------------------|
| Cry Classic  | --       | Native  | Classic wah-wah pedal |
| GxQuack      | Guitarix | LV2     | Wah effect            |

### Parameters -- Cry Classic

| Parameter | Description                        |
|-----------|------------------------------------|
| position  | Pedal position (heel to toe)       |
| Q         | Filter resonance width             |
| mix       | Dry/wet mix                        |
| output    | Output level                       |

---

## Utility

Utility blocks provide non-audio-processing tools that support the signal chain workflow.

### Models

| Model Name         | Brand | Backend | Description                                    |
|--------------------|-------|---------|------------------------------------------------|
| Chromatic Tuner    | --    | Native  | Reference tuner                                |
| Spectrum Analyzer  | --    | Native  | Real-time frequency spectrum display           |

### Parameters -- Chromatic Tuner

| Parameter    | Range        | Default | Description                       |
|--------------|--------------|---------|-----------------------------------|
| reference_hz | 400--480 Hz  | 440 Hz  | Reference pitch for A4 tuning     |

Spectrum Analyzer is a display-only block with no user-adjustable parameters.

---

## Pitch

Pitch blocks provide real-time pitch shifting, correction, and harmonization for monophonic audio sources.

### Models

| Model Name      | Brand   | Backend | Description                          |
|-----------------|---------|---------|--------------------------------------|
| Harmonizer      | Infamous| LV2     | Pitch harmonizer                     |
| x42 Autotune    | x42     | LV2     | Chromatic pitch correction           |
| MDA Detune      | MDA     | LV2     | Subtle pitch detune/doubler          |
| MDA RePsycho!   | MDA     | LV2     | Pitch shifting effect                |

---

## Body

Body blocks simulate the acoustic resonance of a guitar body using impulse responses. They are designed for use with piezo or magnetic pickups to produce a convincing acoustic tone. OpenRig includes **114 body models** spanning a wide range of acoustic guitar brands and body types.

All models use the **IR** backend.

### Models by Brand

| Brand       | Count | Examples                                              |
|-------------|-------|-------------------------------------------------------|
| Martin      | 37    | Dreadnought, OM, 000 series and variants              |
| Taylor      | 31    | Various guitar body types and tonewoods                |
| Gibson      | 9     | J-45, Hummingbird, and other iconic models             |
| Takamine    | 4     | Acoustic-electric models                               |
| Yamaha      | 4     | Concert and dreadnought models                         |
| Guild       | 3     | Jumbo and orchestra models                             |
| Others      | 26    | Ibanez, Ovation, Rainsong, Lowden, classical, vintage  |

---

## Full Rig

A full rig block is reserved for NAM captures that include the complete signal chain — preamp, power amp, cabinet, **and** effects pedals baked in. Currently no models are bundled.

> **Note:** Models previously listed here (Ampeg SVT, Fender Bassman, Vox AC30, etc.) were reclassified as **Amp** blocks (issue #208), since they are amp+cab captures without pedals.

---

## IR Loader

The IR Loader block is a generic impulse response loader that allows users to load their own IR files from disk. Unlike the fixed cab and body IR models bundled with OpenRig, this block accepts any standard WAV-format IR file.

### Models

| Model Name  | Brand | Backend | Description               |
|-------------|-------|---------|---------------------------|
| generic_ir  | --    | Native  | User-supplied IR file     |

---

## NAM Loader

The NAM Loader block is a generic Neural Amp Modeler capture loader that allows users to load their own `.nam` capture files from disk. Unlike the fixed NAM amp and pedal models bundled with OpenRig, this block accepts any compatible NAM capture.

### Models

| Model Name   | Brand | Backend | Description                   |
|--------------|-------|---------|-------------------------------|
| generic_nam  | --    | NAM     | User-supplied NAM capture     |

---

## Summary

| Block Type  | Models  | Backends Available       |
|-------------|---------|--------------------------|
| Preamp      | 5       | Native, NAM              |
| Amp         | 17      | Native, NAM, LV2         |
| Cab         | 12      | Native, IR, LV2          |
| Gain        | 91      | Native, NAM, LV2         |
| Delay       | 14      | Native, LV2              |
| Reverb      | 19      | Native, LV2              |
| Modulation  | 16      | Native, LV2              |
| Dynamics    | 9       | Native, LV2              |
| Filter      | 13      | Native, LV2              |
| Wah         | 2       | Native, LV2              |
| Utility     | 2       | Native                   |
| Pitch       | 4       | LV2                      |
| Body        | 114     | IR                       |
| Full Rig    | 12      | NAM                      |
| IR Loader   | 1       | Native                   |
| NAM Loader  | 1       | NAM                      |
| **Total**   | **331** |                          |

> Gain includes 2 Native models, 43 NAM captures, and 46 LV2 plugins (including 33 Guitarix models). NAM captures reproduce specific hardware settings at capture time; parameters are fixed per capture variant rather than continuously variable.
