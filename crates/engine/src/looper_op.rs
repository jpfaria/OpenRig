//! Responsibility: names the operations a looper accepts from the control side.

use project::chain::LooperSpeed;

/// A control-thread request for one looper. Ops carrying a buffer hand its
/// ownership to the audio thread.
#[derive(Debug)]
pub enum LooperOp {
    /// Claim a free slot for `uid`, recording from and playing on segment
    /// `seg` — the chain segment that serves the looper's chosen input
    /// endpoint (0 = the chain's first input, the default). A looper only
    /// records / plays on its own segment, so a rig whose signal is on
    /// another input is captured, not silence.
    Create {
        uid: u64,
        seg: usize,
    },
    /// Free the slot and return every layer it holds.
    Remove {
        uid: u64,
    },
    /// The record/overdub footswitch tap. `buffer` must be `Some` when the tap
    /// starts a recording (see [`LooperSlot::tap_record`]).
    TapRecord {
        uid: u64,
        buffer: Option<Box<[f32]>>,
    },
    /// Install a layer recorded earlier (restored from disk) as the base
    /// layer of an empty looper, `len_frames` long.
    LoadLayer {
        uid: u64,
        buffer: Box<[f32]>,
        len_frames: usize,
    },
    Play {
        uid: u64,
    },
    Stop {
        uid: u64,
    },
    Undo {
        uid: u64,
    },
    Redo {
        uid: u64,
    },
    Clear {
        uid: u64,
    },
    SetMix {
        uid: u64,
        value: f32,
    },
    SetDecay {
        uid: u64,
        value: f32,
    },
    SetSpeed {
        uid: u64,
        speed: LooperSpeed,
    },
    SetReverse {
        uid: u64,
        value: bool,
    },
}

impl LooperOp {
    pub(crate) fn uid(&self) -> u64 {
        match self {
            Self::Create { uid, .. }
            | Self::Remove { uid }
            | Self::TapRecord { uid, .. }
            | Self::LoadLayer { uid, .. }
            | Self::Play { uid }
            | Self::Stop { uid }
            | Self::Undo { uid }
            | Self::Redo { uid }
            | Self::Clear { uid }
            | Self::SetMix { uid, .. }
            | Self::SetDecay { uid, .. }
            | Self::SetSpeed { uid, .. }
            | Self::SetReverse { uid, .. } => *uid,
        }
    }

    /// Take the buffer out of an op that carries one.
    pub(crate) fn take_buffer(self) -> Option<Box<[f32]>> {
        match self {
            Self::TapRecord { buffer, .. } => buffer,
            Self::LoadLayer { buffer, .. } => Some(buffer),
            _ => None,
        }
    }
}
