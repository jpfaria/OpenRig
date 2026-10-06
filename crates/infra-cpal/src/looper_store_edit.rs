//! Responsibility: carries a waveform edit across the loops of the shared timeline.
//!
//! Every loop on the shared timeline is a whole number of the same cycle, so
//! an edit on one loop only keeps them together if the others lose the same
//! stretch of that cycle: on its own, a trimmed loop comes out a fraction of a
//! cycle off the rest and drifts further every lap. An edit therefore lands on
//! every loop whose length is a whole multiple of the edited loop's (the
//! region comes out of each of its cycles) or a whole divisor of it (it is
//! first repeated up to the edited loop's length, the way it plays under it).
//! A loop that is neither cannot line up with the edit and is left alone, and
//! a take still being recorded is never touched. The loops one edit touched
//! share one history step, so undo and redo move them together.

use domain::ids::ChainId;
use engine::loop_edit::{self, LoopEditError, LoopEditOp};
use engine::LooperState;

use super::LooperStore;

/// How many waveform edits can be undone. A 60 s stereo loop at 48 kHz is
/// ~23 MB, so the cap is memory, not taste — the oldest entry drops first.
pub const LOOPER_EDIT_HISTORY_MAX: usize = 8;

/// Why a waveform edit did not happen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LooperEditRefused {
    /// The looper is recording, overdubbing or playing.
    NotStopped,
    /// Nothing is recorded.
    Empty,
    /// No such chain/looper.
    Unknown,
    /// The region itself does not describe a usable loop.
    Edit(LoopEditError),
}

