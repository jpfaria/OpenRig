//! Responsibility: bypasses an insert's external loop without touching its streams.
//!
//! Issue #967. An insert cuts the chain in two: the send segment writes the
//! external gear, the return segment reads what comes back. Switching the
//! insert OFF used to remove the cut altogether, which changed how many streams
//! the chain needed, so a one-bit flip closed and reopened every stream the
//! chain owned (measured 2.1 s and 3.0 s of silence on the owner's rig).
//!
//! The streams now stay exactly as they are and the loop is bypassed HERE:
//!
//! * The return segment crossfades its INPUT between the gear's answer and the
//!   dry send signal ([`InsertBridge::mix_return`]) — a raised-cosine ramp, so
//!   neither the tail nor a delay line after the insert ever sees a step.
//! * The send is faded to silence while the loop is off
//!   ([`InsertBridge::shape_send`]): the gear gets nothing, exactly as it did
//!   when a disabled insert had no send stream. Switching back on feeds the
//!   gear at once but keeps the dry signal for [`ENABLE_HOLD_FRAMES`] — long
//!   enough for the loop's round trip to deliver the gear's first answer — and
//!   only then crossfades to it, so there is no hole while the loop warms up.
//! * The dry signal is the SUM of every segment feeding the send, read from the
//!   send route's per-callback mix. When the return is driven by the same
//!   device callback (the loop on the guitar's interface) it is read in place,
//!   with no delay. When the return's interface has its own callback, the sum
//!   is parked here and the fill level is held to about one period, so a clock
//!   difference between the interfaces can never pile up delay.
//!
//! Everything below runs on the audio thread under the runtime's processing
//! lock the callbacks already take: no allocation (the deque is sized at build
//! time and never grows), no lock, no syscall.

use std::collections::VecDeque;

use domain::ids::BlockId;

use crate::audio_frame::AudioFrame;
use crate::runtime::FADE_IN_FRAMES;
use crate::runtime_segments::ChainSegment;

/// Frames the dry signal is kept after the loop is switched back on, before
/// the crossfade to the gear's answer starts — covers the send → gear →
/// return round trip at every buffer size we run.
pub(crate) const ENABLE_HOLD_FRAMES: usize = 1024;

/// Frames of slack in a cross-callback dry path — its hard ceiling. The fill
/// is trimmed to about one period long before this is reached.
const BRIDGE_CAPACITY: usize = 8192;

/// The bypass state of one bound insert.
pub(crate) struct InsertBridge {
    /// The insert block this bridge serves — what a toggle is matched against.
    pub(crate) block_id: BlockId,
    /// Route index of the insert's send (its position in the runtime's routes).
    send_route: usize,
    /// Device callback (cpal input index) that runs the send segment(s).
    producer_input: usize,
    /// Device callback (cpal input index) that runs the return segment.
    consumer_input: usize,
    /// Is the insert switched off (the loop bypassed)?
    bypassed: bool,
    /// Weight of the dry signal at the return's input: 0 = gear, 1 = dry.
    return_mix: f32,
    /// Level of the send: 1 = feeding the gear, 0 = silent.
    send_gain: f32,
    /// Dry frames still to keep after a switch-on before crossfading to the gear.
    enable_hold: usize,
    /// Cross-callback dry frames (see the module doc).
    frames: VecDeque<AudioFrame>,
    /// Frames the producer parked / the consumer took on their last callback —
    /// the period the fill level is held to.
    last_park: usize,
    last_take: usize,
}

impl InsertBridge {
    fn new(
        block_id: BlockId,
        enabled: bool,
        send_route: usize,
        producer_input: usize,
        consumer_input: usize,
    ) -> Self {
        Self {
            block_id,
            send_route,
            producer_input,
            consumer_input,
            bypassed: !enabled,
            return_mix: if enabled { 0.0 } else { 1.0 },
            send_gain: if enabled { 1.0 } else { 0.0 },
            enable_hold: 0,
            frames: VecDeque::with_capacity(BRIDGE_CAPACITY),
            last_park: 0,
            last_take: 0,
        }
    }

    pub(crate) fn send_route(&self) -> usize {
        self.send_route
    }

