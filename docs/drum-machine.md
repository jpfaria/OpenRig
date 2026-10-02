# Drum machine

A built-in play-along drum machine: pick a groove, press play, ask for a fill.
It plays on its own output stream, apart from every chain, the same way the
metronome does.

## Engine (`crates/feature-dsp/src/drums/`)

| Piece | What it is |
|---|---|
| `DrumRole` | The kit pieces a groove can address (kick, snare, hats, toms, cymbals...). Grooves never use a kit's note numbers; each kit maps its instruments to roles. A role the kit lacks falls back to the nearest piece it has (`fallback()`), so any groove plays on any kit. `from_general_midi` maps GM percussion notes. |
| `DrumKit` / `DrumPiece` / `DrumLayer` | Mono samples per role, in velocity layers; each layer can hold alternate samples that play round-robin. A piece has gain, constant-power pan and an optional choke group (closed hat cuts the open hat with a 10 ms fade). Samples play one to one, so a kit is prepared at the stream's sample rate before it reaches the engine. |
| `DrumPattern` / `Groove` | Hits positioned in beats. A groove has a main loop (one or more bars) and one-bar fills used in turn. |
| `DrumMachine` | Renders the groove on the kit into a stereo buffer. |

### Timing

Time is counted in whole frames since play. A hit maps to the first frame at or
after its beat, so it lands on the same sample whatever the buffer size; a
tempo change keeps the position in the bar.

### Fills

Asked for mid-bar, the fill takes over from the current position to the end of
the bar. Asked for in the last beat of a bar, it plays the whole next bar. The
groove resumes after the fill. A groove with no fills ignores the request.

### Real-time safety

`render` never allocates, locks or blocks: 64 preallocated voices (the oldest
is stolen when all are busy), voices refer to kit samples by index, and a kit
or groove swap hands the previous one back to the caller to drop off the audio
thread. Stop ends new hits; hits already sounding ring out.
