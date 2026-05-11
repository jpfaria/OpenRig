# Alterações no SO da Orange Pi

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

**Ordem:** alterar no projeto → commit/push → aplicar na placa → validar.

Validação: "se o usuário flashar imagem nova agora, o fix continua lá?".
