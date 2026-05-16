---
name: openrig-code-quality
description: Use when writing, editing, or refactoring code in this project — language-agnostic methodology rules (zero coupling, single source of truth, separation of concerns, file organization, naming, anti-patterns)
---

# Code Quality — Architecture Methodology

Methodology rules for ANY code in this project. Apply BEFORE writing, not after. No exceptions.

---

## LEI — fechar issue exige milestone

**ANTES de chamar `gh issue close N`:** rodar `gh issue view N --json milestone` e confirmar que tem milestone atribuído. Se não tiver:

1. Identificar a release que vai (ou já) contém o fix: a `vX.Y.Z-dev.M` do ciclo de develop atual.
2. Se o milestone está `closed` (porque a tag já saiu), reabrir via `gh api -X PATCH /repos/<owner>/<repo>/milestones/<id> -f state=open`, atribuir, e re-fechar — preserva o histórico real do que entrou em qual release.
3. Só então `gh issue close N`.

Vale igual pra issue criada e fechada na mesma sessão. Sem exceção.

Plus: **PRs também** — `gh pr edit <N> --milestone "<vX.Y.Z-dev.M>"` antes do merge. Quando o GitHub copia a PR pro changelog da release, vê o milestone e classifica.

**Por que isso importa.** O release notes do GitHub agrupa por milestone. Issue/PR fechada sem milestone vira release notes pobre — usuário lendo o changelog não sabe que aquilo foi entregue na release. Já tivemos 20 issues acumuladas sem milestone (sessão 2026-05-13); ter que abrir/atribuir/fechar milestone retroativamente pra 20 issues é o custo de não cobrar antes.

**Anti-padrão:**
```
❌ gh issue close 423
❌ gh issue edit 423 --add-label closed
```

**Padrão correto:**
```
✅ gh issue edit 423 --milestone "v0.1.0-dev.19"
✅ gh issue close 423
```

---

## LEI — docs e referências SEMPRE em sincronia com o código

**Documentação não é "depois". É parte da tarefa.** Toda mudança que altera comportamento, API, fluxo ou modelo precisa atualizar — no MESMO commit — todas as camadas de doc afetadas:

| Camada | Para quem | Quando atualizar |
|---|---|---|
| `docs/**/*.md` | humanos (contribuidores, usuários) | mudou comportamento de áudio, fluxo de UI, block, parâmetro, screen, CLI, deploy, hardware |
| `CLAUDE.md` (raiz) | toda sessão Claude | mudou invariante, hierarquia de trade-offs, regra geral |
| `.claude/skills/*/SKILL.md` | sessão futura do Claude | mudou metodologia, anti-pattern, debt file, gate, processo, gitflow detalhe |
| `~/.claude/projects/<slug>/memory/*.md` | sessão futura do Claude | feedback do user, decisão de projeto, referência externa |
| `README.md` + `README.pt-BR.md` + `README.es-ES.md` | mundo (3 línguas) | mudou tagline, feature list, build/deploy, link |
| `CONTRIBUTING.md` | contribuidores | mudou processo de contribuição |

**Why:** sessão futura (Claude **ou** humano) precisa enxergar o estado real. Doc desatualizada vira mentira que se propaga: o próximo contribuidor segue a doc errada e quebra produção; o próximo Claude lê a skill desatualizada e aplica regra que não vale mais. Vimos isso em #435 — eu não rodei o demo nem uma vez porque a skill `slint-best-practices` não tinha "valide visualmente antes de declarar pronto" como regra explícita. Esse furo foi caro.

**How to apply:**
- Antes de `git commit`, lista mental: mudei comportamento? → quais .md afetam? → atualizou todos?
- Renomeou modelo/parâmetro/effect_type? → grep cross-repo em `docs/**`, `*.md`, `README*`, `CLAUDE.md`, todos `.claude/skills/*/SKILL.md`.
- Aprendeu uma regra nova durante a sessão (feedback do user, anti-pattern descoberto, decisão arquitetural)? → escreve na skill **antes** de fechar a sessão. Não confia em memória pessoal — escreve.
- Mudou processo de gate/build/deploy? → atualiza `openrig-code-quality`, `rust-best-practices`, `slint-best-practices`, **e** o `docs/development/*.md` correspondente.
- Mudou invariante (latência, isolation, mixing)? → `CLAUDE.md` + `docs/architecture.md`.

