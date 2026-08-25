# Gitflow — OpenRig

```
Issue → Branch (da release/vX.Y.Z ativa) → Commits → PR → Review/Merge
```

**Fluxo:** `feature/bug → release/vX.Y.Z → main → develop` · `hotfix → main → develop`

| Branch | Propósito | Merge into |
|---|---|---|
| `main` | Produção — tag `vX.Y.Z` dispara a **entrega final**. NUNCA fica atrás do entregue. | `develop` (back-merge) |
| `develop` | Referência, **SEMPRE à frente**. `release/vX.Y.Z` é cortada dela. | — |
| `release/vX.Y.Z` | Ciclo da versão: recebe feature/bugfix; tag `vX.Y.Z-beta.N` dispara **beta**. | `main` |
| `feature/*` | Funcionalidades | `release/vX.Y.Z` ativa |
| `bugfix/*` | Correções | `release/vX.Y.Z` ativa |
| `hotfix/*` | Urgências em produção | `main` (+ back-merge `develop`) |

**Ciclo:** (1) corta `release/vX.Y.Z` da `develop`; (2) feature/bugfix entram nela via PR; (3) tag `vX.Y.Z-beta.N` na release → build **pré-release**; (4) PR `release/vX.Y.Z → main` + tag `vX.Y.Z` na `main` → **entrega final** (milestone fecha, bump vai pra `develop`); (5) back-merge `main → develop`.

## Regras

1. **Issue primeiro.** `gh issue list --search` antes de criar (evita duplicata). NUNCA criar issue sem pedido explícito do usuário.
2. **Nome de branch: `feature/issue-{N}` ou `bugfix/issue-{N}`** — sem sufixo descritivo. Antes de criar: `git fetch && git branch -a | grep issue-{N}`.
3. **A partir da `release/vX.Y.Z` ativa atualizada**: `git fetch && git checkout release/vX.Y.Z && git pull`. Não existe release ativa ainda? Corta da `develop`: `git checkout develop && git pull && git checkout -b release/vX.Y.Z && git push -u origin release/vX.Y.Z`.

   **Ativa = a release que AINDA NÃO foi finalizada.** Uma release finalizada tem a tag `vX.Y.Z` criada e já foi mergeada na `main` — trabalhar nela (ou abrir PR pra ela) entrega código que nunca sai, porque aquela versão já foi publicada. Existir a branch `release/vX.Y.Z` não significa nada: as antigas ficam no remote. A `develop` estar na versão X.Y.Z também não — o bump acontece quando a release é cortada. **Checagem obrigatória antes de cortar branch e antes de `gh pr create`:**

   ```bash
   git fetch --tags
   git branch -r | grep release/          # candidatas
   git tag -l 'v0.4.0'                    # vazio = não finalizada
   git log --oneline -1 origin/main       # "Merge release/vX.Y.Z into main" = essa acabou
   ```

   A ativa é a MAIOR versão sem tag. Errei isso na #881: cortei e ia abrir PR pra `release/v0.3.0` com `v0.3.0` já taggeada e mergeada na `main`, enquanto a ativa era a `release/v0.4.0`.
4. **Mergear a release ativa antes de qualquer trabalho**: `git merge -X theirs origin/release/vX.Y.Z`.
5. Commits em inglês, sem `Co-Authored-By`, foco no "why".
6. **NUNCA `Closes #N` ou `Fixes #N`** em commits — GitHub auto-fecha.
7. Bugfix/hotfix mergeia imediato. Feature aguarda review. Nunca mergear `feature → release` sem o usuário pedir.
8. **NUNCA rebase.** Sempre `git merge`, nunca `git pull --rebase`.
9. **Quality gate só na criação do PR — NUNCA por push.** Push é direto após o commit. O gate **compartilhado** `xgodev/claude-plugin` (`~/.claude-plugin/tools/quality-gate/qg --base origin/develop` ou a skill `claude-plugin:quality-gate`) roda **uma vez, antes de `gh pr create`**, e o mesmo dispatcher roda no CI do PR (`.github/workflows/pr.yml`): falha lá = sticky comment + request-changes automático. Rodar o gate a cada push arrastou 2 dias de trabalho — proibido. Detalhes em [`quality-gate.md`](quality-gate.md).
10. **Push imediato após cada commit** (sem gate; o gate é só no PR).
11. **PR sempre não-interativo**, com a `--base` correta do fluxo: `feature/bugfix → release/vX.Y.Z` ativa; `release/vX.Y.Z → main`; `hotfix → main`; back-merge `main → develop`. Push a branch first, then `gh pr create --repo jpfaria/OpenRig --base <target> --head <branch> --title "…" --body "…"` — todos os campos explícitos. Sem `--title`/`--body`/`--head` (ou com a branch não pushada) o gh abre o prompt interativo e **pendura** num shell sem TTY até o timeout (~8 min). Guard-rail: `gh config set prompt disabled` (o gh erra na hora em vez de travar).

