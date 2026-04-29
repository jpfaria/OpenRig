# OpenRig — Contexto do Projeto para Claude Code

OpenRig é um pedalboard/rig virtual para guitarra em Rust. Processa áudio em cadeia (chain) com blocos (blocks) de efeitos e amplificadores. UI em Slint. Distribuição cross-platform: macOS, Windows, Linux.

> **Este documento contém apenas regras obrigatórias.** Catálogos, listas e referências moveram-se para `docs/`. Ver índice no final.

---

## OBRIGATORIO — Prioridades de Produto (Non-Regression)

OpenRig é um processador de áudio em tempo real. **Qualidade sonora e latência são os valores centrais.** Toda mudança deve provar que não degrada nenhuma propriedade abaixo antes de mergear.

### Invariantes que NUNCA podem piorar

1. **Latência round-trip**
2. **Qualidade de áudio** (ruído, aliasing, THD, resposta em frequência)
3. **Estabilidade do stream** — zero xruns, dropouts, cliques, glitches, pops
4. **Isolation entre streams** — cada `InputBlock` é um stream paralelo TOTALMENTE isolado. NUNCA um stream pode impactar outro. Sem buffer compartilhado, sem lock compartilhado, sem cache line contestada, sem CPU spike de um afetando outro. "Chain" no YAML é só agrupamento lógico — no runtime são N runtimes paralelos. Mixing entre streams (se necessário) acontece no backend (cpal/JACK), nunca no nosso código. Violação = regressão crítica, igual a perder áudio.
5. **Stream é SEMPRE estéreo internamente** — o bus de processamento de TODO stream é estéreo, regardless de input mode:
   - Mono input → upmix broadcast: `Stereo([s, s])` (mesmo sinal nos 2 canais — centrado).
   - DualMono input → `Stereo([L, R])` com L/R processados independentemente pelo `AudioProcessor::DualMono`.
   - Stereo input → `Stereo([L, R])` direto.
   - 1 InputBlock com `mode: mono, channels: [0, 1]` (duas guitarras na mesma input) → 2 streams estéreo INDEPENDENTES (cada um broadcast pra ambos os canais), somados com escala 1/N (auto-mix sem saturar).
   - Saída final pro device só vira mono se o `OutputBlock` for `mode: mono` — aí o `apply_mixdown` faz Stereo→Mono pelo método configurado (Average / Sum / Left / Right). NUNCA forçar bus Mono no segment quando output é estéreo. NUNCA auto-pan: cada stream sempre presente em AMBOS os canais. Violação = regressão crítica.
