//! Responsibility: commits a block-list edit only when the result obeys the split rules.
//!
//! #328: the edit runs on a COPY of the chain's blocks. A refused edit — a
//! missing block, a bad path, or a result that breaks a split rule — leaves
//! the project exactly as it was, whatever the edit had already moved in the
//! copy. The copy is made on the dispatcher thread, never the audio thread.

use anyhow::Result;
use domain::ids::ChainId;
use project::block::AudioBlock;

use crate::local_dispatcher::LocalDispatcher;
use crate::split_rules::ensure_split_rules;

impl LocalDispatcher {
    pub(crate) fn edit_chain_blocks<R>(
        &self,
        chain: &ChainId,
        edit: impl FnOnce(&mut Vec<AudioBlock>) -> Result<R>,
    ) -> Result<R> {
        self.with_chain(chain, |c| {
            let mut draft = c.blocks.clone();
            let out = edit(&mut draft)?;
            ensure_split_rules(&draft)?;
            c.blocks = draft;
            Ok(out)
        })
    }
}
