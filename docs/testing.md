# Testes

## ⛔ TDD obrigatório — RED-FIRST. Proibido implementar sem teste que falha antes

**Lei do projeto, não recomendação.** É **proibido** escrever ou alterar código de
produção sem um teste que **falhou primeiro**. Teste escrito depois da
implementação (que passa de imediato) não prova nada e "vicia" a suíte — também
proibido.

**Para corrigir um bug, nesta ordem:**

1. **Entrevistar quem reportou** — cenário exato, dados, passos, resultado
   esperado vs. obtido. Não adivinhar.
2. **Escrever um teste que reproduz o bug** pelo caminho mais real possível —
   **sem ler o código procurando a causa antes disso.**
3. **Rodar e ver FALHAR** (RED real). Mostrar a falha. Se o teste passa, ele
   não pegou o bug → refazer; ou, se não for bug de lógica (ex.: renderização
   Slint, que unit test não exercita), **dizer isso honestamente e parar**.
4. **Só depois do RED**, investigar a causa — guiada pelo teste que falhou —
   e corrigir até passar (GREEN).
5. Rodar a suíte cheia + invariantes de áudio.

**Não investigue o código para achar a causa antes do teste existir e
falhar.** Ler o código primeiro produz hipótese enviesada vendida como
"causa". A investigação acontece no passo 4, dirigida pelo RED.

**Provar que um teste não é "viciado":** reverter SÓ a produção para o estado
pré-fix (mantendo os testes) e rodar — tem que dar RED. Restaurar a produção
depois (nada se perde; está commitado).

**Enforcement:** o gate é o hook genérico do plugin `dev-rules` (não mais um
hook local do OpenRig), configurado em `.dev-rules.json` na raiz (globs Rust:
`crates/**/src/**` produção, `**/tests/**`/`*_test*.rs`/`*test*.rs` teste).
Sentinelas em `.dev-rules/` (nunca versionado):

- nenhuma sentinela → leitura E edição de produção bloqueadas (disciplina de bug).
- `.dev-rules/.mode-feature` → leitura de produção liberada pra planejar
  feature/melhoria; edição continua presa ao RED.
- `.dev-rules/.red-first-unlocked` → leitura e edição liberadas (criar só
  depois de mostrar o RED real, passo 3 acima).

Detalhamento e casos reais: `.claude/skills/openrig-code-quality/SKILL.md`.

## Cobertura

- **Ferramenta**: `cargo-llvm-cov` (instalar com `cargo install cargo-llvm-cov` + `rustup component add llvm-tools-preview`)
- **Script local**: `scripts/coverage.sh` — gera relatório HTML em `coverage/`
- **CI**: `.github/workflows/test.yml` — informativo, sem gate

## Convenções

- `#[cfg(test)] mod tests`
- Nomes: `<behavior>_<scenario>_<expected>` (ex.: `validate_project_rejects_empty_chains`)
- Sem framework externo. Helpers no próprio módulo.

## Categorias

