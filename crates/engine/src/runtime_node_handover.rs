//! Responsibility: hands a block over from the node it replaces to the fresh node an in-place edit built.
//!
//! #987: a node built fresh by a live edit used to start at once from its own
//! cold state, fading in from its dry input. An IR convolver plays one
//! partition of zeros and then its onset, a NAM starts from an empty receptive
//! field, and the old node's wet output was cut to the block's dry input on the
//! first sample: a step on every scene switch that changed a cab or an amp.
//! Here the fresh node first runs unheard while the node it replaces (or the
//! dry input, when it replaces nothing) keeps playing, then the two are
//! crossfaded with the same raised cosine as a block toggle.
//!
//! Audio thread: no allocation (the buffer is reserved when the handover is
//! set up), no lock, and the replaced node is never dropped here — a finished
//! handover stays parked on the node until the next edit takes it off on the
//! control thread.

use crossbeam_queue::ArrayQueue;

use crate::runtime::FADE_IN_FRAMES;
use crate::runtime_audio_frame::AudioFrame;
use crate::runtime_dsp::blend_frame;
use crate::runtime_process_segment::apply_block_processor;
use crate::runtime_state::{BlockError, BlockRuntimeNode, FadeState, RuntimeProcessor};

/// Frames the fresh node runs unheard before the crossfade starts: past an IR
/// partition's latency and the first of its response, and most of a NAM's
/// receptive field.
pub(crate) const HANDOVER_WARMUP_FRAMES: usize = 512;
/// The fade a block turned back on starts with (#987): its processor sat
/// frozen while it was off, so it first runs unheard for the warm-up (the dry
/// input plays), then fades in. `process_audio_block` holds the wet gain at 0
/// while more than `FADE_IN_FRAMES` remain.
pub(crate) const WARMED_FADE_IN_FRAMES: usize = HANDOVER_WARMUP_FRAMES + FADE_IN_FRAMES;
/// Largest callback the reserved buffer covers; a larger one plays the fresh
/// node alone rather than allocate on the audio thread.
const HANDOVER_MAX_FRAMES: usize = 8192;

pub(crate) struct NodeHandover {
    /// The node being replaced, still playing until the crossfade ends.
    /// `None`: the block replaces nothing that played (it was off, or new),
    /// so the dry input is what plays until then.
    retiring: Option<BlockRuntimeNode>,
    warmup_remaining: usize,
    fade_remaining: usize,
    heard: Vec<AudioFrame>,
}

impl NodeHandover {
    pub(crate) fn is_done(&self) -> bool {
        self.warmup_remaining == 0 && self.fade_remaining == 0
    }
}

/// Set `fresh` up to take over from `replaced` (control thread). The node the
/// edit replaces only keeps playing when it can take the same frames: same
/// buses in and out, a real processor that is on. Otherwise the dry input
/// plays until the crossfade.
pub(crate) fn begin_handover(fresh: &mut BlockRuntimeNode, replaced: Option<BlockRuntimeNode>) {
    let retiring = replaced.filter(|old| {
        old.input_layout == fresh.input_layout
            && old.output_layout == fresh.output_layout
            && old.block_snapshot.enabled
            && matches!(old.processor, RuntimeProcessor::Audio(_))
    });
    // The dry input can only stand in for a block that keeps the bus; one
    // that changes it keeps the fade it was built with.
    if retiring.is_none() && fresh.input_layout != fresh.output_layout {
        return;
    }
    fresh.fade_state = FadeState::Active;
    fresh.handover = Some(Box::new(NodeHandover {
        retiring,
        warmup_remaining: HANDOVER_WARMUP_FRAMES,
        fade_remaining: FADE_IN_FRAMES,
        heard: Vec::with_capacity(HANDOVER_MAX_FRAMES),
    }));
}

/// Take every finished handover off `nodes` (control thread) so the replaced
/// nodes are dropped by the caller, off the audio thread.
pub(crate) fn take_finished_handovers(nodes: &mut [BlockRuntimeNode]) -> Vec<Box<NodeHandover>> {
    nodes
        .iter_mut()
        .filter(|node| node.handover.as_ref().is_some_and(|h| h.is_done()))
        .filter_map(|node| node.handover.take())
        .collect()
}

/// Audio thread: process a node that is taking over. Returns `false` when the
/// node has no handover running, so the caller processes it as usual.
pub(crate) fn process_handover(
    node: &mut BlockRuntimeNode,
    frames: &mut [AudioFrame],
    error_queue: &ArrayQueue<BlockError>,
) -> bool {
    let Some(mut handover) = node.handover.take() else {
        return false;
    };
    if handover.is_done() || frames.len() > handover.heard.capacity() {
        node.handover = Some(handover);
        return false;
    }
    handover.heard.clear();
    handover.heard.extend_from_slice(frames);
    if let Some(old) = handover.retiring.as_mut() {
        apply_block_processor(old, handover.heard.as_mut_slice(), error_queue);
    }
    apply_block_processor(node, frames, error_queue);
    let fade_total = FADE_IN_FRAMES as f32;
    for (frame, heard) in frames.iter_mut().zip(handover.heard.iter()) {
        if handover.warmup_remaining > 0 {
            *frame = *heard;
            handover.warmup_remaining -= 1;
        } else if handover.fade_remaining > 0 {
            let progress = 1.0 - handover.fade_remaining as f32 / fade_total;
            let wet_gain = 0.5 * (1.0 - (std::f32::consts::PI * progress).cos());
            blend_frame(frame, *heard, 1.0 - wet_gain, wet_gain);
            handover.fade_remaining -= 1;
        }
    }
    node.handover = Some(handover);
    true
}
