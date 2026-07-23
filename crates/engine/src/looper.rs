//! Issue #323 — the per-chain looper core (Boss RC-style, multi-layer).
//!
//! One [`LooperSlot`] is one looper: a stack of interleaved-stereo layers, a
//! transport state machine, and a playback cursor. It is driven frame by frame
//! from the audio callback via [`LooperSlot::tick`], which returns the loop's
//! contribution to add to the chain input (the caller sums it — the slot never
//! touches the dry signal it is handed, beyond recording it).
//!
//! ## Real-time contract
//!
//! Nothing here allocates, locks or frees. Layer buffers are allocated by the
//! control thread and handed in through [`LooperSlot::tap_record`]; buffers the
//! slot no longer needs are parked in `retired` and collected off the audio
//! thread through [`LooperSlot::take_retired`], so the `Box` is dropped where
//! dropping is allowed. The `layers`/`retired` vectors are sized once at
//! construction to [`LOOPER_MAX_LAYERS`], so pushing into them never reallocs.
//!
//! ## Why playback sums the layers on read
//!
//! Keeping a single pre-mixed buffer would make undo a re-mix of up to eight
//! 60-second layers — work that cannot happen on the audio thread and would
//! need an off-thread round trip before the user hears the undo. Summing on
//! read costs one multiply-add per layer per channel per frame (flat and
//! predictable) and makes undo/redo a counter change.

/// How many overdub layers a single looper can hold.
pub const LOOPER_MAX_LAYERS: usize = 8;

/// Transport state of one looper.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LooperState {
    /// Nothing recorded.
    #[default]
    Empty,
    /// Capturing the first layer; the loop length is still growing.
    Recording,
    /// Looping the recorded layers.
    Playing,
    /// Looping while capturing a new layer on top.
    Overdubbing,
    /// Material kept, playback halted.
    Stopped,
}

/// Playback rate of a looper — owned by `project::chain` (it is persisted
/// with the chain); the cursor step below is the runtime reading of it. Not a
/// resample: the read cursor steps by this factor and interpolates, so the
/// pitch shifts with the speed (the classic looper behaviour).
pub use project::chain::LooperSpeed;

fn speed_step(speed: LooperSpeed) -> f64 {
    match speed {
        LooperSpeed::Half => 0.5,
        LooperSpeed::Normal => 1.0,
        LooperSpeed::Double => 2.0,
    }
}

pub struct LooperSlot {
    /// Interleaved-stereo layers, `max_frames * 2` samples each. Layers at
    /// `..active` are audible; `active..` is the redo tail.
    layers: Vec<Box<[f32]>>,
    /// Buffers handed back for the control thread to drop.
    retired: Vec<Box<[f32]>>,
    /// How many layers are audible (undo/redo move this).
    active: usize,
    max_frames: usize,
    /// Frozen loop length; 0 until the first recording is closed.
    len_frames: usize,
    /// Write cursor while recording the first layer.
    write_pos: usize,
    /// Fractional playback cursor, in frames.
    read_pos: f64,
    state: LooperState,
    mix: f32,
    decay: f32,
    speed: LooperSpeed,
    reverse: bool,
}

impl LooperSlot {
    pub fn new(max_frames: usize) -> Self {
        Self {
            layers: Vec::with_capacity(LOOPER_MAX_LAYERS),
            retired: Vec::with_capacity(LOOPER_MAX_LAYERS),
            active: 0,
            max_frames,
            len_frames: 0,
            write_pos: 0,
            read_pos: 0.0,
            state: LooperState::Empty,
            mix: 1.0,
            decay: 1.0,
            speed: LooperSpeed::Normal,
            reverse: false,
        }
    }

    pub fn state(&self) -> LooperState {
        self.state
    }

    pub fn len_frames(&self) -> usize {
        self.len_frames
    }

    pub fn max_frames(&self) -> usize {
        self.max_frames
    }

    /// Audible layer count (what undo/redo move).
    pub fn active_layers(&self) -> usize {
        self.active
    }

    pub fn position_frames(&self) -> usize {
        self.read_pos as usize
    }

    /// Whether another layer can be recorded — the control thread checks this
    /// before allocating a buffer.
    pub fn can_record(&self) -> bool {
        self.active < LOOPER_MAX_LAYERS
    }