- **Integração com áudio real**: `#[ignore]` (rodar com `cargo test -- --ignored`)
- **DSP nativos**: golden samples com tolerância `1e-4`, processar silêncio/sine, verificar non-NaN
- **Caracterização de DSP nativos** (block-delay, `src/dsp_probe.rs`, test-only): provas determinísticas de que cada modelo cumpre a proposta dele — timing de eco (`peaks`), decaimento por feedback, brilho/escurecimento (`spectral_centroid`), saturação (`harmonic_ratio`). Não basta non-NaN: o teste mede a característica que dá nome ao modelo (#388)
- **NAM/LV2/IR builds**: `#[ignore]` (assets externos)
- **Registry tests** em block-* crates: iterar TODOS os modelos via registry
- **Deadline / xrun (timing)**: `#[cfg_attr(debug_assertions, ignore)]` — só
  fazem sentido em release. `engine/src/audio_deadline_tests.rs` (pipe chains)
  e `engine/tests/issue_670_heavy_rig_deadline.rs` (rig pesado, breakdown
  por-bloco) medem o custo por-buffer do audio thread. O custo é dominado
  pela inferência NAM; empilhar vários NAM amps satura o orçamento de 64
  frames → overrun de deadline (xrun) → crackle. O overrun é contado em
  runtime por `ChainRuntimeState::record_callback_load` (#670), alimentado
  pelo callback de input via `infra-cpal::callback_load_timing`.

### Looper (#323)

The looper is a recorder living on the audio thread, so it is covered at
every layer instead of "it plays, ship it":

| Suite | Proves |
|---|---|
| `engine/src/looper_tests.rs` | the state machine and layer maths — record → play, overdub sums, undo/redo, redo tail dropped by a new recording, ceiling freeze, speed/reverse/decay, export mixdown |
| `engine/src/looper_bank_tests.rs` | the op queue: slot claim, buffer hand-back for unknown uids, mono mixdown, per-looper params |
| `engine/src/looper_runtime_tests.rs` | the callback path: recorded dry input reaches the output, a chain without loopers stays byte-identical silence, a loop survives a runtime rebuild, and one chain's loop never reaches another (invariant #4) |
| `audio_alloc_invariant_tests::looper_record_overdub_and_undo_do_not_allocate` | zero allocation on the audio thread while recording / overdubbing / undoing (invariant #8) |
| `infra-cpal/tests/issue_323_controller_loopers.rs` | ops fan out to every runtime of a chain, each with its OWN buffer |
| `application` dispatcher + `query_loopers` tests | command validation, the footswitch uid-0 sentinel, and the read model every transport shares |
| `adapter-gui/tests/issue_323_looper_wiring.rs` | the #614 trap: dispatching alone is dead — a `LooperCommand` must flip the store and the loop's isolated stream, on the bus and with no GUI in the picture |
| `adapter-gui/tests/issue_323_looper_panel_interaction.rs` | real pointer events on the panel: every transport button fires, disabled ones do not, each row reports its own uid |
| `adapter-gui/src/runtime_loopers_tests.rs` | save → reopen round-trip of the wav sidecar (the save dispatched, not called), and that a missing sidecar never blocks opening a project |
| `application/src/local_dispatcher_looper_save_tests.rs` | `SaveProject` exports the loops itself, forgets a cleared loop's stale pointer, and touches nothing when the rig is stopped |
| `infra-cpal/tests/issue_323_looper_hw.rs` (`OPENRIG_HW_TESTS=1`) | the REAL stack: record + 7 overdubs + undo/redo/clear on live CoreAudio streams at buffer 64 cost **zero** xruns / underruns |

The hardware test builds its rig **in the test** instead of loading a fixture
preset: the shipped presets reference the owner's NAM/LV2 capture library, so
on a machine without it every block is dropped, the chain never comes up, and
the counters read zero for a runtime that does not exist — a vacuously green
measurement. It asserts the chain is live before measuring, and drives
`poll_pending_rebuilds` the way the app's timer does, because the cold
activation is asynchronous (#740).

## Workspace

```bash
cargo test --workspace
```

(~1100+ testes)

## Real-hardware battery (issues #670 / #698)

`crates/infra-cpal/tests/issue_670_cab_swap.rs`,
`crates/infra-cpal/tests/issue_670_real_streams_no_xruns.rs`,
`crates/infra-cpal/tests/issue_698_pitch_shifter_live.rs` and
`crates/infra-cpal/tests/issue_698_owner_64_dual_chain.rs` open the REAL
audio interface (CoreAudio streams, the owner's presets and DI takes) and
assert real-time deadlines through the engine's own xrun/underrun counters.
They are the full-fidelity reproduction harness for the #670 crackle and
the #698 multi-chain RT-budget overcommit (shared helpers live in
`tests/hw_harness/`). The #698 owner-recipe tests additionally need the
real capture library via `OPENRIG_OWNER_PLUGINS=<plugins/source>`.

They are only meaningful on an otherwise idle machine, so they are gated by
an environment variable and return immediately (with a loud notice on
stderr) when it is absent — they never fail under the parallel workspace
suite or the quality gate for reasons unrelated to the app. **Any agent or
contributor can (and should) enable them when validating audio-path
changes:**

```sh
OPENRIG_HW_TESTS=1 cargo test -p infra-cpal --release \
    --test issue_670_cab_swap --test issue_670_real_streams_no_xruns \
    --test issue_698_pitch_shifter_live --test issue_698_owner_64_dual_chain
```

Requirements: macOS, a real input/output interface connected (the suite
looks for the Scarlett by name), an idle machine, and ~12 minutes. The
tests serialize access to the physical device across processes via a lock
file.

The same gate covers the metronome's runtime doors
(`crates/adapter-gui/src/metronome_runtime_tests.rs`, issue #127): starting the
click means `find_output_device_by_id` → `host.output_devices()`, so those
tests enumerate the machine's real interfaces and one of them opens a (silent)
output stream. They are seconds, not minutes:

```sh
OPENRIG_HW_TESTS=1 cargo test -p adapter-gui --lib runtime_lifecycle::metronome_tests
```

What the ORDER those doors write the generator's `enabled` flag in — the click
is marked playing only once its stream is proven open — is pinned headless in
`crates/adapter-gui/src/runtime_pipelines_tests.rs` and runs in the normal
suite.

The teardown / whole-graph doors (issue #127) need no hardware either. At the
dispatcher level, `crates/application/src/local_dispatcher_runtime_doors_tests.rs`
drives a spy `RuntimeControl` and pins that `StopProjectRuntime` and
`CloseProject` stop the rig, that `SaveAudioSettings` rebuilds the whole graph
(and surfaces a refusal as a dispatch error), and — the isolation pin — that
`RemoveChain` touches ONLY the deleted chain: the survivor is neither re-synced
nor named, and the rig-wide stop never fires. At the GUI level,
`crates/adapter-gui/src/runtime_lifecycle_control_tests.rs` drives the same
commands against a device-less controller and asserts the controller is dropped
and the engine rate goes back to the reference, plus that the whole-graph
rebuild never CREATES a controller on a stopped rig.

The poll tick's doors (`crates/adapter-gui/src/runtime_health_tests.rs`, issue
#127) need no hardware: they drive a device-less controller
(`ProjectRuntimeController::for_testing`) whose rebuilds resolve their
endpoints from the binding registry. They pin that a finished rebuild is
INSTALLED by the write door (and only into the chain that asked for it), that a
`BlockError` the audio thread posted surfaces through the read door naming its
chain, and that the read drains — so nobody promotes it to a shared query.

`crates/adapter-gui/src/chain_row_seams_tests.rs` pins the meter tick's three
per-chain doors against a device-less controller: a hosted chain reports its own
runtime state (live, with its own xrun/underrun counters) and its own DI state,
a chain with no runtime on a hosted frontend reads `live: false` rather than
`None`, a stopped rig answers neither, and the looper reconcile gives the
project's looper its slot while leaving a sibling chain's store empty.

`crates/adapter-gui/src/block_stream_read_tests.rs` pins the block-diagnostic
read's two states against a device-less controller: a hosted runtime always
answers (an empty table for a block that publishes nothing), and a stopped rig
answers `None` — the distinction the panel uses to decide between "go inactive"
and "keep showing what you have".

The subscription seam is tested on both sides and needs no hardware either.
`crates/application/src/audio_taps_tests.rs` pins the CONTRACT — a tap that
carries no PCM is still a complete implementation, a frontend that hosts no
audio subscribes to nothing, and a `TapPoint` is never satisfied by a stream
index alone. `crates/adapter-gui/src/runtime_taps_tests.rs` drives the GUI's
implementation against a device-less controller: the reduced reading equals the
peak of the window the audio callback pushed, the raw window honours its cap, a
subscription never hears a sibling chain, and one multi-channel subscription
keeps its channels apart.

## Real-plugin VST3 battery (issues #776 / #780)

Tests that load a real catalog VST3 (ChowCentaur) are gated on
`OPENRIG_TEST_VST3_DIR` — the plugins `vst3/` dir (e.g.
`<OpenRig-plugins>/plugins/source/vst3`) — and skip cleanly when it is unset,
so CI and the parallel suite stay green. They must run single-threaded
(`--test-threads=1`): JUCE plugins refuse *concurrent* instantiation.

- `crates/vst3-host/tests/issue_776_catalog_vst3.rs` — discovery + processing.
- `crates/vst3-host/tests/issue_780_capture_params.rs` — `capture_vst3_params`
  reads live controller values; two same-model instances don't collide.
- `crates/project/tests/vst3_editor_open_policy.rs` — editor open resolves by
  block instance key, not model id.
- `crates/application/tests/issue_780_vst3_persist.rs` — end-to-end: a
  native-editor param change persists via `CaptureRigEdits`.

```sh
OPENRIG_TEST_VST3_DIR=<OpenRig-plugins>/plugins/source/vst3 \
    cargo test -p vst3-host -p project -p application \
    --test issue_780_capture_params --test vst3_editor_open_policy \
    --test issue_780_vst3_persist -- --test-threads=1
```
