# Telas principais

Pedalboard virtual: usuário monta cadeia de efeitos visualmente, ajusta parâmetros em tempo real, toca com áudio profissional.

- **Launcher** — criar/abrir projetos
- **Project Setup** — pede o nome ao criar novo projeto
- **Chains** — visualização da cadeia de blocos, drag/reorder
- **Block Editor** — edita parâmetros de um bloco
- **Compact Chain View** — power switches e troca rápida de modelo
- **Settings** — devices de áudio, sample rate, buffer size
- **Chain Editor** — nome, instrumento, I/O blocks
- **Tuner** — janela com 1 tuner por canal ativo (`feature-dsp/pitch_yin.rs` + `engine/input_tap.rs` + `adapter-gui/tuner_session.rs`)
- **Spectrum** — analisador 63-band 1/6 octava por canal de Output (`feature-dsp/spectrum_fft.rs` + `engine/output_tap.rs` + `adapter-gui/spectrum_session.rs`)

Janela principal inicia em **1100×620px lógicos**.