    /// The record/overdub footswitch tap.
    ///
    /// `buffer` must be `Some` when the tap starts a recording (state `Empty`,
    /// `Playing` or `Stopped`) and is ignored — handed straight back through
    /// `retired` — otherwise.
    pub fn tap_record(&mut self, buffer: Option<Box<[f32]>>) {
        match self.state {
            LooperState::Empty => {
                if let Some(buf) = buffer {
                    self.push_layer(buf);
                    self.write_pos = 0;
                    self.len_frames = 0;
                    self.state = LooperState::Recording;
                }
            }
            LooperState::Recording => {
                self.retire(buffer);
                self.freeze();
            }
            LooperState::Playing | LooperState::Stopped => match buffer {
                Some(buf) if self.can_record() => {
                    self.drop_redo_tail();
                    self.push_layer(buf);
                    self.state = LooperState::Overdubbing;
                }
                other => self.retire(other),
            },
            LooperState::Overdubbing => {
                self.retire(buffer);
                self.state = LooperState::Playing;
            }
        }
    }

    /// Start (or resume) playback from the top of the loop.
    pub fn play(&mut self) {
        if self.len_frames == 0 {
            return;
        }
        if matches!(self.state, LooperState::Recording) {
            self.freeze();
            return;
        }
        self.read_pos = 0.0;
        self.state = LooperState::Playing;
    }

    /// Halt playback, keeping the material.
    pub fn stop(&mut self) {
        match self.state {
            LooperState::Empty => {}
            LooperState::Recording => {
                self.freeze();
                self.state = LooperState::Stopped;
            }
            _ => self.state = LooperState::Stopped,
        }
    }

    /// Silence the newest audible layer. O(1) — the layer stays in place as
    /// the redo tail until something new is recorded over it.
    pub fn undo(&mut self) {
        self.active = self.active.saturating_sub(1);
    }

    /// Restore the layer the last undo silenced, if it is still there.
    pub fn redo(&mut self) {
        if self.active < self.layers.len() {
            self.active += 1;
        }
    }

    /// Drop every layer and go back to `Empty`.
    pub fn clear(&mut self) {
        while let Some(buf) = self.layers.pop() {
            self.retired.push(buf);
        }
        self.active = 0;
        self.len_frames = 0;
        self.write_pos = 0;
        self.read_pos = 0.0;
        self.state = LooperState::Empty;
    }

    /// Loop level, 0..=1 (values outside are clamped).
    pub fn set_mix(&mut self, mix: f32) {
        self.mix = mix.clamp(0.0, 1.0);
    }

    /// Per-layer-of-age gain applied to older layers, 0..=1. 1.0 = no decay.
    pub fn set_decay(&mut self, decay: f32) {
        self.decay = decay.clamp(0.0, 1.0);
    }

    pub fn set_speed(&mut self, speed: LooperSpeed) {
        self.speed = speed;
    }

    pub fn set_reverse(&mut self, reverse: bool) {
        self.reverse = reverse;
    }

    /// Install a layer recorded in an earlier session (restored from disk) as
    /// the looper's only layer, `len_frames` long. The looper lands in
    /// `Stopped`: reopening a project must not start playing on its own.
    pub fn load_layer(&mut self, buffer: Box<[f32]>, len_frames: usize) {
        while let Some(buf) = self.layers.pop() {
            self.retired.push(buf);
        }
        self.push_layer(buffer);
        self.len_frames = len_frames.min(self.max_frames).max(1);
        self.write_pos = self.len_frames;
        self.read_pos = 0.0;
        self.state = LooperState::Stopped;
    }

    /// Interleaved-stereo mixdown of the audible layers, one loop long — what
    /// the user would hear in one pass, ready to be written to disk.
    ///
    /// Allocates, so it is for the CONTROL thread only (project save), never
    /// the audio callback. Returns `None` when nothing is recorded.
    pub fn export_mixdown(&self) -> Option<Vec<f32>> {
        if self.active == 0 || self.len_frames == 0 {
            return None;
        }
        let mut out = Vec::with_capacity(self.len_frames * 2);
        for frame in 0..self.len_frames {
            let mut acc = [0.0f32; 2];
            let mut gain = 1.0f32;
            for layer in (0..self.active).rev() {
                let buf = &self.layers[layer];
                acc[0] += buf[frame * 2] * gain;
                acc[1] += buf[frame * 2 + 1] * gain;
                gain *= self.decay;
            }
            out.push(acc[0]);
            out.push(acc[1]);
        }
        Some(out)
    }