**Anti-padrões:**
```
❌ "depois eu atualizo a doc" — sessão acaba, doc fica órfã, próxima quebra.
❌ Commit que mexe em comportamento sem tocar nenhum .md.
❌ Skill desatualizada porque "eu lembro de cabeça" — Claude da próxima sessão não lembra.
❌ README.md (en) atualizado mas pt-BR/es-ES não — [[feedback_readme_three_languages]].
```

**Padrão correto:**
```
✅ Mesma PR/commit: código + docs/<area>.md + .claude/skills/<area>/SKILL.md + (se aplica) READMEs em 3 línguas.
✅ Skill é LIVING DOCUMENT — quando o user corrige um padrão, a correção vai pra skill no MESMO turno, antes de fechar.
```

---

## LEI — TDD obrigatório, sempre teste primeiro

**Antes de tocar QUALQUER linha de código de produção:**

1. **Escreva o teste que prova o bug ou valida a feature.** Reproduz o sintoma exato relatado pelo usuário.
2. **Rode o teste e confirme RED.** Se passa de primeira, o teste está cobrindo o caso errado — refaz.
3. **SÓ ENTÃO** edita código de produção pra fazer o teste passar (GREEN).
4. **Refatora** se necessário, mantendo tests verdes.

Sem exceções. Vale pra:
- Fix de bug (mesmo "trivial").
- Feature nova.
- Refactor que muda comportamento observável.
- Mudança em parser, registry, dispatch, qualquer caminho de dados.

Anti-padrões proibidos:
```
❌ "vou tentar essa correção" → edita prod → roda app → quebra → tenta outra
❌ "depois eu adiciono o teste" → ship sem cobertura → regride na próxima sessão
❌ teste escrito DEPOIS do fix passando — não prova nada
```

Pegou usuário 5 ciclos pra cobrar isso. Não pegue de novo.

---

This skill is **language-agnostic**: it covers methodology principles (decoupling, ownership, naming, file organization) that apply identically to Rust, Slint, Python, shell, YAML, etc. Language-specific rules (Cargo workflow, Slint-only gotchas) live in:

- `rust-best-practices` — Rust idioms + OpenRig Cargo workflow (validate.sh, cargo clean, zero warnings, cfg guards)
- `slint-best-practices` — Slint UI rules (file size cap, sed-safety, `@image-url` constraints)

Premissas gerais do projeto (Superpowers obrigatórios por situação, rastreabilidade de issue, distribuição cross-platform, alterações no SO da placa, atualização de documentação) vivem em `CLAUDE.md` e são carregadas em toda conversa. Esta skill cobre apenas as regras de **metodologia** de qualidade.

---

## Processo de validação (LEI — não pular nenhum passo)

Ordem obrigatória antes de abrir PR:

1. **Implementar** no `.solvers/issue-N/` (workspace isolado do gitflow).
2. **`cargo test --workspace --lib`** verde no solver.
3. **`git push` da branch** (sem PR ainda).
4. **Usuário valida na máquina dele** (`git checkout <branch> && git pull` → roda app/testa cenário). Esperar feedback explícito antes de prosseguir.
5. **`./scripts/qa.sh`** rodar e ficar verde.
6. **Só ENTÃO** `gh pr create`.

Não inverter:
- PR antes da validação do usuário = retrabalho quando ele acha problema no comportamento real.
- PR antes do qa.sh = CI falha e abre sticky comment no PR.
- qa.sh antes do push = bloqueia o usuário de testar enquanto roda (qa.sh demora ~25min).

## Quality Gate — comparativo único (issues #404 / #410)

`scripts/qa.sh` é o **único** gate, igual local e em CI. Roda no passo 5 acima:

```bash
./scripts/qa.sh
```

Compara 6 métricas do PR contra `origin/develop` e falha **apenas** se alguma piorou:

| # | Métrica | Falha se |
|---|---|---|
| 1 | fmt errors | PR > base |
| 2 | clippy errors (`-D warnings`) | PR > base |
| 3 | build errors | PR > base |
| 4 | test failures | PR > base |
| 5 | complexity violations | PR > base |
| 6 | coverage % | PR < base − `QA_COV_MARGIN` (1.0pp) |

