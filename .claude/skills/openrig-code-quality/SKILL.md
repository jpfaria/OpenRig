---
name: openrig-code-quality
description: Use when writing, editing, or refactoring code in OpenRig — project-specific rules that COMPLEMENT (do not duplicate) xgodev `dev-rules` and `quality-gate` skills
---

# Code Quality — OpenRig-specific complement

This skill carries **only what `dev-rules` and `quality-gate` do not cover**:
the OpenRig-specific architecture, gitflow, i18n, audio invariants, file
inventory, and tooling. Anything generic (TDD/RED-first, docs synced same
commit, ownership/coupling/SoT, naming, file organization, DDD, verify
before done, no silent fallback, no skipped tests, communication, generic
red flags, living-document discipline) lives in **`dev-rules`** and is
the source of truth for those rules. Gate mechanics (dispatcher, JSON
parsing, bypass governance) live in **`quality-gate`**. Do not restate
either here; cite them.

---

## LEI ZERO — PROIBIDO supor quando não está claro; perguntar simples até clarear

**EU SOU PROIBIDO DE SUPOR QUANDO AS COISAS NÃO ESTÃO CLARAS. EU PRECISO PERGUNTAR DE FORMA SIMPLES ATÉ TUDO FICAR CLARO.**

Antes de tocar código, arquitetura ou teste: se o escopo, o modelo de dados, o comportamento esperado, a camada certa ou QUALQUER detalhe não está 100% claro, **PARAR e perguntar** — uma pergunta curta e direta de cada vez, até não restar dúvida. PROIBIDO "vou de cabeça e depois conserto"; PROIBIDO inventar caminho; PROIBIDO escolher entre A/B por conta própria quando o usuário não escolheu.

**Why:** supor inverteu o que o usuário pediu e queimou dias — ex.: I/O dentro da chain vs. fora (sistema/binding); "esconder bloco" quando o pedido era NÃO ter bloco; teste-depois quando o pedido era teste-antes. Cada suposição errada vira cascata de retrabalho e quebra a confiança.

**How to apply:** dúvida → pergunta simples → espera a resposta → só então age. Não agrupar várias suposições numa tacada só. Não seguir "parece o passo lógico seguinte". Isto NÃO conflita com "não perguntar o óbvio" (escopo já acordado / default trivial segue direto): a regra vale quando há ambiguidade real — na dúvida entre perguntar e supor, **perguntar**.

---

## LEI — PROIBIDO supor/inventar layout de UI; renderize e confira

É **PROIBIDO** escrever, alterar ou julgar ("ficou bom") QUALQUER layout/UI — `.slint`, posicionamento, espaçamento, hierarquia, alinhamento, componentes visuais — **sem antes invocar `ui-ux-pro-max` (design/UX) + `slint:slint` (plugin slint@slint) + `slint-best-practices`** e **conduzir o trabalho por elas**: layouts nativos do Slint (`HorizontalLayout`/`VerticalLayout`/`GridLayout` com `spacing`/`padding` — nada de `x`/`y` absoluto pra alinhar cluster), hierarquia, estados vazio/erro, alvos ≥44px.

**O agente RENDERIZA e confere** — não chuta:
- Use o renderizador headless do projeto: `tools/slint-render` (slint-interpreter → PNG, fora do workspace). Build: `cargo build --release --manifest-path tools/slint-render/Cargo.toml`. Uso: `slint-render <arquivo.slint> <Componente> <out.png> [w] [h]`. Para componentes embutidos no app, faça um mockup `.slint` standalone com dados fake (root `inherits Window`, tamanho explícito) e renderize ESSE.
- Abra o PNG (Read) e confira alinhamento/espaçamento/hierarquia **ANTES de dizer "pronto"**. Chutar layout e mandar o usuário testar é **anti-padrão proibido** (já queimou um dia inteiro de tokens entregando telas tortas).
- Tocou/criou tela → invocar `ui-ux-pro-max` + `slint:slint` **ANTES da primeira linha de `.slint`**.
- **AUTO-CRÍTICA obrigatória antes de mostrar o PNG** — rode o checklist e CORRIJA você mesmo; NÃO entregue cru esperando o usuário catar o básico (ele não é designer):
  - Hierarquia clara (tamanho/peso, não tudo igual).
  - Cor semântica: estados/categorias distintos com cor+significado (ex.: mono/estéreo badges de cores diferentes), não tudo cinza.
  - TODAS as ações CRUD presentes em cada item (criar, **editar**, excluir) — não só excluir.
  - Estados: vazio ("nada ainda" + ação), desabilitado, erro, selecionado.
  - Regras de domínio refletidas na UI (ex.: mono = 1 canal → 2º canal desabilitado).
  - Alinhamento em grid, espaçamento 8px consistente, alvos toque ok, badges/labels legíveis.
- Só DEPOIS da auto-crítica, fechamento visual final em **loop curto com o usuário** (ele aponta o que ainda está torto) — mas o básico já tem que estar resolvido.

---

## LEI — todo push entrega um bloco de handoff explícito

**Após `git push` numa branch do agente, a resposta no chat para o usuário DEVE conter — no mesmo turno, sem ser pedido — dois blocos:**

