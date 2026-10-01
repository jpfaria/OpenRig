# Scripts de Build e Deploy

| Script | Função |
|--------|--------|
| `scripts/solver-setup.sh <N> <branch> [release-base]` | Builds the agent workspace `.solvers/issue-N` (clone, `plugins` link) and prints the absolute run command with `OPENRIG_PLUGINS_ROOT` |
| `scripts/nam_vendor.py check\|update\|push` | The vendored NeuralAmpModelerCore archive (`deps/NeuralAmpModelerCore.tar.gz` + `.lock`): `check` names a newer upstream release (the first local build after a `git fetch` runs it), `update` vendors it and commits on the current branch, `push` publishes that commit — CI's `nam-refresh` job does both on `develop` after the tests pass. Offline / timeout / failed commit / refused push = warning, exit 0 (#974) |
| `scripts/build-deb-local.sh` | Cross-compila `.deb` arm64 + amd64 via Docker |
| `scripts/build-linux-local.sh` | Build Linux (interno, chamado pelo build-deb-local.sh) |
| `scripts/build-orange-pi-image.sh` | Imagem SD para Orange Pi |
| `scripts/flash-sd.sh` | Flasha SD card |
| `scripts/coverage.sh` | Relatório HTML de cobertura |
| `scripts/patch-coverage.sh [base] [--files] [--fresh]` | Cobertura do PATCH — as linhas que a branch adiciona, o mesmo número que o `codecov/patch` reporta no PR. Rode antes do push; `PATCH_COV_OFF=1` desliga. |
| `scripts/package-linux.sh` | Empacota Linux .tar.gz/.deb/.rpm/.AppImage (patchelf RUNPATH p/ libnam_wrapper + libseat) |
| `scripts/package-macos.sh` | Empacota macOS (assina ad-hoc inside-out + gate de verificação). Plugins bundled: `OPENRIG_PLUGINS_DIR=/path/plugins/source` sobrescreve a origem (default `plugins/source`; override inexistente = erro fatal) |
| `scripts/lib/console-binaries.{sh,tsv}` | Fonte única dos binários console-style empacotados junto da GUI (`openrig-console`, `openrig-console-rig`, `openrig-render` — #741). Os três packagers buildam e stageam a partir dela; testado por `scripts/tests/console_bundle_test.sh` |
| `scripts/lib/release-version.sh` | `release_version_from_tag` (ref → semver) + `set_workspace_version` (grava o `version` de `[workspace.package]`, sem tocar nos pins de `[workspace.dependencies]`). A tag é a fonte da verdade: todo job do `release.yml` roda isso ANTES de compilar, porque o rodapé renderiza `env!("CARGO_PKG_VERSION")` (#820). Recusa entrada não-semver; testado por `scripts/tests/release_version_test.sh` |
| `scripts/install-macos.sh` | Instalador one-liner via `curl` (baixa .dmg de release, copia pro /Applications, tira quarentena) |
| `scripts/install-macos-local.sh` | Dev: builda do checkout atual (via `package-macos.sh`) e instala em `/Applications` (mata instância aberta + abre). `OPENRIG_PLUGINS_DIR` repassado ao packager. `[version]` default `dev` (#774) |
| `scripts/build-lib.sh` | Libs externas |

## macOS dmg — naming and installing

A local macOS build ships as `OpenRig-<ver>-macos-universal.dmg`: the version
belongs in the filename, because these get installed side by side while
validating a release and an unversioned file says nothing about what is in
`/Applications`.

```bash
OPENRIG_PLUGINS_DIR=<plugins> ./scripts/package-macos.sh <ver>
```

**Bump `version` in the workspace `Cargo.toml` before building a dmg meant to
represent a release.** The release workflow only bumps it on the tag, and the
app renders the compiled-in `CARGO_PKG_VERSION`, so a pre-tag build otherwise
shows the previous version on screen. The packager already ad-hoc signs
inside-out and verifies (`codesign --sign -`); the install still strips
quarantine.

**"Cria um DMG pra eu instalar" means build it AND install it** (#921): build in
the solver with `OPENRIG_PLUGINS_DIR` and a local version like `0.4.2-921` via
`set_workspace_version` (reverting the manifest afterwards), then quit the app,
mount, `rm -rf /Applications/OpenRig.app`, copy, detach, strip quarantine,
`open -a OpenRig`, and report the installed version. Handing back a command for
him to paste is a handoff of something the agent can execute — "pq vc nao
executou para mim?". This is **not** a licence to replace `/Applications` when
he did not ask for an install.

## Fluxo branch → .deb → Orange Pi

```bash
git checkout feature/issue-{N} && git merge origin/develop
./scripts/build-deb-local.sh
scp output/deb/openrig_0.0.0-dev_arm64.deb root@192.168.15.145:/tmp/
ssh root@192.168.15.145 "dpkg -i /tmp/openrig_0.0.0-dev_arm64.deb && systemctl restart openrig.service"
```

## Regras de build

- NUNCA compilar na placa. Sempre cross-compile no Mac via `build-deb-local.sh`
- Docker Desktop precisa estar rodando (build usa container arm64)
- Só arm64 vai pra placa Orange Pi (amd64 é pra x86 Linux)

## cargo clean obrigatório em `.solvers/`

Workspaces em `.solvers/issue-N/` acumulam estado inconsistente no `target/` ao longo de merges, edições em vários crates, troca de branches, ou uso compartilhado com Docker. Sintomas: `error[E0460]: possibly newer version of crate X`, `error[E0463]: can't find crate`, ICE em `rmeta/decoder.rs`, build verde mas runtime "fn X not found".

Antes de QUALQUER build que o usuário vá consumir:

```bash
cd .solvers/issue-N && cargo clean && ./scripts/build-deb-local.sh
```

Obrigatório após: `git merge`, edição de struct/enum em ≥2 crates, mudança de `#[cfg(...)]`, primeiro `build-*local.sh` da sessão.
