# I/O Blocks Design Spec

## Overview

Input and Output are blocks inside `chain.blocks`, positioned anywhere in the chain. Each Input creates an isolated parallel audio stream. Each Output is a tap that sums all active streams at that point.

## Data Model

```rust
struct InputBlock {
    model: String,             // "standard" (future: other types)
    entries: Vec<InputEntry>,  // multiple device+channel configs
}

struct InputEntry {
    name: String,
    device_id: DeviceId,
    mode: ChainInputMode,      // Mono, Stereo, DualMono
    channels: Vec<usize>,
}

struct OutputBlock {
    model: String,             // "standard" (future: aux_send, etc)
    entries: Vec<OutputEntry>,
}

struct OutputEntry {
    name: String,
    device_id: DeviceId,
    mode: ChainOutputMode,     // Mono, Stereo
    channels: Vec<usize>,
}
```

InputBlock and OutputBlock are variants of `AudioBlockKind` alongside Nam, Core, Select.

## Chain Structure

```
[In chip] → [Tuner] → [TS9] → [Input block] → [Dynamics] → [Amp] → [Reverb] → [Out chip]
  fixed                          middle block                                      fixed
```

- **First block** = InputBlock fixed (rendered as "In" chip, not draggable, not removable)
- **Last block** = OutputBlock fixed (rendered as "Out" chip, not draggable, not removable)
- **Middle I/O blocks** = rendered with arrow icon (input: arrow in, output: arrow out), draggable, removable, can be enabled/disabled

## Audio Processing

Each InputBlock creates **isolated parallel streams**. Every effect block after an Input gets its own processor instance per stream. Streams never interfere with each other.

```
[Input A: Guitar ch1] → [Comp instance 1] → [Preamp instance 1] → ...
[Input B: Keys ch3+4]                     → [Preamp instance 2] → ...
                                                                    ↓
                                                          [Output: sum streams]
```

- **Input block at position N**: creates stream that processes blocks from N+1 forward
- **Output block at position M**: taps (copies) all active streams at that point, sums them, sends to device
- **Disabled Input**: its stream stops, blocks after it only process remaining active streams

## User Flows

### Chip In (fixed first InputBlock)

1. Click chip "In"
2. Opens entries list window showing entries of this block only
3. Can edit/remove entries (minimum 1 entry required for fixed block)
4. "+ Adicionar entrada" opens device/channels/mode config window
5. After configuring, returns to entries list with new entry
6. "OK" saves entries to this block only (nothing else moves)
7. Project becomes dirty — user saves project separately

### Chip Out (fixed last OutputBlock)

Same as chip In but for outputs. Minimum 1 entry required.

### Picker "+" → INPUT or OUTPUT

1. Click "+" between blocks
2. Select INPUT or OUTPUT from type picker
3. Creates new I/O block at that exact position with empty entries
4. Opens entries list window
5. User adds entries via device/channels/mode config
6. "OK" saves — block stays at inserted position

### Click I/O block in middle

1. Opens entries list window showing entries of that specific block
2. Same editing flow as chips
3. "OK" updates only that block — no other block moves or changes
4. Can have zero entries (block becomes empty/inactive)

### Enable/Disable I/O block in middle

- Same LED toggle as effect blocks
- Disabled Input: its stream stops
- Disabled Output: stops sending to device at that point

### Remove I/O block in middle

- Can be removed like any block
- Fixed first Input and last Output cannot be removed

## YAML Format

```yaml
chains:
  - description: Chain 1
    instrument: electric_guitar
    blocks:
      - type: input
        enabled: true
        model: standard
        entries:
          - name: Guitar 1
            device_id: "scarlett..."
            mode: mono
            channels: [0]
      - type: gain
        enabled: true
        model: ibanez_ts9
        params:
          drive: 35.0
      - type: input
        enabled: true
        model: standard
        entries:
          - name: Keys L
            device_id: "scarlett..."
            mode: stereo
            channels: [2, 3]
      - type: delay
        enabled: true
        model: digital_clean
        params:
          time_ms: 200
      - type: output
        enabled: true
        model: standard
        entries:
          - name: Main Out
            device_id: "scarlett..."
            mode: stereo
            channels: [0, 1]
```

## Visual

- Fixed In/Out: current chip style (not changing)
- Middle Input block: arrow-in icon, label "INPUT", draggable
- Middle Output block: arrow-out icon, label "OUTPUT", draggable
- LED toggle for enable/disable on middle I/O blocks

## Validation

- No two entries (across all blocks) share same device+channel for inputs
- No two entries (across all blocks) share same device+channel for outputs
- Fixed first/last blocks must have at least 1 entry
- Middle blocks can have 0 entries (inactive)

## Key Implementation Rules

- **Save updates ONLY the block being edited** — never reconstructs the entire chain
- **Block position never changes** on save — only entries inside the block change
- **Each I/O block is independent** — editing one never affects another
- **Draft state** tracks which specific block is being edited (`editing_io_block_index`)