Local extrai baseline em `/tmp/qa-baseline` automaticamente; CI passa `QA_BASELINE_DIR=baseline`. Detalhes em `docs/development/quality-gate.md`.

**Regra desta skill:** o gate cuida da regressão de métrica mecânica. Esta skill foca no que o gate não consegue medir — invariantes de áudio, decisões de arquitetura, qualidade **semântica** dos testes (comportamento ≠ cobertura), anti-patterns.

**Forbidden** pra silenciar o gate sem fix real: subir thresholds em `clippy.toml`, `#[ignore]`, `#[allow(...)]` sem causa raiz, `--no-verify`. Sempre causa raiz ou escalar.

---

## Comunicação — claro e objetivo

Resposta ao usuário **default = 1-3 frases**. Nada de testamento.

- Pergunta sim/não → resposta sim/não + 1 frase de contexto se necessário.
- Status → 1 linha por item.
- Decisão → 1 recomendação direta. Outras opções só se pedido.
- Sem headers/tabelas/bullets aninhados a menos que o conteúdo seja referência mecânica.
- Cortar saudação, prefácio, recap do que o usuário acabou de dizer, "espero que ajude".
- Bloco de código curto OK quando É o conteúdo pedido (comando, snippet).

Expandir só quando o usuário pedir explicitamente ("explica em detalhe", "lista as opções", etc).

---

## STOP — Check Before You Code

### 1. Data Ownership

- [ ] Information defined in the RIGHT place? (owner module, not consumer)
- [ ] Reading from source, or duplicating/inferring?
- [ ] Using `starts_with()`, `contains()`, string matching to determine type/brand? → **WRONG**

### 1b. Separation of Concerns (Business vs Presentation)

- [ ] **NEVER mix UI/visual config in business logic code** — colors, fonts, panel sizes, brand strip colors are GUI concerns
- [ ] Business logic modules define ONLY: id, display_name, brand, backend_kind, schema, validate, build
- [ ] Visual config (panel_bg, panel_text, brand_strip_bg, model_font) lives in the GUI layer
- [ ] Visual config should be in configuration files (YAML/JSON) in the GUI assets, NOT in business logic structs
- [ ] Adding or changing a color/font NEVER requires recompiling a business logic module

### 2. Zero Coupling

- [ ] Code references specific model IDs, brand names, effect types? → **WRONG**
- [ ] Adding a new model requires changing this file? → **WRONG**
- [ ] Match/if chain grows when new models are added? → **WRONG**
- [ ] Consumer knows about specific producers? → **WRONG**

### 3. Single Source of Truth

- [ ] `DISPLAY_NAME` ONLY in the owning module (never in schema, never hardcoded elsewhere)
- [ ] `brand` ONLY in the model definition (never inferred from model_id)
- [ ] Colors ONLY in the model visual config (never hardcoded in UI)
- [ ] String appears in 2+ places? → extract to const
- [ ] **ZERO string literals in comparisons** — `==`, `match`, `if` must use `const`. Never `"preamp"`, always `EFFECT_TYPE_PREAMP`
- [ ] Effect types, brands, model IDs — ALL constants, never inline strings

### 4. Naming

- [ ] Module files prefixed by backend (e.g. `native_`, `nam_`, `ir_`, `lv2_`)
- [ ] `DISPLAY_NAME` does NOT contain brand name (brand is its own field)
- [ ] Commits in English, no `Co-Authored-By` trailers
- [ ] Branch names follow `feature/issue-N` or `bugfix/issue-N` (no description suffix)

### 5. No Trash

- [ ] No serde aliases for old names — update the data instead
- [ ] No dead/commented code
- [ ] No workarounds/hacks
- [ ] Renamed something? → ALL references updated (code + YAML data files + presets)

### 6. Impact Analysis (from real failures)

Before making a change, verify:
- [ ] **Build system**: Does any build script depend on file names? (e.g., `starts_with("compressor_")` breaks if file renamed to `native_compressor_`)
- [ ] **UI capabilities**: Does the target UI component support ALL widget types needed? (file picker, bool toggle, numeric, enum)
- [ ] **Callback chain**: Are ALL callbacks connected through the full chain? (model → crate → catalog → adapter-gui → Slint)
- [ ] **Window sizing**: If changing UI content, does the window size accommodate it?