## Fechar issue

Só quando o usuário pedir. Antes do close, atribuir milestone — **plain semver**:

1. O milestone é a **versão da `release/vX.Y.Z` ativa** — o milestone aberto `vX.Y.Z` (hoje `v0.2.0`). `gh api repos/jpfaria/OpenRig/milestones --jq '.[].title'` lista os abertos.
2. **NUNCA criar nem reabrir um milestone `vX.Y.Z-dev.N`** (esquema morto) nem `-beta.N` (beta é tag, não milestone). Use o milestone `vX.Y.Z` aberto.
3. `gh issue edit <N> --milestone "vX.Y.Z"` → `gh issue close <N>`.

## Labels que excluem das release notes

- `duplicate` — escopo idêntico a outra issue (a duplicata é a mais nova).
- `internal` — CI/CD, scripts, workflows, build deps, configs, planejamento, mudanças não visíveis ao usuário final.

## Workspace isolado (.solvers/)

NUNCA editar código no workspace principal. Cada agent trabalha numa **cópia** (`.solvers/issue-N`).

**`git worktree` é PROIBIDO** — qualquer tipo, qualquer lugar. Worktree compartilha o `.git` da pasta principal e trava a branch, abortando o `git checkout` do usuário na pasta dele. Isolamento é sempre via cópia/clone com `.git` próprio em `.solvers/issue-N`, nunca worktree.

**Duas pastas, isolamento simétrico:**

| Pasta | Quem usa | Quem NÃO usa |
|---|---|---|
| principal (`/Users/<user>/.../OpenRig`) | usuário — edição, validação visual, testes em hardware real | agent — NUNCA `git checkout`, `pull`, `commit`, `push`, edit, revert |
| `.solvers/issue-N/` (rsync) | agent — implementação, commits, push, `cargo test` | usuário — NUNCA entra, não testa daqui |

Pra o usuário testar uma branch do agent, ele faz `git fetch && git checkout feature/issue-N && git pull` **na pasta principal dele**. O agent NUNCA propõe `cd .solvers/...` pro usuário — `.solvers/` é exclusivo do agent.

Diretórios sempre excluídos da cópia: `target`, `.logs`, `coverage`, `deps`, `plugins`, `.solvers`.

```bash
# macOS (APFS): clone instantâneo copy-on-write, ~0 byte até divergir
if [[ "$OSTYPE" == "darwin"* ]]; then
  mkdir -p .solvers/issue-{N}
  for d in $(ls -A | grep -Ev '^(target|\.logs|coverage|deps|plugins|\.solvers)$'); do
    cp -cR "$d" ".solvers/issue-{N}/$d"
  done
else
  # Linux/outros: rsync com excludes
  rsync -a \
    --exclude='target' --exclude='.logs' --exclude='coverage' \
    --exclude='deps'   --exclude='plugins' --exclude='.solvers' \
    . .solvers/issue-{N}/
fi

cd .solvers/issue-{N} && git fetch origin
# branch existe? checkout. não existe? checkout release/vX.Y.Z && pull && checkout -b feature/issue-{N}
```

Após merge+close: `rm -rf .solvers/issue-{N}/`.

