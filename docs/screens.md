# Telas principais

Pedalboard virtual: usuário monta cadeia de efeitos visualmente, ajusta parâmetros em tempo real, toca com áudio profissional.

- **Launcher** — criar/abrir projetos
- **Project Setup** — pede o nome ao criar novo projeto
- **Chains** — visualização da cadeia de blocos, drag/reorder. Tapping a chain row's header selects it (outlined) as the active chain — the one a MIDI footswitch bound to `toggle_active_chain_enabled` acts on (#591). A block whose model is not installed (an uninstalled NAM/IR/LV2 pack, or one unsupported on this platform) loads **disabled** and is shown dimmed with an amber tint, distinct from a manually-switched-off block; the chain keeps playing without it and its enable toggle stays inert until the pack is installed and the catalog reloads (#606).
  - **Virtual DI loop** (#614) — each chain tile has a DI-loop icon button next to the per-chain volume knob. Clicking it expands a control row with: (1) a **select / dropdown** to choose the DI source, and (2) a **play / stop** toggle button. Available sources are the bundled CC0 dry-DI loops (shipped under `assets/di-loops/`) plus a **"Choose file…"** entry that opens a file picker for a user-supplied WAV. Pressing play replaces that chain's live device input with the looping DI buffer — the full block graph (amp, cab, pedals) processes it exactly as it would process the guitar signal. All other chains keep their real hardware input unaffected (per-stream isolation). The DI file must be a **dry DI** (clean guitar, no amp or effects); feeding already-processed audio will double-process the tone. A mono DI is broadcast to stereo (`Stereo([s, s])`); a stereo DI is used directly. The loop choice and on/off state are **ephemeral (runtime-only)**: they are never written to the project (`.openrig`) — consistent with ADR 0003. Reopening or reloading the project returns every chain to its live hardware input.
- **Block Editor** — edita parâmetros de um bloco. The header's model select/search picker is shown only for catalog-backed block types; the generic NAM (`nam`) and IR (`ir`) file-loader blocks have no catalog model to choose, so their header shows the loaded model name as a static label instead of the picker (#608). The editor window sizes itself to its content so every parameter is visible without scrolling — the panel (knob-grid) editor grows vertically as knobs wrap into rows, and the form editor grows with its parameter count (it was previously locked to a fixed 820px, hiding the lower params behind an internal scroll) (#622).
- **Compact Chain View** — single-chain focused editor: power switches, quick model swap, preset/scene picker, volume, configure I/O, and a **measure-latency** button whose result renders as a badge **inside this view** (next to the sonar button), not on the chains list behind it (#613). Its block-type picker uses the same tiles as the Chains screen, so the icons match. Chain reorder (move up/down) is intentionally absent here — it lives only on the Chains list, which is the multi-chain view (#613).
- **Settings** — unified configuration screen, reached from the top bar. Two scope headers:
  - *System* (persists to `config.yaml`, stays on the machine when you move the `.openrig`):
    - **Audio interface** — input/output device, sample rate, buffer size, bit depth.
    - **Language** — UI locale override.
    - **MIDI devices** — enable/disable each port, edit per-device alias.
  - *Project* (persists to `.openrig`, travels with the rig):
    - **Project metadata** — name and other project-level fields.
    - **MIDI mapping** — binding editor: click **+ Add**, press a control on your MIDI device (MIDI Learn), then pick a Command. Bindings are stored under `midi.bindings` in the `.openrig`. The system-wide fallback (`midi-bindings.yaml`) is used when the open project has no `midi:` field.
- **Chain Editor** — nome, instrumento, I/O blocks
- **Tuner** — janela com 1 tuner por canal ativo (`feature-dsp/pitch_yin.rs` + `engine/input_tap.rs` + `adapter-gui/tuner_session.rs`)
- **Spectrum** — analisador 63-band 1/6 octava por canal de Output (`feature-dsp/spectrum_fft.rs` + `engine/output_tap.rs` + `adapter-gui/spectrum_session.rs`)

Janela principal inicia em **1100×620px lógicos**.
