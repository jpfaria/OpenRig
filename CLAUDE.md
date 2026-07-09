# OpenRig — Claude Code

Pedalboard/rig virtual para guitarra em Rust + Slint. Cross-platform: macOS, Windows, Linux.

## LEI ZERO — A PASTA PRINCIPAL É INTOCÁVEL PELO AGENTE

**Nenhum agent JAMAIS toca a pasta principal do repo.** Sem `git`, sem `Edit`/`Write`, sem stage, sem worktree, sem `.openrig` — NADA. A pasta principal é exclusiva do usuário.

**O agente trabalha SOMENTE em `.solvers/issue-N/`** — um clone isolado da branch. Edita ali, commita ali, dá push dali. A entrega termina no push; o usuário puxa a branch na pasta dele por conta própria.

**Why:** o usuário roda vários agents em paralelo, cada um na sua branch, e usa a pasta principal pra rodar/testar o app. Um agent mexendo lá sobrescreve trabalho não-commitado do usuário e de outros agents, corrompe o estado de git da pasta e quebra a confiança — já aconteceu repetidas vezes. Regra escrita não basta: depende do agent lembrar.

**How to apply:** se a tua sessão não está enraizada em `.solvers/issue-N/`, você está no lugar errado — clone a branch pra lá e trabalhe lá. Guard determinístico: o hook `main-folder-guard.sh` (PreToolUse) BLOQUEIA `Edit`/`Write`/`git` de qualquer sessão cuja raiz não esteja sob `.solvers/`. O bloqueio não depende do agent obedecer — a harness força.

## LEI ZERO — RESPOSTA CURTA, SEMPRE

**Default = 1–2 frases.** Sem tabelas, sem headers, sem bullets aninhados, sem recap do que o usuário disse, sem "resumo final", sem "next step", sem checklist no chat. Diagnóstico longo, inventário, série de commits → vai pra issue (`gh issue comment`), nunca pro chat.

Só estende a resposta quando o usuário pedir explicitamente ("explica em detalhe", "lista as opções", "me dá o resumo"). Cobrou pra ser curto = curto pelo resto da sessão, sem precisar repetir.

Antes de mandar a mensagem: se tem 3+ frases ou qualquer tabela/header, corta. Se não couber em 2 frases é diagnóstico — vai pra issue.

## LEI — PROIBIDO supor quando não está claro

**EU SOU PROIBIDO DE SUPOR QUANDO AS COISAS NÃO ESTÃO CLARAS. EU PRECISO PERGUNTAR DE FORMA SIMPLES ATÉ TUDO FICAR CLARO.**

Escopo, modelo de dados, comportamento esperado, camada certa, qual arquivo, A vs B — se QUALQUER coisa não está 100% clara, **PARO e pergunto** (uma pergunta curta de cada vez, até não restar dúvida). PROIBIDO "vou de cabeça e depois conserto", PROIBIDO inventar caminho, PROIBIDO escolher entre alternativas que o usuário não escolheu. Supor inverteu o pedido e queimou dias (I/O dentro da chain vs. fora; teste-depois vs. teste-antes) — na dúvida entre perguntar e supor, **perguntar**.

## LEI ZERO — ISOLAMENTO TOTAL entre streams: N streams = N pipelines independentes, NADA misturado no nosso código

**Se o rig tem N streams, são N chains COMPLETAMENTE isoladas.** Cada stream é um pipeline end-to-end próprio — SUA entrada, SEU runtime, SUA saída — e NADA de um stream pode tocar, misturar, somar, compartilhar, agrupar ou depender de outro. **Uma coisa NUNCA interfere na outra:** estado, buffer, rate, route, tap, falha, rebuild, latência — tudo por-stream. Se dois streams "sabem" um do outro no NOSSO código, é BUG. Não importa se têm a mesma taxa, o mesmo device, a mesma interface — continuam sendo pipelines separados que não se enxergam.

