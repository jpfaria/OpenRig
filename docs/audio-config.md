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

## Per-machine device settings (gui-settings.yaml)

Sample rate, buffer size, bit depth são **per-machine**, não per-project. Ficam em:

- macOS: `~/Library/Application Support/OpenRig/gui-settings.yaml`
- Windows: `%APPDATA%\OpenRig\gui-settings.yaml`
- Linux: `~/.config/OpenRig/gui-settings.yaml`

`load_project_session()` popula `project.device_settings` em memória. YAML do projeto **não persiste** `device_settings` (`skip_serializing`), mas YAML antigo com o campo ainda deserializa.

## JACK lifecycle (Linux only)

Com feature `jack`, OpenRig controla o ciclo de vida do JACK. `ensure_jack_running()` em infra-cpal detecta a placa USB, lê SR/buffer do `device_settings`, configura mixer ALSA, lança `jackd -d alsa -d hw:$CARD -r $SR -p $BUF -n 3`, espera o socket aparecer em `/dev/shm/`. Timer de 2s no adapter-gui (`health_timer`) verifica `is_healthy()` e tenta reconectar quando JACK volta. Tudo atrás de `#[cfg(all(target_os = "linux", feature = "jack"))]`.
