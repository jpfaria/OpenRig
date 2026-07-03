# Gitflow — OpenRig

```
Issue → Branch (from develop) → Commits → PR → Review/Merge
```

| Branch | Propósito | Merge into |
|---|---|---|
| `main` | Releases | — |
| `develop` | Próxima release | `main` |
| `feature/*` | Funcionalidades | `develop` |
| `bugfix/*` | Correções | `develop` |
| `hotfix/*` | Urgências em produção | `main` + `develop` |
| `release/*` | Preparação de release | `main` + `develop` |

## Regras

1. **Issue primeiro.** `gh issue list --search` antes de criar (evita duplicata). NUNCA criar issue sem pedido explícito do usuário.
2. **Nome de branch: `feature/issue-{N}` ou `bugfix/issue-{N}`** — sem sufixo descritivo. Antes de criar: `git fetch && git branch -a | grep issue-{N}`.
3. **A partir de develop atualizado**: `git checkout develop && git pull`.
4. **Mergear develop antes de qualquer trabalho**: `git merge -X theirs origin/develop`.
5. Commits em inglês, sem `Co-Authored-By`, foco no "why".
6. **NUNCA `Closes #N` ou `Fixes #N`** em commits — GitHub auto-fecha.
7. Bugfix/hotfix mergeia imediato. Feature aguarda review. Nunca mergear feature→develop sem o usuário pedir.
8. **NUNCA rebase.** Sempre `git merge`, nunca `git pull --rebase`.
9. **Quality gate só na criação do PR — NUNCA por push.** Push é direto após o commit. O gate **compartilhado** `xgodev/claude-plugin` (`~/.claude-plugin/tools/quality-gate/qg --base origin/develop` ou a skill `claude-plugin:quality-gate`) roda **uma vez, antes de `gh pr create`**, e o mesmo dispatcher roda no CI do PR (`.github/workflows/pr.yml`): falha lá = sticky comment + request-changes automático. Rodar o gate a cada push arrastou 2 dias de trabalho — proibido. Detalhes em [`quality-gate.md`](quality-gate.md).
10. **Push imediato após cada commit** (sem gate; o gate é só no PR).

## Fechar issue

Só quando o usuário pedir. Antes do close, atribuir milestone — **plain semver**:

1. O milestone é a **versão semver atual ainda não lançada** (hoje `v0.1.0`); depois que ela for taggeada, o próximo é `v0.2.0`.
2. **NUNCA criar nem reabrir um milestone `vX.Y.Z-dev.N`.** O esquema `-dev.N` está MORTO (virou histórico fechado). Use o milestone aberto da versão atual.
3. `gh issue edit <N> --milestone "v0.1.0"` → `gh issue close <N>`.

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
# branch existe? checkout. não existe? checkout develop && pull && checkout -b feature/issue-{N}
```

Após merge+close: `rm -rf .solvers/issue-{N}/`.

## Issues irmãs

Identificação: o **corpo** começa com `> **Sibling issues (co-evoluem neste ciclo):** #<outra>`. Antes de qualquer implementação numa issue irmã: `git fetch && git merge origin/feature/issue-<irma> --no-edit && cargo build --workspace`. Sync a cada commit lógico.

## Rastreabilidade — comentários na issue

A issue é o log de auditoria. Comentar em: plano antes de começar; cada push (hash + arquivos + build/teste); mudança de plano; cada problema com evidência; análise técnica; merges; validação em hardware; resumo final. Após `git push` ou análise técnica, próximo comando é `gh issue comment <N>`. Opções A/B/C ao usuário vão na issue ANTES da pergunta.

## Release mechanics

- Tag `vX.Y.Z-dev.N` is created on **develop's tip** (not main). `release.yml` triggers on `v*` tag push and derives the version from `GITHUB_REF_NAME` (no `Cargo.toml` bump). `main` is updated by merging develop (API merge, not fast-forward). Re-trigger a failed release by deleting and recreating the tag ref at the new develop tip.
- **Windows x64 + macOS universal are built ONLY at release-tag time** — PR CI skips them, so cross-platform build/packaging regressions surface one ~25-min failure at a time after the tag (v0.1.0-dev.24 needed five sequential fixes: MSVC flag guards, `/EHsc`, `WINDOWS_EXPORT_ALL_SYMBOLS`, macOS `Resources` mkdir — #639–#647). Treat MSVC + macOS packaging as the main release risk; Linux is already covered by PR CI.
- The loudness audit (`qa_audit`, ~22 min) does NOT run in the release path (`QA_AUDIT_SKIP=1`, #641) — it belongs to OpenRig-plugins CI. Keep it that way.
