# Scripts de Build e Deploy

| Script | Função |
|--------|--------|
| `scripts/build-deb-local.sh` | Cross-compila `.deb` arm64 + amd64 via Docker |
| `scripts/build-linux-local.sh` | Build Linux (interno, chamado pelo build-deb-local.sh) |
| `scripts/build-orange-pi-image.sh` | Imagem SD para Orange Pi |
| `scripts/flash-sd.sh` | Flasha SD card |
| `scripts/coverage.sh` | Relatório HTML de cobertura |
| `scripts/package-macos.sh` | Empacota macOS |
| `scripts/build-lib.sh` | Libs externas |

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
