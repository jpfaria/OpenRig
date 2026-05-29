# Telas principais

Pedalboard virtual: usuário monta cadeia de efeitos visualmente, ajusta parâmetros em tempo real, toca com áudio profissional.

- **Launcher** — criar/abrir projetos
- **Project Setup** — pede o nome ao criar novo projeto
- **Chains** — visualização da cadeia de blocos, drag/reorder. Tapping a chain row's header selects it (outlined) as the active chain — the one a MIDI footswitch bound to `toggle_active_chain_enabled` acts on (#591).
- **Block Editor** — edita parâmetros de um bloco
- **Compact Chain View** — power switches e troca rápida de modelo
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