1. **Comandos git literais** que o usuário copia/cola na pasta principal pra puxar a branch. Sempre os três: `git fetch && git checkout <branch> && git pull`. Mesmo que já tenha sido dito em push anterior — o usuário trabalha com vários agents em paralelo e não consegue lembrar qual branch é qual.
2. **Checklist do que validar**, numerado, em pt-BR, ação por ação (UI flow, comando CLI, cenário de áudio). Inclui:
   - Golden path (o caminho feliz que a feature implementa).
   - Edge case que motivou a issue (o bug reproduzível ou o comportamento antigo a ser comparado).
   - Regressões a vigiar (telas/fluxos adjacentes que poderiam ter quebrado).
   - O esperado em cada passo, **um por linha**, sem prosa.

**Why:** o usuário tem N agents abrindo branches simultaneamente. "Cheque a branch" não é instrução — ele precisa do comando exato e da lista do que abrir/clicar/digitar pra ver a mudança. Sem isso, ou ele esquece de testar ou testa só superficialmente e marca como "OK" sem ter exercido o golden path.

**Anti-padrão:**
```
❌ "Push em feature/issue-N. Quer que eu continue?"
   // WRONG: sem comando, sem checklist. Usuário não sabe o que testar.

❌ "Push 68ea1bcf. Mudei chain_preset_wiring.rs."
   // WRONG: descreve arquivo, não validação. Usuário não tem app aberto na cabeça dele.
```

**Padrão correto:**
```
✅ Push <hash> em <branch>.

   Atualizar:
   git fetch && git checkout feature/issue-N && git pull

   Validar:
   1. Abrir tela Chains, clicar [load preset] → picker mostra a lista
   2. Digitar "lead" no campo de busca → só presets com "lead" no nome aparecem
   3. Selecionar um → após carregar, o combobox de preset mostra o nome do arquivo
   4. Voltar pra Launcher → nada quebrou; presets na tela inicial ainda listam normal
```

Vale para CADA push da sessão, inclusive incrementais (commit 2/3, commit 3/3). A repetição é o ponto — o usuário não memoriza branch, ele lê o bloco e segue.

---

## LEI — fechar issue exige milestone

**ANTES de chamar `gh issue close N`:** rodar `gh issue view N --json milestone` e confirmar que tem milestone atribuído. Se não tiver:

1. O milestone é a **versão semver atual ainda não lançada** (hoje `v0.1.0`) — **plain semver**. O esquema `vX.Y.Z-dev.N` está MORTO: **NUNCA crie nem reabra** um milestone `-dev.N`. Use o milestone aberto da versão atual; depois do release dela, o próximo é `v0.2.0`.
2. `gh issue edit N --milestone "v0.1.0"` → `gh issue close N`.

Vale igual pra issue criada e fechada na mesma sessão. Sem exceção.

Plus: **PRs também** — `gh pr edit <N> --milestone "v0.1.0"` antes do merge. Quando o GitHub copia a PR pro changelog da release, vê o milestone e classifica.

**Por que isso importa.** O release notes do GitHub agrupa por milestone. Issue/PR fechada sem milestone vira release notes pobre — usuário lendo o changelog não sabe que aquilo foi entregue na release. Já tivemos 20 issues acumuladas sem milestone (sessão 2026-05-13); ter que abrir/atribuir/fechar milestone retroativamente pra 20 issues é o custo de não cobrar antes.

**Anti-padrão:**
```
❌ gh issue close 423
❌ gh issue edit 423 --add-label closed
```

**Padrão correto:**
```
✅ gh issue edit 423 --milestone "v0.1.0"
✅ gh issue close 423
```

---

## LEI — docs sync: camadas específicas do OpenRig

> Regra geral ("docs no MESMO commit") está em `dev-rules` LAW 2. Aqui ficam só as **camadas concretas** do OpenRig que precisam ser tocadas:

| Camada | Para quem | Quando atualizar |
|---|---|---|
| `docs/**/*.md` | humanos (contribuidores, usuários) | mudou comportamento de áudio, fluxo de UI, block, parâmetro, screen, CLI, deploy, hardware |
| `CLAUDE.md` (raiz) | toda sessão Claude | mudou invariante, hierarquia de trade-offs, regra geral |
| `.claude/skills/*/SKILL.md` | sessão futura do Claude | mudou metodologia OpenRig, anti-pattern, debt file, gate, processo, gitflow detalhe |
| `~/.claude/projects/<slug>/memory/*.md` | sessão futura do Claude | feedback do user, decisão de projeto, referência externa |
| `README.md` + `README.pt-BR.md` + `README.es-ES.md` | mundo (3 línguas) | mudou tagline, feature list, build/deploy, link |
| `CONTRIBUTING.md` | contribuidores | mudou processo de contribuição |

**How to apply (OpenRig-specific):**
- Renomeou modelo/parâmetro/effect_type? → grep cross-repo em `docs/**`, `*.md`, `README*`, `CLAUDE.md`, todos `.claude/skills/*/SKILL.md`.
- Mudou processo de gate/build/deploy? → atualiza `openrig-code-quality`, `rust-best-practices`, `slint-best-practices`, **e** o `docs/development/*.md` correspondente.
- Mudou invariante (latência, isolation, mixing)? → `CLAUDE.md` + `docs/architecture.md`.
- README atualizado em uma língua sem as outras duas é regressão — [[feedback_readme_three_languages]].

---

## LEI — tela/string nova exige atualizar TODOS os catálogos de tradução

**Toda string visível ao usuário passa por i18n. Adicionou/criou tela, componente, dialog, overlay, label, botão com `@tr("chave")` (Slint) ou `t!("chave")` (Rust)? No MESMO commit:**