### 7. Safe Refactoring

- [ ] **After changing struct fields**: update ALL modules that construct the struct
- [ ] **Test visually** before committing UI changes — don't assume it looks right
- [ ] **One concern per commit** — don't mix refactoring with feature changes

### 8. Responsive UI

- [ ] **All UI elements must be responsive** — never invade adjacent areas
- [ ] Elements must adapt to window/panel size
- [ ] No hardcoded absolute positions that break at different sizes
- [ ] Test with minimum and maximum window sizes before committing
- [ ] Overflow/clip must be handled — if content doesn't fit, it should scroll or truncate, never overflow

### 9. File Organization — ONE RESPONSIBILITY PER FILE (ABSOLUTE)

- [ ] **One concern per file — no exceptions.** If you can describe a file with "and", it has too many responsibilities
- [ ] **`lib.rs` (or equivalent module entry) is for re-exports only** — NEVER implement logic there; move it to a named module
- [ ] Configuration files organized by component/domain (e.g., `visual_config/preamp.rs`, `visual_config/delay.rs`)
- [ ] A file with a match/if that grows with every new model → **WRONG, split by component**
- [ ] If a file has 50+ lines of match arms → it needs to be split immediately
- [ ] **God files are forbidden** — a file that 10+ different features touch is a god file; split it
- [ ] New feature? New file. Don't add to an existing file that already has a different concern

> Concrete file size limits per language live in `rust-best-practices` (600 lines for `.rs`) and `slint-best-practices` (500 lines for `.slint`).