## Issues irmãs

Identificação: o **corpo** começa com `> **Sibling issues (co-evoluem neste ciclo):** #<outra>`. Antes de qualquer implementação numa issue irmã: `git fetch && git merge origin/feature/issue-<irma> --no-edit && cargo build --workspace`. Sync a cada commit lógico.

## Rastreabilidade — comentários na issue

A issue é o log de auditoria. Comentar em: plano antes de começar; cada push (hash + arquivos + build/teste); mudança de plano; cada problema com evidência; análise técnica; merges; validação em hardware; resumo final. Após `git push` ou análise técnica, próximo comando é `gh issue comment <N>`. Opções A/B/C ao usuário vão na issue ANTES da pergunta.

**Checklist de validação — obrigatório em toda entrega que depende do usuário.** Quando a entrega precisa da validação dele (ouvido, visual, hardware, comportamento em app real), o comentário na issue E a resposta no chat levam um checklist com: (1) `git fetch && git checkout {tipo}/issue-N && git pull` num bloco de código, (2) itens ENUMERADOS em checkbox (`1. [ ]`, `2. [ ]`, …), um por linha, só o que ELE valida — nunca os testes/build que o agent já rodou. Sem prosa em volta. É a única lista permitida no chat (exceção à LEI ZERO "RESPOSTA CURTA"). Ver CLAUDE.md → LEI ZERO "CHECKLIST DE VALIDAÇÃO SEMPRE".

## Release mechanics

Step-by-step for actually cutting one: [`release.md`](release.md).

- **Two tag kinds, both trigger `release.yml`** (`on: push: tags: v*`), which derives the version from `GITHUB_REF_NAME`:
  - **Beta:** tag `vX.Y.Z-beta.N` on the **`release/vX.Y.Z`** branch → a GitHub **pre-release** (auto-generated notes; the milestone stays OPEN; no version bump to develop).
  - **Final:** tag `vX.Y.Z` on **`main`** → a full release (curated milestone notes; milestone closes; the bump is pushed to `develop`). Ship it via the `release/vX.Y.Z → main` PR, then tag `main`; afterwards back-merge `main → develop`.
  - A tag is a pre-release iff its name contains a `-` (semver pre-release), so `create-release` adds `--prerelease` and `commit-version-bump` is skipped for those.
  - Re-trigger a failed release by deleting and recreating the tag ref at the new tip of its branch.
- **The tag is the source of truth for the version — never bump `Cargo.toml` by hand (#820).** Every build job runs `scripts/lib/release-version.sh` to write the tag's version (including a `-beta.N` pre-release) into `[workspace.package]` *before* compiling, because the launcher footer renders `env!("CARGO_PKG_VERSION")`; skipping that shipped `v0.1.1` artifacts containing a `0.1.0` binary. After a **final** release, the `commit-version-bump` job re-applies the same bump plus `cargo update --workspace` on **`develop`** (the always-ahead reference and manifest source of truth) and pushes it, so the repository never drifts behind the last tag; `main` never falls behind because every release flows `release/vX.Y.Z → main → develop`. The helper is covered by `scripts/tests/release_version_test.sh` and refuses any non-semver input rather than writing an unparseable manifest.
- **A release ships macOS only.** The Linux x86_64, Linux aarch64 and Windows x64 jobs carry a hard `if: false` since #816, so `release.yml` produces a single artifact and the job list shows three skipped builds — expected, not a failure. Packaging is exercised **only** at release-tag time (PR CI never builds installers), so regressions surface one ~25-min failure at a time after the tag; v0.1.0-dev.24 needed five sequential fixes (MSVC flag guards, `/EHsc`, `WINDOWS_EXPORT_ALL_SYMBOLS`, macOS `Resources` mkdir — #639–#647). Re-enabling a platform means flipping its build job **and** the matching artifact download in `create-release`.
- The loudness audit (`qa_audit`, ~22 min) does NOT run in the release path (`QA_AUDIT_SKIP=1`, #641) — it belongs to OpenRig-plugins CI. Keep it that way.