**Mixing entre streams NUNCA acontece no nosso código — só no backend (cpal/JACK).** O sistema operacional soma streams do mesmo device físico; nós não. Qualquer código que combine/some/filtre-junto/selecione-em-grupo os runtimes de mais de um stream (ex.: um output que "mistura TODOS os runtimes da mesma taxa" — seleção por RATE em vez de por STREAM/DEVICE) **VIOLA esta lei e é causa raiz de cross-talk / underrun / interferência**. Um output stream serve APENAS o runtime do SEU próprio stream.

**Why:** o usuário roda vários streams em paralelo (ex.: 4 streams em 2 interfaces) e cada um TEM que ser 100% independente — mexer, quebrar, atrasar ou reconstruir um NÃO pode afetar outro. "Selecionar por taxa" é um proxy furado de isolamento: mesma taxa em dois devices cross-mistura, e rate mal-resolvido agrupa errado → underrun/cross-talk. A isolação é por STREAM/DEVICE, JAMAIS por rate nem por "todos que casam". Já foi repetido incontáveis vezes; virou LEI.

**How to apply:** qualquer seleção/agrupamento de runtimes no caminho de I/O (`slots_for_*`, output mixing, taps, DI, meters) tem que ser por identidade de STREAM/DEVICE — nunca por rate, nunca por "todos os que batem". Um output só toca o runtime do seu stream; um tap só lê o seu; um rebuild só reconstrói o seu. Achou código que junta/seleciona runtimes de streams diferentes por qualquer critério que não seja a identidade do próprio stream/device → é violação, PARA, reporta e corrige. Ver invariante #4 e o red flag correspondente abaixo.

## Invariantes que NUNCA podem piorar

OpenRig é áudio em tempo real. **Qualidade sonora e latência são os valores centrais.** Toda mudança que toca audio thread, DSP, roteamento, I/O ou cadeia de blocos precisa provar que não regride NADA abaixo:

1. **Latência round-trip.**
2. **Qualidade de áudio** (ruído, aliasing, THD, resposta em frequência).
3. **Estabilidade do stream** — zero xruns, dropouts, cliques, pops.
4. **Isolation entre streams** — cada `InputBlock` é um runtime paralelo TOTALMENTE isolado. Sem buffer/lock/route/tap compartilhado. "Chain" no YAML é só agrupamento lógico. Mixing entre streams acontece no backend (cpal/JACK), nunca no nosso código.
5. **Stream é SEMPRE estéreo internamente.** Mono input → broadcast `Stereo([s,s])`. DualMono → `Stereo([L,R])` independentes. Stereo → direto. Output só vira mono se `OutputBlock.mode == mono` (via `apply_mixdown`). NUNCA forçar bus mono no segment quando output é estéreo. NUNCA auto-pan.
6. **Jitter do callback** estável.
7. **Custo de CPU no audio thread** — regressão vira xrun.
8. **Zero alocação, lock, syscall ou I/O no audio thread.** Sem exceção.
9. **Determinismo numérico** — golden samples passam dentro da tolerância.
10. **Volume por stream IMUTÁVEL** sem pedido explícito do usuário. Se `volume_invariants_tests.rs` quebra, **a fonte está errada, não o teste**. Pinned em `crates/engine/src/volume_invariants_tests.rs`.

### Red flags — PARAR e reportar

- Novo xrun, dropout, clique
- Latência sobe > 1ms sem justificativa
- Golden tests falham
- `Mutex`/`RwLock`/log/print/file I/O no processamento
- "Em macOS/Windows/Linux o som mudou" → regressão, não compatibilidade
- "Volume ficou diferente" → regressão. NUNCA ajuste o teste de invariância
- Buffer/runtime state/tap/route compartilhado entre 2+ streams → viola isolation

### Hierarquia de trade-offs

1. Som + estabilidade + isolation (empate)
2. Latência
3. CPU no audio thread
4. Cross-platform
5. Ergonomia de código
6. Funcionalidade nova

