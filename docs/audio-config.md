# Configuração de áudio

## I/O como blocos

`Input`, `Output`, `Insert` são variantes de `AudioBlockKind` dentro de `chain.blocks`. Não existem listas separadas.

- `blocks[0]` = InputBlock (fixo, não removível)
- `blocks[N-1]` = OutputBlock (fixo, não removível)
- Inputs/Outputs/Inserts extras podem ser inseridos no meio
- Cada Input cria stream paralelo isolado; Output é tap não-destrutivo
- Insert divide a chain em segmentos; desabilitado = bypass (sinal passa direto)

Exemplo YAML mínimo:

```yaml
chains:
  - description: guitar 1
    instrument: electric_guitar
    blocks:
      - { type: input,  model: standard, enabled: true, entries: [{ name: In1, device_id: "...", mode: mono, channels: [0] }] }
      - { type: preamp, model: marshall_jcm_800_2203, enabled: true, params: { volume: 70, gain: 40 } }
      - { type: insert, model: external_loop, enabled: true, send: {...}, return_: {...} }
      - { type: delay,  model: digital_clean, enabled: true, params: { time_ms: 350, feedback: 40, mix: 30 } }
      - { type: output, model: standard, enabled: true, entries: [{ name: Out1, device_id: "...", mode: stereo, channels: [0,1] }] }
```

Sample rates: 44.1/48/88.2/96 kHz. Buffer sizes: 32/64/128/256/512/1024. Bit depths: 16/24/32. YAML antigo (`inputs:`/`outputs:` separados, `input_device_id`/`output_device_id` únicos) é migrado automaticamente.

## Per-machine device settings (config.yaml)

Sample rate, buffer size, bit depth, language são **per-machine**, não per-project. Vivem no `config.yaml` unificado (#287):

- macOS: `~/Library/Application Support/OpenRig/config.yaml`
- Windows: `%APPDATA%\OpenRig\config.yaml`
- Linux: `~/.config/OpenRig/config.yaml`

Schema:

```yaml
recent_projects: [...]
paths: { thumbnails, screenshots, metadata }
input_devices: [{ device_id, name, sample_rate, buffer_size_frames, bit_depth, ... }]
output_devices: [...]
language: pt-BR  # ou en-US, ou null para seguir o OS
```

`gui-settings.yaml` legado é migrado automaticamente para `config.yaml` no primeiro boot e removido — sem ação manual.

`load_project_session()` popula `project.device_settings` em memória. YAML do projeto **não persiste** `device_settings` (`skip_serializing`), mas YAML antigo com o campo ainda deserializa.

## JACK lifecycle (Linux only)

Com feature `jack`, OpenRig controla o ciclo de vida do JACK. `ensure_jack_running()` em infra-cpal detecta a placa USB, lê SR/buffer do `device_settings`, configura mixer ALSA, lança `jackd -d alsa -d hw:$CARD -r $SR -p $BUF -n 3`, espera o socket aparecer em `/dev/shm/`. Timer de 2s no adapter-gui (`health_timer`) verifica `is_healthy()` e tenta reconectar quando JACK volta. Tudo atrás de `#[cfg(all(target_os = "linux", feature = "jack"))]`.
