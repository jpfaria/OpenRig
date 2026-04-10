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
1. Baixa o binário `openrig-*-linux-aarch64.tar.gz` do GitHub Releases
2. Clona/atualiza o Armbian build framework em `.orange-pi-build/`
3. Monta o overlay com o binário + rootfs customizado
4. Roda o build Armbian via Docker (~30–60 min)

Imagem gerada em: `output/orange-pi/Armbian_*.img`

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
2. Ligue — o OpenRig sobe automaticamente via systemd
3. A tela exibe o boot splash (logo OpenRig) e depois a UI

### Áudio (Teyun Q-26)

Conecte a interface USB antes de ligar. O ALSA já está configurado para pinná-la como `Q26` via udev. O serviço do OpenRig aguarda o dispositivo aparecer antes de iniciar.

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
      asound.conf                    ← ALSA: Teyun Q-26 como device padrão
      environment.d/50-slint.conf    ← SLINT_BACKEND=linuxkms
      systemd/system/openrig.service ← auto-start do OpenRig
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
| OS | Armbian Bookworm (Debian 12, kernel current) |
| Display | Slint `linuxkms` + renderer software (sem Wayland/X11) |
| Áudio | ALSA, Teyun Q-26 USB (Vendor 1852, Product 5065) |