    /// Frames the cross-callback dry path holds right now — its delay.
    #[cfg(test)]
    pub(crate) fn held_frames(&self) -> usize {
        self.frames.len()
    }

    /// Switch the insert on or off. Applied on the audio thread (the toggle
    /// queue); the ramps below carry the change, so this is click-free.
    pub(crate) fn set_enabled(&mut self, enabled: bool) {
        if enabled != self.bypassed {
            return;
        }
        self.bypassed = !enabled;
        // Hold the dry signal only if the return is actually on it; a switch
        // back on before the crossfade ever left the gear has nothing to wait for.
        self.enable_hold = if enabled && self.return_mix > 0.0 {
            ENABLE_HOLD_FRAMES
        } else {
            0
        };
    }

    /// Is any dry signal needed right now (off, or still crossfading back)?
    fn needs_dry(&self) -> bool {
        self.bypassed || self.return_mix > 0.0
    }

    fn crosses_callbacks(&self) -> bool {
        self.producer_input != self.consumer_input
    }

    /// Crossfade the return segment's input (`gear`, the frames its device
    /// delivered) toward the dry send signal. `same_callback_send` is the send
    /// route's mix of THIS callback when the send segments already ran in it;
    /// otherwise the parked frames are used.
    pub(crate) fn mix_return(
        &mut self,
        gear: &mut [AudioFrame],
        same_callback_send: Option<&[AudioFrame]>,
    ) {
        if !self.needs_dry() && self.enable_hold == 0 {
            return;
        }
        let count = gear.len();
        self.last_take = count;
        if same_callback_send.is_none() && self.return_mix == 0.0 && self.frames.len() < count {
            // Cross-callback bypass that just started: let the dry path fill a
            // period before leaving the gear, so the crossfade never runs into
            // an empty bridge.
            return;
        }
        let step = 1.0 / FADE_IN_FRAMES as f32;
        for (i, frame) in gear.iter_mut().enumerate() {
            let dry = match same_callback_send {
                Some(send) => send.get(i).copied(),
                None => self.frames.pop_front(),
            }
            .unwrap_or_else(|| silence_like(*frame));
            let target = if self.bypassed || self.enable_hold > 0 {
                1.0
            } else {
                0.0
            };
            if !self.bypassed && self.enable_hold > 0 {
                self.enable_hold -= 1;
            }
            self.return_mix = ramp(self.return_mix, target, step);
            *frame = blend(*frame, dry, raised_cosine(self.return_mix));
        }
        if !self.needs_dry() {
            self.frames.clear();
        }
    }

    /// Called once per callback with the send route's mix after every segment
    /// ran: park the dry sum for a return on another callback, then fade the
    /// send toward silence (loop off) or full level (loop on).
    pub(crate) fn shape_send(&mut self, send: &mut [AudioFrame]) {
        if self.crosses_callbacks() && self.needs_dry() {
            self.park(send);
        }
        let target = if self.bypassed { 0.0 } else { 1.0 };
        if self.send_gain == target && target == 1.0 {
            return;
        }
        let step = 1.0 / FADE_IN_FRAMES as f32;
        for frame in send.iter_mut() {
            self.send_gain = ramp(self.send_gain, target, step);
            *frame = scale(*frame, raised_cosine(self.send_gain));
        }
    }

    /// Park the dry sum and hold the fill to about one period: whatever a
    /// clock difference adds beyond that is dropped from the OLD end, so the
    /// bypassed path never builds delay.
    fn park(&mut self, send: &[AudioFrame]) {
        self.last_park = send.len();
        for &frame in send {
            if self.frames.len() == self.frames.capacity() {
                self.frames.pop_front();
            }
            self.frames.push_back(frame);
        }
        let period = self.last_park.max(self.last_take);
        if self.frames.len() > 2 * period {
            let excess = self.frames.len() - period;
            self.frames.drain(..excess);
        }
    }

    /// Take over the moving state (ramps, hold, parked frames) of the bridge
    /// this one replaces, then apply this bridge's enable flag through the
    /// ramps — an in-place update neither jumps nor forgets a switch.
    fn carry_from(&mut self, old: InsertBridge) {
        let enabled = !self.bypassed;
        self.bypassed = old.bypassed;
        self.return_mix = old.return_mix;
        self.send_gain = old.send_gain;
        self.enable_hold = old.enable_hold;
        self.frames = old.frames;
        self.last_park = old.last_park;
        self.last_take = old.last_take;
        self.set_enabled(enabled);
    }
}