impl std::fmt::Display for LooperEditRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotStopped => write!(f, "the looper must be stopped to be edited"),
            Self::Empty => write!(f, "the looper holds no material"),
            Self::Unknown => write!(f, "no such looper"),
            Self::Edit(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for LooperEditRefused {}

/// One entry of a loop's edit history: the audio to go back to, and the edit
/// it belongs to — the same for every loop that edit touched.
pub(super) struct EditStep {
    pcm: Vec<f32>,
    group: u64,
}

type LoopKey = (ChainId, u64);

impl LooperStore {
    /// Reshape a STOPPED loop, carry the same edit to every loop it lines up
    /// with, and return the edited loop's new length in frames. Each touched
    /// loop lands Stopped, its pre-edit audio on its undo stack.
    pub fn apply_edit(
        &mut self,
        chain: &ChainId,
        uid: u64,
        op: LoopEditOp,
        start: usize,
        end: usize,
    ) -> Result<usize, LooperEditRefused> {
        let key = (chain.clone(), uid);
        let entry = self.slots.get(&key).ok_or(LooperEditRefused::Unknown)?;
        if entry.slot.state() != LooperState::Stopped {
            return Err(LooperEditRefused::NotStopped);
        }
        let before = entry.slot.export_raw().ok_or(LooperEditRefused::Empty)?;
        // The region is fixed on the edited loop, so the others get exactly it.
        let (op, start, end) = loop_edit::resolve_region(&before, op, start, end, self.sample_rate)
            .map_err(LooperEditRefused::Edit)?;
        let edited =
            loop_edit::apply_edit(&before, op, start, end).map_err(LooperEditRefused::Edit)?;
        let new_len = edited.len() / 2;
        let cycle = before.len() / 2;

        let mut changes = Vec::new();
        for (other, pcm) in self.followers(&key) {
            if let Some(out) =
                follow_edit(&pcm, cycle, op, start, end).map_err(LooperEditRefused::Edit)?
            {
                changes.push((other, pcm, out));
            }
        }
        changes.push((key, before, edited));
        self.commit_edit(changes);
        Ok(new_len)
    }

    /// Step back one waveform edit, on every loop it touched. `false` when
    /// there is nothing to undo. Independent of the transport's undo, which is
    /// a no-op here.
    pub fn undo_edit(&mut self, chain: &ChainId, uid: u64) -> bool {
        self.step_edit_history(chain, uid, true)
    }

    /// Step forward one undone waveform edit, on every loop it touched.
    pub fn redo_edit(&mut self, chain: &ChainId, uid: u64) -> bool {
        self.step_edit_history(chain, uid, false)
    }

    /// (undo depth, redo depth) — what the editor's buttons enable on.
    pub fn edit_history_depth(&self, chain: &ChainId, uid: u64) -> (usize, usize) {
        self.slots
            .get(&(chain.clone(), uid))
            .map(|e| (e.edit_undo.len(), e.edit_redo.len()))
            .unwrap_or((0, 0))
    }

    /// Forget a loop's edit history — its buffers describe audio that no
    /// longer exists, and undoing into them would resurrect a replaced take.
    pub(super) fn clear_edit_history(&mut self, chain: &ChainId, uid: u64) {
        if let Some(e) = self.slots.get_mut(&(chain.clone(), uid)) {
            e.edit_undo.clear();
            e.edit_redo.clear();
        }
    }

    /// Every other loop holding a finished take, with its audio.
    fn followers(&self, edited: &LoopKey) -> Vec<(LoopKey, Vec<f32>)> {
        self.slots
            .iter()
            .filter(|(key, e)| {
                *key != edited
                    && e.take.is_none()
                    && !matches!(
                        e.slot.state(),
                        LooperState::Empty | LooperState::Recording | LooperState::Overdubbing
                    )
            })
            .filter_map(|(key, e)| Some((key.clone(), e.slot.export_raw()?)))
            .collect()
    }

    /// Install every loop an edit produced, as one history step.
    fn commit_edit(&mut self, changes: Vec<(LoopKey, Vec<f32>, Vec<f32>)>) {
        self.edit_group = self.edit_group.wrapping_add(1);
        let group = self.edit_group;
        for ((chain, uid), before, after) in changes {
            self.load(&chain, uid, &after);
            if let Some(entry) = self.slots.get_mut(&(chain, uid)) {
                entry.edit_redo.clear();
                entry.edit_undo.push(EditStep { pcm: before, group });
                if entry.edit_undo.len() > LOOPER_EDIT_HISTORY_MAX {
                    entry.edit_undo.remove(0);
                }
            }
        }
    }

    fn step_edit_history(&mut self, chain: &ChainId, uid: u64, undo: bool) -> bool {
        let stack = |e: &super::LoopEntry| {
            if undo {
                e.edit_undo.last().map(|s| s.group)
            } else {
                e.edit_redo.last().map(|s| s.group)
            }
        };
        let Some(group) = self.slots.get(&(chain.clone(), uid)).and_then(stack) else {
            return false;
        };
        let members: Vec<LoopKey> = self
            .slots
            .iter()
            .filter(|(_, e)| stack(e) == Some(group))
            .map(|(key, _)| key.clone())
            .collect();
        for (chain, uid) in members {
            let Some(entry) = self.slots.get_mut(&(chain.clone(), uid)) else {
                continue;
            };
            let Some(current) = entry.slot.export_raw() else {
                continue;
            };
            let (from, to) = if undo {
                (&mut entry.edit_undo, &mut entry.edit_redo)
            } else {
                (&mut entry.edit_redo, &mut entry.edit_undo)
            };
            let Some(target) = from.pop() else {
                continue;
            };
            to.push(EditStep {
                pcm: current,
                group,
            });
            self.load(&chain, uid, &target.pcm);
        }
        true
    }
}

/// The edit `cycle`-frame loop took, applied to a loop of another length:
/// `None` when the two lengths do not line up.
fn follow_edit(
    pcm: &[f32],
    cycle: usize,
    op: LoopEditOp,
    start: usize,
    end: usize,
) -> Result<Option<Vec<f32>>, LoopEditError> {
    let len = pcm.len() / 2;
    if len == 0 || cycle == 0 {
        return Ok(None);
    }
    if len % cycle == 0 {
        return loop_edit::apply_edit_per_cycle(pcm, op, start, end, cycle).map(Some);
    }
    if cycle % len == 0 {
        // A shorter loop plays several times under the edited one: the region
        // comes out of those passes, so it ends up exactly as long.
        return loop_edit::apply_edit(&pcm.repeat(cycle / len), op, start, end).map(Some);
    }
    Ok(None)
}
