# Bundled DI loops

Dry guitar DI loops shipped with OpenRig for the **per-chain virtual DI loop**
tone-shaping feature (issue #614). When a chain's DI loop is active, the chain
reads one of these (or a user-picked WAV) instead of the live device input, so
you can shape tone hands-free. These are meant to be **dry** signals — the chain
re-amps them through its own blocks.

The file stem is the loop's id shown in the Chains-screen selector.

## Sources & licenses

All bundled files are **CC0 1.0 (public domain dedication)** — no attribution
required; credit is given here as good practice.

| File | Source | Author | License |
|---|---|---|---|
| `slowcore-guitar-dry-50bpm.wav` | [Freesound #721687](https://freesound.org/people/josefpres/sounds/721687/) | josefpres | CC0 1.0 |
| `clean-electric-guitar-loop.wav` | [Freesound #319235](https://freesound.org/people/mooncubedesign/sounds/319235/) | mooncubedesign | CC0 1.0 |

### Quality note

These were derived from the publicly available **lossy preview (MP3)** of each
Freesound entry, transcoded to WAV (44.1 kHz, stereo, 16-bit PCM) — the
original lossless WAV requires a Freesound account to download. They are
adequate as practice defaults; for critical tone work, load your own dry-DI WAV
via the "Choose file…" entry in the chain's DI-loop selector. Replacing these
with the lossless originals later is a drop-in (same filenames).
