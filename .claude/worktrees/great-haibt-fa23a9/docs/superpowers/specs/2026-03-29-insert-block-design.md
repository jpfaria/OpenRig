# Insert Block Design Spec

## Overview

An Insert block sends audio to an external device and receives it back, enabling hardware processing in the chain (external pedals, rack effects, pedalboards).

## Data Model

```rust
pub struct InsertBlock {
    pub model: String,  // "standard"
    pub send: InsertEndpoint,
    pub return_: InsertEndpoint,
}

pub struct InsertEndpoint {
    pub device_id: DeviceId,
    pub mode: ChainInputMode,  // Mono, Stereo, DualMono
    pub channels: Vec<usize>,
}
```

`InsertBlock` is a new variant of `AudioBlockKind`.

## YAML Format

```yaml
- type: insert
  enabled: true
  model: standard
  send:
    device_id: "coreaudio:...MK-300..."
    mode: stereo
    channels: [0, 1]
  return:
    device_id: "coreaudio:...MK-300..."
    mode: stereo
    channels: [0, 1]
```

## Audio Processing

1. Audio arrives at the Insert block
2. **Send**: frames are written to the send device+channels (like an Output tap)
3. **Return**: frames are read from the return device+channels (like an Input)
4. The return frames replace the signal — blocks after the Insert process the returned audio
5. **Disabled (bypass)**: signal passes through unchanged, no send/return streams created

The Insert creates TWO audio streams:
- One output stream (send) writing to the external device
- One input stream (return) reading from the external device

Latency: the external device adds latency (round-trip through hardware). This is inherent and expected.

## Chain Example

```
[Scarlett In] → [Comp] → [EQ] → [Insert: MK-300] → [Delay] → [Reverb] → [Scarlett Out]
                                   ↓           ↑
                                 MK-300 Out   MK-300 In
                                   ↓           ↑
                                 External hardware processing
```

## Chain View

- Visual: single block with loop/circular arrow icon
- Label: "INSERT"
- Draggable, removable
- Enable/disable LED (disabled = bypass)
- Click opens config window with Send and Return device/channels/mode

## Behavior

- **Send and Return are always paired** — one block, not two
- **No blocks between Send and Return** — they are the same block
- **Bypass when disabled** — signal passes through unchanged
- **Can use different devices** for send and return
- **Always 1 send + 1 return** — no multiple sends/returns

## Config Window

When clicking the Insert block, opens a config window with:
- **Send section**: device selector, mode (mono/stereo), channel selector
- **Return section**: device selector, mode (mono/stereo), channel selector
- OK / Cancel buttons
- Enable/disable toggle + delete (for middle blocks)

## Validation

- Send device+channels must be valid
- Return device+channels must be valid
- Send and return can use same or different devices
