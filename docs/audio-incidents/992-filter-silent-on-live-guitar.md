# #992 — a filter "has no effect on the live guitar", works on the DI loop

Status: **DIAGNOSED; one real defect found and being fixed on the branch.**
Issue: https://github.com/jpfaria/OpenRig/issues/992 · Branch: `bug/issue-992`

## Symptom (reported)

- 2026-09-29: an `lv2_x42_fil4` block on `ANAL+DIG` (`rig:input-7`) "makes no
  difference" on the live guitar, while the same block on the DI loop does. A
  native filter did nothing on the live guitar either; the NAM amp's knobs do.
- Later the same night, on `DIGITAL` (`rig:input-4`, no insert): switching the
  fil4 off and on "does nothing", while "the DI works perfectly".

## Symptom (measured)

| When | Reading |
|---|---|
| 03:52-03:53 UTC, DI loop `di_chords.wav`, fil4 `gain` 0 → −18 | `openrig://di` `out_dbfs` −10.6/−11.3 → −18.5, −44.6, −33.9 (single instantaneous peaks, read at arbitrary points of a 12 s loop) |
| 03:55 UTC, live guitar, same edit | `openrig://routes` peak over 4 s: −5.9 / −4.6 → −6.1 dBFS |
| Chain at the time (03:52 `openrig://ids`) | fil4 FIRST, then `nam_marshall_1959bja_a2` (gain 6, input high, gate −65.5 dB), then `ir_green_day_dookie`; `guitarra-1` (mono In 1 → stereo Main `[0,1]`); no insert |
| 05:14 UTC, app log of the owner's `DIGITAL` activation | `processing=Stereo` for both heads; fil4 built with the 19 numeric params the project holds; no fault |

## Rig

PreSonus Quantum HD 8, 44.1 kHz / 64. `guitarra-1` / `guitarra-2`: mono In 1 / In 2
into a stereo Main `[0,1]`. The owner's config also holds `guitarra-1-main-l`
/ `-main-r` (and the same for guitars 2-4): mono In → a single mono Main channel.

## Reproduce

```sh
nice -n 19 cargo test -j 4 -p infra-cpal --lib issue_992
OPENRIG_HW_TESTS=1 cargo test -p infra-cpal \
    --test issue_992_block_edit_reaches_the_live_input -- --nocapture --test-threads=1
```

The HW file needs the BlackHole 2ch and MJAudioRecorder virtual loopbacks (the
owner's interface is never touched). The fil4 is a repo fixture
(`crates/engine/tests/fixtures/plugins/lv2/x42_fil4`, git-lfs).

## Hypotheses

| # | Hypothesis | Verdict | Evidence |
|---|---|---|---|
| H1 | The live edit door never delivers an LV2 param change to the runtime the live input feeds | REFUTED | `infra-cpal` lib: gain turned down while the tone plays, fresh rebuild and in-place (chain holding a VST3): 18.0 dB drop. Real streams (`start_with_io_bindings` + the `sync_live_chain_runtime` door): 18.0 dB, stereo Main, one and two guitars |
| H2 | A true-stereo LV2 block is skipped on a mono guitar input | REFUTED for a stereo Main, CONFIRMED for a mono Main (H6) | stereo Main: `processing=Stereo`, 18.0 dB; mono Main: see H6 |
| H3 | The DI loop and the live guitar build the chain differently | REFUTED | the DI pre-render (`engine::di_render::build_routed_di_runtime`) calls the same `build_chain_runtime_state`. The issue chain rebuilt from the transcript, the session's own `di_chords.wav` through both paths: fil4 −18 moves the output 0.8 dB on the DI path and 0.8 dB on the live path (repo DI: 1.1 / 1.1 dB) |
| H4 | The DI's "23-34 dB drop" was the filter acting | REFUTED | those were single instantaneous `out_dbfs` peaks read at different points of a loop with decays and silence; measured over the loop the DI moves 0.8 dB, like the guitar. A −18 dB pre-gain into a Marshall 1959 at gain 6 / input high changes its output ~1 dB because the amp is saturated |
| H5 | The owner's `DIGITAL` fil4 is broken live | REFUTED | its stored curve nets out: master −18 dB against a +18 dB high shelf (1879 Hz, Q 0.22) and +7.3/+6.6/+10.6 dB bands. Fil4 on vs off, owner's chain and plugins, 80 Hz-6 kHz: ≤0.5 dB on the DI path AND the live path. Each term acts on its own (gain alone −18.0 dB, without band 4 −10.5 dB at 700 Hz) |
| H6 | A chain whose outputs are all mono processes the stream in MONO, and every true-stereo block is swapped for a faulted bypass | CONFIRMED | real streams, mono In → mono Main: log `processing=Mono` + "filter model 'lv2_x42_fil4' with audio mode 'true_stereo' does not accept mono input — inserting faulted bypass", fil4 −18 → 0.0 dB drop. Deterministic RED: `a_fil4_on_a_guitar_into_a_mono_main_cuts_it_by_18_db` (0.0 dB). Violates invariant #5 (stream is stereo inside; a mono output is a mixdown) |
| H7 | A block stored without its switches (`enable`, `sec1..4`) builds with them at 0 | REFUTED | `a_fil4_stored_without_its_switches_still_cuts_the_live_input_by_18_db` green: missing ports take the plugin's TTL default |

## Shipped

Nothing yet.

## Open

- H6 fix.
- Which native filter and chain the owner tried; not measured.

## Related

- #771 (DI rendered off the live runtime), #979 (insert head fed mono send).