Feature nova **não justifica** regressão. Trade-off → discutir antes.

### Cross-platform

- NUNCA hardcodar paths. macOS `~/Library/Application Support/OpenRig/`, Windows `%APPDATA%\OpenRig\`, Linux `~/.local/share/openrig/`.
- Fix de Linux/Orange Pi/JACK fica atrás de `cfg` guards. NUNCA mudar comportamento cross-platform pra resolver UM SO.

## Regras gerais de código

- **Zero warnings** (`cargo build` limpo).
- **Zero acoplamento** — blocos não referenciam modelos/brands específicos.
- **Single source of truth** — constantes uma vez.
- **Separação de concerns** — business logic sem config visual/UI.
- Documentação é parte da tarefa: mudou modelo/block/parâmetro/tela/comportamento de áudio → atualizar `docs/` no mesmo commit.
- **Bateria de hardware (#670):** os testes que abrem a interface de áudio real ficam atrás de `OPENRIG_HW_TESTS=1` (máquina ociosa; ver `docs/testing.md` → "Real-hardware battery"). Qualquer agente PODE e DEVE habilitá-los ao validar mudança no caminho de áudio.
- **TDD red-first OBRIGATÓRIO** — proibido implementar/alterar produção sem um teste que falhou ANTES. Bug = entrevistar → teste que reproduz → ver falhar → só então corrigir. Teste-depois (passa de primeira) é proibido. Spec: `docs/testing.md` + `.claude/skills/openrig-code-quality/SKILL.md`.
- **NUNCA parar o processo pra perguntar o óbvio.** O agente decide e segue: escopo já acordado, default sensato, ou destravamento trivial → fazer, não perguntar. Só perguntar quando a resposta muda o resultado e não dá pra inferir do código/contexto. Doc e README no mesmo commit, sempre, sem ser mandado.

### Leis de arquitetura (inegociáveis)

- **Toda operação que muda estado nasce `Command`.** `crates/application/src/command.rs` é a única fonte de verdade do que o app faz. GUI dispara via `dispatcher.dispatch`; MCP/gRPC herdam a mesma variante (paridade). Funcionalidade que existe num frontend mas não é `Command` = gap do bus, fecha no mesmo PR. Nunca `borrow_mut()` direto num callback.
- **Tela não tem regra de negócio.** Slint é dispatcher puro: callback → `Event` → função pura testável. Sem `AppWindow` em teste.
- **Backend transport-agnostic.** Core (`State`/`Event`/`Command`/`SideEffect`) sem dependência de Slint. Vai virar gRPC + MCP + remoto.
- **Conteúdo de repo sempre em inglês.** Todo `.md` (`docs/**`, `CLAUDE.md`, READMEs, specs/plans), comentários de código, commits, branches, PRs e comentários de issue no GitHub: inglês. Única exceção: `README.pt-BR.md` / `README.es-ES.md`.
- **Config: sistema vs projeto (ADR 0003).** Setting nasce em `config.yaml` (sistema) ou dentro de `project.openrig` (projeto) por uma regra única: *"se eu mandar o `.openrig` pra outra máquina, esse valor tem que ir junto?"* Sim → projeto. Não → sistema. Precedência no load: projeto sobrescreve sistema. Spec: `docs/adr/0003-system-vs-project-config.md` + `docs/config-taxonomy.md`.

## Diretrizes de trabalho (agente)

**Comunicação.** Chat e raciocínio em pt-BR (conteúdo de repo em inglês — ver lei acima). Default 1-3 frases, problema antes da solução; sem testamento, sem headers/tabelas salvo referência mecânica. Diagnóstico longo vai pra issue/skill, não pro chat.

**Postura.** Nunca parar pra perguntar "devo continuar"; escopo claro = ir direto pro código. Só o que foi pedido — NUNCA criar crate/binário/exemplo/issue/PR/refactor não pedido. Bloqueio real → reportar e parar, não inventar caminho. Invocar a skill relevante ANTES de qualquer ação não-trivial. Mapear escopo + causa raiz + plano antes de tocar código (zero trial-and-error). Avaliar com certeza total antes de pedir tag/build/install ao usuário.

**Mudanças.** Nunca reverter commit nem apagar arquivo que o agente criou/editou (refazer por cima sim); verificar git antes de restaurar; nunca reescrever do zero. Delete só o escopo literal pedido — nunca expandir. Proibido script regex/sed pra migrar conteúdo — análise caso a caso.

**Git / gitflow** (detalhe em `docs/development/gitflow.md`). PR e merge só com pedido explícito — o trabalho termina no push. Branch `{tipo}/issue-N` (zero sufixo) a partir de develop atualizado + merge develop antes. `.solvers/issue-N/` é exclusivo do agente; pasta principal é exclusiva do usuário (agente nunca faz git lá). Stage paths explícitos — NUNCA `git add -A` no `.solvers`. Push direto após cada commit. **Quality gate compartilhado roda só na criação do PR (o CI roda no PR); NUNCA rodar o gate por push.** Após CADA push: `gh issue comment` (hash + arquivos + build/teste) e incluir o bloco `git checkout feature/issue-N && git pull` na resposta. Antes de fechar issue, atribuir milestone (close not-planned/duplicate/superseded NÃO leva milestone). Checar `docs/superpowers/specs/` + `gh issue list` antes de planejar. Não proliferar issues (cada uma vira branch+workspace de GBs). `@claude` no GitHub: seguir o template de premissas obrigatórias. **Limpeza de `.solvers/issue-N/` só com a issue FECHADA (#568)** — `rm -rf` é destrutivo: leva qualquer WIP não-commitado junto, e o WIP não volta do remote. Confirmar com `gh issue view N --json state` antes de apagar; issue OPEN = off-limits mesmo com pedido genérico tipo "limpa o solver / limpa o lixo / lima o solver".

**UI/Slint.** **OBRIGATÓRIO antes de qualquer trabalho de tela/layout (`.slint`, posicionamento, espaçamento, hierarquia, componente visual): invocar `ui-ux-pro-max` (design/UX) + `slint:slint` + `slint-best-practices`. PROIBIDO supor/inventar layout — RENDERIZE com `tools/slint-render` (PNG headless via slint-interpreter; ver LEI do `openrig-code-quality`) e confira o PNG ANTES de dizer "pronto"; depois feche o visual em loop curto com o usuário.** Nunca glifo como ícone (vira tofu no Orange Pi) — sempre SVG via `@image-url` + colorize. Bebas Neue é a fonte default por escolha — não propor trocar. Manter consistência visual cross-screen.

**Docs.** README sempre nas 3 línguas juntas: `README.md` (en) + `README.pt-BR.md` + `README.es-ES.md`.

## Referências (ler quando precisar)

| Doc | Quando |
|---|---|
| `docs/development/gitflow.md` | Issues, branches, commits, fechamento, workspace `.solvers/`, comentários |
| `docs/development/file-organization.md` | Onde mora cada coisa, caps de LOC, LV2 audio_mode |
| `docs/hardware/orange-pi-deploy.md` | Alterar SO da placa via `platform/orange-pi/` |
| `docs/blocks-catalog.md` | Tipos de bloco, modelos, parâmetros, backends |
| `docs/screens.md` | Telas (Launcher, Chains, Tuner, Spectrum, Block Editor) |
| `docs/cli.md` | Args e env vars do `openrig` |
| `docs/scripts.md` | Build/deploy, fluxo .deb→Orange Pi |
| `docs/audio-config.md` | I/O como blocos, JACK lifecycle |
| `docs/architecture.md` | Crates, registry, assets, BlockEditorPanel |
| `docs/testing.md` | Cobertura, convenções, comandos |
| `CONTRIBUTING.md` | Detalhes de regras de código |