1. Adicionar a `chave` ao `crates/adapter-gui/translations/adapter-gui.pot`.
2. Adicionar `msgid "chave"` + `msgstr "..."` traduzido em **TODOS** os locales: `crates/adapter-gui/translations/<locale>/LC_MESSAGES/adapter-gui.po` (de_DE, en_US, es_ES, fr_FR, hi_IN, ja_JP, ko_KR, pt_BR, zh_CN — confirmar a lista com `ls translations/`).
3. Recompilar os `.mo` (o `build.rs` gera de `.po`; rodar o build/`validate.sh` confirma).
4. **Nunca** deixar `msgstr ""` numa chave nova — `.mo` vazio = a UI mostra a **tag crua** (`btn-load-preset` em vez de "Carregar preset"). Foi exatamente o bug do overlay de presets (#479): chave nova sem catálogo → tela "só com as tags".

**Por que:** sem isso a tela sai com as chaves cruas pra todo usuário não-inglês (ou todos). Não dá pra "traduzir depois" — vai pra produção quebrado. Validação: `grep -L 'msgid "chave"' translations/*/LC_MESSAGES/adapter-gui.po` deve ser vazio.

**Anti-padrão 1:** `text: "Texto cru"` direto no `.slint` (sem `@tr()`). Texto visível ao usuário **NUNCA** é literal — sempre `@tr("chave")`. Símbolos visuais (`✓`, `▼`, etc.) viram SVG via `@image-url`, não `Text`.

**Anti-padrão 2:** `@tr("nova-chave")` num componente novo sem tocar nenhum `.po`. **Padrão:** mesmo commit = componente + `.pot` + todos os `.po` + `.mo` regenerados.

**Validação automatizada (i18n_tests.rs):**

1. `every_tr_key_has_translation_in_en_pt_es` — varre todo `.slint` em `crates/adapter-gui/ui/`, extrai cada `@tr("…")` (decodificando `\u{NNNN}` e `\"`), e exige `msgstr` não-vazio em pt_BR + es_ES. RED automático se alguém adiciona `@tr` sem traduzir.
2. `no_raw_text_literals_in_settings_slint` — varre o escopo da tela de Settings, falha se `text:` aponta pra string literal não-`@tr()`. Expandir o escopo desse teste antes de adicionar `text: "x"` em qualquer .slint.
3. `settings_screen_tr_keys_are_translated_in_pt_br` — guarda específico da tela #513.

Os testes rodam em `cargo test -p adapter-gui --lib`. CI bloqueia regressão.

---

## LEI — toda funcionalidade nova é um `Command` (paridade GUI/MCP/gRPC)

**Nenhuma operação que muda estado vive só num frontend.** O `Command` enum em `crates/application/src/command.rs` é a **única fonte de verdade** do "que o app sabe fazer". Toda funcionalidade nova que muta `Project`/sessão:

1. **Nasce como variante de `Command`** + (se observável) variante de `Event`, com handler no `LocalDispatcher`. Nunca como `borrow_mut()` direto dentro de um callback de frontend.
2. **GUI** dispara via `dispatcher.dispatch(cmd)` e reage aos `Event` — nunca muta `Project` direto.
3. **MCP** (`adapter-mcp`) ganha a tool **automaticamente** (schema auto-derivado de `Command` via `application::command_schema`). Não há passo manual — mas o teste de paridade (`tool count == command_variant_names().len()`) **tem** que continuar verde.
4. **gRPC** (`adapter-server`, quando existir): mesma variante, idem.

Funcionalidade que existe num frontend mas **não** é `Command` = **gap do command bus**, não "feature do frontend". Fechar o gap adicionando a variante (ex.: `SetLanguage` foi exatamente isso — idioma era derivado de env, nunca settável; #165 fechou). Consistente com #295 ("um `Command` por operação de usuário") e com `[[feedback_backend_transport_agnostic]]`.

**Why:** o core vai virar gRPC + MCP + remoto. Se a operação só existe na GUI, o agente (MCP) e o cliente remoto (gRPC) ficam cegos pra ela — a "mão do agente" não alcança o que a "mão do usuário" alcança. Paridade não é opcional; é o contrato da arquitetura de adapters.

**How to apply:** antes de escrever qualquer fluxo que muda estado — "isso é um `Command`?". Não? Cria a variante primeiro (TDD: teste no `local_dispatcher_tests.rs`), depois liga o frontend. Auditoria de paridade ao adicionar feature: `command_variant_names()` cobre toda operação de usuário? Gap → variante nova, no mesmo PR.

**Anti-padrão:**
```
❌ on_troca_idioma => { session.project.borrow_mut().language = tag; }
   // WRONG: muta direto no callback. MCP/gRPC nunca verão "trocar idioma".

❌ "essa feature é só da GUI, não precisa de Command"
   // WRONG: toda operação de estado é Command. Sem exceção de frontend.
```
**Padrão:**
```
✅ Command::SetLanguage { tag } + Event::LanguageChanged + handler no LocalDispatcher
   → GUI dispatch(cmd); MCP tool auto; gRPC variante idem.
```

---

## LEI — toda leitura também tem paridade GUI/MCP/gRPC/MIDI

**O par dual do `Command` é a `Query`.** Se a GUI lê algum estado, todo
outro transporte tem que ler também. Não há "este número só a GUI
precisa". Meters, level peaks, latency probes, device lists, project
YAML, scene/preset state, tuner readings, spectrum frames — toda
janela de observação que a GUI tem **tem que existir em
`application::bridge::QueryKind`** (ou no equivalente da nova camada
de query) e ser servida por **todo adapter**: MCP como resource ou
tool, gRPC como method, MIDI como SysEx/CC reply onde fizer sentido,
backplanes futuros idem.

1. **Nasce como variante de `QueryKind`** + handler no GUI thread
   drain (`bridge.serve_queries`) que serializa o estado e devolve.
   Nunca direto da `RefCell<Project>` num resource ad-hoc.
2. **MCP** ganha `openrig://<nome>` em `adapter-mcp::resources` (ou
   uma tool `read_*` quando faz mais sentido como ação). Schema
   coberto por teste de paridade (`QueryKind` variantes ↔ MCP
   resources/tools).
3. **gRPC** ganha o RPC com o mesmo response shape.
4. **MIDI** (quando o estado é footswitch-relevante): expose por
   reply CC/SysEx — opcional pra agora, mas o slot precisa existir
   no `QueryKind` pra quando o adapter MIDI consumir.

Funcionalidade de leitura que a GUI tem mas não está em `QueryKind`
= **gap de query bus**. Fechar no mesmo PR.

**Why:** o agente (MCP) e qualquer cliente remoto (gRPC) precisam
**ver o que o usuário vê** pra tomar decisão informada. Sem paridade
de leitura, o agente é cego: corrige timbre sem ver clipping,
ajusta scene sem ver param atual, sugere chain sem ver meters. A
paridade de Command sem paridade de Query é uma "mão" sem "olho".

**How to apply:** antes de adicionar qualquer property `meter_*`,
`latency_*`, `peak_*`, `*_dbfs` na UI — "isso é uma `Query`?". Não?
Cria a variante primeiro (TDD: teste no `bridge_tests.rs` ou crate
equivalente), depois liga a UI. Auditoria de paridade ao revisar
PR: o número que o usuário vê na tela está em `QueryKind`? Sem
exceção pra "é só visual".

**Anti-padrão:**
```
❌ row.meter_in_dbfs lê direto do engine.pop_peak_dbfs no timer da GUI
   // WRONG: MCP/gRPC nunca veem o meter. Agente fica cego.

❌ "isso é runtime, não pertence ao Project, então não precisa expor"
   // WRONG: runtime != só-GUI. Toda observação visível ao usuário
   // é parte do contrato com os outros transportes também.
```
**Padrão:**
```
✅ QueryKind::ChainMeters → bridge.serve_queries serializa por chain id
   → GUI lê via mesmo bridge (sem path paralelo)
   → MCP `openrig://meters` retorna o resultado serializado
   → gRPC `GetChainMeters` retorna o proto equivalente.
```

---

## Processo de validação (OpenRig gitflow — não pular nenhum passo)

> Princípio "verify before claiming done" é `dev-rules` LAW 3. Esta seção é a **ordem concreta** do gitflow OpenRig (`.solvers/`, `cargo clean` condicional, push antes do gate):

1. **Implementar** no `.solvers/issue-N/` (workspace isolado do gitflow).
2. **`cargo clean` se necessário, ANTES de validar.** Se a mudança envolveu: arquivo gerado por `build.rs` (registries), rename/move de arquivo, `.rs` removido/adicionado, mudança de dep no `Cargo.toml`, ou qualquer suspeita de artefato obsoleto em `target/` → rodar `cargo clean` e rebuildar antes de pedir validação. Senão o usuário faz `git checkout` e o build dele quebra por cache velho (ex.: `generated_registry.rs` apontando pra módulo deletado, `E0761` por `.rs` órfão). Na dúvida, limpa.
3. **`cargo test --workspace --lib`** verde no solver (após o clean, se houve).
4. **`git push` da branch** (sem PR ainda).
5. **Usuário valida na máquina dele** (`git checkout <branch> && git pull` → roda app/testa cenário). Esperar feedback explícito antes de prosseguir.
6. **Quality gate compartilhado** rodar e ficar verde — invocar via skill `quality-gate` (mecânica do gate, JSON, bypass governance ficam todos lá).
7. **Só ENTÃO** o PR, **sempre não-interativo** — push a branch first, then pass every field explicitly:

   ```bash
   gh pr create --repo jpfaria/OpenRig --base develop --head <branch> \
     --title "<type>(#<issue>): <summary>" --body "Closes #<issue>. …"
   ```

   NUNCA rode `gh pr create` sem `--title`/`--body`/`--head` (ou com a branch não pushada): o shell do agente não tem TTY, o gh abre o prompt interativo e **pendura** lendo um stdin que nunca vem — até o timeout (~8 min). Guard-rail de máquina: `gh config set prompt disabled` faz o gh **errar na hora** em vez de travar.

Não inverter:
- PR antes da validação do usuário = retrabalho quando ele acha problema no comportamento real.
- PR antes do gate = CI falha e abre sticky comment no PR.
- Gate antes do push = bloqueia o usuário de testar enquanto roda (gate demora ~25min).

**Foco desta skill (não do gate):** invariantes de áudio, decisões de arquitetura OpenRig (Command/Query/i18n), qualidade **semântica** dos testes (comportamento ≠ cobertura), anti-patterns brand/model. Métrica mecânica (fmt/lint/build/test/complexity/coverage) é a skill `quality-gate`.

---

## File Organization — known god files OpenRig

> Regra geral "one responsibility per file / lib.rs = re-exports / split match-chains" está em `dev-rules` (STOP checklist). Aqui ficam só os caps de tamanho e o **inventário concreto** de god files do OpenRig.

Caps concretos por linguagem:
- `rust-best-practices` — 600 linhas por `.rs`
- `slint-best-practices` — 500 linhas por `.slint`

**Known god files — never expand further (tracked em issue #276). Check current size before touching:**
- `crates/adapter-gui/src/lib.rs` — split in progress
- `crates/project/src/block.rs` — split in progress
- `crates/block-core/src/lib.rs` — split in progress
- `crates/block-core/src/param.rs` — split in progress

```
❌ Adding a new function to adapter-gui/src/lib.rs
   // WRONG: already a god file. Create a new module instead.

❌ A match arm in block.rs growing from 13 to 14 branches
   // WRONG: the dispatch belongs in each block's own crate via trait

✅ crates/adapter-gui/src/device.rs — only device management
✅ crates/adapter-gui/src/project.rs — only project persistence
✅ crates/adapter-gui/src/chain.rs — only chain editing
```

---

## Test Coverage — OpenRig specifics

> RED-first TDD = `dev-rules` LAW 1. "No skipped tests to go green" = `dev-rules` LAW 5. Esta seção é o **plano concreto OpenRig**:

- Nomenclatura: `<behavior>_<scenario>_<expected>` (ex: `validate_project_rejects_empty_chains`).
- **Builds que dependem de assets externos**: bundlar fixture mínimo dentro de `crates/<x>/tests/fixtures/` (ver `engine/tests/fixtures/plugins/source/nam/` em #413). Test passa SEMPRE.
- **Registry tests**: iterar sobre TODOS os modelos via registry (schema, validate, build).
- Helpers de teste no próprio módulo — sem crate de test-utils separado. Sem `mockall` ou frameworks de mock — testar código real.

### `#[ignore]` é PROIBIDO (LEI específica OpenRig — endurece LAW 5)

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

---

## YAML Data Files (OpenRig)

When renaming effect types, models, or identifiers:
- Update `project.yaml` in project root
- Update `preset.yaml` if exists
- Update ANY yaml files the user mentions
- **Never** add serde aliases — update the data instead (consistente com `dev-rules` "No Trash")
- Search: `grep -rn "old_name" **/*.yaml`

---

## Anti-Patterns OpenRig (brand/model/effect_type)

> Princípios "data ownership / single source of truth / zero coupling" em `dev-rules`. Aqui ficam só os **exemplos concretos** com o domínio do OpenRig (brand, model_id, effect_type):

```
❌ if model_id.starts_with("marshall") { "marshall" }
   // WRONG: inferring from string

❌ match model_id { "american_clean" => color(...) }
   // WRONG: hardcoding by model_id

❌ pub const DISPLAY_NAME: &str = "Marshall JCM 800";
   // WRONG: brand in display name (brand é campo próprio)

❌ if effect_type == "preamp" { ... }
   // WRONG: string literal in comparison; use EFFECT_TYPE_PREAMP const

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

**Correct patterns:**
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

### Naming OpenRig

- Module files prefixed by backend (e.g. `native_`, `nam_`, `ir_`, `lv2_`).
- `DISPLAY_NAME` does NOT contain brand name (brand é campo próprio).
- Commits in English, no `Co-Authored-By` trailers.
- Branch names follow `feature/issue-N` or `bugfix/issue-N` (no description suffix).

### Impact analysis OpenRig (from real failures)

- **Build system**: alguma `build.rs` depende de nome de arquivo? (ex.: `starts_with("compressor_")` quebra se o arquivo virar `native_compressor_`).
- **UI capabilities**: o BlockEditorPanel suporta TODOS os widget types necessários? (file picker, bool toggle, numeric, enum).
- **Callback chain**: todos os callbacks conectados na cadeia completa (model → crate → catalog → adapter-gui → Slint)?
- **Window sizing**: se mudou conteúdo da UI, a janela acomoda?

---

## Responsive UI (OpenRig)

- Todo elemento deve ser responsivo — nunca invadir áreas adjacentes.
- Sem posições absolutas hardcoded que quebrem em tamanhos diferentes.
- Testar com janela mínima E máxima antes de commitar.
- Overflow/clip tem que ser tratado — se não cabe, scroll ou truncate, nunca overflow.

---

## LEI — testes que contradizem invariante pinado: PARAR, não decidir sozinho

Se dois testes exigem comportamentos incompatíveis e um deles é invariante **pinado** (`volume_invariants_tests.rs`, qualquer teste marcado como pin de CLAUDE.md #10):

- O invariante pinado **vence por padrão**. O outro teste está obsoleto.
- **NUNCA** enfraquecer/editar o invariante pinado sem pedido explícito do usuário (única via sancionada).
- **NUNCA** chutar no audio path pra "fazer os dois passarem".
- Reportar ao usuário em **1-2 frases**: qual o conflito, qual teste está obsoleto, e seguir com o que não depende do conflito.

**Caso real (2026-05-15, #350 vs #400):** testes Fase-2 do #350 (`two_channel_mono_input_must_not_saturate/cancel`) assumiam split-mono **não soma**; `g02`/`g03` (pinados, #400) exigem split-mono dual **soma** (`[0.3,0.3]→0.6`, `[0.8,0.8]→tanh(1.6)`). Decisão posterior (#355/#400) tornou a soma o invariante correto → os 2 testes Fase-2 do #350 ficaram obsoletos. Resolução: manter os obsoletos `#[ignore]` com a razão do conflito documentada, seguir com a parte não afetada (multi-device, Fase 3). Não mexer em `g02`/`g03`.

---

## LEI — GUI sem regra de negócio. Estado → Command. SIMPLES.

Critério definido pelo usuário (NÃO interpretar, NÃO recategorizar):

- **Abrir/fechar tela/janela = regra de TELA.** Pode ficar na GUI.
- **TODA ação que ALTERA ESTADO** (modelo, rig, projeto, config, persistência, runtime) **= regra de NEGÓCIO = obrigatoriamente um `Command`** despachado pro dispatcher. A GUI só despacha e renderiza — zero lógica.
- `Command` é **domínio-puro**: nunca importa tipo de UI/Slint/view. Pode expressar intenção por chave/enum de domínio.
- **PROIBIDO** auditar isso com script Python (heurística regex erra e mascara o trabalho — decidido 2026-05-18). Análise é **item a item**, callback por callback, **documentada na issue** (#436) como checklist a atacar. A verdade é a lista revisada na issue, não um número de script.
- Um arquivo por responsabilidade (file-per-feature): dispatcher = roteador fino; cada handler em seu arquivo. NUNCA crescer arquivo acima do cap (`scripts/validate.sh`) — dividir antes.

**Caso real (2026-05-18, #436):** o usuário repetiu a regra dezenas de vezes; eu fiquei recategorizando ("navegação é tela", "idioma é tela") e errando, fazendo-o repetir ("parece que falo com uma porta"). A regra é a frase acima, literal. Não reabrir o debate.

---

## LEI — bug de runtime/áudio: INSTRUMENTAR antes de teorizar

Bug de áudio/real-time cuja causa não salta da leitura do código: **após a PRIMEIRA hipótese falhar, parar de teorizar e instrumentar.** Adicionar uma medição dos **valores reais** (sample rates, tamanhos de buffer, níveis do elastic, contadores de underrun/xrun, qual arquivo/path) e fazer o usuário rodar e observar. Os números resolvem em uma rodada; hipóteses encadeadas queimam a paciência e a credibilidade.

**Como instrumentar (RT-safe — invariante #8):**
- **NUNCA** `eprintln!`/log no audio thread (`process_input_f32`/`process_output_f32`/`pop`/`push`). I/O no callback é proibido.
- No audio thread: incrementar **átomo `Relaxed`** (contador de underrun/xrun/load), e **drenar/imprimir fora da thread** (timer da GUI, loader, wiring). Modelo: `ChainRuntimeState::record_callback_load`/`xrun_count`/`underrun_count` (#670).
- Em loaders/wiring (fora do audio thread) um `eprintln!` temporário tagueado (`[#670-probe]`) é OK. OpenRig loga em **stderr** via `env_logger` (sem arquivo): rodar `cargo run … 2>/tmp/openrig.log` e `grep` a tag.
- `openrig://*` (MCP) **não** expõe sample rate viva nem estado de runtime — serializa projeto/devices/meters. Pra valor em execução: instrumentar ou ler o stderr. **Nunca** afirmar um valor de runtime que você não observou.
- Tag o diagnóstico, commit como `chore:` separado, e **reverter** quando a causa for confirmada (ou promover a contador permanente surfacado, como #670).

**Why:** #669 (DI loop em câmera lenta) — chutei a causa 2× (engine_sr preso em 48000; depois "deve ser o device stream") e entreguei fix que não resolveu, porque raciocinei sobre o estado em vez de observá-lo. Um `eprintln!` de `file_sr/engine_sr/out_frames` mostrou na hora que o loop foi construído a 48000 e **nunca reconstruído** quando o device foi pra 44.1k. Dois fixes errados vs uma linha de log. Padrão recorrente nesta própria sessão (#670): teorizei "rig pesado demais / NAM domina / chains competindo" com base na mediana, até o usuário cravar "single chain em 64 também" — ~18% de CPU não craqueia, é underrun/stall, e só instrumentando dá pra ver.

**How to apply (extra):** bug de resampling ligado a uma config (rate/buffer) → checar se o buffer **já carregado** é reconstruído na mudança, não só os loads novos. O "stall intermitente" num único stream leve aponta pro **decoupling input↔output** (elastic buffer), não pra custo de DSP.

**Anti-padrão:**
```
❌ "deve ser X" → fix → "deve ser Y" → fix   (2+ hipóteses sem observar)
❌ afirmar engine_sr/buffer/contagem sem ter lido o valor real
❌ eprintln! dentro de process_input_f32 / pop  (I/O no audio thread)
```
**Padrão:** 1 hipótese falhou → contador atômico + dump off-thread tagueado → usuário roda em 64 → observa underruns vs xruns → causa cravada → teste que reproduz → fix.

**LEI dentro da LEI — PROVE com teste ANTES de anunciar a descoberta.** Ler o código e dizer "achei o bug, é X" é HIPÓTESE, não prova — e atrapalha/queima credibilidade quando está errado. Antes de afirmar que descobriu a causa: escreva um teste DETERMINÍSTICO que demonstra o defeito (ex.: #670 — em vez de "o worker LV2 roda inline", escrevi um teste que agenda trabalho do worker e checa em qual thread roda; ele FALHOU = provado). E "provei que o defeito X existe" ≠ "provei que X causa a craquejada DO USUÁRIO" — se a medição aponta pra outro bloco/sintoma, diga isso; não conflate um bug real achado de passagem com a causa que o usuário está perseguindo. Caso real #670: provei o worker LV2 inline, mas o stall medido era no NAM (off-CPU) — bugs diferentes; anunciar o worker como "a causa" teria sido errado.

---

## LEI — bug de áudio se acha e se prova por TESTE AUTOMÁTICO. NUNCA validação de ouvido.

**PROIBIDO pedir ao usuário pra ouvir e confirmar ("rode e me diz se o estalo sumiu").** O usuário recusou e foi enfático (2026-06-17): *"me recuso a testar, é obrigação sua"*, *"testes de ouvido têm regressão no futuro"*, *"NUNCA mais me peça para testar de ouvido"*. Achar o defeito E provar o fix é trabalho do agente, via teste determinístico — não do ouvido do usuário.

**Why:** ouvido não é teste — não é determinístico, não deixa guarda de regressão, e empurra a obrigação do engenheiro pro usuário. Bug "confirmado de ouvido" volta calado.

**How to apply (áudio / real-time / scheduling):** reduza o defeito a uma propriedade que um teste assere **sem hardware e sem ouvir**:
- **Lógica pura:** `BudgetTracker` (#698 RT budget churn), math de bloco DSP, routing, mixdown → teste de unidade na função direto.
- **Propriedade de sinal** no buffer renderizado: NaN/Inf, clique (salto sample-a-sample > limite de banda), run de hard-clip, DC, nível → `engine/src/audio_signal_integrity_tests.rs`.
- **Contadores/accounting:** asserir o invariante (ex.: overload TEM que incrementar um contador surfaced), não o som.
- **Quando o dano é timing** (worker stall, late buffer), teste a LÓGICA que o dispara (ex.: o budget re-declarando em spike transitório de wall-clock), que é determinística, em vez da magnitude wall-clock flaky.
- A bateria de hardware (`OPENRIG_HW_TESTS=1`) é pro AGENTE rodar e observar — nunca substitui a guarda determinística, nunca é julgada de ouvido pelo usuário.

**Caso real (2026-06-17, estalo single-chain):** o usuário reproduz o estalo com UMA chain / UM input (não é custo de multi-chain — eu supus isso e errei). Numa Mac M4/16GB o DSP custa microssegundos: "load" de 1.5–9× o deadline é **scheduling**, não CPU. Observado (rc=0, promoção RT OK; worker preemptado 2-3ms mid-DSP; #698 re-declara a política RT 10-13×/25s reagindo a wall-clock inflado por preempção = churn). A/B (re-budget on/off) derrubou o pico de ~9× pra <1.6× → causa = churn do #698. Provado por **teste de unidade determinístico** no `BudgetTracker` (spike transitório NÃO pode re-declarar o budget), não de ouvido.

---

## LEI — PROIBIDO marretar sample rate (ou qualquer valor dependente de device)

**Nenhum caminho de áudio/análise pode assumir uma taxa de amostragem fixa.** Cada interface roda na taxa que for melhor pra ela (44.1k, 48k, 96k…). Todo cálculo que depende da taxa — pitch (tuner), bins de FFT→Hz (spectrum), latência, período, resample de loop (DI), timing — **tem que usar a taxa REAL que o stream negociou**, nunca um literal.

**A fonte de verdade da taxa viva:**
- `ProjectRuntimeController::sample_rate()` — a taxa que os streams realmente abriram (espelhada de `resolved.sample_rate`).
- `LocalDispatcher::engine_sr()` — a mesma taxa, sincronizada via `attach_engine_sr` (caminho do #669).
- `adapter_gui::sample_rate::resolve_input_sample_rate(project, device_id, live)` — helper único: setting salvo do device (autoritativo, o stream é forçado a ele ou falha) → senão a taxa viva. **Use este nos consumidores de análise (tuner/spectrum/latency).** NUNCA reimplemente a resolução com `unwrap_or(48_000)`.

**A marreta quase nunca é um literal solto — é o FALLBACK.** O erro recorrente é `…device_settings…find(device)…map(|d| d.sample_rate).unwrap_or(48_000)`: quando o device não tem setting salvo, marreta 48000 enquanto o stream roda a 44.1k → tudo lê ~1.47 semitom acima (#723: E vira F; #669: loop de DI em câmera lenta). Variante igualmente errada: `.device_settings.iter().next()` (primeiro device) em vez do device DAQUELA chain/input.

**Why:** taxa hardcoded é bug silencioso e dependente de hardware — passa na máquina do dev (48k), quebra na do usuário (44.1k). "Em macOS/Windows mudou o som" = regressão (red flag do CLAUDE.md). É a mesma doença que já mordeu tuner, spectrum, latency probe (#723) e o DI loop (#669).

**How to apply:** precisa de uma taxa? Pergunte "de onde vem a taxa VIVA deste stream?" — `controller.sample_rate()` / `dispatcher.engine_sr()` / `resolve_input_sample_rate`. Uma vez que existe stream, a taxa viva SEMPRE existe; "preciso de algum valor de fallback" é falso pra caminho vivo. Exceções legítimas (não são marreta): default pré-device sobrescrito na ativação (estado inicial do controller, init de catálogo VST3), defaults de render/CLI que o usuário sobrescreve, fallback Linux/JACK de device não-configurado atrás de `cfg`, e compensação de design que USA a variável real (`scale = sample_rate / 44_100.0`).

| Desculpa | Realidade |
|---|---|
| "Preciso de ALGUM valor pro detector rodar" | Caminho vivo sempre tem taxa viva: `controller.sample_rate()`/`engine_sr()`. O fallback marretado é o bug. |
| "48000 é o default do projeto, é seguro" | Default pré-device ≠ taxa de um stream vivo. Num caminho de análise, 48000 fixo detona quem roda a 44.1k. |
| "`unwrap_or(default_sample_rate())` resolve" | Continua marreta no caminho vivo. A taxa autoritativa é a do stream, não uma constante. |
| "É só o primeiro device (`.next()`)" | Tem que ser o device DAQUELA chain/input. Primeiro device é outro stream/taxa. |
| "É cosmético (beep/curva), não afeta som" | Mesmo assim é marreta: extraia helper que recebe a taxa real. Sem 48000 plantado. |

**Red flags — PARAR:**
- `unwrap_or(48_000)` / `unwrap_or(44_100)` em código que lê samples vivos ou calcula Hz/latência/período.
- Um literal `48_000`/`44_100` num `.rs` de produção fora de: const default pré-device, `cfg(linux)` JACK, ou ratio de design com a variável real.
- Re-derivar a taxa por conta própria em vez de chamar `resolve_input_sample_rate` / `controller.sample_rate()`.
- `device_settings.iter().next()` pra achar "a taxa".

---

## Audio runtime / DSP facts (hard-won — verify before touching these areas)

- **`ChainRuntimeState` locks (#580):** the audio thread takes `processing.try_lock()` in `process_input_f32` and emits a SILENT buffer on failure. Any accessor the GUI calls repeatedly (meter timer at 30 Hz, spectrum, tuner) must NEVER take `processing.lock()` — mirror the value as an atomic (`AtomicUsize` etc.) updated at the rare write sites (`build_chain_runtime_state` + the rebuild path in `runtime_graph.rs`). Symptom of a violation is buffer-size dependent (32–64 glitches, 256+ absorbs); offline single-threaded tests pass while production clicks. Pinned by `crates/engine/src/stream_count_contention_tests.rs`.
- **Cabinet IR = `crates/ir` (#617):** the CAB block is `block_ir` uniformly-partitioned FFT convolution, NOT the NAM C++ `dsp::ImpulseResponse` path. `ir::PARTITION_SIZE` must stay ≤ the smallest supported device buffer (64) or the per-partition FFT burst lands in one callback and xruns ("clips at 64, fine at 128"). Clamp `accum[0].im` and Nyquist `.im` to 0 before the inverse FFT (`realfft` panics on round-off there). The cold-start cushion is decoupled: `engine::IR_COLD_START_CUSHION_FRAMES`.
- **NAM CAPI surface (#612):** the linked `libNeuralAudioCAPI` dylib exposes ONLY model load/inference + input-level adjustment. There is NO gate/EQ/tone-stack entry point — the NAM block's `noise_gate.*`/`eq.*` knobs have nothing to forward to, and reimplementing that DSP in Rust is forbidden (user decision, #612; a Rust expander was reverted in #496). Before forwarding any NAM param: `nm -gU libs/nam/<platform>/libNeuralAudioCAPI.dylib` and match an exported `Set*`.
- **Native blocks are pure-Rust DSP:** every `native_*` block compiles into the binary; `libs/lv2/` and the `build.rs` `plugin_binary_present()` check are vestigial. An absent `libs/` does not break natives — investigate the actual symptom instead.
- **NAM captures live in `plugins_path` (#623):** debug "NAM silent/not working" against `paths.plugins_path` from `config.yaml` (the user's OpenRig-plugins checkout), not the bundled `asset-runtime/captures` (shipped A1-only — gives a false "no A2" read). A2/SlimmableContainer captures need core ≥ `9c7b185`; pre-fix they failed SILENT.

## GUI ↔ runtime wiring traps

- **Two block-creation paths (#675):** `Command::AddBlock` → `block_factory::build_default_block`, AND the GUI block editor → builds the block itself → `Command::InsertPrebuiltBlock` (never calls `build_default_block`). Param/manifest seeding must go through `block_factory::default_params_for_model`, which BOTH paths call — fixing only `build_default_block` leaves GUI-added blocks broken. Seed into the user-visible knob (visible, editable, persisted), never a silent load-time default.
- **Dispatch alone is dead (#614):** a `Command` targeting a `ChainRuntimeState` only records intent + emits an `Event`. The GUI callback must ALSO apply the runtime effect inline right after dispatch (mirror `wire_mute_inline` in `tuner_wiring.rs`). Always add an end-to-end test driving the callback path and asserting the runtime state flipped — applier-only unit tests hide the gap.
- **"Doesn't resize / doesn't fit / not updating" UI bugs (#622):** the logic usually exists but a code path doesn't TRIGGER it (e.g. the model-picker path rebuilt params without calling `apply_panel_dimensions`). Reproduce in the running app first; when told "do the same as X", mirror X's trigger, don't reinvent the calculation. NAM amps use the PANEL editor, generic/VST3 the FORM editor (`use_panel_editor` in `catalog.rs`) — don't assume which. Pin GUI-wiring fixes with source-presence tests (`no_native_dialogs.rs` convention).
