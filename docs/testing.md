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
