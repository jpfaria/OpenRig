# OpenRig

<img src="docs/assets/openrig-logo.svg" alt="OpenRig logo" width="300">

OpenRig is a pedalboard platform built to give musicians one sound system across desktop, plugin, server, and dedicated hardware.

Build your rig once. Use it on stage, in the studio, and inside your DAW.

## What OpenRig Will Be

- A standalone pedalboard app for Windows, macOS, and Linux
- A VST3 plugin for use inside DAWs
- A server mode for remote control and shared setups
- A dedicated hardware unit for live use
- Phone and tablet control for musicians on stage

## Hardware

The hardware version is designed as a shared stage unit.

- one or more instruments connect to the same unit
- each musician gets their own track
- each musician controls their own track from a phone or tablet
- the audio runs inside the hardware itself

More detail is in [docs/hardware.md](docs/hardware.md).

## Sound

OpenRig is being built to support:

- NAM processing
- IR support
- internal effects
- track-based routing
- scenes and preset workflows

## Status

OpenRig is under active development.

The current codebase already has a working audio path and the foundation for internal plugins, but the long-term product vision is broader: one platform that can live on the computer, inside a DAW, on a server, and in dedicated hardware.

## For Developers

The implementation lives under `crates/`.
