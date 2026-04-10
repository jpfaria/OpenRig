# OpenRig — Orange Pi 5B

Scripts e configurações para gerar e gravar uma imagem Linux mínima para Orange Pi 5B rodando OpenRig.

---

## Pré-requisitos

- **macOS** (Apple Silicon recomendado)
- **Docker** — para o Armbian build
- **gh** (GitHub CLI) — para baixar a release

```bash
brew install docker gh
```

---

## Fluxo completo

```
build-orange-pi-image.sh  →  flash-sd.sh  →  (boot na Orange Pi)  →  openrig-install-to-emmc
```

---

## 1. Gerar a imagem

```bash
# Última release estável
./scripts/build-orange-pi-image.sh

# Versão específica
./scripts/build-orange-pi-image.sh --version v1.2.0

# Dry-run (só imprime os passos)
./scripts/build-orange-pi-image.sh --dry-run
```

O que o script faz:
1. Baixa o pacote `openrig_*_arm64.deb` do GitHub Releases
2. Clona/atualiza o Armbian build framework em `.orange-pi-build/`
3. Monta o overlay com o `.deb` + rootfs customizado (systemd units, plymouth, helpers)
4. Roda o build Armbian via Docker (~30–60 min). Dentro do chroot, `customize-image.sh` instala o `.deb` com `apt install /tmp/overlay/openrig.deb`

Imagem gerada em: `output/orange-pi/Armbian_*.img`

### Base Linux

| | Valor |
|---|---|
| Distro | Ubuntu 24.04 LTS (`noble`) via Armbian |
| Kernel | Armbian `edge` branch (mainline mais recente, para driver `scarlett-gen2` atualizado) |
| Fragmento de kernel | `orange-pi/kernel-config/orangepi5b-edge.config` — habilita `CONFIG_PREEMPT_RT=y` |
| Display | Weston (Wayland, DRM backend, kiosk) + Slint `wayland` |
| Boot splash | Plymouth theme customizado (`/usr/share/plymouth/themes/openrig/`) |

---

## 2. Gravar no SD card

```bash
./scripts/flash-sd.sh
# ou
./scripts/flash-sd.sh output/orange-pi/Armbian_*.img
```

O script lista os discos externos disponíveis, pede confirmação e grava com `dd`.

---

## 3. Primeiro boot na Orange Pi

1. Insira o SD card na Orange Pi 5B
2. Ligue — a tela mostra o splash com a logo OpenRig, e a imagem salta direto para o Wayland/OpenRig sem piscar nenhuma tty, sem prompt de login, sem wizard Armbian.

### Credenciais (SSH / recovery)

Se você precisar entrar via SSH para debugar:

| Usuário | Senha |
|---|---|
| `root` | `root` |
| `openrig` | `openrig` |

> Imagem de appliance, destinada a rede local. Troque as senhas se for expor a internet.

### Locale, teclado e timezone

- **Locale:** `en_US.UTF-8`
- **Teclado:** `br-abnt2`
- **Timezone:** `America/Sao_Paulo`

Tudo pré-configurado no `customize-image.sh` — nenhum prompt é exibido no primeiro boot.

### Áudio (USB)

Conecte a interface USB antes de ligar. O `jackd.service` aguarda até 10s por uma placa USB Audio aparecer e então inicia o JACK em **48000 Hz / 256 frames / 3 periods** (~16 ms de latência). O OpenRig aguarda o JACK estar pronto antes de subir.

**Por que 48 kHz / 256 / 3 e não algo menor:**

- **Scarlett Gen 4** é travada em 48 kHz pelo driver mainline `scarlett-gen2`. Abaixo disso o driver falha com `Error initialising Scarlett Gen 4 Mixer Driver: -71` (EPROTO).
- No **Rockchip RK3588 xHCI** (controladora USB do Orange Pi 5B), buffers pequenos (ex.: 128 × 2) provocam resets do host controller e quedas da interface sob carga — correlacionado com LED vermelho fixo na Scarlett. 256 × 3 é o menor config estável observado.

**IRQ affinity:** o `jackd.service` tem um `ExecStartPre` que fixa as IRQs do `xhci-hcd` no CPU 4 (big core A76) antes de iniciar o daemon, reduzindo jitter de áudio. Em RK3588S, CPUs 0–3 são A55 (little) e CPUs 4–7 são A76 (big).

---

## 4. Instalar no eMMC (opcional)

Para instalar permanentemente no armazenamento interno da Orange Pi (sem depender do SD):

```bash
# Na Orange Pi, como root
sudo openrig-install-to-emmc
```

O script detecta o eMMC (`/dev/mmcblk1`), pede confirmação e usa `armbian-install` (ou `dd` como fallback).

---

## Validação local (Apple Silicon)

Para validar a imagem numa VM ARM64 com tela e áudio antes de gravar no hardware:

```bash
./scripts/validate-utm.sh
```

Instala e configura automaticamente uma VM UTM com:
- Ubuntu 22.04 ARM64
- Display VirtioGPU
- Áudio VirtioSound
- OpenRig baixado automaticamente no primeiro boot

Após o boot: login `openrig / openrig`, depois `openrig-start`.

```bash
# Recriar VM do zero
./scripts/validate-utm.sh --reinstall
```

---

## Estrutura dos arquivos

```
orange-pi/
  README.md                          ← este arquivo
  customize-image.sh                 ← hook rodado dentro do chroot Armbian
  rootfs/
    etc/
      environment.d/50-slint.conf        ← SLINT_BACKEND=linuxkms
      systemd/system/jackd.service       ← JACK2 48 kHz/256/3 + IRQ affinity (xhci → CPU 4)
      systemd/system/weston.service      ← Wayland compositor (kiosk)
      systemd/system/openrig.service     ← auto-start do OpenRig (depende de jackd + weston)
    usr/
      local/bin/openrig-install-to-emmc  ← instala SD → eMMC
      share/plymouth/themes/openrig/     ← boot splash (logo OpenRig)

scripts/
  build-orange-pi-image.sh           ← gera a imagem Armbian
  flash-sd.sh                        ← grava no SD card (macOS)
  validate-utm.sh                    ← VM UTM ARM64 para validação local
```

---

## Hardware alvo

| Item | Detalhe |
|------|---------|
| Board | Orange Pi 5B |
| SoC | Rockchip RK3588S (4×A76 + 4×A55) |
| OS | Armbian Ubuntu Noble (24.04 LTS), kernel `edge` + PREEMPT_RT fragment |
| Display | Weston (Wayland, DRM backend) + Slint `wayland` |
| Áudio | JACK2 (ALSA backend), qualquer USB Audio (testado: Focusrite Scarlett 2i2 Gen 4, Teyun Q-26) |