    /// Collect one buffer the slot is done with, so the control thread can
    /// drop it. Call until it returns `None`.
    pub fn take_retired(&mut self) -> Option<Box<[f32]>> {
        self.retired.pop()
    }

    /// Advance one frame: record `dry` when armed, and return the loop's
    /// contribution to add to the chain input.
    pub fn tick(&mut self, dry: [f32; 2]) -> [f32; 2] {
        match self.state {
            LooperState::Empty | LooperState::Stopped => [0.0, 0.0],
            LooperState::Recording => {
                self.write_frame(self.active - 1, self.write_pos, dry);
                self.write_pos += 1;
                if self.write_pos >= self.max_frames {
                    self.freeze();
                }
                [0.0, 0.0]
            }
            LooperState::Playing | LooperState::Overdubbing => {
                // While overdubbing, the layer being written is not yet
                // audible — it becomes audible when the tap closes it.
                let audible = if matches!(self.state, LooperState::Overdubbing) {
                    self.active - 1
                } else {
                    self.active
                };
                let out = self.read_mixed(audible);

                if matches!(self.state, LooperState::Overdubbing) {
                    let idx = self.read_pos as usize % self.len_frames.max(1);
                    self.write_frame(self.active - 1, idx, dry);
                }

                self.advance();
                out
            }
        }
    }

    // ── internals ───────────────────────────────────────────────────────

    fn push_layer(&mut self, buf: Box<[f32]>) {
        self.layers.push(buf);
        self.active = self.layers.len();
    }

    fn retire(&mut self, buffer: Option<Box<[f32]>>) {
        if let Some(buf) = buffer {
            self.retired.push(buf);
        }
    }

    /// Drop the layers a previous undo silenced — a new recording invalidates
    /// the redo tail.
    fn drop_redo_tail(&mut self) {
        while self.layers.len() > self.active {
            if let Some(buf) = self.layers.pop() {
                self.retired.push(buf);
            }
        }
    }

    /// Close the first recording: the loop length is fixed from here on.
    fn freeze(&mut self) {
        self.len_frames = self.write_pos.max(1).min(self.max_frames);
        self.read_pos = 0.0;
        self.state = LooperState::Playing;
    }

    fn write_frame(&mut self, layer: usize, frame: usize, v: [f32; 2]) {
        if let Some(buf) = self.layers.get_mut(layer) {
            let i = frame * 2;
            if i + 1 < buf.len() {
                buf[i] = v[0];
                buf[i + 1] = v[1];
            }
        }
    }

    /// Sum `count` layers at the current cursor, newest at unity and each
    /// older layer one decay step down, scaled by the loop level.
    fn read_mixed(&self, count: usize) -> [f32; 2] {
        if count == 0 || self.len_frames == 0 {
            return [0.0, 0.0];
        }
        let len = self.len_frames;
        let i0 = (self.read_pos as usize) % len;
        let frac = (self.read_pos - self.read_pos.floor()) as f32;
        let i1 = (i0 + 1) % len;

        let mut acc = [0.0f32; 2];
        let mut gain = 1.0f32;
        for layer in (0..count).rev() {
            let buf = &self.layers[layer];
            for ch in 0..2 {
                let a = buf[i0 * 2 + ch];
                let b = buf[i1 * 2 + ch];
                acc[ch] += (a + (b - a) * frac) * gain;
            }
            gain *= self.decay;
        }
        [acc[0] * self.mix, acc[1] * self.mix]
    }

    fn advance(&mut self) {
        let len = self.len_frames as f64;
        if len <= 0.0 {
            return;
        }
        let step = speed_step(self.speed) * if self.reverse { -1.0 } else { 1.0 };
        let mut next = self.read_pos + step;
        while next < 0.0 {
            next += len;
        }
        while next >= len {
            next -= len;
        }
        self.read_pos = next;
    }
}

#[cfg(test)]
#[path = "looper_tests.rs"]
mod tests;
