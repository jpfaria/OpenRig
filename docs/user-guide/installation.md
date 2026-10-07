# Installing OpenRig

| Platform | How |
|---|---|
| **macOS** 11 or later, Apple Silicon or Intel | [Download](#macos) — one command, or the `.dmg` by hand |
| **Linux** x86_64 / aarch64 | [Build from source](#linux) — no package yet |
| **Windows** 10 or later, x86_64 | [Build from source](#windows) — no package yet |

You also need an audio interface with an instrument input: built-in computer audio has no guitar input and adds latency.

## macOS

### Install with one command (recommended)

Open **Terminal**, paste this line and press Return:

```bash
curl -fsSL https://raw.githubusercontent.com/jpfaria/OpenRig/develop/scripts/install-macos.sh | bash
```

It downloads the latest release, copies `OpenRig.app` to `/Applications` and clears the macOS download block (see below). Then open OpenRig from Applications.

To install a specific version instead of the latest, add the tag: `... | bash -s -- vX.Y.Z`.

### Install by hand

1. Download `OpenRig-<version>-macos-universal.dmg` from the [latest release](https://github.com/jpfaria/OpenRig/releases/latest).
2. Open the `.dmg` and drag **OpenRig** to **Applications**.
3. Open **Terminal** and run:

   ```bash
   xattr -dr com.apple.quarantine /Applications/OpenRig.app
   ```

Step 3 is needed because OpenRig is not notarized by Apple. Without it, macOS says *"OpenRig is damaged and can't be opened"*. The app is not damaged: macOS blocks any downloaded app that Apple has not notarized.

### Updating

When a newer release is out, the launcher shows an **Update to vX.Y.Z** button next to the version number. It runs the same one-command installer.

## Linux

There is no Linux package for the current version yet, so you build it from source.

### 1. Install the build tools

```bash
# Debian / Ubuntu
sudo apt install git git-lfs cmake pkg-config libasound2-dev libfontconfig-dev

# Fedora
sudo dnf install git git-lfs cmake pkg-config alsa-lib-devel fontconfig-devel
```

Then install Rust with [rustup](https://rustup.rs). cmake must be 3.16 or later (`cmake --version`).

### 2. Build

```bash
git lfs install
git clone https://github.com/jpfaria/OpenRig.git
cd OpenRig
cargo build --release -p adapter-gui
./target/release/adapter-gui
```

### 3. Get the models

The LV2 and VST3 effects come with the clone, under `plugins/source/`. Make sure their Git LFS objects are there (the clone above fetches them when `git lfs install` ran first):

```bash
git lfs pull --include="plugins/source/**"
```

Run the app from the `OpenRig` folder and it finds them on its own. NAM and IR captures are not included; to use your own, open **Settings → Paths → Plugins** and choose the folder that holds them.

### 4. Set up audio

1. **Plug in a USB audio interface.** Class-compliant interfaces need no driver on Linux. Check that it is detected with `aplay -l` or `cat /proc/asound/cards`.
2. **Install the JACK server.** OpenRig starts `jackd` itself, so the daemon must be installed, not only its libraries:

   ```bash
   sudo apt install jackd2                        # Debian / Ubuntu
   sudo dnf install jack-audio-connection-kit     # Fedora
   ```

3. **Join the `audio` group**, then log out and back in:

   ```bash
   sudo usermod -aG audio "$USER"
   ```

4. **Free the interface from PipeWire / PulseAudio.** If a sound server holds the interface, `jackd` cannot open it. Suspend the device in the sound server, or use the PipeWire JACK bridge.
5. In OpenRig's audio settings, pick the interface as input and output and choose the sample rate and buffer size.

OpenRig sets the interface's ALSA playback mixer to 0 dB, unmuted, before starting JACK, because many USB interfaces start attenuated. If yours uses a mixer control OpenRig does not recognise, set it by hand and save it:

```bash
amixer -c <CARD> sset <CONTROL> 100% unmute   # <CARD> from cat /proc/asound/cards
sudo alsactl store
```

## Windows

There is no download yet (#978): CI builds the `.msi` and `.zip` but the release does not publish them. Build from source as in [Building OpenRig](../development/building.md), then run `target\release\adapter-gui.exe`.

Audio: OpenRig uses your interface's ASIO driver when one is installed, and WASAPI otherwise (onboard audio, class-compliant interfaces).

If something goes wrong, each session writes its own log under `%APPDATA%\OpenRig\logs\`.

## Troubleshooting

### "OpenRig is damaged and can't be opened" (macOS)

The app is fine; macOS blocks apps that Apple has not notarized. Use the [one-command install](#install-with-one-command-recommended), or run the `xattr` command from [Install by hand](#install-by-hand).

### The block picker has no amps or pedals

OpenRig cannot find the plugins. In a source build, run the app from the `OpenRig` folder and pull the plugin tree with `git lfs pull --include="plugins/source/**"`. The macOS app ships the LV2/VST3 plugins inside it, so this only happens with a source build.

### "is a Git LFS pointer" when building

The repository was cloned without Git LFS. Fetch the LFS files and build again:

```bash
git lfs install
git lfs pull
```

### ALSA errors when building (Linux)

Install the ALSA development headers: `libasound2-dev` on Debian/Ubuntu, `alsa-lib-devel` on Fedora.

### cmake is too old

OpenRig needs cmake 3.16 or later. If your distribution ships an older one, install a newer one from [cmake.org](https://cmake.org/download/).

## Building on macOS

Developers building the Mac app from source: [Building OpenRig](../development/building.md).