6. **Jitter do callback** — tempo de processamento estável
7. **Custo de CPU no audio thread** — regressão de CPU vira xrun
8. **Zero alocação, lock, syscall ou I/O no audio thread** — sem exceção
9. **Determinismo numérico** — golden samples passam dentro da tolerância
10. **Volume por stream — IMUTÁVEL por qualquer mudança que não seja explicitamente um pedido de mudança de volume.** Refactor, fix, performance work, cleanup, split — NADA pode reduzir nem aumentar o nível de saída de um stream. Se um teste de \`volume_invariants_tests.rs\` quebra, **a fonte está errada, não o teste** — ajuste o código pra fazer o teste passar, NUNCA o contrário. Se você acredita que precisa mudar o volume, isso é uma feature explícita que exige issue dedicada e aprovação do usuário antes do código. Lição #355: o fix #350 reduziu volume "por design" em split-mono multi-channel sem o usuário ter pedido, causando regressão percebida no Mac. Pinned via tests em \`crates/engine/src/volume_invariants_tests.rs\`.

### Checklist obrigatório antes do PR/merge

Se a mudança toca audio thread, DSP, roteamento, I/O ou cadeia de blocos, responder no PR/issue:

- [ ] Afeta o audio thread? Medi CPU/callback antes e depois? Escutei ≥60s sem glitch?
- [ ] Afeta latência? Qual o delta em ms? Justificado?
- [ ] Afeta o som de algum bloco? Golden tests passando? Fiz A/B auditivo?
- [ ] Introduz alocação, lock, syscall ou lazy init no hot path? Se sim, reverter.
- [ ] **Isolation entre streams preservada?** Testei com 2+ inputs paralelos? Glitch num NÃO afeta outro? CPU spike num NÃO afeta callback do outro? Existe algum buffer/lock/route/tap compartilhado entre streams? Se sim → regressão crítica, reverter.
- [ ] **Volume por stream preservado?** Rodei \`cargo test -p engine --lib volume_invariants\` e tudo passou? Setup single-input, split-mono solo, split-mono dual — todos saem no nível esperado? Se NÃO, o som mudou — investiga ANTES de mergear.

### Red flags — PARAR e reportar

- Novo xrun, dropout ou clique audível
- Latência sobe > 1ms sem justificativa
- Golden sample tests falham
- Pico de callback acima do buffer period
- Necessidade de `Mutex`/`RwLock`/`Arc::clone`/log/print/file I/O no processamento
- "Em macOS/Windows/Linux o som mudou" → é regressão, não compatibilidade
- **"O volume ficou diferente"** → regressão. Volume nunca muda sem pedido explícito. Procure o commit que causou e corrija a fonte; NÃO ajuste o teste de invariância de volume.
- **Buffer / runtime state / tap / route / scratch compartilhado entre 2+ streams** — viola isolation, é regressão crítica

### Hierarquia de trade-offs

1. Qualidade do som **e** estabilidade do stream **e** isolation entre streams (empate no topo)
2. Latência
3. Custo de CPU no audio thread
4. Compatibilidade cross-platform
5. Ergonomia de código
6. Funcionalidade nova

Feature nova **não justifica** regressão. Trade-off nesses eixos → **discutir com o usuário antes**.

---

## Fluxo de Desenvolvimento — Gitflow (OBRIGATORIO)

```
Issue → Branch (from develop) → Commits → PR → Review/Merge
```

| Branch | Propósito | Merge into |
|--------|-----------|------------|
| `main` | Releases | — |
| `develop` | Integração para próxima release | `main` |
| `feature/*` | Novas funcionalidades | `develop` |
| `bugfix/*` | Correções de bugs | `develop` |
| `hotfix/*` | Urgências em produção | `main` + `develop` |
| `release/*` | Preparação de release | `main` + `develop` |

### Regras

1. **Issue primeiro** — criar no GitHub antes de qualquer código. Sempre `gh issue list --search` antes de criar (evitar duplicatas). **NUNCA criar issue sem pedido explícito do usuário.** Sugerir follow-up em comentário é OK; abrir tracker novo sem ele autorizar não é. Antes de `gh issue create`, sempre perguntar "quer que eu abra issue para X?" e esperar `sim`/`abre`/equivalente.
2. **UMA branch por issue, nome `feature/issue-{N}` ou `bugfix/issue-{N}`** — NUNCA sufixo descritivo. Antes de criar, `git fetch && git branch -a | grep "issue-{N}"`. Se existe, usar; se precisa recomeçar, resetar a existente.
3. **Sempre a partir de develop atualizado** — `git checkout develop && git pull` antes de criar branch.
4. **Mergear develop antes de qualquer trabalho** — `git merge -X theirs origin/develop` (develop tem prioridade em conflitos).
5. **Commits em inglês**, sem `Co-Authored-By`, foco no "why".
6. **NUNCA `Closes #N` ou `Fixes #N` em commits** — GitHub auto-fecha issues.
7. **Merge policy**: bugfix/hotfix mergeia imediato; feature aguarda review. Nunca mergear feature→develop sem o usuário pedir.
8. **NUNCA rebase** — sempre `git merge`, nunca `git pull --rebase`.
9. **NUNCA fechar issues** — só quando o usuário pedir. **Ao fechar, sempre atribuir ao próximo milestone antes do close.** Procedimento:
   1. Listar releases publicadas: `gh release list --limit 20`. Identificar a última tag `vX.Y.Z-dev.N`.
   2. Se há tags dev publicadas → o próximo milestone é `vX.Y.Z-dev.(N+1)` (somar +1 no último N).
   3. Se esse milestone ainda não existe como milestone aberto, **criar**: `gh api repos/<owner>/<repo>/milestones -f title="vX.Y.Z-dev.(N+1)" -f state="open" -f description="Next dev release after dev.N."` — criação automática só vale neste caso. Para qualquer outra criação de milestone, perguntar ao usuário primeiro.
   4. Se NÃO há ciclo dev em curso (release final) → usar o próximo milestone aberto comum, perguntando ao usuário se houver mais de um.
   5. Se nenhum milestone aberto E nem dev em curso → parar e perguntar (nome + descrição).
   6. Atribuir: `gh issue edit <N> --milestone "<title>"`; depois fechar: `gh issue close <N>`.
   - **NUNCA** atribuir ao milestone de release final (`vX.Y.Z` puro) enquanto o ciclo dev estiver ativo.
   - Issue fechada sem milestone não aparece nos relatórios de release.
10. **Push imediato após cada commit.**
11. **Labels que excluem das release notes** — duas labels controlam o que sai do gerador automático em `.github/workflows/release.yml`:
    - **`duplicate`** — aplicar quando descobrir que a issue duplica outra existente (mesmo escopo, mesmo body). Cronologicamente: a duplicata é a mais nova; a original mantém o histórico.
    - **`internal`** — aplicar em qualquer issue cujo escopo seja CI/CD, scripts, workflows, dependências de build, configs do projeto, processos de planejamento, ou qualquer outra mudança não visível ao usuário final do app.
    - **Antes de criar issue nova:** `gh issue list --state all --search "..."` é regra #1; se houver duplicata, NÃO crie — comente na original. Se já criou e descobriu depois, aplique `duplicate` na nova e linke a original.

### Workspace isolado (.solvers/)

**NUNCA editar código no workspace principal.** Cada agent trabalha numa cópia:

```
OpenRig/                  ← workspace principal (read-only para código)
  .solvers/issue-{N}/     ← cópia isolada por issue
  .solvers/doc/           ← cópia para documentação (branch main)
```

```bash
rsync -a --exclude='target' --exclude='.solvers' . .solvers/issue-{N}/
cd .solvers/issue-{N} && git fetch origin
# branch existe? checkout. não existe? checkout develop && pull && checkout -b feature/issue-{N}
```

Leitura/exploração no workspace principal é OK. Após push, sugerir só `git checkout <branch> && git pull` ao usuário (ele trabalha no principal). Após merge+close de issue, `rm -rf .solvers/issue-{N}/`.

### Issues irmãs — co-evolução obrigatória

Quando duas issues tocam os mesmos arquivos, são **irmãs** e co-evoluem. Identificação: o **corpo** (não comentário) começa com bloco quote `> **Sibling issues (co-evoluem neste ciclo):** #<outra>`. Antes de QUALQUER nova implementação numa issue irmã, `git fetch && git merge origin/feature/issue-<irma> --no-edit && cargo build --workspace`. Sync a cada commit lógico, não só no início. Ao descobrir overlap, editar o body das DUAS issues com o bloco (navegação simétrica).

### Rastreabilidade — comentários obrigatórios na issue

A issue é o log de auditoria. Commits dizem o "o que"; decisões, conflitos, análises, respostas técnicas se perdem se não forem registradas.

**Comentar em:** plano antes de começar; cada push (hash + arquivos + resultado de build/teste); mudança de plano (antes de executar); cada problema com evidência mínima; cada análise técnica não-trivial; resposta técnica relevante dada ao usuário; merges (o que veio, conflitos, resoluções); validação em hardware; resumo final.

**Regra prática:** depois de todo `git push` ou análise técnica, o próximo comando é `gh issue comment <N>`. Em dúvida, comente — excesso tem custo zero, ausência custa o trabalho de reconstruir decisões. **Opções A/B/C ao usuário** vão na issue ANTES da pergunta; resposta dele também vira comentário.

---

## Cross-platform e distribuição

OpenRig roda em **macOS, Windows, Linux**:

- NUNCA hardcodar paths
- Paths de assets via config central (LV2, NAM, IR captures)
- Por plataforma: macOS `~/Library/Application Support/OpenRig/`, Windows `%APPDATA%\OpenRig\`, Linux `~/.local/share/openrig/`
- Antes de qualquer decisão: "isso funciona se o usuário instalar no Windows?"
- **Isolamento absoluto**: fix de Linux/Orange Pi/JACK fica atrás de `cfg` guards. NUNCA mudar comportamento cross-platform pra resolver UM SO.

---

## Documentação é parte da tarefa

Mudança em modelo, block type, parâmetro, tela ou comportamento de áudio → atualizar a doc correspondente em `docs/` no mesmo commit. Feature não documentada é dívida técnica.

---

## Alterações no SO da placa (Orange Pi)

Toda alteração no SO da placa TEM que ter equivalente em `platform/orange-pi/` antes de encerrar — patch que só vive na placa evapora no próximo flash.

| Alteração na placa | Arquivo no projeto |
|---|---|
| Kernel cmdline (`armbianEnv.txt extraargs=`) | `platform/orange-pi/customize-image.sh` (`KERNEL_ARGS`) |
| Systemd unit | `platform/orange-pi/rootfs/etc/systemd/system/` |
| Systemd drop-in | `platform/orange-pi/rootfs/etc/systemd/system/<unit>.d/` |
| `/etc/` config (sysctl, security, udev) | `platform/orange-pi/rootfs/etc/` |
| Binário em `/usr/local/bin/` | `platform/orange-pi/rootfs/usr/local/bin/` |
| Device Tree overlay | `platform/orange-pi/dtbo/` |
| Runtime (chown, groupadd, setcap, mkdir) | bloco em `customize-image.sh` |

**Ordem:** alterar no projeto → commit/push → aplicar na placa → validar. Patch experimental na placa é OK, mas voltar ao projeto antes de declarar resolvido. Pergunta de validação: "se o usuário flashar imagem nova agora, o fix continua lá?".

---

## Regras gerais de código

- **Zero warnings** — `cargo build` limpo
- **Zero acoplamento** — blocos não referenciam modelos, brands ou effect types específicos
- **Single source of truth** — constantes uma vez, nunca duplicadas
- **Separação de concerns** — business logic não tem config visual/UI

Ver `CONTRIBUTING.md` para detalhes.

---

## Referências (não-regras, ler quando precisar)

| Doc | Conteúdo |
|---|---|
| `docs/blocks-catalog.md` | Tipos de bloco, 498+ modelos, parâmetros comuns, backends, instrumentos |
| `docs/screens.md` | Telas principais (Launcher, Chains, Tuner, Spectrum, Block Editor, etc.) |
| `docs/cli.md` | Argumentos posicionais e env vars do `openrig` (com exemplos) |
| `docs/scripts.md` | Scripts de build/deploy + fluxo branch→.deb→Orange Pi + cargo clean obrigatório |
| `docs/audio-config.md` | I/O como blocos, per-machine device settings, JACK lifecycle Linux |
| `docs/architecture.md` | Crates, registry auto-gerado, assets, BlockEditorPanel |
| `docs/testing.md` | Cobertura, convenções, categorias, comandos |