/// One bridge per bound insert, wired from the segments the chain was cut into:
/// the send route and callback of the segment(s) feeding insert `k`, and the
/// callback of the segment reading its return.
pub(crate) fn bridges_for(
    segments: &[ChainSegment],
    bound_inserts: &[(BlockId, bool)],
) -> Vec<InsertBridge> {
    bound_inserts
        .iter()
        .enumerate()
        .filter_map(|(k, (id, enabled))| {
            let producer = segments.iter().find(|s| s.insert_send == Some(k))?;
            let consumer = segments.iter().find(|s| s.insert_return == Some(k))?;
            Some(InsertBridge::new(
                id.clone(),
                *enabled,
                *producer.output_route_indices.first()?,
                producer.cpal_input_index,
                consumer.cpal_input_index,
            ))
        })
        .collect()
}

/// Replace `current` with `fresh`, carrying each insert's moving state over by
/// block id. Runs on the control thread inside the swap critical section.
pub(crate) fn replace_bridges(current: &mut Vec<InsertBridge>, mut fresh: Vec<InsertBridge>) {
    let mut old = std::mem::take(current);
    for bridge in fresh.iter_mut() {
        if let Some(pos) = old.iter().position(|o| o.block_id == bridge.block_id) {
            bridge.carry_from(old.swap_remove(pos));
        }
    }
    *current = fresh;
}

fn ramp(value: f32, target: f32, step: f32) -> f32 {
    if value < target {
        (value + step).min(target)
    } else {
        (value - step).max(target)
    }
}

fn raised_cosine(x: f32) -> f32 {
    0.5 * (1.0 - (std::f32::consts::PI * x).cos())
}

fn silence_like(frame: AudioFrame) -> AudioFrame {
    match frame {
        AudioFrame::Mono(_) => AudioFrame::Mono(0.0),
        AudioFrame::Stereo(_) => AudioFrame::Stereo([0.0, 0.0]),
    }
}

fn scale(frame: AudioFrame, gain: f32) -> AudioFrame {
    match frame {
        AudioFrame::Mono(s) => AudioFrame::Mono(s * gain),
        AudioFrame::Stereo([l, r]) => AudioFrame::Stereo([l * gain, r * gain]),
    }
}

/// `gear·(1-w) + dry·w` in the gear frame's layout — a mono dry signal is
/// broadcast onto a stereo return, a stereo one summed onto a mono return.
fn blend(gear: AudioFrame, dry: AudioFrame, w: f32) -> AudioFrame {
    let g = 1.0 - w;
    match (gear, dry) {
        (AudioFrame::Stereo([gl, gr]), AudioFrame::Stereo([dl, dr])) => {
            AudioFrame::Stereo([gl * g + dl * w, gr * g + dr * w])
        }
        (AudioFrame::Stereo([gl, gr]), AudioFrame::Mono(d)) => {
            AudioFrame::Stereo([gl * g + d * w, gr * g + d * w])
        }
        (AudioFrame::Mono(gm), AudioFrame::Mono(d)) => AudioFrame::Mono(gm * g + d * w),
        (AudioFrame::Mono(gm), AudioFrame::Stereo([dl, dr])) => {
            AudioFrame::Mono(gm * g + (dl + dr) * 0.5 * w)
        }
    }
}

/// Every Insert block of the chain that has no bridge (its E/S does not
/// resolve here): a toggle on one is a no-op, never a "not found" error.
pub(crate) fn passive_insert_ids(
    chain: &project::chain::Chain,
    bound_inserts: &[(BlockId, bool)],
) -> Vec<BlockId> {
    chain
        .blocks
        .iter()
        .filter(|b| matches!(b.kind, project::block::AudioBlockKind::Insert(_)))
        .filter(|b| !bound_inserts.iter().any(|(id, _)| id == &b.id))
        .map(|b| b.id.clone())
        .collect()
}

#[cfg(test)]
#[path = "insert_bridge_tests.rs"]
mod tests;
