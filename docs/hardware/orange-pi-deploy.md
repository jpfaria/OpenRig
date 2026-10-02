# Changes to the Orange Pi OS

Every change to the board's OS MUST have its counterpart in `platform/orange-pi/` before the work is done — a patch that lives only on the board disappears on the next flash.

| Change on the board | File in the project |
|---|---|
| Kernel cmdline (`armbianEnv.txt extraargs=`) | `platform/orange-pi/customize-image.sh` (`KERNEL_ARGS`) |
| Systemd unit | `platform/orange-pi/rootfs/etc/systemd/system/` |
| Systemd drop-in | `platform/orange-pi/rootfs/etc/systemd/system/<unit>.d/` |
| `/etc/` config (sysctl, security, udev) | `platform/orange-pi/rootfs/etc/` |
| Binary in `/usr/local/bin/` | `platform/orange-pi/rootfs/usr/local/bin/` |
| Device Tree overlay | `platform/orange-pi/dtbo/` |
| Runtime step (chown, groupadd, setcap, mkdir) | a block in `customize-image.sh` |

**Order:** change the project → commit/push → apply on the board → validate.

The check: "if the user flashes a new image now, is the fix still there?"