**Known god files — never expand further (tracked in issue #276). Check current size before touching:**
- `crates/adapter-gui/src/lib.rs` — split in progress
- `crates/project/src/block.rs` — split in progress
- `crates/block-core/src/lib.rs` — split in progress
- `crates/block-core/src/param.rs` — split in progress

**Anti-patterns:**
```
❌ Adding a new function to adapter-gui/src/lib.rs
   // WRONG: already a god file. Create a new module instead.

❌ lib.rs with 200 lines of business logic
   // WRONG: lib.rs = re-exports only

❌ A match arm in block.rs growing from 13 to 14 branches
   // WRONG: the dispatch belongs in each block's own crate via trait

✅ crates/adapter-gui/src/device.rs — only device management
✅ crates/adapter-gui/src/project.rs — only project persistence
✅ crates/adapter-gui/src/chain.rs — only chain editing
```

### 10. Test Coverage (OBRIGATORIO)

- [ ] **Toda feature/bugfix DEVE ter testes** — sem exceção
- [ ] Nomenclatura: `<behavior>_<scenario>_<expected>` (ex: `validate_project_rejects_empty_chains`)
- [ ] Testar comportamento real, não mocks de fachada
- [ ] **Builds que dependem de assets externos**: bundlar fixture mínimo dentro de `crates/<x>/tests/fixtures/` (ver `engine/tests/fixtures/plugins/source/nam/` em #413). Test passa SEMPRE.
- [ ] **Registry tests**: iterar sobre TODOS os modelos via registry (schema, validate, build)
- [ ] Helpers de teste no próprio módulo — sem crate de test-utils separado

> Detalhes Rust-específicos (golden samples `1e-4`, `#[cfg(test)] mod tests`, `cargo test --workspace`) ficam em `rust-best-practices`.

### 10b. `#[ignore]` é PROIBIDO (LEI)

`cargo test --workspace` é o gate de comportamento. Test marcado `#[ignore]` NÃO PARTICIPA do gate — vira documentação morta. **Em hipótese alguma** adicionar `#[ignore]` (ou equivalente: `#[cfg(any())]`, `if false {}`, etc.). Auditoria de 2026-05-11 encontrou 40 ignored em 1771 totais; alvo é **zero**.

Razões "razoáveis" que NÃO são exceção:

| Caso real | Saída CORRETA |
|---|---|
| "depende de asset externo (NAM, IR, LV2)" | Bundle fixture mínimo dentro de `tests/fixtures/`. ~1 MB é aceitável. |
| "precisa --release pra timing" | Vire benchmark (`cargo bench`) ou aumente tolerância em debug. Não ignore. |
| "pending issue #X — comportamento atual está errado" | Test asserta o SINTOMA ATUAL ou descreve a regressão; quebra quando fixar #X. Não ignore. |
| "depende de FFI/dylib externo" | `build.rs` copia dylib pro `target/`; ou skip por plataforma com `#[cfg(target_os = "...")]`. Cfg-skip é OK; ignore não é. |
| "paths absolutos da máquina do dev" | COPIE pra dentro do repo (ver `engine/tests/fixtures/`). |
| "demora demais no CI" | Cobertura unitária equivalente + um path sample no integration. Não ignore. |

Validação: `cargo test --workspace 2>&1 \| grep "ignored" \| grep -v "0 ignored"` deve retornar VAZIO. Qualquer `ignored > 0` é débito a fixar antes de merge.

**Anti-Pattern (Testes):**
```
❌ Commitar código sem testes
   // WRONG: código sem teste é dívida técnica

❌ #[ignore] em qualquer test
   // WRONG: vira documentação morta. Bundle o fixture, copie o
   // asset, ajuste o cfg — mas NUNCA ignore.

❌ Criar crate test-utils separado
   // WRONG: cada módulo deve ser autossuficiente em testes

❌ Usar mockall ou frameworks de mock
   // WRONG: testar código real, não mocks
```

---

## YAML Data Files

When renaming effect types, models, or identifiers:
- Update `project.yaml` in project root
- Update `preset.yaml` if exists
- Update ANY yaml files the user mentions
- **Never** add serde aliases — update the data instead
- Search: `grep -rn "old_name" **/*.yaml`

---

## Anti-Patterns

```
❌ if model_id.starts_with("marshall") { "marshall" }
   // WRONG: inferring from string

❌ match model_id { "american_clean" => color(...) }
   // WRONG: hardcoding by model_id

❌ pub const DISPLAY_NAME: &str = "Marshall JCM 800";
   // WRONG: brand in display name

❌ if effect_type == "preamp" { ... }
   // WRONG: string literal in comparison

❌ #[serde(alias = "amp_head")]
   // WRONG: legacy alias

❌ use_panel_editor: true  // for ALL types without checking UI supports them
   // WRONG: enabling feature without verifying capability

❌ // UI color/font in a business-logic module:
   pub const MODEL_DEFINITION = GainModelDefinition {
       panel_bg: [0x1a, 0x5c, 0x2a],   // UI color in business logic!
       model_font: "Permanent Marker", // UI font in business logic!
   };
   // WRONG: visual config in business logic crate. Move to UI config
```

## Correct Patterns

```
✅ // Business data from catalog
   let brand = catalog_entry.brand;
   let type_label = catalog_entry.type_label;

✅ // Visual config from UI layer (NOT from business crate)
   let vc = visual_config::for_model(&item.brand, &item.model_id);
   let panel_bg = vc.panel_bg;

✅ // Model definition has ONLY business logic
   pub const MODEL_DEFINITION = PreampModelDefinition {
       id: MODEL_ID,
       display_name: DISPLAY_NAME,   // No brand in name
       brand: "marshall",            // Business data
       backend_kind: PreampBackendKind::Nam,
       schema, validate, build,      // Business logic only
       // NO colors, fonts, or visual config here
   };

✅ // Before renaming files, check build.rs
   grep "starts_with\|stem ==" crates/block-*/build.rs
```

---

## Review Trigger

After writing code:
1. Add new model WITHOUT touching the UI layer? If yes → coupling
2. Change brand color WITHOUT touching the UI? If yes → coupling
3. File has match/if listing specific models? → coupling
4. Visual result matches expectation? → test before commit

## Red Flags — STOP and Redesign

- Adding a model requires changes in 3+ files
- Match arm count equals number of models
- Consumer imports producer's internal types
- Same string appears in code AND UI AND YAML
- Feature flag enables something the UI can't handle
- "Quick fix" that hardcodes a value
- Path is hardcoded as string literal

---

## Living Document

This skill is a LIVING DOCUMENT. Every time the user corrects a methodology mistake:
1. Identify the violated principle
2. Add a rule or anti-pattern to this skill (if methodology) or to `rust-best-practices` / `slint-best-practices` (if language-specific)
3. Commit the updated skill

This ensures the same mistake is never repeated.
